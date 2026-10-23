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

テンプレートは[人型JSON](../templates/humanoid.atom.json)／[MJCF](../templates/humanoid.xml)、[双腕JSON](../templates/dual-arm.atom.json)／[MJCF](../templates/dual-arm.xml)。歩行・バランス・把持方策や実機精度は含まない。部品の変形はcollision形状の編集であり、関節支点や他リンクを自動移動しない。関節支点の編集、一体の変形、可動グリッパーは後続の要件。

🟢 [Done] 短時間MuJoCoプレビューをGUI／CLIへ接続。Rustが生成したMJCFと同梱の固定Python workerを、Pythonの公式MuJoCo bindingで実行する。物理はMuJoCoネイティブエンジン、文書・検証・プロセス管理・最終姿勢・描画はRust所有。WebViewには命令と小さい状態だけを渡す。外部Pythonは`ATOM_MUJOCO_PYTHON`、未指定時は`python3`。動作確認版は3.1.6で、新しい全バージョンの互換性は保証しない。

新規ボックスは床と重ならない高さY＝1から配置する。既存文書の位置は保持するため、床に深く埋まる初期配置は実行前に移動・修正する（強い接触補正が発生する）。

毎回編集状態から0.002秒刻みで計算し、GUIは1／500／1000step、CLIは1〜1000stepを選択する。計算上限15秒、終了時の姿勢と接触数を返す。途中のフレーム再生・継続stepではない。文書を変えずRustの一時Sceneへ描画し、resetで初期配置へ戻す。計算中／結果表示中は文書編集を止め、失敗時はロックを解除する。遅延結果のtokenを照合し、変更後の文書へ古い姿勢を適用しない。実行環境がなければ明示エラーとし、編集・保存・MJCF出力は引き続き可能。

検証（2026-10-23、Apple Silicon macOS）：

- `cargo test --workspace --locked`：181テスト成功（同梱テンプレートと生成コードの一致を含む）、環境依存等14件は通常実行から除外。
- `cargo test --locked -p atom-core --test physics -- --ignored`：MuJoCo実行3テスト成功。4重力preset＋任意方向の自由落下、姿勢軸変換、低／高摩擦の滑り差、密度・スケール由来の質量／慣性、両ロボットの2秒計算と固定ベースを確認。
- `cargo clippy --locked -p atom-core -p atom-desktop -p atom-cli --all-targets -- -D warnings`成功。依存`block 0.1.6`にはRust将来互換性の通知が残る。
- UI：`http://127.0.0.1:8768/?view=scene`／`?view=workflow`、280×850と220×650、英語・日本語・简体中文。Browser plugin not available（専用browser skillなし）のため既存Playwrightで検証。ページ識別・非空・overlayなし・consoleエラーなし・横overflowなし・スクリーンショットと入力操作を確認。ブラウザー側のIPC shim試験と実Tauri試験は区別する。
- 実Tauri／wgpu画面：人型追加→月重力→ゴム素材→Undoでアルミへ復帰→Redo→保存→再読込→MuJoCo 1秒結果→reset、再起動復元、双腕追加・全体フォーカス・両モデル同時の計算と結果フォーカスを確認。検証専用プロジェクトを使用し、既存のユーザープロジェクトは変更していない。

English: Gravity and physical-material authoring, persistence/history, generic humanoid/dual-arm templates, MJCF export and a bounded MuJoCo final-pose preview are implemented. Continuous playback, joint-pivot editing, locomotion/grasp policies, liquids and AI integrations remain future work.

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

98接続の内訳は脊椎24、頭1、顎1、左右各肩帯／上腕／前腕／手掌4＋手指14、左右各大腿／下腿／足関節／足4＋足趾14。回転軸は胴頸部76、両上肢56、両下肢50で182。位置サーボは保存した関節角（既定0）を目標とする。前腕の回内外は肘リンクの第2軸、手根は手掌の2軸へ集約した近似であり、完全な解剖学モデルではない。

English: The detailed engineering humanoid has 99 links, 98 articulated connections and 182 rotational DoF, plus 6 floating-base DoF. It includes segmented spine, hands and toes; this is not a universal anatomical joint count or a medically calibrated model.

简体中文：详细工程人体模型包含99个连杆、98个可动连接和182个旋转自由度，另有6个浮动基座自由度。支持分节脊柱、手指及脚趾；这不代表医学上的人体关节总数，也不是经过医学标定的模型。

## 関節角度の編集と姿勢

