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
| S4 | [Genesis 1.2.1 Installation](https://genesis-world.readthedocs.io/en/v1.2.1/user_guide/overview/installation.html) | Apple SiliconのGPU／CPU／viewer／headless対応を掲載。Mac用の後続候補として比較する |
| S5 | [Isaac Lab 3.0 beta Local Installation](https://isaac-sim.github.io/IsaacLab/v3.0.0-beta/source/setup/installation/index.html) | Windows／Linux向け導入とNewton kit-less構成を確認。Isaac Simを常に必須とはしない |
| S6 | [Tauri Architecture](https://v2.tauri.app/concept/architecture/) / [WebView API](https://v2.tauri.app/reference/javascript/api/namespacewebview/) | RustとWebViewの分担。子WebView作成にはunstable APIの注意があり、安易に基盤依存にしない |
| S7 | [Wry wgpu example](https://github.com/tauri-apps/wry/blob/dev/examples/wgpu.rs) | ネイティブGPUとWebViewの併用例。Tauri 2のドッキング製品実装を保証する資料ではない |
| S8 | [wgpu Surface](https://docs.rs/wgpu/latest/wgpu/struct.Surface.html) | Rust側のplatform surfaceへ描画する。閲覧時のdocs表示は30.0.1。採用版はTauri等との検証後に固定 |
| S9 | [LeRobotDataset v3](https://huggingface.co/docs/lerobot/en/lerobot-dataset-v3) | 数値時系列・動画・metadataを分けた形式。エピソード境界をmetadataで管理する |
| S10 | [SmolVLA](https://huggingface.co/docs/lerobot/en/smolvla) | カメラ・ロボット状態・言語指示からaction chunkを生成する。chat endpointだけでは方策実行にならない |
| S11 | [MLX-LM LoRA](https://github.com/ml-explore/mlx-lm/blob/main/mlx_lm/LORA.md) | LoRA／QLoRA、adapter保存・再開、データ形式を提供。対象モデルで実測する独立Worker候補 |
| S12 | [TRL SFT Trainer](https://huggingface.co/docs/trl/en/sft_trainer) | 言語モデルのSFTとPEFT adapter学習。Linux GPU側へ拡張するときの候補 |

検索で見つかったIsaac Lab `release/3.0.0/source/isaaclab/installation.html`は直接取得で404だった。
そのため、取得できた3.0 beta資料を根拠として記録し、安定版3.0のリリース済み判定には使用していない。
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
