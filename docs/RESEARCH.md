# 調査記録

確認日：2026-10-23（JST）。公式資料と隣接リポジトリを優先した。
日付は調査日を表す。各URLの`latest`／`stable`／`main`は更新されるため、調査時の内容を恒久仕様とはしない。
ここで挙げた版は閲覧できた資料の版であり、「すべて最新安定版」と認定した一覧ではない。
実装時に互換性を検証してlockfile・revision・artifact hashを固定する。

## 外部の一次資料

| ID | 資料 | 確認した事実と設計への影響 |
|---|---|---|
| S1 | [MuJoCo Overview](https://mujoco.readthedocs.io/en/stable/overview.html) | Cエンジン、MJCF／URDF、MJX／Warpを区別。ATOMは初期物理をC側、描画をwgpu側に分ける |
| S2 | [MuJoCo-rs 6.0.x Installation](https://mujoco-rs.readthedocs.io/en/v6.0.x/installation.html) | Rust 1.95以上と記載。物理のみ利用でき、macOSの既存viewerにはglutinの注意がある。viewerを必須にしない |
| S3 | [MuJoCo Warp](https://mujoco.readthedocs.io/en/latest/mjwarp/index.html) | Warp backendは別実装・数値特性を持つ。CPUと同一挙動／速度を仮定しない |
| S4 | [Genesis World Installation](https://genesis-world.readthedocs.io/en/latest/user_guide/overview/installation.html) | 追加調査で現行資料を再確認。Metalを含む対応表とbackend別実行を掲載。全solverのMac実機動作を意味しない |
| S5 | [Isaac Lab v3.0.0 release](https://github.com/isaac-sim/IsaacLab/releases/tag/v3.0.0) / [正式版Installation](https://isaac-sim.github.io/IsaacLab/v3.0.0/source/isaaclab/installation.html) | 再調査で3.0正式版を確認。physics／renderer／visualizer分離、Newton／OV Libraries、任意のIsaac Sim経路を区別する |
| S6 | [Tauri Architecture](https://v2.tauri.app/concept/architecture/) / [WebView API](https://v2.tauri.app/reference/javascript/api/namespacewebview/) | RustとWebViewの分担。子WebView作成にはunstable APIの注意があり、安易に基盤依存にしない |
| S7 | [Wry wgpu example](https://github.com/tauri-apps/wry/blob/dev/examples/wgpu.rs) | ネイティブGPUとWebViewの併用例。Tauri 2のドッキング製品実装を保証する資料ではない |
| S8 | [wgpu Surface](https://docs.rs/wgpu/latest/wgpu/struct.Surface.html) | Rust側のplatform surfaceへ描画する。閲覧時のdocs表示は30.0.1。採用版はTauri等との検証後に固定 |
| S9 | [LeRobotDataset v3](https://huggingface.co/docs/lerobot/en/lerobot-dataset-v3) | 数値時系列・動画・metadataを分けた形式。エピソード境界をmetadataで管理する |
| S10 | [SmolVLA](https://huggingface.co/docs/lerobot/en/smolvla) | カメラ・ロボット状態・言語指示からaction chunkを生成する。chat endpointだけでは方策実行にならない |
| S11 | [MLX-LM LoRA](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/LORA.md) | LoRA／QLoRA、adapter保存・再開、データ形式を提供。対象モデルで実測する独立Worker候補 |
| S12 | [TRL SFT Trainer](https://huggingface.co/docs/trl/en/sft_trainer) | 言語モデルのSFTとPEFT adapter学習。Linux GPU側へ拡張するときの候補 |

初回調査ではIsaac Labの`release/3.0.0`資料URLが404で、beta資料のみを根拠にしていた。
今回の再調査では公式release一覧のv3.0.0（10月20日公開表示）と`v3.0.0/source/isaaclab/installation.html`を取得できたため、S5と設計を正式版の情報へ更新した。
LeRobotのページには新しい機能一覧と古いリリース予定の文言が併存していたため、ページ単独から最新package版を断定しない。
Hugging Faceの接続ツールは`UNAVAILABLE`だったため、同社の公式Webドキュメントで確認した。

## ローカル確認

- ATOMは作業開始時に空のディレクトリで、既存アプリやGit管理はなかった。
- vLLM-appleのREADME、pyproject、Local Text Preview資料、`vllm_apple/api.py`を確認。
- StrataのAGENTS、Mac資料、`serve/server.py`、`strata_control/tuning.py`、Tauri Cargo設定を確認。
- 詳細なrevision・APIの差は[接続設計](RUNTIME-INTEGRATION.ja.md)に記録。
- `uname -m`はarm64。Rust 1.99.1、Node v26.12.0を確認。これらの版でのアプリbuildは未実施。
- メモリー／機種のsysctl取得は権限制限で失敗。現在のMacを資料のM4／32 GiBと同一だとは認定しない。
- モデル起動、ネットワークAPI疎通、GPU probe、学習、native viewport試験、性能測定は未実施。

## 主な設計判断

追加のローカル調査としてThreeX／UNGEのREADME・取り込み資料・roadmap・関連Rust実装を確認した。
[再利用設計](REFERENCE-INTEGRATION.ja.md)に採用範囲、未実装部分、Tauri固定版の相違、描画clearの合成条件を記録した。
両repoには未コミット変更があり、[主要ファイルのhash](reference-snapshot.json)を別途保存した。
元repoへの変更、build、GPU／ウインドウ操作試験は今回実施していない。

1. 最初の成果は小さな課題のデータ生成→LLM重み更新→再評価。大規模環境の構築を先行させない。
2. 既存ランタイムは推論・教師候補として利用し、学習依存を混在させない。
3. GPU描画面とWebViewの共存は初期スパイクで判定し、未検証のゼロコピー統合を前提にしない。
4. 初期の計画実験は構造化された正解状態を利用する。画像を見た操作の成果とは明確に分ける。
5. 学習済みartifactの評価経路を先に設計し、既存previewのhash gateで一巡が詰まることを避ける。
6. 共有クラウドは後段とし、通常VPSからManaged DBへ直接接続しない。


## 既存ロボット学習基盤の追加調査（2026-10-23）

目的：[ロードマップの統合設計](ROADMAP.md#統合ロボット学習環境への拡張設計)の根拠を更新する。以下は公式資料の確認結果であり、ATOMでの導入・学習・benchmark実行結果ではない。性能の倍率や他製品に対する優位性は採用判断の実測値として使用していない。

| ID | 一次資料 | 確認内容・判断 |
|---|---|---|
| S13 | [MuJoCo Warp](https://mujoco.readthedocs.io/en/latest/mjwarp/) | 多数環境のthroughputと単一stepのlatencyを区別。対話操作の初期基準をCPU MuJoCoとする |
| S14 | [MJX](https://mujoco.readthedocs.io/en/latest/mjx.html) | JAX／Warpの経路を区別し、hardware・対応機能・自動微分を共通扱いしない |
| S15 | [Genesis World Introduction](https://genesis-world.readthedocs.io/en/latest/user_guide/overview/what_is_genesis.html) | 多物理、並列環境、sensor／render／compilerの構成を参考に、能力別にWorkerを追加する |
| S16 | [Genesis Differentiable simulation](https://genesis-world.readthedocs.io/en/latest/user_guide/theory/differentiable_simulation.html) | 接触・couplerの勾配と長時間rolloutのメモリー制約を確認。独立した検証段階に置く |
| S17 | [RoboCasa365概要](https://robocasa.ai/docs/build/html/introduction/overview.html) / [公式repo](https://github.com/robocasa/robocasa) | 物理エンジンではなく、課題・scene・demo・評価を接続する基盤として採用を計画 |
| S18 | [RoboCasa導入](https://robocasa.ai/docs/build/html/introduction/installation.html) | robosuiteのmaster依存を案内。ATOMでは実装時に検証済みcommitを固定し、assetは別取得・hash管理 |
| S19 | [RoboCasa評価概要](https://robocasa.ai/docs/build/html/benchmarking/benchmarking_overview.html) / [Multi-task](https://robocasa.ai/docs/build/html/benchmarking/multitask_learning.html) | multi-task／foundation／lifelongとtask setを分ける。独自評価から公式scoreを推定しない |
| S20 | [RoboCasa Subtask Annotations](https://robocasa.ai/docs/build/html/use_cases/subtask_annotations.html) | privileged state由来のラベルとleaderboardでの使用禁止を確認。研究profileと公式profileを別管理 |

調査時点のIsaac Lab候補はv3.0.0、RoboCasa資料の表示版は1.0.1。Genesis／MuJoCoの`latest`は固定版の証拠にしない。各adapterの着手時にtag／commit、依存lock、asset／dataset hash、OS／driver、採用solver・rendererをRunManifestへ記録する。

今回の設計上の推論は、各基盤を一律に同じエンジンAPIへ押し込むより、SimulationBackend／TaskSuiteAdapter／TrainerAdapter／RenderProviderの責務を分ける方が、公式評価とbackend固有機能を維持しやすいというもの。これはATOMの設計判断で、各プロジェクトがATOMとの互換性を保証しているという意味ではない。

既存のローカル確認節は初回設計時の記録。ATOM自身の現在の実装・GPU検証は[FOUNDATION.md](FOUNDATION.md)と[DOCKING.md](DOCKING.md)を参照する。外部シミュレーターのlive接続・学習性能は今回も未検証。

## 重力・物性・液体・外部AI操作の追加調査（2026-10-23）

[物理設定](ROADMAP.md#重力素材摩擦液体の設定)と[MCP接続](ROADMAP.md#mcp主要aiからのアクセスと制御)の計画に反映した。以下は一次資料の確認であり、ATOMでの物理適用・液体計算・AIクライアント接続の実行結果ではない。

| ID | 一次資料 | 確認内容・設計への反映 |
|---|---|---|
| S21 | [NASA Moon Fact Sheet](https://nssdc.gsfc.nasa.gov/planetary/factsheet/moonfact.html) / [Mars by the Numbers](https://solarsystem.nasa.gov/mars-by-the-numbers/) | 月1.62、火星3.71 m/s²を代表値とし、地球は標準重力9.80665 m/s²を採用する計画。均一重力のpresetで、位置・高度依存の重力場とは区別 |
| S22 | [MuJoCo XML reference](https://mujoco.readthedocs.io/en/stable/XMLreference.html) | gravityは3成分、geomのmaterial参照とfrictionは別設定。frictionはsliding／torsional／rollingであり静止／動摩擦の二値と同一ではない。質量・密度・慣性の推定条件と接触の組合せ規則をadapterで扱う |
| S23 | [MuJoCo Fluid forces](https://mujoco.readthedocs.io/en/stable/computation/fluid.html) | 流体状態を持たない二つの近似モデル。完全な流体動力学は対象外。水中剛体の抵抗と、自由表面を持つ液体の計算を別段階にする根拠 |
| S24 | [Genesis Coupling](https://genesis-world.readthedocs.io/en/latest/user_guide/theory/coupling/index.html) | rigid／SPH／MPM等のsolver組合せとcouplerごとに対応が異なる。液体・剛体連成は個別の課題・backendで検証し、Metalでの全機能対応を推定しない |
| S25 | [MCP specification](https://modelcontextprotocol.io/specification/2026-07-28) / [Transports](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports) | 調査時のlatestが指す版は2026-07-28。stdio／Streamable HTTPを確認。SDK・protocol版を固定し、ATOMのjob寿命をtransportの接続寿命から分離する |
| S26 | [MCP Tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools) | 入出力schema、構造化結果を持つtoolの契約を参照。ATOMではGUIと同じRust命令へ変換し、非対応能力・競合・数値検証を共通化する計画 |
| S27 | [MCP Authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization) | HTTPの認可とstdioの資格情報の扱いを区別。遠隔endpointは採用版の認可仕様に合わせ、projectと操作のscopeを設ける計画 |

重力と媒質の独立設定、有限水域・浮力の実装範囲、素材ペアの保存形式、Rust Control API、function calling bridgeはATOM側の設計判断。各社AI製品での接続方式・対応版は未検証で、ブランド名から互換性を保証しない。vLLM-apple／Strataへの推論接続と、外部AIからATOMを操作するMCP入口を分ける。


## Hugging Face／OpenUSDの実装調査（2026-10-23）

- S28：[SimReady Central Library公式資料](https://docs.omniverse.nvidia.com/simready/latest/simready-central.html)と[指定Space](https://huggingface.co/spaces/nvidia/simready-central)。Spaceは検索UIであり、SDKや資産全体そのものではない。資産は各datasetに版管理される。Hugging Face接続ツールでSpace metadataを確認し、公開Hub APIで配布構造を確認した。
- S29：[nvidia/simready-assets](https://huggingface.co/datasets/nvidia/simready-assets/tree/524747d9b5e1f21b5b18b56041788c01d745f1a1)。調査・実行revisionは`524747d9b5e1f21b5b18b56041788c01d745f1a1`。ルートカタログは2,575ディレクトリ。manifest、`.metadata`のBOM／root_usds、per-asset license、USD／payload／texture／MDLを確認。全件の動作検証数ではない。
- S30：[箱資産manifest](https://huggingface.co/datasets/nvidia/simready-assets/blob/524747d9b5e1f21b5b18b56041788c01d745f1a1/box_cardboard_a01/com.nvidia.simready.packaging.json)。`format_version=1.0`、CC-BY-4.0、BOMとfile SHA-256を実データで検証。箱2種を選択取得し、554／14,992三角形として変換した。元のUSD・licenseは検証用プロジェクト配下へ保存し、配布ソースへ第三者assetを同梱していない。
- S31：[OpenUSD Mesh API](https://openusd.org/dev/api/class_usd_geom_mesh.html)と[UsdGeom概要](https://openusd.org/release/user_guides/schemas/usdGeom/overview.html)。メッシュのpoints／faceVertexCounts／faceVertexIndices、単位、transform、instance proxyをSDKで取得。公式PyPIパッケージ`usd-core==26.8`をATOM専用`.venv-usd`に導入し、実行版を固定した。最新の全SDK版への対応保証ではない。

静的に合成した形状を単一メッシュへ変換し、元データは保持する方針はATOMの設計判断。SimReady側の検証結果、PhysX／MuJoCo variant、MDL、関節制御がATOMでも同一に再現できたとは判定しない。private／gated datasetやライセンス同意が必要なアクセスは今回扱っていない。

## 人体関節の扱い（2026-10-23）

[ISB Joint Coordinate Systems Part II](https://media.isbweb.org/images/documents/standards/isb_jcs_part_ii.pdf)は肩・肘・手首・手の関節運動を部位ごとの座標系で扱う。ATOMも単一の「人体の総関節数」を固定せず、親子接続と軸別自由度を別に記録する。本テンプレートの軸・角度範囲・形状は実装用の初期近似であり、ISB準拠や臨床的校正を主張しない。

[MuJoCo XML Reference — joint](https://mujoco.readthedocs.io/en/stable/XMLreference.html#body-joint)に基づき、1つのbodyに複数の回転hingeを設ける。工学的な接続数98とMuJoCoの回転hinge数182を区別し、freejointを別途6速度自由度として数える。今回の実行検証はローカルMuJoCo 3.1.6で行った。