`RobotLink.angles`はaxes()順のラジアン配列。空配列は全軸0で、旧文書の互換性を保つ。非空の場合は回転軸数と一致し、各値が軸のrange内で有限であることを検証する。rootの角度編集は拒否する。`SetJointAngle`は共通revisionと履歴に従い、変更のない値で履歴を増やさない。

`robots/pose.rs`がrestのworld支点と親のrigid transformから順運動学を計算する。Eulerの表示規則は既存ThreeXと同じRx·Ry·Rz。非一様スケールは形状の属性で、子リンクの骨格寸法を変更しない。文書のobjectsにはrest形状を保持し、AppCoreのposed cache・Scene・Snapshot・選択・focusが導出した姿勢を使用する。選択物体のworld形状編集はFKの逆変換で文書へ戻す。関節角の編集と、関節支点の編集は区別する。

GUIでは関節を選び、度単位の数値／スライダーから軸を編集する。スライダー操作中もRustのプレビュー姿勢を3Dへ反映し、離す時点で1件のUndo履歴を確定する。数値のEnter／blurで確定し、Escは未確定値を戻す。制限外の数値は確定せず直前値に戻す。物理／変形／資産importの処理中は編集を無効化する。

非ゼロ角を含むMJCFは、bodyの深さ優先順にqpos、actuator宣言順にctrlを並べた`authored` keyframeを持つ。Workerはstep前にkeyframeを適用する。サーボは同じ角度を目標とするが、重力・接触・出力制限によって最終姿勢は変わる。物理結果は既にworld座標なので、表示専用projectからrobotグラフを外してFKを二重適用しない。元文書のrobotグラフと角度は維持し、resetで戻す。

検証：通常workspaceテスト、関節の182軸上下限、ディスク保存・再読込・Undo／Redo・競合・preview lock・姿勢付き形状変形。MuJoCo 3.1.6のFKとの全リンク照合、qpos／ctrl一致、物理結果とresetを確認。GUIは実Rust coreのHTTPテストホストで3言語・280／220pxを確認。ネイティブTauri IPCとwgpuの統合操作はPending。

## ライブ関節操作とGPUガイド

`AppCore::begin_joint / preview_joint / commit_joint / cancel_joint`がownerと一意tokenで操作を管理する。プレビュー中のProject、FK objects、Sceneは別に保持し、Snapshot／object選択／rendererはこれを読み、`project()`と保存文書は確定状態を返す。sequenceだけを進め、確定時に共通Commandを1回適用する。取消は元状態をそのまま使い、無変更確定も履歴を増やさない。トランザクション中の保存はアプリ境界で拒否し、一般編集・物理・importはcoreで拒否する。

JSは最大1RPCのpumpで最新角度だけを送り、pointerupを受けたら最後の角度の反映後にcommitする。Esc／pointercancel／lostpointercapture／window blurはcancel。token取得前に取消された場合も、beginの応答を待ってからcancelする。数値入力は従来どおりEnter／blurで1編集。別パネルからの更新は共通Snapshotで表示し、native Undoでpreviewが取消された場合もGUIのpumpを終了する。

`Project::joint_ranges`は親のrigid frameと先行するintrinsic回転から各軸のworld方向とゼロ方向を求める。自身の現在角度をゼロ方向へ二重適用しない。`joints::range_view`を実アプリとGPU統合テストで共有する。レンダラーは3軸×105線分までの固定容量bufferと太線用WGSLを持ち、範囲円弧・上下限・現在角・支点軸をoverlayする。selected boxのoutlineに依存せず、viewportのscissorとnear planeを守る。物理結果には関節角の時系列がないため、physics preview中はこのガイドを表示しない。

検証：連続プレビュー／1履歴／取消／no-op／owner／stale token／revision／全体FK、実coreにつないだブラウザの押下中操作・中断・保存復元・Undo／Redo・3言語・狭幅。Metal GPUの実ピクセルと人体形状の変化も検証。native Tauri IPCとWebViewから他ネイティブviewへpointerが跨る操作は引き続きPending。

## 3D関節ハンドルの直接ドラッグ

`joints/drag.rs`が描画と共通の`arc_point / axis_line`を使い、native windowのphysical座標からガイドを選択する。7 CSS pxの許容幅と支点中心4 CSS pxの曖昧な領域を区別し、軸インデックスは保存文書と同じ順序を使う。DPIとviewportのオフセットを固定して操作を開始し、親関節と先行するintrinsic回転を反映したworld frameを使う。

円弧と現在角度rayは、関節平面へのray交差から増分角を求める。初回クリックでは角度を変更せず、3 CSS pxを越えた移動からpreviewする。±πで増分をunwrapし、制限端での余分な移動を蓄積しない。回転平面がedge-onのときは画面接線で角度を求め、shaftは投影軸に直交する方向を使う。特異点では不正値を送らず、復帰時に基準角を取り直す。

