# vLLM-apple / Strata 接続設計

2026-10-23のローカルソース確認に基づく。以下は接続の設計であり、疎通・モデル推論を実行した結果ではない。
両リポジトリのtracked working treeは確認時にcleanだった。

| 対象 | 確認したrevision |
|---|---|
| `../Soliton/vLLM-apple` | `90f9c2bbd79c24d6d1f82122366e86345268b8b5` |
| `../Soliton/Strata` | `9de04cd44e927a9a3df6bbc0e94fc1be8b844ba4` |

## 1. vLLM-appleは入口を二つに分ける

### Local text preview

[LOCAL-TEXT-PREVIEW.md](../../Soliton/vLLM-apple/docs/LOCAL-TEXT-PREVIEW.md)で確認した範囲：

- `python -m vllm_apple.local_text`による専用入口。既定は`127.0.0.1:8000`。
- `GET /v1/models`、`POST /v1/chat/completions`。モデルaliasは`default_model`。
- 検証対象はM4／32 GiBと固定Gemma 2 2B 4-bit artifact。ハードウェア・依存・モデルhash gateがある。
- context合計4096、最大出力512、concurrency 1、allocator 8 GiBなどの固定上限。
- system role、tools、画像、一般モデル、長時間安定性を対応済みと扱わない。
- daemonのhealth／profile／event APIをこの入口に期待しない。

ATOMでは`vllm_apple_preview`として登録し、models→短いuser-only chatで接続確認する。
JSONを要求しても構造保証はないので、短いSkillPlanをRustでparse・validateする。
過大な状態はobject_idと必要な関係へ要約し、モデルが処理できないときは明示的に失敗する。
最初の役割は軽量な候補生成・説明と接続検証であり、ロボット計画品質は別途評価する。

### General daemon

[api.py](../../Soliton/vLLM-apple/vllm_apple/api.py:493)に次のルートが存在する。

| API | ATOMでの利用 |
|---|---|
| `GET /health` | `control_ready`と`inference_ready`を分けて確認 |
| `GET /v1/runtime` | 状態snapshot |
| `GET /v1/hardware` | ハードウェア情報が取得できる場合に利用 |
| `GET /v1/profiles` | 実行profileの取得 |
| `GET /v1/events` | bounded event購読 |
| `GET /v1/models` | 接続先が公開するモデルIDの確認 |
| `POST /v1/chat/completions` | text／SSE推論 |

`vllm_apple_daemon`をpreviewと別profileにする。`/ready`はcontrolの準備状態であり、推論可能とは限らない。
MLX-LM backendやvLLM-Metal backendの能力はdoctor・設定・モデル・疎通結果ごとに記録する。
予約処理はdaemon内のリクエストadmissionであり、ATOMが他プロセス分まで予約できる公開APIではない。
UDS／session-token-fileが使える構成は専用transportとして後続追加する。

## 2. Strata

[serve/server.py](../../Soliton/Strata/serve/server.py:4325)から確認した契約：

| API／仕様 | ATOMでの扱い |
|---|---|
| `GET /health` | 生存確認。モデルの利用可否はstatusも確認 |
| `GET /v1/models` | 現在のモデルIDを採用 |
| `GET /v1/status` | loaded、concurrency、vision、structured_output等の能力確認 |
| `POST /v1/chat/completions` | 計画候補、タスク候補、失敗説明の生成 |
| JSON形式 | `json_object`／`json_schema`は`prompt_and_validate`方式 |
| constrained decoding | statusでは`False`。文法制約生成として扱わない |
| structured output + tools/MCP | 同時利用はコード上で拒否されるため別モードにする |

[Mac導入資料](../../Soliton/Strata/docs/MAC.md)では既定APIが`http://127.0.0.1:8080/v1`。
ポートは設定可能にし、8000／8080を決め打ちして接続しない。
モデル名や性能の記載はリポジトリ側の報告であり、本調査で実測していない。
大型モデルの常駐とシミュレーター／学習の同時実行を前提にしない。

用途はオフライン教師・タスク分解・計画候補が中心。`vision.available`が真でも個別モデルの画像理解品質を検証する。
Strataのautotuneは実行設定の最適化であり、モデル重みを更新する学習ではない。

## 3. 共通RuntimeGateway

Rust側の共通インターフェース案：

```text
probe(endpoint) -> RuntimeCapabilities
list_models() -> ModelDescriptor[]
generate_plan(PlanRequest) -> ValidatedPlan | TypedError
cancel(request_id) -> CancellationResult
subscribe_status() -> bounded RuntimeEvent stream
```

`RuntimeCapabilities`にはtext／vision／tools／JSON mode／constrained decoding／SSE、
context上限、出力上限、実行可能数、API入口種別、調査・probe時刻を持たせる。
不明はfalseと混同せず`unknown`にする。非対応のfieldを一律送信しない。

接続先が変わったらcapabilityを再確認する。モデル一覧が取れただけでは推論成功と扱わない。
モデルhashがAPIから取得できなければローカルmanifestで補う。正確なartifactが特定できない実験は再現性未確認とする。
各リクエストに元のtask版、prompt hash、seed／sampling設定、token数、遅延、検証エラーを紐付ける。

SSEのtokenはUIプレビューに利用できるが、全応答完了と検証が終わるまで行動を適用しない。
初期案は接続3秒・生成60秒のdeadline、待機queue上限8、計画応答64 KiB上限、JSON修正再試行1回。
実測で調整し、モデルごとのcontext・出力上限を常に優先する。
HTTP 429／503は期限内の有界backoffに限定する。認証・schemaエラーは無条件再送しない。
timeoutで未完了の推論がサーバー側に残る可能性があるため、socket切断を停止完了と同一視しない。
state／request IDで遅延応答を無効化し、重複した行動を実行しない。

## 4. 学習済みモデルを戻す経路

学習artifactは`base_model + tokenizer + adapter + config + hashes`として登録する。
初期の学習前／後比較は、同じMLX-LM evaluation Workerで行う。
Workerは専用環境でmodel＋adapterを読み、ATOM独自のplan RPCでJSONテキストを返す。
これは追加実装する経路であり、既存previewの機能として扱わない。

vLLM-appleのgeneral daemon経由に昇格する際は、指定backendのadapterロードまたは検証済みfusedモデルへの対応を確認する。
Strataも任意LoRAのロードや任意モデル形式のサポートを仮定せず、最初は教師側として利用する。
対応していないartifactを既存モデルへ上書きしたり、hash gateを解除したりしない。

MLX-LMとPyTorch／PEFT間でadapterの互換性を仮定しない。
base revision、target modules、tensor layout、tokenizer、量子化方式を記録し、変換時には一致検証を行う。

## 5. プロセス所有権

初期モードは既存サービスへの接続。疎通できないときに自動download・重いモデル起動を始めない。
管理起動を追加する場合は、ユーザーが選んだ実行ファイル・モデル・ポートをRustが検証し、
プロセス識別と所有権を記録する。ATOMが起動したWorkerだけを終了処理の対象にする。
既存リポジトリのPython環境へ学習依存を追加しない。

本設計では起動コマンドを実行しておらず、APIのlive確認、GPU利用、性能、学習互換性は未確認。
