# ATOM foundation 0.1

2026-10-23（JST）。ローカル開発用のベース実装。現在は[同一ウインドウとドッキング](DOCKING.md)も実装済み。

## 現在できること

- Tauri 2の操作パネルを起動し、英語・日本語・简体中文を切り替える。
- Rust側で所有するプロジェクトの名前変更、ボックス追加・削除・位置／回転／スケール編集、Undo／Redoを行う。
- JSONファイルに保存し、再読込する。古いrevisionの編集を拒否する。
- 別プロセスが同じプロジェクトを開く場合、協調ロックで書き込み競合を防ぐ。
- 外部で保存ファイルが変更されていたら、上書き保存を拒否する。
- UNGEでReachのタスク設定8件を生成・検証する。結果の実行episode数は0。
- ThreeX／wgpuでRust所有のSceneを操作パネルと同じウインドウへ描画する。
- シーン／ワークフロー／3Dをタブ化・縦横の入れ子分割・フローティングし、閉じる操作で戻す。
- タブグループ全体を浮動化・復帰し、ウインドウ位置・サイズ・選択を再起動後に復元する。
- Rust共有のカメラを回転・パン・ズームし、一覧／3Dで選択した箱へフォーカスする。
- 選択輪郭、XZグリッドと座標軸を表示し、グリッド／軸を個別に切り替える。温かいニュートラル材質・照明を使用する。
- 面法線に基づく方向光・環境光と見下ろすカメラで、ボックスの立体感を表示する。
- 操作パネルの閉じる操作では、未保存の確定編集があれば終了を止める。

全ウインドウが一つのRust AppCoreを参照する。WebViewへ渡すのは最大256個の小さな物体記述と状態のみ。
画像、動画、GPUバッファ、モデル重みをJSへ渡す経路はない。
CPU依存のプロジェクト・CLIと、Tauri／wgpuのデスクトップ層を分離している。

## 起動と検証

```sh
cd /Users/Shared/Program/Tauri/ATOM
cargo run --locked -p atom-desktop
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt -p atom-core -p atom-cli -p atom-desktop -- --check
node --check apps/desktop/ui/app.js
```

キャッシュ済みの環境では各Cargoコマンドに`--offline`を追加できる。
確認環境はmacOS arm64、Rust 1.99.1。Cargo.lockを含み、外部path依存はない。
HTML／CSS／JSをTauriに埋め込むため、起動にNode.jsビルドや開発HTTPサーバーは不要。
CLI・Desktopともプロジェクトの親ディレクトリが存在する必要がある。

macOSでは`python3 scripts/build-dev-app.py`で`target/debug/ATOM.app`を生成できる。
ローカル開発用の未署名bundleであり、配布用パッケージではない。

```sh
cargo run --locked -p atom-cli -- init /tmp/atom-example.atom.json "My Lab"
cargo run --locked -p atom-cli -- inspect /tmp/atom-example.atom.json
cargo run --locked -p atom-cli -- validate /tmp/atom-example.atom.json
```

同じファイルをDesktopで開いている間はCLIにもロックエラーが返る。
`.lock`ファイル自体は残るが、ロックはOSが管理する。ファイルの存在を使用中判定に使わない。
外部変更を検出する仕組みは非協調プロセスとの完全な排他を保証しない。
保存は一時ファイルのsync→rename→Unixのdirectory syncで行う。複数ファイルtransaction、Undo永続化は未実装。

## 実測した範囲

| 確認 | 結果 |
|---|---|
| 通常workspace test | 175 passed、0 failed、11 ignored（GPU5件とbenchmark6件） |
| GPU追加試験 | `cargo test -p threex-renderer gpu -- --ignored`：5 passed |
| Clippy | workspace／all-targets、警告をエラーとして成功 |
| Desktop／CLI build | 成功 |
| コピーしたソースのhash | 元のimport hash、ATOM拡張のoverride hashと照合 |
| CLI | init、validateで2ノード完了・タスク設定8件・episode 0件 |
| macOSネイティブ操作 | 追加、名前変更、保存、再読込、設定検証、三言語切り替え |
| macOS GPU表示 | 同一Windowで3Dと2つの子パネルを表示。浮動側から編集し、再ドッキングして状態が一致することを確認 |
| ブラウザー単独表示 | レイアウト表示とDesktop必須表示。Rust APIのmock成功は表示しない |