`camera_input`のTao bridgeはjoint handleを一般の形状操作・camera orbitより先に判定する。Shiftパン／Alt orbitは従来どおり優先。1イベント周回に最新ポインターだけをflushし、mouseup直前の値もpreviewしてからcommitする。native-joint専用ownerとtokenで操作を保護し、Esc／native Undo／ビュー外／フォーカス喪失／layout・size・DPI変更はcancelする。古いcontrollerからのcancelでは新しい操作を取消さない。UIの角度欄も共通Snapshotから追従する。

検証はcontrollerの押下座標→角度→core preview／履歴、親姿勢付きの複数軸、Retina／offset／near clip、制限端と復帰、無変更・取消・stale操作を含む。GPU統合試験は同controllerとアプリ側のガイド変換を使い、確定前の人体ピクセル変化、commitとUndo／Redo、取消をMetalで確認。実OSのウインドウイベント配送を伴う手動確認はPending。

## 関節の上下限と現在姿勢の原子的編集

`SetJointRange { id, axis, range }`は既存のprimary range／extra_axes rangeを書き換える。下限＜上限、各値±π以下・有限値、非root、軸の存在を検証する。現在角度が新しい範囲から外れる場合はclampし、rest形状は保持する。範囲と角度の両方が同じProject versionに入るため、保存復元・Undo／Redoが一体になる。0を含まない範囲も対応し、非ゼロ初期姿勢はMJCF authored keyframeへ出力される。

`JointPreviewInfo.edit`はAngle／Lower／Upperを区別する。`begin_joint`は互換性のためAngleを既定とし、端点は`begin_joint_edit`を使う。範囲previewは反対側の端点の変更を拒否し、元の確定Projectから新しい範囲と補正角度を計算する。これにより、狭めた後にドラッグを戻して範囲を広げた場合、元角度が再び範囲内なら元の姿勢へ戻せる。Angle tokenで範囲を変える要求と、その逆の要求は拒否する。commitは同じSetJointRangeを1回適用し、cancelは元文書・履歴を維持する。

3Dのlimit_handleは下限をradius×0.88、上限をradius×1.12に置いたgrip barで、描画とpickが同じ形状を使う。±πで角度位置が一致しても2つのgripが区別できる。下限シアン／上限マゼンタ、操作中は白色。境界のヒットを円弧や角度rayより優先し、既存の回転平面／edge-on mappingを再利用する。nativeドラッグの交差制限はf32::next_down／next_upで表現可能なstrict順序を維持し、非常に狭い区間でもclampの逆転を起こさない。

GUIは各軸に下限・上限の数値欄を持ち、Enter／blurで1編集、Escで未確定値を戻す。現在角度やスライダーのmin／maxも同じSnapshotに追従する。3D preview中は他の編集と保存をロックする。

検証：coreのatomic補正・保存・Undo／Redo・不正範囲・typed token、nativeの両端点・交差・無変更・取消・full turn・極小区間、Metalの描画と履歴統合、MuJoCoのrange／ctrlrange・authored初期値・step／reset、実core GUIの3言語／280・220pxを確認。OSイベント配送そのものを伴う手動QAはPending。

## 位置IKと共有編集

`Command::SolveIk { id, base, target }`は`AppCore::apply`のrevision・previewロック・candidate検証と履歴を共用する。基点は同一ロボット内の厳密な祖先、targetはY-up・mで各成分±10000以内の有限値。末端は選択objectの中心で、基点の下だけをdistal順・intrinsic軸の逆順に探索する。基点の現在のworld姿勢を使うため、回転済み祖先・複数軸にも対応する。geometry・支点・範囲は変更せず、角度だけを保存する。

CCDの相対回転を関節範囲に制限し、3×3の減衰付き最小二乗補正をbacktrackingで改善時だけ受理する。0／±0.16radの決定的seed、最大240 sweep／seed、CCD更新と補正FKを合わせた6000の評価予算を持つ。1mm以内で成功。到達済みなら履歴なし。探索はprivate candidateで行い、成功したrobotだけを反映し、失敗は`ik_unreachable`で文書・dirty・履歴を変えない。これは局所探索の失敗codeで、到達不能を証明しない。

Tauriの`edit`は`spawn_blocking`で共有coreを更新し、既存snapshot通知とmain-thread描画を利用する。GUIは祖先ID・XYZ目標だけを送る。非同期呼び出しでもexpected revisionと同じ共有Mutexで競合を拒否する。IK中は別の専用文書を作らない。GUIの目標入力は作業用で、保存の対象は成功した関節姿勢。向き・自己衝突・環境接触・バランス・物理時間制御は未対応。

