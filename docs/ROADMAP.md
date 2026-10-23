# ATOM Roadmap

更新：2026-10-23（JST）。ベース実装済み。詳細は[FOUNDATION.md](FOUNDATION.md)。

状態の意味：

- 🟢 [Done] implemented in the current codebase。実装にのみ使用する。設計文書の作成を機能実装と数えない。
- 🟠 [Next] high-priority unfinished work。直近の実装列。
- 🔴 [Later] planned, but not the closest next step。前段の成立後に行う。
- ⭕️ [Pending] Anything that can't be tested in the dev environment is on hold for now。必要環境がない検証。

今回の成果物はTauriアプリ、Rustコア、CLI、コピーした依存ソース、テストと設計・検証文書。
ThreeX／UNGEの採用ソースはhashとコピー元revisionを記録した。
参考repoのDoneはATOMのDoneへ転記しない。
## 実装済みのベース

- 🟢 [Done] ThreeX core／renderer、UNGE core／executorをATOM内へコピー。hashと元revisionを記録。
- 🟢 [Done] 単一workspaceとCargo.lock。Tauri 2.12.4／wgpu 27.0.1でbuildとClippyが成功。
- 🟢 [Done] Rustの共有Project／State、revision競合拒否、入力検証、ThreeXへの派生Scene。
- 🟢 [Done] JSON保存・再読込、プロセス間の協調ロック、外部変更検出、一時ファイルからの置換。
- 🟢 [Done] CLIのinit／inspect／validate、UNGEのTaskSet→Validateの実行。物理実行0件を明記。
- 🟢 [Done] 三言語Tauri操作パネル、ボックス追加・削除・名前変更・保存・再読込。
- 🟢 [Done] 別ウインドウのThreeX／wgpu描画。macOS実機で2個のボックスを表示。
- 🟢 [Done] 通常143テストと追加GPU2テスト、CLI・ネイティブUI操作の確認。

下記M0以降は完成形に向けた工程。ベースで完了した部分と追加作業を区別する。

## 同一ウインドウとパネル（2026-10-23）

- 🟢 [Done] 親WindowのThreeX／wgpu描画とtoolbar／scene／workflow／viewport子WebViewを合成。
- 🟢 [Done] Rustのタブ・分割ツリー、中央へのタブ結合、四辺への縦横分割、入れ子構成とタブ順変更。
- 🟢 [Done] 分割境界のライブ調整・矢印キー操作、キャンセル、配置競合拒否とリセット。
- 🟢 [Done] 全3パネルの個別フローティング。3Dも同じScene／Device／Rendererを維持してSurfaceを切り替える。
- 🟢 [Done] 同じWebViewをreparent／表示切り替えし、ワークフローの結果を保持。
- 🟢 [Done] 配置v3のツリー・選択・比率・浮動グループ・三言語を保存し、v1／v2を移行。
- 🟢 [Done] 通常168テスト、実GPU5テスト、ビルド、Clippy。macOSでタブ・分割・3D切り離しと共有Scene更新を確認。
- 🟢 [Done] ネイティブWindowの位置・サイズ保存、モニター作業領域／DPIに合わせた画面外復旧。macOSで再起動復元を確認。
- 🟢 [Done] タブグループ単位のフローティング・復帰・個別再分離。3Dを含むグループの選択・並び順を保持。
- 🟢 [Done] ThreeXコピーの面法線・方向光・補助光・環境光と見下ろすカメラ。実GPUで3面の陰影を確認。
- 🟢 [Done] Rust共有カメラのorbit／pan／zoom、一覧と3Dクリックによる選択、選択オブジェクトへのフォーカス、浮動・再ドッキング後の視点保持。
- 🟢 [Done] 深度を使った選択シルエット、ズーム追従XZグリッド、色分け座標軸と個別切替。材質・照明・背景・UIを温かいニュートラル色へ調整。
- 🟠 [Next] オブジェクトの移動・回転・スケールと、変形操作のUndo／Redo。
- 🔴 [Later] カメラの再起動復元、複数選択、全体フィット、ピンチ専用ジェスチャー。
- 🔴 [Later] パネル複製、浮動グループ内の分割、影の投影・PBR。
- ⭕️ [Pending] 複数モニターの実DPI移動、狭いウインドウと最小化復帰、他OSのネイティブ検証。

操作・制限・検証は[DOCKING.md](DOCKING.md)。

## M0 — 接続と描画の成立性を先に確認

🟢 [Done] ThreeX／UNGEの選択crateとATOM単一Tauriホストの依存を解決する。

完了条件：未コミット変更を含め採用ソースを固定し、一つのCargo.lockでCPU APIの統合試験が通る。
初期wgpu候補は27.0.1。ThreeX hostのTauri 2.12.1とUNGEサンプルの2.12.4をそのまま結合しない。

🟢 [Done] Tauri 2／Rust workspaceと小さなheadless CLIを作る。

完了条件：DocumentをRustで生成・保存・再読込でき、UIウインドウを閉じても正本が維持される。
三言語の表示キーとエラーcodeの契約を用意する。依存lockを固定し、使用toolchainを検証記録に残す。