wgpu経由の依存`block 0.1.6`には将来のRust互換性警告がある。今回のbuild／test失敗ではない。
通常testがskipするGPU試験は、GUIセッションのGPUにアクセス可能な実行環境で別途実行した。
この結果はWindows／Linux、他GPU、長時間稼働、全Surfaceライフサイクルの認定ではない。

初期ベース時点の表示記録（現在の構成・検証は[DOCKING.md](DOCKING.md)）：

![日本語の操作パネル](validation/foundation-main.png)

![ThreeX／wgpuネイティブ描画](validation/foundation-viewport.png)

## 未実装・制限

- MuJoCoの連続再生・pause・継続step、関節編集、学習用センサー。短時間の物理計算とテンプレート関節／接触は下記の範囲で実装済み。
- vLLM-apple／Strata接続、モデルdownload、LoRA、学習Worker、実験ジョブ管理。
- 自由なノード編集。現在は登録済みの固定2ノードで設定検証を行う。
- GPU Graphとの合成、パネル複製、浮動グループ内の分割。
- 任意mesh、テクスチャ、学習用RGB／depth出力。カメラ操作と単一選択・フォーカスは実装済み。
- 編集時の派生Sceneは全再構築。ThreeXの差分更新への最適化は後続。
- 最小幅240pxのWindow実表示検証。1024×621へのOS配置・再起動復元と380×560の3D表示は確認済み。
- 署名・notarization、Windows／Linux、複数ディスプレイ、最小化復帰はPending。

## ソース配置

| パス | 内容 |
|---|---|
| `crates/atom-core` | Project、AppCore、保存、UNGE Registry、ThreeX Scene変換 |
| `crates/atom-cli` | GUI不要の初期化・検査・設定検証 |
| `apps/desktop` | Tauriホスト、native viewport、静的操作UI |
| `vendor/threex` | コピーしたcore／renderer |
| `vendor/unge` | コピーしたcore／executor、ライセンス、参照資料 |
| `vendor/manifest.json` | コピー元・HEAD・未コミット状態・ファイルhash |

English: The foundation includes a native desktop shell, Rust-owned project editing and persistence,
UNGE task-spec validation, and an embedded ThreeX/wgpu viewport with tabbed, split and floating Scene, Workflow and 3D panels. Bounded MuJoCo previews are now implemented (see below); LLM inference, training,
and graph editing remain future work. Tab groups can float together; native window geometry persists, and lighting reveals object faces. The local validation passed 175 normal and five GPU tests.

简体中文：基础版包括原生桌面界面、Rust持有的项目编辑与保存、UNGE任务配置验证、嵌入式ThreeX/wgpu视图，以及支持标签、分割和浮动的场景、工作流与3D面板。
已实现MuJoCo短时预览（见下文）；LLM推理、训练和节点编辑尚未实现。已支持整组浮动、原生窗口位置与尺寸恢复，以及物体光照。本机验证通过175项普通测试及5项GPU测试。

## 重力・物理素材・ロボットテンプレート（2026-10-23）

🟢 [Done] Rustの`PhysicsSpec`と物体ごとの`PhysicalMaterial`を実装。地球9.80665／月1.62／火星3.71／無重力0 m/s²とカスタムXYZ、8素材presetと密度、滑り・ねじり・転がり摩擦を編集できる。重力は物理Z-upで保存し、MuJoCoモデルとwgpuのY-upへ`[x, z, -y]`で回転する。数値変更はカスタムとなる。素材は物理専用で表示色は変更しない。

