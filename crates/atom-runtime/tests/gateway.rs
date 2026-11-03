use atom_runtime::*;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
struct Request {
    path: String,
    headers: String,
    body: Value,
}
struct Reply {
    status: u16,
    body: String,
    extra: String,
    delay_ms: u64,
}
impl Reply {
    fn json(v: Value) -> Self {
        Self {
            status: 200,
            body: v.to_string(),
            extra: String::new(),
            delay_ms: 0,
        }
    }
}
struct Fake {
    origin: String,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    requests: Arc<Mutex<Vec<Request>>>,
}
impl Fake {
    fn start(handler: impl Fn(&Request) -> Reply + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let end = stop.clone();
        let requests = Arc::new(Mutex::new(vec![]));
        let history = requests.clone();
        let thread = thread::spawn(move || {
            while !end.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut s, _)) => {
                        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                        s.set_nodelay(true).unwrap();
                        let mut bytes = vec![];
                        loop {
                            let mut b = [0];
                            match s.read(&mut b) {
                                Ok(0) => break,
                                Ok(_) => bytes.push(b[0]),
                                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                                Err(_) => break,
                            }
                            if bytes.ends_with(b"\r\n\r\n") || bytes.len() > 8192 {
                                break;
                            }
                        }
                        // Clients may cancel a connection before sending a request.
                        // Never emit an unsolicited HTTP response on EOF/partial headers.
                        if !bytes.ends_with(b"\r\n\r\n") {
                            continue;
                        }
                        let headers = String::from_utf8_lossy(&bytes).to_string();
                        let first = headers.lines().next().unwrap_or_default();
                        if !(first.starts_with("GET ") || first.starts_with("POST "))
                            || !first.ends_with("HTTP/1.1")
                        {
                            continue;
                        }
                        let n = headers
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|n| n.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if n > 8192 {
                            continue;
                        }
                        let mut body = vec![0; n];
                        if s.read_exact(&mut body).is_err() {
                            continue;
                        }
                        let request = Request {
                            path: headers
                                .lines()
                                .next()
                                .unwrap_or_default()
                                .split_whitespace()
                                .nth(1)
                                .unwrap_or_default()
                                .into(),
                            headers,
                            body: serde_json::from_slice(&body).unwrap_or(Value::Null),
                        };
                        let reply = handler(&request);
                        history.lock().unwrap().push(request);
                        let header = format!(
                            "HTTP/1.1 {} OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n{}Connection: close\r\n\r\n",
                            reply.status,
                            reply.body.len(),
                            reply.extra
                        );
                        if reply.delay_ms == 0 {
                            let _ = s.write_all(format!("{header}{}", reply.body).as_bytes());
                        } else {
                            let _ = s.write_all(header.as_bytes());
                            thread::sleep(Duration::from_millis(reply.delay_ms));
                            let _ = s.write_all(reply.body.as_bytes());
                        }
                        // Half-close after a complete response; drain the peer's FIN so a
                        // fast client close cannot turn a valid fixture response into RST.
                        let _ = s.shutdown(std::net::Shutdown::Write);
                        let _ = s.read(&mut [0; 1]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        });
        Self {
            origin,
            stop,
            thread: Some(thread),
            requests,
        }
    }
    fn gateway(&self, p: Profile) -> Gateway {
        Gateway::new(
            Config {
                profile: p,
                origin: self.origin.clone(),
            },
            None,
        )
        .unwrap()
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap()
    }
}
fn models() -> Value {
    json!({"data":[{"id":"test-model"}]})
}
fn completion() -> Value {
    json!({"model":"test-model","choices":[{"message":{"role":"assistant","content":"ATOM OK"},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":3}})
}
#[test]
fn profiles_preserve_unknowns_and_user_only_generation_contract() {
    for p in [
        Profile::VllmApplePreview,
        Profile::VllmAppleDaemon,
        Profile::Strata,
    ] {
        let s = Fake::start(|r| {
            Reply::json(match r.path.as_str() {
                "/v1/models" => models(),
                "/health" => json!({"control_ready":true,"inference_ready":true}),
                "/v1/status" => {
                    json!({"service":"strata","loaded":true,"vision":{"available":false},"context":{"max_positions":8192},"concurrency":{"serving":2},"structured_output":{"formats":["json_object","json_schema"],"constrained_decoding":false}})
                }
                "/v1/chat/completions" => completion(),
                _ => panic!("unexpected endpoint"),
            })
        });
        let g = s.gateway(p);
        let c = g.probe();
        assert!(c.connected && !c.generation_verified);
        assert_eq!(c.models[0].id, "test-model");
        if p == Profile::VllmApplePreview {
            assert!(c.inference_ready.is_none());
            assert_eq!(c.context_tokens, Some(4096));
        }
        if p == Profile::VllmAppleDaemon {
            assert!(c.vision.is_none() && c.json_mode.is_none());
        }
        if p == Profile::Strata {
            assert_eq!(c.constrained_decoding, Some(false));
            assert_eq!(c.json_mode, Some(true));
            assert_eq!(c.context_tokens, Some(8192));
        }
        let result = g.smoke("test-model", 32).unwrap();
        assert!(result.exact_smoke_reply);
        assert_eq!(result.completion_tokens, Some(3));
        let requests = s.requests.lock().unwrap();
        let r = requests
            .iter()
            .find(|r| r.path == "/v1/chat/completions")
            .unwrap();
        assert_eq!(
            r.body,
            json!({"model":"test-model","messages":[{"role":"user","content":PROMPT}],"max_tokens":32,"temperature":0.0,"stream":false})
        );
    }
}
#[test]
fn endpoint_readiness_model_and_budget_gates_never_post() {
    for url in [
        "https://127.0.0.1:8000",
        "http://example.com:8000",
        "http://127.0.0.1:8000/v1",
        "http://user:secret@127.0.0.1:8000",
        "http://127.0.0.1:8000/?token=secret",
        "http://127.0.0.2:8000",
    ] {
        assert!(
            Gateway::new(
                Config {
                    profile: Profile::Strata,
                    origin: url.into()
                },
                None
            )
            .is_err()
        );
    }
    assert_eq!(
        Config {
            profile: Profile::Strata,
            origin: "http://localhost:8080/".into()
        }
        .normalized()
        .unwrap()
        .origin,
        "http://127.0.0.1:8080"
    );
    for p in [Profile::VllmAppleDaemon, Profile::Strata] {
        let s = Fake::start(|r| {
            Reply::json(if r.path == "/v1/models" {
                models()
            } else if r.path == "/health" {
                json!({"control_ready":true,"inference_ready":false})
            } else {
                json!({"service":"strata","loaded":false})
            })
        });
        let g = s.gateway(p);
        assert_eq!(
            g.smoke("test-model", 32).unwrap_err().code,
            "runtime_not_ready"
        );
        assert!(s.requests.lock().unwrap().iter().all(|r| r.body.is_null()));
    }
    let s = Fake::start(|_| Reply::json(models()));
    let g = s.gateway(Profile::VllmApplePreview);
    assert_eq!(
        g.smoke("wrong-model", 32).unwrap_err().code,
        "runtime_model_missing"
    );
    let n = s.requests.lock().unwrap().len();
    assert_eq!(
        g.smoke("test-model", 129).unwrap_err().code,
        "runtime_invalid_budget"
    );
    assert_eq!(n, s.requests.lock().unwrap().len());
}
#[test]
fn http_errors_size_malformed_and_auth_are_bounded_and_redacted() {
    // A cancelled/incomplete request must not produce unsolicited response bytes.
    let cancelled_server = Fake::start(|_| Reply::json(models()));
    let mut peer =
        std::net::TcpStream::connect(cancelled_server.origin.trim_start_matches("http://"))
            .unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    peer.write_all(b"GET /v1/models HTTP/1.1\r\n").unwrap();
    peer.shutdown(std::net::Shutdown::Write).unwrap();
    let mut unsolicited = vec![];
    peer.read_to_end(&mut unsolicited).unwrap();
    assert!(unsolicited.is_empty());
    assert!(cancelled_server.requests.lock().unwrap().is_empty());
    assert!(
        cancelled_server
            .gateway(Profile::VllmApplePreview)
            .probe()
            .connected
    );

    for (status, code) in [
        (401, "runtime_unauthorized"),
        (429, "runtime_busy"),
        (503, "runtime_busy"),
        (500, "runtime_http_error"),
        (302, "runtime_redirect_rejected"),
    ] {
        let s = Fake::start(move |_| Reply {
            status,
            body: "private server error".into(),
            extra: "Location: http://example.com/\r\n".into(),
            delay_ms: 0,
        });
        let g = Gateway::new(
            Config {
                profile: Profile::VllmApplePreview,
                origin: s.origin.clone(),
            },
            Some("private-token".into()),
        )
        .unwrap();
        let c = g.probe();
        assert_eq!(c.checks[0].code, code, "status {status}");
        assert!(!c.connected);
        assert!(!serde_json::to_string(&c).unwrap().contains("private"));
        assert!(
            s.requests.lock().unwrap()[0]
                .headers
                .contains("Bearer private-token")
        );
    }
    for body in [
        "not json".into(),
        "x".repeat(MAX_RESPONSE + 1),
        "{\"data\":[{}]}".into(),
    ] {
        let s = Fake::start(move |_| Reply {
            status: 200,
            body: body.clone(),
            extra: String::new(),
            delay_ms: 0,
        });
        assert!(!s.gateway(Profile::VllmApplePreview).probe().connected);
    }
}
#[test]
fn generation_rejects_unusable_or_over_budget_output_and_image_models() {
    for kind in 0..5 {
        let s = Fake::start(move |r| {
            Reply::json(if r.path == "/v1/models" {
                models()
            } else {
                let mut v = completion();
                match kind {
                    0 => v["model"] = json!("different"),
                    1 => v["choices"][0]["message"]["content"] = json!(""),
                    2 => v["usage"]["completion_tokens"] = json!(1000),
                    3 => v["choices"][0]["finish_reason"] = json!("tool_calls"),
                    _ => v["choices"][0]["message"]["tool_calls"] = json!([{}]),
                };
                v
            })
        });
        assert!(
            s.gateway(Profile::VllmApplePreview)
                .smoke("test-model", 32)
                .is_err()
        );
    }
    let s = Fake::start(|_| {
        Reply::json(
            json!({"data":[{"id":"test-model","architecture":{"output_modalities":["image"]}}]}),
        )
    });
    assert_eq!(
        s.gateway(Profile::Strata).probe().models[0].text_output,
        Some(false)
    );
    assert_eq!(
        s.gateway(Profile::VllmApplePreview)
            .smoke("test-model", 32)
            .unwrap_err()
            .code,
        "runtime_model_missing"
    );
}
#[test]
fn stalled_body_obeys_probe_deadline() {
    let s = Fake::start(|_| Reply {
        status: 200,
        body: models().to_string(),
        extra: String::new(),
        delay_ms: 3500,
    });
    let now = Instant::now();
    let c = s.gateway(Profile::VllmApplePreview).probe();
    assert!(!c.connected);
    assert_eq!(c.checks[0].code, "runtime_timeout");
    assert!(now.elapsed() < Duration::from_secs(5));
}

#[test]
fn declared_aliases_and_null_tool_calls_are_supported_but_not_arbitrary_models() {
    let s = Fake::start(|r| {
        Reply::json(if r.path == "/v1/models" {
            json!({"data":[{"id":"alias","alias_of":"test-model"}]})
        } else {
            let mut v = completion();
            v["choices"][0]["message"]["tool_calls"] = Value::Null;
            v
        })
    });
    let c = s
        .gateway(Profile::VllmApplePreview)
        .smoke("alias", 32)
        .unwrap();
    assert_eq!(c.requested_model, "alias");
    assert_eq!(c.model, "test-model");
}

#[test]
fn scene_to_typed_plan_to_dry_run_is_bounded_user_only_and_non_mutating() {
    use atom_core::*;
    let core = AppCore::new(Project::new("private source name").unwrap()).unwrap();
    let id = core.project().objects[0].id;
    let context = core.planning_context(Some(id)).unwrap();
    let before = serde_json::to_vec(&core.snapshot()).unwrap();
    for profile in [
        Profile::VllmApplePreview,
        Profile::VllmAppleDaemon,
        Profile::Strata,
    ] {
        let s = Fake::start(move |r| {
            Reply::json(match r.path.as_str() {
                "/v1/models" => models(),
                "/health" => json!({"control_ready":true,"inference_ready":true}),
                "/v1/status" => {
                    json!({"service":"strata","loaded":true,"context":{"max_positions":4096}})
                }
                _ => {
                    let mut v = completion();
                    v["choices"][0]["message"]["content"]=json!(json!({"schema_version":1,"summary":"Move candidate","steps":[{"skill":"move_object","id":id,"position":[1,2,3]}]}).to_string());
                    v
                }
            })
        });
        let r = s
            .gateway(profile)
            .generate_plan(&context, "Move selected object", "test-model")
            .unwrap();
        assert!(r.validation.valid);
        assert!(!r.scene_current);
        assert!(r.validation.dry_run.unwrap().would_change);
        let history = s.requests.lock().unwrap();
        let req = history
            .iter()
            .find(|r| r.path == "/v1/chat/completions")
            .unwrap();
        assert_eq!(req.body["max_tokens"], 512);
        assert_eq!(req.body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(req.body["messages"][0]["role"], "user");
        let prompt = req.body["messages"][0]["content"].as_str().unwrap();
        assert!(prompt.len() <= atom_runtime::planning::MAX_PROMPT_BYTES);
        assert!(!prompt.contains("private source name"));
        assert!(prompt.contains("scene"));
        assert!(req.body.get("tools").is_none() && req.body.get("response_format").is_none());
    }
    assert_eq!(before, serde_json::to_vec(&core.snapshot()).unwrap());
}
#[test]
fn invalid_or_truncated_generated_json_is_visible_as_a_rejected_candidate() {
    use atom_core::*;
    let core = AppCore::new(Project::new("Plan").unwrap()).unwrap();
    let c = core.planning_context(None).unwrap();
    for (raw, finish, code) in [
        ("ATOM OK", "stop", "plan_invalid_json"),
        (
            "{\"schema_version\":1,\"summary\":\"floor\",\"steps\":[{\"skill\":\"set_floor\",\"enabled\":true}]}",
            "length",
            "plan_generation_truncated",
        ),
    ] {
        let s = Fake::start(move |r| {
            Reply::json(if r.path == "/v1/models" {
                models()
            } else {
                let mut v = completion();
                v["choices"][0]["message"]["content"] = json!(raw);
                v["choices"][0]["finish_reason"] = json!(finish);
                v
            })
        });
        let result = s
            .gateway(Profile::VllmApplePreview)
            .generate_plan(&c, "Make a plan", "test-model")
            .unwrap();
        assert!(!result.validation.valid);
        assert_eq!(result.validation.issues[0].code, code);
        assert_eq!(result.raw, raw);
        assert!(result.validation.dry_run.is_none());
    }
}
#[test]
fn context_budget_and_invalid_instruction_are_rejected_without_generation() {
    use atom_core::*;
    let core = AppCore::new(Project::new("Budget").unwrap()).unwrap();
    let c = core.planning_context(None).unwrap();
    let s = Fake::start(|r| {
        Reply::json(if r.path == "/v1/models" {
            models()
        } else {
            json!({"service":"strata","loaded":true,"context":{"max_positions":32}})
        })
    });
    assert_eq!(
        s.gateway(Profile::Strata)
            .generate_plan(&c, "Set gravity to moon", "test-model")
            .unwrap_err()
            .code,
        "plan_context_budget"
    );
    assert!(s.requests.lock().unwrap().iter().all(|r| r.body.is_null()));
    let n = s.requests.lock().unwrap().len();
    assert!(
        s.gateway(Profile::Strata)
            .generate_plan(&c, &"x".repeat(513), "test-model")
            .is_err()
    );
    assert_eq!(n, s.requests.lock().unwrap().len());
}
