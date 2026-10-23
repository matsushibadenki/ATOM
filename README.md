# ATOM

<img src="docs/images/atom-image.png">

ロボットシミュレーションから、言語による計画と動作方策を学習・評価するデスクトップ環境。

**実装済み：Tauri 2／Rust、wgpuの3Dビュー、ドッキング・フローティング、物理設定・ロボットテンプレート、Hugging Face／OpenUSDの静的メッシュ取り込み・保存。**
タブを中央へ落とすと結合、四辺へ落とすと分割、ウインドウ外で離すとフローティングします。3Dビューも切り離せます。グループアイコンでタブをまとめて浮動化でき、ウインドウの位置・サイズ・選択タブは再起動後も復元します。3Dは温かいニュートラル材質・陰影で表示し、選択輪郭・XZグリッド・座標軸に対応しています。左ドラッグで回転、右・中ドラッグ／Shift＋左でパン、ホイールでズーム。選択した箱へFキーまたはフォーカスアイコンで視点を合わせられます。
選択オブジェクトはWで移動、Eで回転、Rでスケール、Vでカメラ操作。XYZ入力や軸ハンドルに対応し、⌘Z／⌘⇧Z（Windows・LinuxはCtrl）でUndo／Redoできます。履歴は実行中の最大128件です。
重力プリセット／数値、物理素材・摩擦・密度、床、固定物体、ロボットテンプレートを編集・保存でき、Undo／Redoに対応しています。MuJoCoによる0.002秒／1秒／2秒の物理プレビューは最終姿勢を表示します。LLM接続・学習・連続再生は未実装です。
更新日：2026-10-23（JST）。vLLM-apple／Strataは未接続。ThreeX／UNGEの必要なソースを`vendor/`へコピーし、元ディレクトリに依存せずビルドできます。

## 起動

```sh
cd /Users/Shared/Program/Tauri/ATOM
cargo run --locked -p atom-desktop
```

RustとTauriのOS別開発環境が必要です。Node.jsのビルド工程はありません。
既定ではアプリデータ領域の`workspace.atom.json`を使用します。
任意の保存先は`cargo run --locked -p atom-desktop -- --project /absolute/path/project.atom.json`で指定できます。
親ディレクトリは事前に作成してください。

```sh
cargo run --locked -p atom-cli -- init /tmp/example.atom.json "My Lab"
cargo run --locked -p atom-cli -- inspect /tmp/example.atom.json
cargo run --locked -p atom-cli -- validate /tmp/example.atom.json
```

`validate`は8件のタスク設定を生成・検証します。物理実行や学習は行いません。

- [実装範囲・検証結果](docs/FOUNDATION.md)
- [ドッキングの操作・構成・制限](docs/DOCKING.md)
- macOS開発用アプリ：`python3 scripts/build-dev-app.py`で`target/debug/ATOM.app`を作成。署名済み配布物ではありません。

## 重力・素材・ロボットテンプレート

シーンパネルの「環境」で地球・月・火星・無重力またはXYZ数値を設定します。重力はZ-upの物理座標、3D表示と物体配置はY-upです。物体を選択し「物理素材」で素材、密度、滑り・ねじり・転がり摩擦を編集できます。「適用先：床」で床の摩擦も変更できます。数値変更はカスタム設定になり、保存・再読込・Undo／Redoに反映されます。

テンプレートの＋で人型（99リンク・98可動接続・182回転自由度＋浮動ベース6自由度）、双腕（13リンク・各腕6関節・固定ベース）を追加します。初期姿勢を保持する出力制限付きサーボを備える汎用モデルです。歩行・バランス・把持の学習済み制御は含みません。ルート選択で全体へフォーカスでき、ルートの削除はロボット一体の削除になります。