🟠 [Next] 埋め込んだThreeXの3DにUNGEのGPU Graphを加え、共通Device上で合成する。

3D＋WebViewの同一ウインドウ、タブ、分割、3Dを含む3パネルの移動は実装済み。GPU Graph合成は未完了。

完了条件：同一ウインドウで3Dオブジェクトを表示し、resize・Retina・入力・Surface再生成が動く。
パネルを切り離し／再結合しても選択とDocumentが一致する。フレームデータがJS IPCを通らない。
3D／Graphのclearが互いを消さず、同一Device上のtextureを合成できる。
ATOMのタブ・分割・浮動操作で検証済みの状態維持をGPU Graphでも確認する。
専用Viewportウインドウだけが動いても、ドッキング要件は完了にしない。

🟠 [Next] vLLM-apple preview／daemonとStrataを別profileで接続する。

完了条件：利用可能な実サービスでmodels＋短いchatを確認し、未対応機能・timeout・不正JSONを扱える。
サービスが起動していなければmock契約試験だけを完了とし、実接続は未確認のまま残す。
モデルの追加downloadは接続実装の必須処理にしない。

## M1 — 一つの課題を実行して再現する

🟠 [Next] MuJoCoの小型アームとReach課題をRustから実行する。

完了条件：headlessで固定seedのreset／step／停止が動き、状態をwgpuで表示できる。
joint順・単位・固定刻みを検証し、成功・timeout・関節制限で終了する。
当初は1環境で測定し、並列数とメモリーを実測で増やす。

🟠 [Next] Episode／PlanTrace／RunManifestの保存と再生。

完了条件：中断した書き込みを正常データとして読まない。保存済み観測とactionの時刻が一致する。
CLIから再生・結果比較ができる。UI描画の速度が学習記録を欠落させない。

## M2 — LLM追加学習の一巡

🔴 [Later] UNGEへRollout／BuildDataset／FineTune／EvaluateのATOM固有ノードを登録する。

完了条件：大容量値をResource IDで渡し、ATOM JobManagerの進捗・再開・停止をnodeへ投影できる。
物理ループをDAGへ移さず、Futureキャンセル後もWorker停止を確認する。

🔴 [Later] Pick／Place／複数段階課題、型付きSkillPlan、教師候補の検証。

完了条件：手書き制御の基準成功例があり、計画候補を実行結果で選別できる。
遅延応答・リセット前の計画・存在しない対象を拒否する。

🔴 [Later] JSONL生成→MLX-LM LoRA→再読込→未学習課題評価。

完了条件：重みが更新され、同じ評価Workerでbase／adapterを比較できる。
data split・hash・checkpoint・失敗時再開情報が揃う。
学習完走と性能改善を別に報告する。test漏洩がない評価で改善が確認できた場合のみ改善と表示する。
これを最初の「LLM学習システムとして使える」到達点とする。

## M3 — 画像とロボット方策

🔴 [Later] wgpuセンサー、LeRobotDataset v3 export、BC／ACT基準。

完了条件：RGB／depthの座標・時刻・色／単位を検証し、公式loaderで時系列を読み込める。
方策への入力に評価用の正解情報が混入しない。

🔴 [Later] SmolVLA等の方策追加学習とaction chunk推論。

完了条件：採用backendで短い学習と再読込が通り、未学習sceneで基準方策と比較できる。
必要な演算がMacで非対応なら、無断のCPU fallbackで性能保証せずGPU Worker側へ分ける。

## M4 — 規模拡大

🔴 [Later] Domain randomization、失敗例の優先収集、カリキュラム、方策RL。

🔴 [Later] Genesis／MJX／MuJoCo Warp／Isaac Lab・Newtonの追加backend。

🔴 [Later] チーム共有とXServer VPS Cloud連携。通常VPS／GPU WorkerはHTTPS／キュー経由。

各完了条件：既存backendとの物理差・評価条件を明示し、遠隔切断・再接続でジョブと成果物が重複しない。

## 環境が必要な確認

⭕️ [Pending] Linux NVIDIA GPUでの並列物理・CUDA学習。対応機器は本調査で確認できていない。

⭕️ [Pending] Windows／Linuxのnative viewport、入力、ドッキング。各OSでの実機検証が必要。

⭕️ [Pending] 実ロボットのsim-to-real。対象機体・センサー・制御仕様が未指定。

⭕️ [Pending] XServer実環境の接続・DB・NFS・ジョブlease試験。インフラ未構築。

⭕️ [Pending] 配布署名・notarization・独立clean-machine試験。配布環境で実施する。

ベース確認の追加事項：

- ⭕️ [Pending] 狭いウインドウでの実表示、複数モニターのDPI切り替え、最小化復帰のnative確認。Rustの配置計算はDPI 1／1.25／2と極小サイズを検証済み。OSのタイル操作による1024×621の浮動配置と380×560の3D表示は確認済み。最小幅240pxと跨モニターの実機確認は未実施。
- 🟠 [Next] 派生Sceneの全再構築を差分更新へ置き換え、物理tickと永続編集の経路を分離する。