`tests/ik.rs`は到達・制限・基点固定・複数軸・全身チェーン・退化姿勢・保存・1履歴・無変更・無効／非収束／競合時の原子性を検証する。MuJoCoのauthored keyframeから手先位置・qpos／ctrl・限界を照合。Metal試験はIK後の実フレーム変化とUndo／Redoの完全なピクセル復元を確認する。GUIは実core＋HTTPホストで適用・取消・保存と3言語／幅900・280・220pxを確認し、実Tauri IPC・OSイベントはPendingに残す。

## Reach／Episode v1

`reach::Runner`はRustが持つ永続Python Workerを使う。選択したDualArm templateをsourceから分離し、重力だけゼロへ固定する。source hashとGUIのrevision、task project、MJCF hash、seed、profile、reference action、joint channel→qpos/qvel indexをmanifestへ保存。最初の非root leafをendpointとし、SplitMix64-v1による可動範囲内の参照角からFKでtargetを得る。元のProjectは更新せず、物理poseは`reach::Preview`の独立した派生sceneへ反映する。これは固定無重力の参照制御課題で、任意scene・学習方策・衝突回避・重力補償は未実装。

resetは`mj_resetData`とauthored keyframe、初期ctrlから開始する。stepは同じ`MjData`のqpos・qvel・warmstart等を継続し、指令を10 ticks（dt=0.002s）保持する。frame 0はaction=None、frame nのactionは(n−1)×0.02〜n×0.02秒で適用した指令。各frameには生qpos/qvel、同期済みgeom pose、endpoint、距離、outcome、engine／Python／platformを保持する。指令はJSON化する前に有限性・次元・関節範囲を検証。観測もID集合・次元・時刻・有限性を検証し、有効な返信だけを記録する。3.5cm以内を5連続で成功、horizon 1〜500でtimeoutとし、同時なら成功を先にする。

Workerはprivate codeのみを`python -I`へ渡し、stdin/stdoutの1要求1返信で制御する。replyは512KiB、startup 15秒、step 3秒、run全体15秒＋最後のstep待ち以内。成功／timeout／エラー／dropでkill/waitする。異常な途中stepは追加せず、最後の有効な部分記録を保存可能にする。Runningの記録はpartialであり、完了したtrialとして数えない。

Episodeは最大64MiB。構造・manifest再生成・MJCF hash・初期qpos／qvel／位置・時刻と指令区間・engineの一貫性・連続成功／終了を検証し、SHA-256付きenvelopeを同じdirectory内のsync済みtempからno-clobberで公開する。同一内容の再保存は冪等、別内容は上書きしない。途中書込・checksum不一致・未来frameや終端後frameを拒否する。`serde_json/float_roundtrip`でf64観測の保存読込を維持する。checksumは署名ではなく、記録済み軌道の正当性を再物理検証するものでもない。

`AppCore::reach_command`はdocument identity／revision、reset/loadのsequence、run ID、step indexで共有状態を保護する。Reach中は通常編集・import・汎用simulationを拒否する。`render_project()`は記録側metadataを参照し、source側meshが同じIDの記録物体／markerへ誤って割り当たることを防ぐ。snapshotは選択frameの物体と小さなReach summaryだけを通知し、全EpisodeはJSへ送らない。cursorとplay generationはRust正本で、Tauriの20ms replay loopは古い世代で停止する。録画再生はPythonを起動せず、loaded Episodeは読み取り専用。記録中も過去をseekできるが、物理継続は末尾へ戻してからに限定する。保存データからsolverを再開する機能はない。

CLIのreach-runは終了まで記録して保存し、episode-replayは指定frameとworld objectsを出力する。episode-compareは一意run IDを除いてmanifestと全frameの完全一致を検証する。同じソース・seed・モデル・runtimeでも環境差の影響はあるため、記録姿勢の再生と物理の再実行を区別する。MuJoCo 3.1.6・Python 3.10の開発環境でseed 0〜9を各2回実行し、10組すべてで全遷移が一致し、各seedで成功した。seed 7は24step／0.48秒、horizon=1はtimeout。unit試験はsemantic corruptionと保存原子性、実MuJoCo試験はdeterminism／終了／worker異常とstale request、Metal試験はseekでのpixel復元を確認。GUIの実core＋MuJoCo試験はHTTP shimで行い、native IPC／window schedulerの手動確認はPending。