「ワークフロー」のMuJoCo再生アイコンで短時間計算し、リセットで編集状態へ戻ります。Pythonの`mujoco`パッケージが必要です。既定は`python3`、別環境は`ATOM_MUJOCO_PYTHON=/absolute/path/to/python`で指定します。未導入時も編集・保存・MJCF出力は使えます。モデル・依存パッケージの自動インストールは行いません。

```sh
# 既存ファイルを上書きせず、テンプレートから新規プロジェクトを作成
cargo run --locked -p atom-cli -- template /tmp/humanoid.atom.json humanoid
cargo run --locked -p atom-cli -- template /tmp/dual-arm.atom.json dual_arm
cargo run --locked -p atom-cli -- export-mjcf /tmp/dual-arm.atom.json /tmp/dual-arm.xml
cargo run --locked -p atom-cli -- simulate /tmp/dual-arm.atom.json 500
```

配布用の[人型プロジェクト](templates/humanoid.atom.json)／[MJCF](templates/humanoid.xml)と[双腕プロジェクト](templates/dual-arm.atom.json)／[MJCF](templates/dual-arm.xml)もATOM内に用意しています。手作業で作成した箱形状のモデルで、外部ロボットassetへの依存はありません。詳細は[実装・検証記録](docs/FOUNDATION.md#重力物理素材ロボットテンプレート2026-10-23)を参照してください。

## Hugging Face／OpenUSD

シーンの「OpenUSD · SimReady」で取得元を選びます。Hugging Faceではデータセット（既定：`nvidia/simready-assets`）、revision、パッケージ名を指定し、一覧アイコンでカタログを取得、＋で取り込みます。ローカルUSDは絶対パスで指定します。USD／USDA／USDCに対応し、reference・payloadの依存を含めて合成します。

公式OpenUSD SDK用のATOM専用環境は次で用意できます。この開発環境には導入済みです。

```sh
sh scripts/setup-usd.sh
# 別のSDK環境を使う場合
ATOM_USD_PYTHON=/absolute/path/to/python cargo run --locked -p atom-desktop

cargo run --locked -p atom-cli -- catalog nvidia/simready-assets
cargo run --locked -p atom-cli -- import-hf /tmp/asset.atom.json nvidia/simready-assets box_cardboard_a01
cargo run --locked -p atom-cli -- import-usd /tmp/local.atom.json /absolute/path/asset.usda
```

SimReadyの公開package v1・root USDが1件の資産を取得し、commit固定、BOM／ファイルhash検証、license保持を行います。元のUSDと依存ファイルはプロジェクト横の`.asset-cache`へ保存。表示用メッシュはプロジェクトに含まれるため、保存済み文書はオフラインで復元できます。CLIのimportは既存プロジェクトへ追加し、保存します。

現段階は**フレーム0の静的メッシュ表示**です。座標・単位・通常instance・ベース色を反映し、変形、保存、Undo／Redoに対応します。関節は表示姿勢で固定、texture／MDLの描画とUSD物理の完全変換は今後対応。MuJoCoの接触は凸包近似です。資産のⓘで近似・未対応を確認できます。出典・revision・licenseは保存され、元のライセンス本文もキャッシュに保持します。

English: Import public Hugging Face SimReady v1 packages or local USD/USDA/USDC. The official OpenUSD SDK composes dependencies; Rust/wgpu retains and renders static meshes with base colors. Imports support persistence, transforms and Undo/Redo. Textures, full USD physics, articulated control and USDZ remain planned.

简体中文：支持公共Hugging Face SimReady v1资源包及本地USD／USDA／USDC。官方OpenUSD SDK负责组合依赖，Rust／wgpu保留并绘制带基础色的静态网格。支持保存、变换和撤销／重做。纹理、完整USD物理、关节控制及USDZ仍待实现。

## 日本語

Tauri 2をUIシェル、RustをDocument／State、wgpuを3D描画に使います。以下は今後の拡張方針です。
Mac上のMuJoCoで小さな卓上アーム課題から始め、実行記録を使って小型LLMをLoRA追加学習し、未学習の条件で比較します。
vLLM-appleは軽量なローカル推論、Strataは教師候補・計画候補の生成に活用します。
重みを更新する学習は、独立したMLX／PyTorch Workerが担当します。

- [システム設計](docs/ARCHITECTURE.ja.md)
- [既存ランタイムとの接続設計](docs/RUNTIME-INTEGRATION.ja.md)
- [ThreeX／UNGEを活用する設計](docs/REFERENCE-INTEGRATION.ja.md)
- [実装ロードマップ](docs/ROADMAP.md)
- [調査根拠・未確認事項](docs/RESEARCH.md)

## English

ATOM is a Tauri 2 / Rust desktop foundation for robot simulation, language-planner fine-tuning, and robot-policy evaluation.
Rust owns documents, state, and wgpu rendering. WebViews send commands and display bounded state snapshots. Job management and the following training pipeline are planned.
The first target is a small tabletop-arm task in MuJoCo on macOS. vLLM-apple and Strata provide inference and teacher candidates;
separate MLX or PyTorch workers update model weights. Training is followed by held-out simulation evaluation.
ThreeX supplies native 3D rendering; UNGE supplies workflow execution and macOS panel-composition references. Selected source crates are vendored locally. The foundation integrates an embedded native ThreeX viewport and UNGE task validation. Scene, Workflow and the 3D View support tab groups, nested horizontal/vertical splits, resizable dividers and floating windows. Entire tab groups can float and return together. Layout, selection and native window position/size persist across restarts. Directional and ambient lighting reveal object faces. Warm neutral materials, selection outlines, an adaptive XZ grid and world axes help inspect the scene. Orbit, pan and zoom the shared camera; select boxes in 3D or the Scene list and press F to focus.
The control panel supports English, Japanese, and Simplified Chinese. Gravity presets/vectors, physical materials/friction/density and humanoid/dual-arm templates support save, restore and Undo/Redo. A bounded MuJoCo preview displays the final pose without changing authored data. LLM connections, training and continuous playback remain unimplemented. Run `cargo run --locked -p atom-desktop`.

## 简体中文

ATOM计划采用Tauri 2和Rust，构建机器人仿真、语言规划模型微调与动作策略评估的桌面工作环境。
Rust负责文档、状态、任务管理和wgpu渲染；WebView只发送命令并显示有界的状态快照。
第一阶段在macOS上使用MuJoCo完成小型桌面机械臂任务。vLLM-apple和Strata负责推理及教师候选生成，
独立的MLX或PyTorch Worker负责更新模型权重，并在未参与训练的仿真条件下评估。
ThreeX作为原生3D渲染候选，UNGE提供工作流节点图和macOS面板组合的参考实现；所需源码已复制到本项目，基础版已接入同一窗口内的ThreeX原生3D视图与UNGE任务配置验证。场景、工作流和3D视图支持标签组、横纵嵌套分割、可调分隔线和浮动窗口；支持整组浮动与返回，重启后恢复布局、选中项以及窗口的位置和尺寸。方向光与环境光使物体各面清晰可辨。支持温暖的中性材质、选择轮廓、XZ网格和世界坐标轴。支持相机旋转、平移、缩放，在3D或场景列表中选择方块后按F聚焦。
界面支持英语、日语和简体中文。已实现重力预设／数值、物理材质／摩擦／密度及人形／双臂模板，支持保存、恢复和撤销／重做。MuJoCo短时预览显示最终姿态，不改写编辑状态。LLM连接、训练及连续播放尚未实现。使用`cargo run --locked -p atom-desktop`启动。

Transform objects using W (move), E (rotate), R (scale), or the XYZ inspector. Undo/Redo is shared across windows and keeps up to 128 edits per session.

使用 W（移动）、E（旋转）、R（缩放）或 XYZ 数值面板编辑物体。撤销／重做由所有窗口共享，每次运行最多保留128项编辑记录。