🟢 [Done] floorの有効化・物理素材と、単独物体の固定／自由を設定可能。床との接触ペアは係数ごとの幾何平均をMJCFの5成分へ展開し、その他の物体間接触はMuJoCoの混合規則を使う。摩擦は滑りが無次元、ねじり・転がりがm。静止／動摩擦を分離するUIではない。密度kg/m³×箱の体積で質量を得て、MuJoCoが慣性を再計算する。素材presetの値は乾燥接触の初期例で、校正済みの実測データではない。

🟢 [Done] schema v2に物理設定・素材・ロボット関節を保存。v1は読み込み時に既定値を補い、明示保存までは元ファイルを変更しない。全変更は既存のrevision検証と128件の履歴を使う。保存復元には設定値・初期配置・関節を含み、Undoスタックと計算中の状態は永続化しない。

🟢 [Done] オリジナルの箱形状から人型（99リンク、98可動接続、182回転自由度、free baseは別途6自由度）と双腕（13リンク、各腕6 hinge、fixed base）を作成。関節軸・範囲・damping、出力上限付き位置サーボをMJCFに記述する。追加は1件の履歴とし、既存物体から離して配置。ルート削除で一体を削除し、Undoで復元する。部品単独削除はトポロジー破損を避けるため拒否。ルートへのFフォーカスは全体を含む。

テンプレートは[人型JSON](../templates/humanoid.atom.json)／[MJCF](../templates/humanoid.xml)、[双腕JSON](../templates/dual-arm.atom.json)／[MJCF](../templates/dual-arm.xml)。歩行・バランス・把持方策や実機精度は含まない。部品の変形はcollision形状の編集であり、関節支点や他リンクを自動移動しない。関節角・支点の編集、一体の変形、可動グリッパーは後続の要件。

🟢 [Done] 短時間MuJoCoプレビューをGUI／CLIへ接続。Rustが生成したMJCFと同梱の固定Python workerを、Pythonの公式MuJoCo bindingで実行する。物理はMuJoCoネイティブエンジン、文書・検証・プロセス管理・最終姿勢・描画はRust所有。WebViewには命令と小さい状態だけを渡す。外部Pythonは`ATOM_MUJOCO_PYTHON`、未指定時は`python3`。動作確認版は3.1.6で、新しい全バージョンの互換性は保証しない。

新規ボックスは床と重ならない高さY＝1から配置する。既存文書の位置は保持するため、床に深く埋まる初期配置は実行前に移動・修正する（強い接触補正が発生する）。

毎回編集状態から0.002秒刻みで計算し、GUIは1／500／1000step、CLIは1〜1000stepを選択する。計算上限15秒、終了時の姿勢と接触数を返す。途中のフレーム再生・継続stepではない。文書を変えずRustの一時Sceneへ描画し、resetで初期配置へ戻す。計算中／結果表示中は文書編集を止め、失敗時はロックを解除する。遅延結果のtokenを照合し、変更後の文書へ古い姿勢を適用しない。実行環境がなければ明示エラーとし、編集・保存・MJCF出力は引き続き可能。

検証（2026-10-23、Apple Silicon macOS）：

- `cargo test --workspace --locked`：181テスト成功（同梱テンプレートと生成コードの一致を含む）、環境依存等14件は通常実行から除外。
- `cargo test --locked -p atom-core --test physics -- --ignored`：MuJoCo実行3テスト成功。4重力preset＋任意方向の自由落下、姿勢軸変換、低／高摩擦の滑り差、密度・スケール由来の質量／慣性、両ロボットの2秒計算と固定ベースを確認。
- `cargo clippy --locked -p atom-core -p atom-desktop -p atom-cli --all-targets -- -D warnings`成功。依存`block 0.1.6`にはRust将来互換性の通知が残る。
- UI：`http://127.0.0.1:8768/?view=scene`／`?view=workflow`、280×850と220×650、英語・日本語・简体中文。Browser plugin not available（専用browser skillなし）のため既存Playwrightで検証。ページ識別・非空・overlayなし・consoleエラーなし・横overflowなし・スクリーンショットと入力操作を確認。ブラウザー側のIPC shim試験と実Tauri試験は区別する。
- 実Tauri／wgpu画面：人型追加→月重力→ゴム素材→Undoでアルミへ復帰→Redo→保存→再読込→MuJoCo 1秒結果→reset、再起動復元、双腕追加・全体フォーカス・両モデル同時の計算と結果フォーカスを確認。検証専用プロジェクトを使用し、既存のユーザープロジェクトは変更していない。

