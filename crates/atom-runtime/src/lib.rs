//! Bounded local inference and candidate generation; never edits the shared document.
use reqwest::{Url, blocking::Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    time::{Duration, Instant},
};
pub mod evaluation;
pub mod planning;
pub mod session;
pub const PROMPT: &str = "Reply with ATOM OK only.";
pub const MAX_RESPONSE: usize = 64 * 1024;
pub const MAX_TOKENS: u32 = 128;
pub const GENERATION_SECONDS: u64 = 15;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    VllmApplePreview,
    VllmAppleDaemon,
    Strata,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub profile: Profile,
    pub origin: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Error {
    pub code: String,
    pub retryable: bool,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.code)
    }
}
impl std::error::Error for Error {}
impl Error {
    pub fn code(code: &str) -> Self {
        Self::new(code)
    }
    fn new(code: &str) -> Self {
        Self {
            code: code.into(),
            retryable: matches!(
                code,
                "runtime_timeout" | "runtime_unavailable" | "runtime_busy"
            ),
        }
    }
}
pub type Result<T> = std::result::Result<T, Error>;
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
impl Config {
    pub fn normalized(&self) -> Result<Self> {
        let mut url =
            Url::parse(&self.origin).map_err(|_| Error::new("runtime_invalid_endpoint"))?;
        if self.origin.len() > 256
            || url.scheme() != "http"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || url.port().is_none()
        {
            return Err(Error::new("runtime_invalid_endpoint"));
        }
        match url.host_str() {
            Some("127.0.0.1" | "[::1]") => {}
            Some("localhost") => {
                url.set_host(Some("127.0.0.1"))
                    .map_err(|_| Error::new("runtime_invalid_endpoint"))?;
            }
            _ => return Err(Error::new("runtime_invalid_endpoint")),
        }
        Ok(Self {
            profile: self.profile,
            origin: url.as_str().trim_end_matches('/').into(),
        })
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub stage: String,
    pub ok: bool,
    pub code: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Model {
    pub id: String,
    pub text_output: Option<bool>,
    pub canonical_id: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Capabilities {
    pub config: Config,
    pub connected: bool,
    pub control_ready: Option<bool>,
    pub inference_ready: Option<bool>,
    pub vision: Option<bool>,
    pub json_mode: Option<bool>,
    pub constrained_decoding: Option<bool>,
    pub context_tokens: Option<u64>,
    pub concurrency: Option<u64>,
    pub models: Vec<Model>,
    pub models_truncated: bool,
    pub checks: Vec<Check>,
    pub generation_verified: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Completion {
    pub config: Config,
    pub model: String,
    pub requested_model: String,
    pub text: String,
    pub finish_reason: String,
    pub request_max_tokens: u32,
    pub prompt_sha256: String,
    pub response_sha256: String,
    pub elapsed_ms: u64,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub exact_smoke_reply: bool,
}
pub struct Gateway {
    config: Config,
    client: Client,
    token: Option<String>,
}
fn network(e: reqwest::Error) -> Error {
    Error::new(if e.is_timeout() {
        "runtime_timeout"
    } else {
        "runtime_unavailable"
    })
}
impl Gateway {
    pub fn new(config: Config, token: Option<String>) -> Result<Self> {
        let config = config.normalized()?;
        if token.as_ref().is_some_and(|t| {
            t.is_empty() || t.len() > 4096 || t.bytes().any(|b| b < 32 || b == 127)
        }) {
            return Err(Error::new("runtime_invalid_token"));
        }
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(GENERATION_SECONDS))
            .build()
            .map_err(network)?;
        Ok(Self {
            config,
            client,
            token,
        })
    }
    fn request(&self, path: &str, body: Option<&Value>, seconds: u64) -> Result<Value> {
        let url = format!("{}{path}", self.config.origin);
        let mut req = if let Some(b) = body {
            self.client.post(url).json(b)
        } else {
            self.client.get(url)
        };
        req = req.timeout(Duration::from_secs(seconds));
        if let Some(t) = &self.token {
            req = req.bearer_auth(t);
        }
        let started = Instant::now();
        let response = req.send().map_err(network)?;
        if !response.status().is_success() {
            return Err(Error::new(match response.status().as_u16() {
                401 | 403 => "runtime_unauthorized",
                404 => "runtime_endpoint_missing",
                429 | 503 => "runtime_busy",
                300..=399 => "runtime_redirect_rejected",
                _ => "runtime_http_error",
            }));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err(Error::new("runtime_response_limit"));
        }
        let mut bytes = vec![];
        response
            .take(MAX_RESPONSE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| {
                Error::new(
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || started.elapsed() >= Duration::from_secs(seconds)
                    {
                        "runtime_timeout"
                    } else {
                        "runtime_unavailable"
                    },
                )
            })?;
        if bytes.len() > MAX_RESPONSE {
            return Err(Error::new("runtime_response_limit"));
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| Error::new("runtime_invalid_response"))?;
        if !value.is_object() {
            return Err(Error::new("runtime_invalid_response"));
        }
        Ok(value)
    }
    pub fn probe(&self) -> Capabilities {
        let mut c = Capabilities {
            config: self.config.clone(),
            connected: false,
            control_ready: None,
            inference_ready: None,
            vision: None,
            json_mode: None,
            constrained_decoding: None,
            context_tokens: None,
            concurrency: None,
            models: vec![],
            models_truncated: false,
            checks: vec![],
            generation_verified: false,
        };
        let mut record = |stage: &str, result: Result<Value>| -> Option<Value> {
            match result {
                Ok(v) => {
                    c.checks.push(Check {
                        stage: stage.into(),
                        ok: true,
                        code: "ok".into(),
                    });
                    Some(v)
                }
                Err(e) => {
                    c.checks.push(Check {
                        stage: stage.into(),
                        ok: false,
                        code: e.code,
                    });
                    None
                }
            }
        };
        let models = self.request("/v1/models", None, 3).and_then(|v| {
            let data = v["data"]
                .as_array()
                .ok_or_else(|| Error::new("runtime_invalid_response"))?;
            if data.iter().any(|m| {
                m["id"].as_str().is_none_or(|s| {
                    s.is_empty() || s.len() > 256 || s.chars().any(char::is_control)
                })
            }) {
                return Err(Error::new("runtime_invalid_response"));
            }
            Ok(v)
        });
        if let Some(v) = record("models", models) {
            let data = v["data"].as_array().unwrap();
            c.models_truncated = data.len() > 64;
            for m in data.iter().take(64) {
                let modalities = m
                    .pointer("/architecture/output_modalities")
                    .and_then(Value::as_array);
                let text_output = modalities.map(|a| a.iter().any(|v| v.as_str() == Some("text")));
                let id = m["id"].as_str().unwrap().to_owned();
                if !c.models.iter().any(|m| m.id == id) {
                    let canonical_id = m["alias_of"]
                        .as_str()
                        .filter(|s| {
                            !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
                        })
                        .map(str::to_owned);
                    c.models.push(Model {
                        id,
                        text_output,
                        canonical_id,
                    });
                }
            }
        }
        match self.config.profile {
            Profile::VllmApplePreview => {
                c.vision = Some(false);
                c.json_mode = Some(false);
                c.constrained_decoding = Some(false);
                c.context_tokens = Some(4096);
                c.concurrency = Some(1);
            }
            Profile::VllmAppleDaemon => {
                if let Some(v) = record(
                    "health",
                    self.request("/health", None, 3).and_then(|v| {
                        if v["control_ready"].is_boolean() && v["inference_ready"].is_boolean() {
                            Ok(v)
                        } else {
                            Err(Error::new("runtime_invalid_response"))
                        }
                    }),
                ) {
                    c.control_ready = v["control_ready"].as_bool();
                    c.inference_ready = v["inference_ready"].as_bool();
                }
            }
            Profile::Strata => {
                if let Some(v) = record(
                    "status",
                    self.request("/v1/status", None, 3).and_then(|v| {
                        if v["service"] == "strata" && v["loaded"].is_boolean() {
                            Ok(v)
                        } else {
                            Err(Error::new("runtime_invalid_response"))
                        }
                    }),
                ) {
                    c.control_ready = Some(true);
                    c.inference_ready = v["loaded"].as_bool();
                    c.vision = v.pointer("/vision/available").and_then(Value::as_bool);
                    c.json_mode = v
                        .pointer("/structured_output/formats")
                        .and_then(Value::as_array)
                        .map(|v| v.iter().any(|f| f == "json_object"));
                    c.constrained_decoding = v
                        .pointer("/structured_output/constrained_decoding")
                        .and_then(Value::as_bool);
                    c.context_tokens = v.pointer("/context/max_positions").and_then(Value::as_u64);
                    c.concurrency = v.pointer("/concurrency/serving").and_then(Value::as_u64);
                }
            }
        }
        c.connected = c.checks.iter().all(|c| c.ok);
        c
    }
    pub fn smoke(&self, model: &str, max_tokens: u32) -> Result<Completion> {
        self.complete(model, PROMPT, max_tokens, MAX_TOKENS, 4096)
    }
    fn complete(
        &self,
        model: &str,
        prompt: &str,
        max_tokens: u32,
        token_limit: u32,
        text_limit: usize,
    ) -> Result<Completion> {
        if !(1..=token_limit).contains(&max_tokens) {
            return Err(Error::new("runtime_invalid_budget"));
        }
        let capabilities = self.probe();
        if let Some(f) = capabilities.checks.iter().find(|c| !c.ok) {
            return Err(Error::new(&f.code));
        }
        if capabilities.inference_ready == Some(false) || capabilities.control_ready == Some(false)
        {
            return Err(Error::new("runtime_not_ready"));
        }
        if prompt != PROMPT
            && capabilities
                .context_tokens
                .is_some_and(|n| prompt.len() as u64 + u64::from(max_tokens) + 256 > n)
        {
            return Err(Error::new("plan_context_budget"));
        }
        let selected = capabilities
            .models
            .iter()
            .find(|m| m.id == model && m.text_output != Some(false))
            .ok_or_else(|| Error::new("runtime_model_missing"))?;
        let body = json!({"model":model,"messages":[{"role":"user","content":prompt}],"max_tokens":max_tokens,"temperature":0.0,"stream":false});
        let started = Instant::now();
        let v = self.request("/v1/chat/completions", Some(&body), GENERATION_SECONDS)?;
        let choices = v["choices"]
            .as_array()
            .filter(|a| a.len() == 1)
            .ok_or_else(|| Error::new("runtime_invalid_response"))?;
        let choice = &choices[0];
        let text = choice
            .pointer("/message/content")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty() && s.len() <= text_limit)
            .ok_or_else(|| Error::new("runtime_invalid_response"))?;
        let reason = choice["finish_reason"]
            .as_str()
            .filter(|s| matches!(*s, "stop" | "length"))
            .ok_or_else(|| Error::new("runtime_invalid_response"))?;
        let actual_model = v["model"]
            .as_str()
            .filter(|m| *m == model || selected.canonical_id.as_deref() == Some(*m))
            .ok_or_else(|| Error::new("runtime_invalid_response"))?;
        if choice.pointer("/message/role").and_then(Value::as_str) != Some("assistant")
            || choice
                .pointer("/message/tool_calls")
                .is_some_and(|v| !v.is_null() && v.as_array().is_none_or(|a| !a.is_empty()))
        {
            return Err(Error::new("runtime_invalid_response"));
        }
        let usage = |name: &str| -> Result<Option<u64>> {
            match v.get("usage").and_then(|u| u.get(name)) {
                None | Some(Value::Null) => Ok(None),
                Some(n) => n
                    .as_u64()
                    .map(Some)
                    .ok_or_else(|| Error::new("runtime_invalid_response")),
            }
        };
        let completion_tokens = usage("completion_tokens")?;
        if completion_tokens.is_some_and(|n| n > u64::from(max_tokens)) {
            return Err(Error::new("runtime_response_limit"));
        }
        Ok(Completion {
            config: self.config.clone(),
            model: actual_model.into(),
            requested_model: model.into(),
            text: text.into(),
            finish_reason: reason.into(),
            request_max_tokens: max_tokens,
            prompt_sha256: digest(prompt.as_bytes()),
            response_sha256: digest(text.as_bytes()),
            elapsed_ms: started.elapsed().as_millis() as u64,
            prompt_tokens: usage("prompt_tokens")?,
            completion_tokens,
            exact_smoke_reply: text.trim() == "ATOM OK",
        })
    }
}
