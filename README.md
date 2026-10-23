# ATOM

ロボットシミュレーションから、言語による計画と動作方策を学習・評価するデスクトップ環境。

**実装済み：Tauri 2／Rust、同一ウインドウの3Dビュー、3パネルのタブ化・縦横分割・フローティング、共有状態・保存。**
タブを中央へ落とすと結合、四辺へ落とすと分割、ウインドウ外で離すとフローティングします。3Dビューも切り離せます。グループアイコンでタブをまとめて浮動化でき、ウインドウの位置・サイズ・選択タブは再起動後も復元します。3Dは温かいニュートラル材質・陰影で表示し、選択輪郭・XZグリッド・座標軸に対応しています。左ドラッグで回転、右・中ドラッグ／Shift＋左でパン、ホイールでズーム。選択した箱へFキーまたはフォーカスアイコンで視点を合わせられます。
物理シミュレーション、LLM接続、学習は未実装です。
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
The control panel supports English, Japanese, and Simplified Chinese. Physics, LLM connections and training remain unimplemented. Run `cargo run --locked -p atom-desktop`.

## 简体中文

ATOM计划采用Tauri 2和Rust，构建机器人仿真、语言规划模型微调与动作策略评估的桌面工作环境。
Rust负责文档、状态、任务管理和wgpu渲染；WebView只发送命令并显示有界的状态快照。
第一阶段在macOS上使用MuJoCo完成小型桌面机械臂任务。vLLM-apple和Strata负责推理及教师候选生成，
独立的MLX或PyTorch Worker负责更新模型权重，并在未参与训练的仿真条件下评估。
ThreeX作为原生3D渲染候选，UNGE提供工作流节点图和macOS面板组合的参考实现；所需源码已复制到本项目，基础版已接入同一窗口内的ThreeX原生3D视图与UNGE任务配置验证。场景、工作流和3D视图支持标签组、横纵嵌套分割、可调分隔线和浮动窗口；支持整组浮动与返回，重启后恢复布局、选中项以及窗口的位置和尺寸。方向光与环境光使物体各面清晰可辨。支持温暖的中性材质、选择轮廓、XZ网格和世界坐标轴。支持相机旋转、平移、缩放，在3D或场景列表中选择方块后按F聚焦。
基础操作界面已支持英语、日语和简体中文。物理仿真、LLM连接和训练尚未实现。使用`cargo run --locked -p atom-desktop`启动。