English: Gravity and physical-material authoring, persistence/history, generic humanoid/dual-arm templates, MJCF export and a bounded MuJoCo final-pose preview are implemented. Continuous playback, joint editing, locomotion/grasp policies, liquids and AI integrations remain future work.

简体中文：已实现重力和物理材质编辑、保存与历史、人形／双臂通用模板、MJCF导出及MuJoCo短时最终姿态预览。连续播放、关节编辑、行走／抓取策略、液体与AI集成仍待实现。


## Hugging Face／OpenUSD（2026-10-23）

🟢 [Done] シーンにOpenUSD · SimReadyパネルを追加。公開Hugging Face datasetのrevision・packageを指定し、カタログ取得・名前による絞り込み・取り込み・中止を行う。ローカルUSD／USDA／USDCも絶対パスから取り込める。公式`usd-core==26.8`をATOM専用`.venv-usd`に配置。別環境は`ATOM_USD_PYTHON`で指定し、起動時の自動インストールは行わない。

🟢 [Done] SimReady packaging v1の単一root USD、BOMのhash、各fileのサイズ・SHA-256を確認。元のlayer／payload／texture／MDL／licenseを`.asset-cache`にコピーする。サムネイルは取得しない。1 file 64 MiB、package 256 MiB／2,000 file、layer 256、取り込み180秒、カタログ90秒を上限とする。中止時はRustが子Workerを終了・回収し、文書と履歴を変更しない。

🟢 [Done] reference・payload・通常instance proxy、フレーム0の階層変換、Y／Z-up・metersPerUnitをSDKで解決。Mesh／Cube、凹多角形の三角形化、hole、左手系・負スケールを扱う。表示は一つの静的meshにまとめる。三角形メッシュの上限は300,000頂点／asset、計900,000頂点／project。Rustで有限値・topology・content hashを確認する。

🟢 [Done] schema v3へ出典・commit・license・SDK版・prim path・近似情報と表示形状を保存。v1／v2は読み込み時に移行。Arc共有により履歴で形状を複製せず、取り込み・削除・変形は既存Undo／Redoを使う。保存した文書はSDK・通信・元ファイルがなくても再表示できる。Projectの上限は64 MiB。UIへは頂点配列を送らず、最大256個の物体と小さな資産summaryだけを通知する。

🟢 [Done] wgpuにimported mesh専用pipeline・頂点色buffer・index bufferを追加し、hashで再利用する。通常オブジェクトと同一depthで描画し、変形ではinstance行列を更新する。色はdisplayColor先頭／材質のbase color／face subsetによる近似。選択とフォーカスは現在、外接boxを利用する。既存の箱向け輪郭描画はimported meshには適用せず、精密な輪郭・三角形ピッキングはNext。

🟢 [Done] MuJoCoにmeshを出力し、接触は凸包を用いる。元の単一mass／SimReady metadata massがあれば保持し、それ以外は実行時にcollision形状から算出する。密度・摩擦にはATOMのカスタム既定値を割り当て、ⓘに表示する。MuJoCoがmeshの重心・慣性軸へ座標を変更した分は、結果を表示する際に戻す。凹形状の穴を維持する接触やUSD物理材質・関節の完全変換は未実装。

実データ検証は`nvidia/simready-assets`のcommit `524747d9b5e1f21b5b18b56041788c01d745f1a1`に固定。カタログは2,575 package。`box_cardboard_a01`は554三角形、`box_blue_a01`は14,992三角形を変換し、元のCC-BY-4.0本文と出典をキャッシュに保持した。青い箱は関節を含むため、現在の表示姿勢で固定した旨を警告情報に記録。全カタログの互換性確認ではない。

最終チェック：workspaceの通常テスト186件成功（環境依存等19件は通常実行では除外）、OpenUSD／meshの外部実行4件成功、既存MuJoCo実行3件成功、rendererのGPU6件＋benchmark6件成功。workspace Clippy、format、JavaScript構文チェックも成功。

検証経路：

- Rust：取り込み・保存復元・Undo／Redo・削除時のasset回収、snapshotに形状配列がないこと、不正mesh／hash・古い形式、取り込み中の編集拒否・失敗時の解除を確認。
- OpenUSD SDK：reference合成、centimeter→meter、Z-up→Y-up、凹多角形の面積、依存コピー、不足・範囲外の依存拒否、中止によるWorker終了を確認。
- MuJoCo：実メッシュを用いた物理計算と明示質量、compiler補正後の固定meshの姿勢維持、既存の重力・摩擦・ロボットを確認。
- GPU：代理boxが残らず実三角形・頂点色を表示すること、通常物体との混在・削除時のbuffer更新、既存のグリッド・輪郭・lighting等の回帰を確認。
- UI：Browser plugin not available（専用browser skillなし）のため、既存Playwrightで`http://127.0.0.1:8768/?view=scene`／workflowを検証。英語・日本語・简体中文、280×850／220×650、カタログ・取り込み・summary・Undo／Redoと既存入力／保存操作。ページ識別・非空・overlayなし・consoleエラーなし・横overflowなしを確認。IPC shimのUI検証は実Tauri接続と区別する。
- Native Tauri：取り込み済み実資産を開き、14,992三角形のwgpu表示・全体フォーカス・license／出典tooltip、UIからの2,575件catalog取得とcommit固定を確認。取り込み操作の開始後にmacOSがロックされたため、その後のNative保存・Undo／Redo・cancel操作の確認は⭕️ [Pending]。同機能のRust／ブラウザー試験は成功。

🔴 [Later] USDZ、PointInstancer、texture／UV／normal／PBR・MDL、subdivision評価、time-sampled animation、階層・joint編集、USD差分の書き戻し、中央semantic search。⭕️ [Pending] private／gated datasets、全資産・別OS・別SDK版・巨大sceneとRTX／PhysX等のbackend固有機能。

English: Public SimReady v1 packages and local USD/USDA/USDC now import as retained static meshes. Composition, units, transforms, persistence/history, bounded cancellation and native wgpu geometry are implemented. Textures, articulated control and full USD physics remain planned.

简体中文：现可将公共SimReady v1资源包及本地USD／USDA／USDC导入为静态网格。已实现组合、单位与变换、保存与历史、限时取消和原生wgpu绘制。纹理、关节控制和完整USD物理仍待实现。

## 詳細人体モデル

現在の人型テンプレートは99リンク・98可動接続・182回転自由度＋浮動ベース6自由度。部位別の定義は`crates/atom-core/src/robots/human.rs`、共有する軸・制限の契約は`robots.rs`に置く。`RobotLink.extra_axes`は既定空配列で旧schema 3文書との互換性を維持する。既存のaxis/rangeを第1軸とし、追加の直交回転軸を最大2本保持。rootの追加軸、不正範囲、非有限値、同一方向の軸を拒否する。

98接続の内訳は脊椎24、頭1、顎1、左右各肩帯／上腕／前腕／手掌4＋手指14、左右各大腿／下腿／足関節／足4＋足趾14。回転軸は胴頸部76、両上肢56、両下肢50で182。位置サーボは初期角0を保持する。前腕の回内外は肘リンクの第2軸、手根は手掌の2軸へ集約した近似であり、完全な解剖学モデルではない。

English: The detailed engineering humanoid has 99 links, 98 articulated connections and 182 rotational DoF, plus 6 floating-base DoF. It includes segmented spine, hands and toes; this is not a universal anatomical joint count or a medically calibrated model.

简体中文：详细工程人体模型包含99个连杆、98个可动连接和182个旋转自由度，另有6个浮动基座自由度。支持分节脊柱、手指及脚趾；这不代表医学上的人体关节总数，也不是经过医学标定的模型。
