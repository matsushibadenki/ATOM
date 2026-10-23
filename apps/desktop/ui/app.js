const translations = {
  en: { project:'Project', scene:'Scene', workflow:'Workflow', tasks:'Task specifications', validation:'Validate specifications', foundation:'Foundation', rust_owned:'Document · State · Renderer / Rust', save:'Save', reload:'Reload saved project', rename:'Rename project', add:'Add box', remove:'Remove box', viewport:'Open viewport', validate:'Validate tasks', closed:'Closed', opening:'Opening…', ready:'Ready', not_run:'Not run', connecting:'Connecting…', saved:'Saved', unsaved:'Unsaved', box:'Box', scope:'Validates eight task specifications. This validator does not run physics. Use MuJoCo preview separately. LLM inference and training are not connected yet.', validated:'8 specifications validated · 0 episodes', desktop_required:'Open this panel in the ATOM desktop app.', revision_conflict:'The project changed. Review the latest state and try again.', unsupported_version:'Unsupported project version.', invalid_project:'Check the project name and values.', project_locked:'The project is open in another process.', external_change:'The saved file changed externally. Save was cancelled.', unsaved_changes:'Save your changes before reloading or closing.', io_error:'Could not read or save the project.', invalid_json:'Invalid project file.', internal_error:'An internal error occurred.', forbidden:'This window is not authorized.', missing_project:'The saved project was not found.', gpu_unavailable:'GPU unavailable', error:'Operation failed.' },
  ja: { project:'プロジェクト', scene:'シーン', workflow:'ワークフロー', tasks:'タスク設定', validation:'設定を検証', foundation:'ベース実装', rust_owned:'Document · State · Renderer / Rust', save:'保存', reload:'保存済みプロジェクトを再読込', rename:'プロジェクト名を変更', add:'ボックスを追加', remove:'ボックスを削除', viewport:'描画ウインドウを開く', validate:'タスクを検証', closed:'未起動', opening:'起動中…', ready:'準備完了', not_run:'未実行', connecting:'接続中…', saved:'保存済み', unsaved:'未保存', box:'ボックス', scope:'8件のタスク設定を検証します。この検証は物理を実行しません。物理はMuJoCoプレビューから実行します。LLM推論・学習は未接続です。', validated:'設定8件を検証 · 実行0件', desktop_required:'ATOMデスクトップアプリで開いてください。', revision_conflict:'別の操作で更新されました。最新の状態を確認してください。', unsupported_version:'未対応のプロジェクト形式です。', invalid_project:'名前と入力値を確認してください。', project_locked:'別のプロセスが使用中です。', external_change:'保存先が外部で変更されました。保存を中止しました。', unsaved_changes:'再読込・終了の前に変更を保存してください。', io_error:'プロジェクトを読み書きできませんでした。', invalid_json:'プロジェクトファイルが不正です。', internal_error:'内部エラーが発生しました。', forbidden:'このウインドウには操作権限がありません。', missing_project:'保存済みプロジェクトが見つかりません。', gpu_unavailable:'GPU利用不可', error:'操作に失敗しました。' },
  'zh-CN': { project:'项目', scene:'场景', workflow:'工作流', tasks:'任务配置', validation:'验证配置', foundation:'基础实现', rust_owned:'Document · State · Renderer / Rust', save:'保存', reload:'重新加载已保存的项目', rename:'重命名项目', add:'添加方块', remove:'删除方块', viewport:'打开渲染窗口', validate:'验证任务', closed:'未启动', opening:'启动中…', ready:'就绪', not_run:'未运行', connecting:'连接中…', saved:'已保存', unsaved:'未保存', box:'方块', scope:'验证8个任务配置。本验证不运行物理仿真，请使用MuJoCo预览。LLM推理与训练尚未连接。', validated:'已验证8个配置 · 运行0个回合', desktop_required:'请在ATOM桌面应用中打开。', revision_conflict:'项目已更新，请检查最新状态。', unsupported_version:'不支持的项目版本。', invalid_project:'请检查名称和输入值。', project_locked:'另一个进程正在使用此项目。', external_change:'文件已被外部修改，保存已取消。', unsaved_changes:'重新加载或关闭前请保存更改。', io_error:'无法读取或保存项目。', invalid_json:'无效的项目文件。', internal_error:'发生内部错误。', forbidden:'此窗口没有操作权限。', missing_project:'找不到已保存的项目。', gpu_unavailable:'GPU不可用', error:'操作失败。' }
};
const dockTranslations = {
 en:{float_group:'Float tab group',return_group:'Return tab group to dock',viewport:'3D View',drag:'Drag a tab to a panel: center joins tabs; edges split. Outside floats. Esc cancels.',float:'Float active panel',target:'Dock target',placement:'Placement',center:'Tab',left:'Left',right:'Right',top:'Above',bottom:'Below',dock_apply:'Apply docking',return_panel:'Return to previous dock',workspace:'Workspace',reset_layout:'Reset layout',resize:'Resize split; arrow keys adjust, Esc cancels',layout_conflict:'The layout changed. Try again.',invalid_layout:'Invalid layout.',panel_busy:'A layout gesture is in progress.',panel_error:'Could not move the panel.',layout_save_failed:'Layout changed, but could not be saved.',gpu_waiting:'Waiting for GPU',objects:'objects'},
 ja:{float_group:'タブグループを切り離す',return_group:'タブグループをドックへ戻す',viewport:'3Dビュー',drag:'タブをドラッグ：中央でタブ結合、四辺で分割、ウインドウ外で切り離し。Escで中止。',float:'アクティブなパネルを切り離す',target:'移動先のパネル',placement:'配置方法',center:'タブ',left:'左',right:'右',top:'上',bottom:'下',dock_apply:'配置を適用',return_panel:'元のドックへ戻す',workspace:'ワークスペース',reset_layout:'配置をリセット',resize:'分割を調整。矢印キーで微調整、Escで中止。',layout_conflict:'配置が更新されました。もう一度操作してください。',invalid_layout:'配置が不正です。',panel_busy:'配置を操作中です。',panel_error:'パネルを移動できませんでした。',layout_save_failed:'配置は変更されましたが、保存できませんでした。',gpu_waiting:'GPU待機中',objects:'オブジェクト'},
 'zh-CN':{float_group:'浮动标签组',return_group:'将标签组返回停靠区',viewport:'3D视图',drag:'拖动标签：中央合并，边缘拆分，窗口外浮动。Esc取消。',float:'浮动当前面板',target:'目标面板',placement:'位置',center:'标签',left:'左侧',right:'右侧',top:'上方',bottom:'下方',dock_apply:'应用布局',return_panel:'返回原停靠位置',workspace:'工作区',reset_layout:'重置布局',resize:'调整分隔线。方向键微调，Esc取消。',layout_conflict:'布局已更新，请重试。',invalid_layout:'无效的布局。',panel_busy:'正在调整布局。',panel_error:'无法移动面板。',layout_save_failed:'布局已更改，但无法保存。',gpu_waiting:'等待GPU',objects:'个对象'}
};
for(const language of Object.keys(translations)) Object.assign(translations[language],dockTranslations[language]);
const cameraTranslations={en:{show_grid:'XZ grid (Y = 0); spacing follows zoom',show_axes:'World axes: X red, Y green (up), Z blue',pan_mode:'Pan with left drag',selected_object:'Selected object',focus_selection:'Focus selected object (F)',reset_camera:'Reset camera (0)',select_object:'Select object',no_selection:'Select an object first.',camera_help:'Drag a joint arc/axis to pose. Left drag elsewhere: orbit. Right/middle drag or Shift + left drag: pan. Wheel: zoom. Click an object to select. F: focus selection. 0: reset. Esc: cancel drag.'},ja:{show_grid:'XZグリッド（Y＝0）。ズームに応じて間隔を調整',show_axes:'ワールド座標軸：Xは赤、Yは緑（上）、Zは青',pan_mode:'左ドラッグをパンに切り替える',selected_object:'選択中のオブジェクト',focus_selection:'選択オブジェクトにフォーカス（F）',reset_camera:'視点をリセット（0）',select_object:'オブジェクトを選択',no_selection:'オブジェクトを選択してください。',camera_help:'関節の円弧・軸は左ドラッグで角度変更。それ以外は左ドラッグで視点回転。右・中ドラッグ、またはShift＋左ドラッグでパン。ホイールでズーム。クリックで選択。Fでフォーカス、0で視点リセット、Escでドラッグを中止。'},'zh-CN':{show_grid:'XZ网格（Y＝0），间距随缩放调整',show_axes:'世界坐标轴：X红色、Y绿色（向上）、Z蓝色',pan_mode:'左键拖动平移',selected_object:'选中对象',focus_selection:'聚焦选中对象（F）',reset_camera:'重置视角（0）',select_object:'选择对象',no_selection:'请先选择对象。',camera_help:'拖动关节圆弧或轴可调整角度，其他位置左键旋转视角；右键、中键或Shift＋左键拖动平移；滚轮缩放。单击对象选择，F聚焦，0重置，Esc取消拖动。'}};
for(const language of Object.keys(translations))Object.assign(translations[language],cameraTranslations[language]);
const transformTranslations={
 en:{undo:'Undo (⌘Z / Ctrl+Z)',redo:'Redo (⌘⇧Z / Ctrl+Shift+Z)',tool_camera:'Camera / select (V)',tool_move:'Move (W)',tool_rotate:'Rotate (E)',tool_scale:'Scale (R)',transform_axis:'Transform axis (X / Y / Z)',axis_free:'Free',position:'Position',rotation:'Rotation (degrees)',scale_value:'Scale',transform_help:'Drag the selected object or an axis handle. W move, E rotate, R scale, V camera. X/Y/Z constrain an axis. Alt + left drag orbits; right/middle drag pans. Esc cancels. A completed drag is one undo step.',numeric_help:'Edit XYZ values; rotation is in degrees. Enter or leaving a field commits one history step. Esc cancels. Undo/Redo affects document edits; camera and layout stay independent.',edit_busy:'Reset the physics preview or finish/cancel the active transform.',history_empty:'No history available.'},
 ja:{undo:'取り消す（⌘Z / Ctrl+Z）',redo:'やり直す（⌘⇧Z / Ctrl+Shift+Z）',tool_camera:'カメラ・選択（V）',tool_move:'移動（W）',tool_rotate:'回転（E）',tool_scale:'スケール（R）',transform_axis:'変形の軸（X / Y / Z）',axis_free:'自由',position:'位置',rotation:'回転（度）',scale_value:'スケール',transform_help:'選択した物体または軸ハンドルをドラッグ。Wで移動、Eで回転、Rでスケール、Vでカメラ。X/Y/Zで軸を制限。Alt＋左で視点回転、右・中ドラッグでパン。Escで中止。1回のドラッグを1件の履歴として確定します。',numeric_help:'XYZを入力。回転は度。Enterまたは入力欄を離れると1件の履歴として確定、Escで中止。Undo／Redoは文書の編集が対象です。',edit_busy:'物理プレビューをリセット、または操作中の変形を確定・中止してください。',history_empty:'操作履歴がありません。'},
 'zh-CN':{undo:'撤销（⌘Z / Ctrl+Z）',redo:'重做（⌘⇧Z / Ctrl+Shift+Z）',tool_camera:'相机／选择（V）',tool_move:'移动（W）',tool_rotate:'旋转（E）',tool_scale:'缩放（R）',transform_axis:'变换轴（X / Y / Z）',axis_free:'自由',position:'位置',rotation:'旋转（度）',scale_value:'缩放',transform_help:'拖动选中对象或轴手柄。W移动，E旋转，R缩放，V相机。X/Y/Z约束轴。Alt＋左键旋转视角，右键或中键平移。Esc取消，一次拖动记录为一次操作。',numeric_help:'输入XYZ数值，旋转单位为度。Enter或离开输入框提交一次操作，Esc取消。撤销／重做针对文档编辑。',edit_busy:'请重置物理预览，或提交／取消当前变换。',history_empty:'没有可用历史。'}
};
for(const language of Object.keys(translations))Object.assign(translations[language],transformTranslations[language]);
const physicsTranslations = {
 en:{environment:'Environment',gravity:'Gravity',earth:'Earth',moon:'Moon',mars:'Mars',zero:'Zero gravity',custom:'Custom',gravity_help:'Gravity in m/s², physics coordinates: Z up. Converted to the Y-up 3D view in Rust. Numeric edits use Custom. Range: ±1000 per axis.',floor_enabled:'Contact floor · Y = 0',floor:'Floor',robot_template:'Robot template',humanoid:'Humanoid',dual_arm:'Dual arm',add_robot:'Add robot template',robot_help:'Generic box-link robots with joints, limits and bounded position servos. Humanoid: floating base, 98 articulated connections / 182 rotational DoF (plus 6 floating-base DoF); dual arm: fixed base, 6 hinges per arm. Engineering approximation; not a medical anatomical joint count. No balance or grasp policy. Remove the root to remove a robot. Moving geometry does not move its joint pivots.',physical_material:'Physical material',material_target:'Apply to',density:'Density · kg/m³',sliding:'Sliding μ',torsional:'Torsion · m',rolling:'Rolling · m',fixed_body:'Fixed body',material_help:'Illustrative dry-contact presets, not calibrated measurements. MuJoCo sliding, torsional and rolling friction; not separate static/dynamic coefficients. Mass and inertia follow box volume × density. Visual color stays unchanged. Floor pairs use geometric-mean friction; object pairs use MuJoCo mixing.',wood:'Wood',steel:'Steel',aluminum:'Aluminum',rubber:'Rubber',plastic:'Plastic',glass:'Glass',concrete:'Concrete',ice:'Ice',physics_preview:'MuJoCo preview',run_physics:'Run bounded physics preview',reset_physics:'Restore authored scene',duration:'Simulation duration from authored state',simulation_help:'Each run starts from the authored scene, at 0.002 s per step. Shows the final pose; does not bake it into the saved document. Reset before editing. Worker timeout: 15 s. Requires Python with MuJoCo; set ATOM_MUJOCO_PYTHON if needed.',sim_running:'Simulating…',sim_idle:'Authored state',physics_unavailable:'MuJoCo unavailable. Set ATOM_MUJOCO_PYTHON to a Python with mujoco installed.',physics_failed:'Physics failed or timed out. Check geometry and runtime.',base:'Base',pelvis:'Pelvis',torso:'Torso',head:'Head',left:'Left',right:'Right',thigh:'Thigh',shin:'Shin',foot:'Foot',upper_arm:'Upper arm',forearm:'Forearm',gripper:'Gripper',shoulder:'Shoulder',elbow:'Elbow',wrist:'Wrist'},
 ja:{environment:'環境',gravity:'重力',earth:'地球',moon:'月',mars:'火星',zero:'無重力',custom:'カスタム',gravity_help:'単位m/s²、物理座標はZが上。RustでY-upの3D表示へ変換します。数値編集はカスタム設定です。各軸±1000まで。',floor_enabled:'接触する床 · Y＝0',floor:'床',robot_template:'ロボットテンプレート',humanoid:'人型ロボット',dual_arm:'双腕ロボット',add_robot:'ロボットテンプレートを追加',robot_help:'関節・可動範囲・出力制限付き位置サーボを持つ汎用モデル。人型は98可動接続・182回転自由度＋浮動ベース6自由度、双腕は固定ベースと片腕6関節。工学的な近似で、医学的な総関節数ではありません。姿勢安定・把持方策は未搭載。ルートを削除すると一体を削除します。形状の移動では関節支点は動きません。',physical_material:'物理素材',material_target:'適用先',density:'密度 · kg/m³',sliding:'滑り摩擦 μ',torsional:'ねじり · m',rolling:'転がり · m',fixed_body:'固定する',material_help:'プリセットは乾燥接触の初期例で、実測保証値ではありません。MuJoCoの滑り・ねじり・転がり摩擦です。静止／動摩擦の区別はありません。箱の体積×密度で質量と慣性を算出。表示色は維持。床との摩擦は幾何平均、物体間はMuJoCoの組合せ規則を使います。',wood:'木材',steel:'鋼',aluminum:'アルミニウム',rubber:'ゴム',plastic:'樹脂',glass:'ガラス',concrete:'コンクリート',ice:'氷',physics_preview:'MuJoCoプレビュー',run_physics:'短時間の物理プレビューを実行',reset_physics:'編集時のシーンへ戻す',duration:'編集時の状態からの実行時間',simulation_help:'毎回、編集時の状態から0.002秒刻みで計算し、最終姿勢を表示します。保存文書には焼き込みません。編集前にリセットしてください。処理上限15秒。MuJoCo入りPythonが必要です。必要に応じATOM_MUJOCO_PYTHONを指定。',sim_running:'物理計算中…',sim_idle:'編集時の状態',physics_unavailable:'MuJoCoが見つかりません。ATOM_MUJOCO_PYTHONにMuJoCo入りPythonを指定してください。',physics_failed:'物理計算に失敗、または時間上限に達しました。形状と実行環境を確認してください。',base:'基台',pelvis:'骨盤',torso:'胴体',head:'頭',left:'左',right:'右',thigh:'大腿',shin:'下腿',foot:'足',upper_arm:'上腕',forearm:'前腕',gripper:'グリッパー',shoulder:'肩',elbow:'肘',wrist:'手首'},
 'zh-CN':{environment:'环境',gravity:'重力',earth:'地球',moon:'月球',mars:'火星',zero:'零重力',custom:'自定义',gravity_help:'单位m/s²，物理坐标Z向上。Rust转换至Y向上的3D视图。数值修改使用自定义，每轴范围±1000。',floor_enabled:'接触地面 · Y＝0',floor:'地面',robot_template:'机器人模板',humanoid:'人形机器人',dual_arm:'双臂机器人',add_robot:'添加机器人模板',robot_help:'带关节、限位及限力位置伺服的通用模型。人形：浮动基座、98个可动连接及182个旋转自由度，另有6个基座自由度；双臂：固定基座、每臂6关节。属于工程近似，并非医学关节总数。无平衡或抓取策略。删除根节点可移除整台机器人。移动几何体不会移动关节支点。',physical_material:'物理材质',material_target:'应用于',density:'密度 · kg/m³',sliding:'滑动摩擦 μ',torsional:'扭转 · m',rolling:'滚动 · m',fixed_body:'固定物体',material_help:'预设是干燥接触的起始示例，不是校准数据。使用MuJoCo滑动、扭转和滚动摩擦，不区分静摩擦与动摩擦。质量和惯性随体积与密度更新，保持显示颜色。与地面接触采用几何平均，其余采用MuJoCo混合规则。',wood:'木材',steel:'钢',aluminum:'铝',rubber:'橡胶',plastic:'塑料',glass:'玻璃',concrete:'混凝土',ice:'冰',physics_preview:'MuJoCo预览',run_physics:'运行短时物理预览',reset_physics:'恢复编辑场景',duration:'从编辑状态开始的时长',simulation_help:'每次从编辑状态开始，以0.002秒步长计算并显示最终姿态。不写入保存文档，编辑前请重置。超时15秒。需要安装MuJoCo的Python，可用ATOM_MUJOCO_PYTHON指定。',sim_running:'仿真中…',sim_idle:'编辑状态',physics_unavailable:'找不到MuJoCo，请用ATOM_MUJOCO_PYTHON指定已安装MuJoCo的Python。',physics_failed:'物理计算失败或超时，请检查几何体和运行环境。',base:'基座',pelvis:'骨盆',torso:'躯干',head:'头部',left:'左',right:'右',thigh:'大腿',shin:'小腿',foot:'脚',upper_arm:'上臂',forearm:'前臂',gripper:'夹爪',shoulder:'肩',elbow:'肘',wrist:'腕'}
};
for(const language of Object.keys(translations))Object.assign(translations[language],physicsTranslations[language]);

const usdTranslations={
 en:{asset_source:'Source',local_usd:'Local USD',hf_repo:'Dataset',asset_revision:'Revision / commit',asset_catalog:'Load package catalog',asset_package:'Package name / filter',import_asset:'Import OpenUSD asset',usd_help:'Compose USD/USDA/USDC with the official OpenUSD SDK. Hugging Face: SimReady packaging v1, one root USD, public datasets. Local dependencies must be inside the USD folder. Imported as one static mesh at frame 0; units and transforms are baked. Textures, MDL shaders, articulations and full USD physics are not reproduced. Meshes remain in Rust/GPU. Imports support save and Undo.',cancel_asset:'Cancel import',usd_cancelled:'Import cancelled.',usd_importing:'Importing…',usd_catalog_loading:'Loading catalog…',usd_catalog_count:'packages',usd_imported:'Imported',usd_unavailable:'OpenUSD SDK unavailable. Run scripts/setup-usd.sh or set ATOM_USD_PYTHON.',usd_network:'Could not fetch Hugging Face data. Check connection and public repository access.',usd_invalid:'Invalid USD, repository or package.',usd_budget:'Asset exceeds the file, mesh or time budget.',usd_unsupported:'Unsupported USD feature or package format.',usd_integrity:'Asset checksum verification failed.',usd_dependency:'A USD dependency is missing or outside the package.',usd_failed:'USD worker failed or timed out.',articulation_static:'Joint hierarchy is frozen into the visible pose; articulation control is not imported.',display_color_approximation:'Varying displayColor uses its first color in this version.',physics_defaults:'ATOM contact/density defaults are assigned; authored mass is retained when available.',static_mesh_import:'Imported as one static mesh; source hierarchy/physics remain in the cached USD.',convex_collision:'MuJoCo collision uses the convex hull of the mesh.',mdl_approximation:'MDL shading is approximated with base colors.',textures_not_rendered:'Textures are cached but not rendered yet.',subdivision_control_mesh:'Displaying the subdivision control mesh.',unsupported_primitives:'Some non-mesh geometry is not displayed.'},
 ja:{asset_source:'取得元',local_usd:'ローカルUSD',hf_repo:'データセット',asset_revision:'revision・commit',asset_catalog:'パッケージ一覧を取得',asset_package:'パッケージ名・絞り込み',import_asset:'OpenUSD資産を取り込む',usd_help:'公式OpenUSD SDKでUSD／USDA／USDCを合成します。Hugging FaceはSimReady packaging v1・root USDが1件の公開データセットに対応。ローカルの依存はUSDと同じフォルダー内に配置。フレーム0の静的メッシュとして単位・変換を反映します。テクスチャ・MDL・関節・USD固有の物理の完全再現は今後対応。形状はRust／GPUに保持。保存・Undoに対応。',cancel_asset:'取り込みを中止',usd_cancelled:'取り込みを中止しました。',usd_importing:'取り込み中…',usd_catalog_loading:'一覧を取得中…',usd_catalog_count:'パッケージ',usd_imported:'取り込み完了',usd_unavailable:'OpenUSD SDKがありません。scripts/setup-usd.shを実行するかATOM_USD_PYTHONを指定してください。',usd_network:'Hugging Faceへ接続できません。通信と公開リポジトリへのアクセスを確認してください。',usd_invalid:'USD・リポジトリ・パッケージの指定が不正です。',usd_budget:'ファイル・形状・処理時間の上限を超えました。',usd_unsupported:'未対応のUSD機能またはパッケージ形式です。',usd_integrity:'取得した資産のhash検証に失敗しました。',usd_dependency:'USDの依存が不足、またはパッケージの範囲外です。',usd_failed:'USD処理に失敗、または時間上限に達しました。',articulation_static:'関節を含む階層は表示姿勢で固定。関節制御は取り込んでいません。',display_color_approximation:'複数のdisplayColorは先頭色による近似表示です。',physics_defaults:'接触・密度にはATOMの既定値を設定。取得できた元の質量は保持します。',static_mesh_import:'静的メッシュ1個として取り込み。元の階層・物理はキャッシュ内のUSDに保持。',convex_collision:'MuJoCoの接触判定はメッシュの凸包を使用。',mdl_approximation:'MDL材質はベース色で近似表示。',textures_not_rendered:'テクスチャは保持していますが、描画は未対応。',subdivision_control_mesh:'サブディビジョンの制御メッシュを表示。',unsupported_primitives:'メッシュ以外の一部形状は表示対象外。'},
 'zh-CN':{asset_source:'来源',local_usd:'本地USD',hf_repo:'数据集',asset_revision:'版本／提交',asset_catalog:'加载资源包目录',asset_package:'资源包名称／筛选',import_asset:'导入OpenUSD资源',usd_help:'使用官方OpenUSD SDK组合USD／USDA／USDC。支持公共Hugging Face数据集的SimReady packaging v1及单个根USD。本地依赖需位于USD文件夹内。导入第0帧静态网格，保留单位与变换。纹理、MDL、关节和完整USD物理仍待实现。网格保留在Rust／GPU，支持保存与撤销。',cancel_asset:'取消导入',usd_cancelled:'已取消导入。',usd_importing:'正在导入…',usd_catalog_loading:'正在加载目录…',usd_catalog_count:'个资源包',usd_imported:'已导入',usd_unavailable:'缺少OpenUSD SDK。请运行scripts/setup-usd.sh或设置ATOM_USD_PYTHON。',usd_network:'无法访问Hugging Face，请检查网络及公共仓库权限。',usd_invalid:'USD、仓库或资源包无效。',usd_budget:'资源超过文件、网格或时间预算。',usd_unsupported:'不支持的USD功能或资源包格式。',usd_integrity:'资源校验失败。',usd_dependency:'USD依赖缺失或位于资源包范围之外。',usd_failed:'USD处理失败或超时。',articulation_static:'关节层级已冻结为可见姿态，尚未导入关节控制。',display_color_approximation:'多值displayColor暂时使用首个颜色近似显示。',physics_defaults:'接触与密度使用ATOM默认值，保留可获取的源质量。',static_mesh_import:'以单个静态网格导入，源层级与物理保留在缓存USD中。',convex_collision:'MuJoCo碰撞使用网格的凸包。',mdl_approximation:'MDL材质使用基础色近似显示。',textures_not_rendered:'已缓存纹理，尚不支持纹理渲染。',subdivision_control_mesh:'显示细分控制网格。',unsupported_primitives:'部分非网格几何体尚不显示。'}
};
for(const language of Object.keys(translations))Object.assign(translations[language],usdTranslations[language]);

const paths={step:'m4 3 9 7-9 7z M16 3v14',pause:'M6 3v14 M14 3v14',undo:'M7 5 3 9l4 4 M3 9h8a5 5 0 0 1 5 5',redo:'m13 5 4 4-4 4 M17 9H9a5 5 0 0 0-5 5',select:'M4 2l12 9-6 1-3 6z',move:'M10 2v16 M2 10h16 M7 5l3-3 3 3 M15 7l3 3-3 3 M7 15l3 3 3-3 M5 7l-3 3 3 3',rotate:'M16 6a7 7 0 1 0 1 7 M16 2v5h-5',scale:'M3 3h5v5H3z M11 11h6v6h-6z M8 8l4 4 M12 8v4H8',grid:'M3 3h14v14H3z M3 8h14 M3 12h14 M8 3v14 M12 3v14',axes:'M5 15h12 M5 15V3 M5 15l8-8 M14 12l3 3-3 3 M2 6l3-3 3 3 M9 7h4v4',pan:'M10 2v16 M2 10h16 M7 5l3-3 3 3 M7 15l3 3 3-3 M5 7l-3 3 3 3 M15 7l3 3-3 3',focus:'M3 7V3h4 M13 3h4v4 M17 13v4h-4 M7 17H3v-4 M6 10h8 M10 6v8',group:'M6 2h12v12H6z M2 6v12h12 M6 6h12 M10 2v4',return_group:'M2 3h16v14H2z M2 7h16 M7 3v4 M12 9l-3 3 3 3 M9 12h6',save:'M3 3h11l3 3v11H3z M6 3v5h8V3 M6 17v-6h8v6',reload:'M16 6a7 7 0 1 0 1 7 M16 2v5h-5',check:'m4 10 4 4 8-9',plus:'M10 3v14 M3 10h14',remove:'M4 5h12 M8 5V3h4v2 M6 5l1 12h6l1-12 M9 8v6 M11 8v6',play:'m6 3 10 7-10 7z',cube:'m10 2 7 4v8l-7 4-7-4V6z M3 6l7 4 7-4 M10 10v8',float:'M7 3h10v10H7z M3 7v10h10',return:'M3 4h14v13H3z M11 7l-3 3 3 3 M8 10h7',layout:'M3 3h14v14H3z M7 3v14 M13 3v14',scene:'m10 2 7 4v8l-7 4-7-4V6z M3 6l7 4 7-4 M10 10v8',workflow:'M3 3h5v5H3z M12 12h5v5h-5z M8 5h6v7',viewport:'M2 3h16v12H2z M7 18h6 M10 15v3'};
function icon(name){const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 20 20');svg.setAttribute('aria-hidden','true');const p=document.createElementNS(svg.namespaceURI,'path');p.setAttribute('d',paths[name]);svg.append(p);return svg;}
document.querySelectorAll('[data-icon]').forEach(el=>el.append(icon(el.dataset.icon)));
const $=id=>document.getElementById(id),view=new URLSearchParams(location.search).get('view')||'toolbar',isSplit=view.startsWith('split-');
document.body.dataset.view=view;if(isSplit)document.body.dataset.split='true';
let camera=null;
let numeric=null,numericOwners=0,numericQueue=Promise.resolve();
let jointDrag=null;
let locale='ja',current=null,layout=null,busy=false,statusKey='connecting',gpuKey='opening',reportKey='not_run',drag=null,splitDrag=null;
const t=key=>translations[locale][key]||translations[locale].error,api=window.__TAURI__;
function nodes(root,kind){if(!root)return [];if(root.kind===kind)return [root,...(root.kind==='split'?[...nodes(root.first,kind),...nodes(root.second,kind)]:[])];return root.kind==='split'?[...nodes(root.first,kind),...nodes(root.second,kind)]:[];}
function groupFor(s,p){return nodes(s?.root,'tabs').find(g=>g.panels.includes(p))||s?.floating_groups.find(g=>g.panels.includes(p));}
function controls(){
 document.querySelectorAll('button').forEach(b=>b.disabled=busy||!current||!api||!!jointDrag);
 for(const id of ['cameraFocus','sceneFocus'])$(id).disabled=busy||!api||!camera?.selected;
 for(const id of ['undo','viewUndo'])$(id).disabled=busy||!api||!current?.can_undo;for(const id of ['redo','viewRedo'])$(id).disabled=busy||!api||!current?.can_redo;
 document.querySelectorAll('[data-tool]').forEach(b=>b.disabled=busy||!api||!!current?.previewing);$('transformAxis').disabled=busy||!api||!!current?.previewing;
 $('transformFields').disabled=busy||!api||!camera?.selected||(!!current?.previewing&&numericOwners===0);
 $('reload').disabled=busy||!current||current.dirty||current.previewing||!api;$('save').disabled=busy||!current||current.previewing||!api;$('locale').disabled=busy||!layout||!api;
 const locked=busy||!current||!api||!!current?.previewing;
 $('ikFields').disabled=locked;
 $('jointFields').disabled=locked&&!(jointDrag&&!jointDrag.ending);$('jointSelect').disabled=locked||!!jointDrag;
 $('jointFields').querySelectorAll('input,button').forEach(el=>el.disabled=!!jointDrag&&(el!==jointDrag.input||!!jointDrag.ending));
 $('assetFields').disabled=locked;$('cancelAssetImport').disabled=!current?.asset_importing;$('cancelAssetImport').hidden=!current?.asset_importing;
 $('environmentFields').disabled=locked||!physicsCatalog;$('materialFields').disabled=locked||!selectedMaterial()||!physicsCatalog;
 $('objectFixed').disabled=locked||materialId()===null||!materialId()||!!current?.robots.some(r=>r.links.some(l=>l.object_id===materialId()));
 $('robotTemplate').disabled=locked;$('addRobot').disabled=locked;$('add').disabled=locked;
 document.querySelectorAll('[data-remove-id]').forEach(b=>{const id=Number(b.dataset.removeId);b.disabled=locked||!!current?.robots.some(r=>r.root!==id&&r.links.some(l=>l.object_id===id));});
 reachControls();
 $('simulate').disabled=busy||!current||!api||!!current.simulation_running||(current.previewing&&!current.simulation);$('simulationSteps').disabled=busy||!!current?.simulation_running;
 $('resetSimulation').disabled=busy||!current?.simulation||!!current?.simulation_running||!api;
 $('materialTarget').disabled=busy||!!current?.previewing;
 $('dockApply').disabled=busy||!layout||!$('dockTarget').options.length;$('floatPanel').disabled=busy||!layout||(layout.floating.includes(view)&&groupFor(layout,view)?.panels.length===1);
 $('floatGroup').hidden=!!layout?.floating.includes(view);$('floatGroup').disabled=busy||!layout||(groupFor(layout,view)?.panels.length||0)<2;
 $('returnGroup').hidden=!layout?.floating.includes(view)||(groupFor(layout,view)?.panels.length||0)<2;
 $('returnPanel').hidden=!layout?.floating.includes(view);$('dockTarget').disabled=busy||!layout;$('dockZone').disabled=busy||!layout;
}
function localize(){
 document.documentElement.lang=locale;
 document.querySelectorAll('[data-text]').forEach(el=>el.textContent=t(el.dataset.text));document.querySelectorAll('[data-label]').forEach(el=>{el.title=t(el.dataset.label);el.setAttribute('aria-label',t(el.dataset.label));});
 $('status').textContent=t(statusKey);$('gpu').textContent=t(gpuKey);$('report').textContent=t(reportKey);$('locale').value=locale;
 document.querySelectorAll('.transform-number').forEach(el=>el.setAttribute('aria-label',`${t(el.dataset.labelKey)} ${'XYZ'[Number(el.dataset.axis)]}`));
 if(current)render(current);else controls();
}
function render(s){
 if(current&&s.sequence<current.sequence)return;if(!current||current.revision!==s.revision)reportKey='not_run';current=s;
 if(jointDrag?.token!=null&&!jointDrag.ending&&s.joint_preview?.token!==jointDrag.token){jointDrag.ending='aborted';wakeJoint(jointDrag);}
 if(document.activeElement!==$('name'))$('name').value=s.name;
 $('dirty').textContent=t(s.dirty?'unsaved':'saved');$('revision').textContent=`rev ${s.revision}`;$('report').textContent=t(reportKey);$('objectCount').textContent=`${s.objects.length} ${t('objects')}`;
 if(view==='scene')$('objects').replaceChildren(...s.objects.map(o=>{const row=document.createElement('li');row.dataset.selected=String(camera?.selected===o.id);row.append(icon('cube'));const name=document.createElement('button');name.className='object-name';name.textContent=objectName(o);name.setAttribute('aria-label',`${t('select_object')} ${o.id}`);name.setAttribute('aria-pressed',String(camera?.selected===o.id));name.onclick=()=>cameraCommand({kind:'select',id:o.id});row.append(name);const pos=document.createElement('span');pos.className='position';pos.textContent=o.position.map(n=>n.toFixed(1)).join(' / ');row.append(pos);const remove=document.createElement('button');remove.title=t('remove');remove.setAttribute('aria-label',`${t('remove')} ${o.id}`);remove.append(icon('remove'));remove.dataset.removeId=String(o.id);remove.onclick=()=>mutate({kind:'remove_box',id:o.id});row.append(remove);return row;}));renderInspector();controls();
}
function applyLayout(s){
 if(layout&&s.revision<layout.revision)return;
 const changed=!layout||s.revision!==layout.revision;layout=s;locale=s.locale;
 if(changed){cancelDrag();cancelSplit();}
 localize();
 if(isSplit){const split=nodes(s.root,'split')[Number(view.slice(-1))];if(split){$('splitter').dataset.axis=split.axis;$('splitter').setAttribute('aria-orientation',split.axis==='horizontal'?'vertical':'horizontal');$('splitter').setAttribute('aria-valuenow',String(Math.round(split.ratio*100)));$('splitter').title=t('resize');$('splitter').setAttribute('aria-label',t('resize'));}return;}
 if(!changed||view==='toolbar')return;
 const group=groupFor(s,view);
 $('tabs').replaceChildren(...(group?.panels||[view]).map(panel=>{
  const tab=document.createElement('button');tab.type='button';tab.className='tab';tab.dataset.panel=panel;tab.setAttribute('role','tab');tab.setAttribute('aria-selected',String(panel===(group?.active||view)));tab.setAttribute('aria-label',t(panel));tab.title=`${t(panel)} — ${t('drag')}`;tab.append(icon(panel));const label=document.createElement('span');label.textContent=t(panel);tab.append(label);
  tab.onpointerdown=e=>beginDrag(e,panel);tab.onpointermove=previewDrag;tab.onpointerup=endDrag;tab.onpointercancel=cancelDrag;tab.onclick=e=>{if(e.detail===0)command({kind:'select',panel});};
  tab.onkeydown=e=>{if(!['ArrowLeft','ArrowRight','Home','End'].includes(e.key)||!group)return;e.preventDefault();let i=group.panels.indexOf(panel);i=e.key==='Home'?0:e.key==='End'?group.panels.length-1:(i+(e.key==='ArrowLeft'?-1:1)+group.panels.length)%group.panels.length;command({kind:'select',panel:group.panels[i]});};return tab;
 }));
 const selected=$('dockTarget').value;const targets=nodes(s.root,'tabs').flatMap(n=>n.panels).filter(p=>p!==view);
 if(!s.root)targets.push('');$('dockTarget').replaceChildren(...targets.map(p=>{const o=document.createElement('option');o.value=p;o.textContent=t(p||'workspace');return o;}));if(targets.includes(selected))$('dockTarget').value=selected;
 controls();
}
function status(key,isError=false){statusKey=key;$('status').textContent=t(key);$('status').parentElement.dataset.error=String(isError);}
async function action(fn){if(busy||!api||jointDrag)return;busy=true;controls();try{await numericQueue;await fn();status('ready');}catch(e){status(e?.code||'error',true);try{if(e?.code==='revision_conflict')render(await api.core.invoke('snapshot'));if(['layout_conflict','panel_error','layout_save_failed'].includes(e?.code))applyLayout(await api.core.invoke('workspace_state'));}catch{/* Keep original failure visible. */}}finally{busy=false;controls();}}
function mutate(command){return action(async()=>render(await api.core.invoke('edit',{request:{expected_revision:current.revision,project_id:current.project_id,command}})));}
function command(command){return action(async()=>applyLayout(await api.core.invoke('workspace_command',{command,expectedRevision:layout.revision})));}
function applyCamera(s){if(camera&&s.revision<camera.revision)return;const selectionChanged=camera?.selected!==s.selected;camera=s;document.querySelectorAll('[data-tool]').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.tool===s.tool)));$('transformAxis').value=s.axis;$('cameraGrid').setAttribute('aria-pressed',String(s.show_grid));$('cameraAxes').setAttribute('aria-pressed',String(s.show_axes));$('cameraPan').setAttribute('aria-pressed',String(s.pan_mode));$('cameraSelection').textContent=s.selected?`#${s.selected}`:'';$('cameraSelection').title=s.selected?`${t('selected_object')} · ${t('box')} ${s.selected}`:'';if(current&&selectionChanged)render(current);else {renderInspector();controls();}}
function cameraCommand(command){return action(async()=>applyCamera(await api.core.invoke('camera_command',{command,expectedRevision:current.revision})));}
function setTool(tool,axis=camera?.axis||'free'){return cameraCommand({kind:'tool',tool,axis});}
document.querySelectorAll('[data-tool]').forEach(b=>b.onclick=()=>setTool(b.dataset.tool));$('transformAxis').onchange=()=>setTool(camera?.tool||'camera',$('transformAxis').value);
for(const id of ['undo','viewUndo'])$(id).onclick=()=>mutate({kind:'undo'});for(const id of ['redo','viewRedo'])$(id).onclick=()=>mutate({kind:'redo'});
document.addEventListener('keydown',e=>{
 if(e.target.closest('input,select,textarea')||isSplit||!current)return;
 if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='z'){e.preventDefault();if(e.shiftKey?current.can_redo:current.can_undo)mutate({kind:e.shiftKey?'redo':'undo'});return;}
 if(!['scene','viewport'].includes(view)||e.metaKey||e.ctrlKey||e.altKey)return;
 const tools={v:'camera',w:'move',e:'rotate',r:'scale'},key=e.key.toLowerCase();
 if(tools[key]){e.preventDefault();setTool(tools[key]);}else if(['x','y','z'].includes(key)){e.preventDefault();setTool(camera?.tool||'camera',camera?.axis===key?'free':key);}
});
$('cameraGrid').onclick=()=>cameraCommand({kind:'guides',grid:!camera?.show_grid,axes:camera?.show_axes??true});$('cameraAxes').onclick=()=>cameraCommand({kind:'guides',grid:camera?.show_grid??true,axes:!camera?.show_axes});
$('cameraPan').onclick=()=>cameraCommand({kind:'pan_mode',enabled:!camera?.pan_mode});
$('cameraFocus').onclick=$('sceneFocus').onclick=()=>cameraCommand({kind:'focus'});$('cameraReset').onclick=()=>cameraCommand({kind:'reset'});
document.addEventListener('keydown',e=>{if(!['scene','viewport'].includes(view)||e.target.closest('input,select,textarea')||e.ctrlKey||e.metaKey||e.altKey)return;if(e.key.toLowerCase()==='f'){e.preventDefault();cameraCommand({kind:'focus'});}else if(e.key==='0'){e.preventDefault();cameraCommand({kind:'reset'});}});
$('locale').onchange=()=>command({kind:'locale',locale:$('locale').value});$('resetLayout').onclick=()=>command({kind:'reset'});
$('renameForm').onsubmit=e=>{e.preventDefault();mutate({kind:'rename',name:$('name').value});};$('add').onclick=()=>mutate({kind:'add_box'});
$('save').onclick=()=>action(async()=>render(await api.core.invoke('save')));$('reload').onclick=()=>action(async()=>render(await api.core.invoke('reload')));
$('floatGroup').onclick=()=>command({kind:'float_group',panel:view});$('returnGroup').onclick=()=>command({kind:'return_group',panel:view});
$('floatPanel').onclick=()=>command({kind:'dock',panel:view,target:null,zone:'floating'});$('returnPanel').onclick=()=>command({kind:'return',panel:view});$('dockApply').onclick=()=>command({kind:'dock',panel:view,target:$('dockTarget').value||null,zone:$('dockZone').value});
$('validate').onclick=()=>action(async()=>{const revision=current.revision;const result=await api.core.invoke('validate_workflow',{expectedRevision:revision});if(current.revision===revision&&result.task_count===8&&result.simulated_episodes===0){reportKey='validated';$('report').textContent=t(reportKey);}});
// Coordinates remain in CSS pixels until the host maps them across native children/windows.
let dragQueue=Promise.resolve(),dragPending=false;
function beginDrag(e,panel){if(e.button!==0||busy||!layout||!api)return;e.preventDefault();drag={element:e.currentTarget,id:e.pointerId,panel,x:e.clientX,y:e.clientY,active:false,revision:layout.revision};drag.element.setPointerCapture(e.pointerId);}
async function previewDrag(e){if(!drag)return;if(Math.hypot(e.clientX-drag.x,e.clientY-drag.y)>6)drag.active=true;if(!drag.active||dragPending)return;dragPending=true;drag.element.classList.add('dragging');try{dragQueue=api.core.invoke('panel_drag',{panel:drag.panel,phase:'preview',pointer:[e.clientX,e.clientY],expectedRevision:drag.revision});await dragQueue;}catch(e){status(e?.code||'error',true);}finally{dragPending=false;}}
function clearDrag(){const d=drag;drag=null;if(d?.element.hasPointerCapture(d.id))d.element.releasePointerCapture(d.id);d?.element.classList.remove('dragging');return d;}
function endDrag(e){if(!drag)return;const d=clearDrag(),pointer=[e.clientX,e.clientY];if(d.active)action(async()=>{await dragQueue.catch(()=>{});await api.core.invoke('panel_drag',{panel:d.panel,phase:'drop',pointer,expectedRevision:d.revision});});else command({kind:'select',panel:d.panel});}
function cancelDrag(){const d=clearDrag();if(!d||!api)return;dragQueue.catch(()=>{}).then(()=>api.core.invoke('panel_drag',{panel:d.panel,phase:'cancel',pointer:null,expectedRevision:d.revision})).catch(()=>{});}
let splitQueue=Promise.resolve(),splitPending=false;
const separator=$('splitter');
const splitPointer=e=>[e.clientX,e.clientY];
separator.onpointerdown=e=>{if(e.button!==0||busy||!layout||!api)return;const split=nodes(layout.root,'split')[Number(view.slice(-1))];if(!split)return;e.preventDefault();separator.focus();splitDrag={pointerId:e.pointerId,id:split.id,revision:layout.revision};separator.setPointerCapture(e.pointerId);};
separator.onpointermove=async e=>{if(!splitDrag||splitPending)return;splitPending=true;try{splitQueue=api.core.invoke('split_resize',{id:splitDrag.id,phase:'preview',pointer:splitPointer(e),ratio:null,expectedRevision:splitDrag.revision});await splitQueue;}catch(e){status(e?.code||'error',true);}finally{splitPending=false;}};
function clearSplit(){const d=splitDrag;splitDrag=null;if(d&&separator.hasPointerCapture(d.pointerId))separator.releasePointerCapture(d.pointerId);return d;}
separator.onpointerup=e=>{const d=clearSplit();if(!d)return;const pointer=splitPointer(e);action(async()=>{await splitQueue.catch(()=>{});try{applyLayout(await api.core.invoke('split_resize',{id:d.id,phase:'commit',pointer,ratio:null,expectedRevision:d.revision}));}catch(error){await api.core.invoke('split_resize',{id:d.id,phase:'cancel',pointer:null,ratio:null,expectedRevision:d.revision}).catch(()=>{});throw error;}});};
function cancelSplit(){const d=clearSplit();if(!d||!api)return;splitQueue.catch(()=>{}).then(()=>api.core.invoke('split_resize',{id:d.id,phase:'cancel',pointer:null,ratio:null,expectedRevision:d.revision})).catch(()=>{});}
separator.onpointercancel=cancelSplit;
separator.onkeydown=e=>{const s=nodes(layout?.root,'split')[Number(view.slice(-1))];if(!s)return;const decrease=s.axis==='horizontal'?'ArrowLeft':'ArrowUp',increase=s.axis==='horizontal'?'ArrowRight':'ArrowDown';if(e.key!==decrease&&e.key!==increase)return;e.preventDefault();action(async()=>applyLayout(await api.core.invoke('split_resize',{id:s.id,phase:'commit',pointer:null,ratio:Math.min(.9,Math.max(.1,s.ratio+(e.key===decrease?-.02:.02))),expectedRevision:layout.revision})));};
document.addEventListener('keydown',e=>{if(e.key==='Escape'){cancelDrag();cancelSplit();}});window.addEventListener('blur',()=>{cancelDrag();cancelSplit();});
const transformInputs=[];
for(const [part,label,shape] of [['position','position','move'],['rotation','rotation','rotate'],['scale','scale_value','scale']]){
 const row=document.createElement('div');row.className='transform-row';const mark=document.createElement('span');mark.className='transform-mark';mark.dataset.label=label;mark.append(icon(shape));row.append(mark);
 for(let axis=0;axis<3;axis++){
  const cell=document.createElement('label');cell.className='transform-cell';const letter=document.createElement('span');letter.textContent='XYZ'[axis];letter.dataset.axis=String(axis);cell.append(letter);
  const input=document.createElement('input');input.type='number';input.className='transform-number';input.dataset.part=part;input.dataset.axis=String(axis);input.dataset.labelKey=label;input.step='any';input.inputMode='decimal';input.required=true;
  input.min=part==='scale'?'0.001':part==='rotation'?'-360000':'-10000';input.max=part==='scale'?'100':part==='rotation'?'360000':'10000';
  input.oninput=()=>previewNumeric(input);input.onchange=()=>finishNumeric(input,!input.checkValidity());input.onblur=()=>finishNumeric(input,!input.checkValidity());
  input.onkeydown=e=>{if(e.key==='Enter'||e.key==='Escape'){e.preventDefault();e.stopPropagation();finishNumeric(input,e.key==='Escape'||!input.checkValidity());input.blur();renderInspector();}};
  transformInputs.push(input);cell.append(input);row.append(cell);
 }
 $('transformFields').append(row);
}
function renderInspector(){
 renderPhysics();
 const object=current?.objects.find(o=>o.id===camera?.selected);$('inspectorSelection').textContent=object?objectName(object):'—';
 const robot=current?.robots?.find(r=>r.links.some(l=>l.object_id===object?.id));
 const link=robot?.links.find(l=>l.object_id===object?.id);
 $('jointSummary').textContent=robot?`${t('joints')} ${robot.links.length-1} · ${t('dof')} ${robot.links.filter(l=>l.parent!==null).reduce((n,l)=>n+1+(l.extra_axes?.length||0),0)}`:'';
 renderJointEditor(robot,link);
 $('jointAxes').textContent=link?.parent!=null?[{axis:link.axis,range:link.range},...(link.extra_axes||[])].map(a=>`${'XYZ'[a.axis.findIndex(v=>Math.abs(v)>0.5)]} ${a.range.map(v=>Math.round(v*180/Math.PI)).join('…')}°`).join(' · '):'';

 for(const input of transformInputs){if(document.activeElement===input)continue;const value=object?.[input.dataset.part][Number(input.dataset.axis)];input.value=value===undefined?'':String(Number((input.dataset.part==='rotation'?value*180/Math.PI:value).toFixed(input.dataset.part==='rotation'?2:4)));}
}
async function numericRequest(session,phase,transform=null){
 const result=await api.core.invoke('transform_command',{request:{phase,expected_revision:session.revision,project_id:session.project,id:session.id,transform}});render(result);return result;
}
async function recoverNumeric(session,error){
 session.failed=true;try{if(session.started)await numericRequest(session,'cancel');}catch{}status(error?.code||'invalid_project',true);
}
function previewNumeric(input){
 if(!api||!current||!camera?.selected||!input.checkValidity()||!Number.isFinite(input.valueAsNumber))return;
 if(numeric&&numeric.input!==input)finishNumeric(numeric.input,false);
 if(!numeric){const object=current.objects.find(o=>o.id===camera.selected);if(!object)return;numeric={input,id:object.id,project:current.project_id,base:{position:[...object.position],rotation:[...object.rotation],scale:[...object.scale]},started:false,failed:false,canceled:false};numericOwners++;}
 const session=numeric,pose=structuredClone(session.base);pose[input.dataset.part][Number(input.dataset.axis)]=input.dataset.part==='rotation'?input.valueAsNumber*Math.PI/180:input.valueAsNumber;
 numericQueue=numericQueue.then(async()=>{if(session.failed||session.canceled)return;if(!session.started){session.revision=current.revision;await numericRequest(session,'begin');session.started=true;}await numericRequest(session,'preview',pose);}).catch(e=>recoverNumeric(session,e));
}
function finishNumeric(input,cancel){
 const session=numeric;if(!session||session.input!==input){if(cancel)renderInspector();return;}
 numeric=null;session.canceled=cancel;
 numericQueue=numericQueue.then(async()=>{if(session.started&&!session.failed)await numericRequest(session,cancel?'cancel':'commit');if(cancel&&!input.checkValidity())status('invalid_project',true);}).catch(e=>recoverNumeric(session,e)).finally(()=>{numericOwners--;renderInspector();controls();});
}
window.addEventListener('blur',()=>{if(numeric)finishNumeric(numeric.input,true);});
let physicsCatalog=null;

const humanLabels={en:{lumbar:'Lumbar',thoracic:'Thoracic',cervical:'Cervical',jaw:'Jaw',clavicle:'Clavicle',palm:'Palm',ankle:'Ankle',finger:'Finger',toe:'Toe',joints:'Connections',dof:'Rotational DoF'},ja:{lumbar:'腰椎',thoracic:'胸椎',cervical:'頸椎',jaw:'顎',clavicle:'鎖骨',palm:'手掌',ankle:'足関節',finger:'手指',toe:'足趾',joints:'可動接続',dof:'回転自由度'},'zh-CN':{lumbar:'腰椎',thoracic:'胸椎',cervical:'颈椎',jaw:'下颌',clavicle:'锁骨',palm:'手掌',ankle:'踝关节',finger:'手指',toe:'脚趾',joints:'可动连接',dof:'旋转自由度'}};
for(const [lang,labels] of Object.entries(humanLabels))Object.assign(translations[lang],labels);
function bodyLabel(label){
 const numbered=/^(lumbar|thoracic|cervical|finger|toe)_(\d+)(?:_(\d+))?$/.exec(label);
 return numbered?`${t(numbered[1])} ${numbered[2]}${numbered[3]?` · ${numbered[3]}`:''}`:t(label);
}
function objectName(o){
 if(o.mesh_ref)return `${o.label} #${o.id}`;
 if(!o.label)return `${t('box')} ${o.id}`;
 const side=/^(left|right)_(.*)$/.exec(o.label);
 return `${side?`${t(side[1])} · ${bodyLabel(side[2])}`:bodyLabel(o.label)} #${o.id}`;
}
function wakeJoint(session){if(session.wake){const wake=session.wake;session.wake=null;wake();}}
function startJoint(input,id,axis,spec,restore,source){
 if(jointDrag)return jointDrag.input===input&&!jointDrag.ending;
 if(busy||!api||!current||current.previewing||numericOwners)return false;
 const session={input,id,axis,spec,restore,source,revision:current.revision,project:current.project_id,token:null,latest:null,sent:null,ending:null,wake:null};
 jointDrag=session;controls();session.task=runJoint(session);return true;
}
function finishJoint(cancel){const session=jointDrag;if(!session||session.ending)return;session.ending=cancel?'cancel':'commit';wakeJoint(session);controls();}
async function jointRequest(session,phase,angle=null){
 const result=await api.core.invoke('joint_command',{request:{phase,expected_revision:session.revision,project_id:session.project,id:session.id,axis:session.axis,token:session.token,angle}});
 if(phase==='begin')session.token=result.joint_preview.token;
 render(result);return result;
}
async function runJoint(session){
 try{
  await jointRequest(session,'begin');
  if(current.joint_preview?.token!==session.token)session.ending='aborted';
  for(;;){
   if(session.ending==='aborted')break;
   if(session.ending==='cancel'){await jointRequest(session,'cancel');break;}
   if(session.latest!==null&&session.latest!==session.sent){const angle=session.latest;await jointRequest(session,'preview',angle);session.sent=angle;continue;}
   if(session.ending==='commit'){await jointRequest(session,'commit');break;}
   await new Promise(resolve=>session.wake=resolve);
  }
 }catch(error){
  try{if(session.token!==null)await jointRequest(session,'cancel');}catch{}
  try{render(await api.core.invoke('snapshot'));}catch{}
  status(error?.code||'invalid_project',true);
 }finally{
  if(jointDrag===session){jointDrag=null;session.restore();renderInspector();controls();}
 }
}
window.addEventListener('blur',()=>finishJoint(true));
window.addEventListener('pagehide',()=>finishJoint(true));
document.addEventListener('visibilitychange',()=>{if(document.hidden)finishJoint(true);});
const jointTranslations={en:{joint_select:'Joint',joint_angle:'Joint angle',joint_reset:'Reset axis',joint_help:'Edit angles in degrees, within the joint limits. Sliders update 3D live; one drag is one undoable edit. Release to commit; Escape, lost pointer capture or window blur cancels. Press Enter or leave a numeric field to apply it. Drag colored arcs or short rotation axes directly in 3D. Limits are enforced; release commits one edit and Esc cancels. Shift + left drag pans; Alt + left drag orbits. Drag the cyan lower-limit grip or magenta upper-limit grip to edit bounds, or enter Lower/Upper values (−180°…180°, lower < upper). Narrowing clamps the current angle; one Undo restores both. Colored arcs show angular limits; gold rays show current angles. Child links follow the joint. Physics starts from this pose. Geometry XYZ fields edit shapes, not joint pivots.'},ja:{joint_select:'関節',joint_angle:'関節角度',joint_reset:'軸をゼロへ',joint_help:'角度は度単位。可動範囲内で編集できます。スライダー操作中も3Dに反映し、1回のドラッグを1件の履歴にします。離すと確定、Esc・ポインター操作の中断・ウインドウのフォーカス喪失で取消。数値欄はEnterまたは欄を離れると確定。3Dの色付き円弧や短い回転軸も直接ドラッグできます。可動範囲内で動き、離すと1件として確定、Escで取消。Shift＋左ドラッグはパン、Alt＋左ドラッグは視点回転。シアンの下限端点／マゼンタの上限端点をドラッグするか、下限／上限を数値入力して範囲を編集できます（−180°〜180°、下限＜上限）。現在角度が範囲外になれば端へ補正し、同じUndoで両方戻ります。円弧は可動範囲、金色の線は現在角度です。子孫リンクが追従し、物理計算もこの姿勢から開始します。XYZ欄は形状の編集で、関節支点の編集ではありません。'},'zh-CN':{joint_select:'关节',joint_angle:'关节角度',joint_reset:'轴归零',joint_help:'以度为单位，在关节限位内编辑。拖动滑块时实时更新3D，一次拖动记录为一次操作。松开提交，Esc、指针中断或窗口失焦取消。数值框按Enter或离开时提交。可直接拖动3D彩色圆弧或短旋转轴，角度受限，松开提交一次操作，Esc取消。Shift＋左键平移，Alt＋左键旋转视角。拖动青色下限端点／品红色上限端点，或输入下限／上限以编辑范围（−180°至180°，下限＜上限）。缩小范围时当前角度会被限制在边界，一次撤销可恢复两者。彩色圆弧表示限位，金色射线表示当前角度。子连杆随关节运动，物理计算从此姿态开始。XYZ编辑几何形状，不改变关节支点。'}};
for(const [language,labels] of Object.entries(jointTranslations))Object.assign(translations[language],labels);
for(const [language,labels] of Object.entries({en:{joint_lower:'Lower',joint_upper:'Upper'},ja:{joint_lower:'下限',joint_upper:'上限'},'zh-CN':{joint_lower:'下限',joint_upper:'上限'}}))Object.assign(translations[language],labels);
let jointEditorKey='';
function renderJointEditor(robot,link){
 const editor=$('jointEditor'),fields=$('jointFields'),select=$('jointSelect');editor.hidden=!robot;renderIk(robot,link);
 if(!robot){fields.replaceChildren();jointEditorKey='';return;}
 const key=`${current.project_id}:${robot.root}:${link?.object_id}:${locale}:${JSON.stringify([link?.axis,link?.range,link?.extra_axes,robot.links.map(l=>[l.object_id,l.parent])])}`;
 if(jointEditorKey!==key){
  jointEditorKey=key;
  const placeholder=new Option(t('joint_select'),'');placeholder.disabled=true;
  select.replaceChildren(placeholder,...robot.links.filter(l=>l.parent!==null).map(l=>new Option(objectName(current.objects.find(o=>o.id===l.object_id)),String(l.object_id))));
  select.value=link?.parent!=null?String(link.object_id):'';
  select.onchange=()=>cameraCommand({kind:'select',id:Number(select.value)});
  fields.replaceChildren();
  if(link?.parent!=null){
   const specs=[{axis:link.axis,range:link.range},...(link.extra_axes||[])];
   for(const [i,spec] of specs.entries()){
    const row=document.createElement('div');row.className='joint-row';row.dataset.jointAxis=String(i);
    const axisName='XYZ'[spec.axis.findIndex(v=>Math.abs(v)>0.5)];
    const label=document.createElement('span');label.textContent=axisName;label.dataset.axis=String('XYZ'.indexOf(axisName));label.title=spec.range.map(v=>(v*180/Math.PI).toFixed(1)).join(' … ')+'°';
    const slider=document.createElement('input');slider.type='range';slider.step='any';
    const number=document.createElement('input');number.type='number';number.step='any';number.required=true;
    for(const input of [slider,number]){input.min=String(spec.range[0]*180/Math.PI);input.max=String(spec.range[1]*180/Math.PI);input.setAttribute('aria-label',`${t('joint_angle')} ${axisName} (°)`);}
    const reset=document.createElement('button');reset.type='button';reset.append(icon('reload'));reset.title=t('joint_reset');reset.setAttribute('aria-label',`${t('joint_reset')} ${axisName}`);
    const restore=()=>{const actual=current?.robots.flatMap(r=>r.links).find(l=>l.object_id===link.object_id);const degrees=(actual?.angles?.[i]||0)*180/Math.PI;slider.value=String(degrees);number.value=String(Math.max(Number(number.min),Math.min(Number(number.max),Number(degrees.toFixed(5)))));};
    const commit=async input=>{
     if(!input.reportValidity()){restore();return;}
     const angle=Math.max(spec.range[0],Math.min(spec.range[1],input.valueAsNumber*Math.PI/180));
     await mutate({kind:'set_joint_angle',id:link.object_id,axis:i,angle});restore();
    };
    slider.onpointerdown=e=>{
     if(e.button!==0||!startJoint(slider,link.object_id,i,spec,restore,'pointer'))return;
     slider.setPointerCapture(e.pointerId);
    };
    slider.oninput=()=>{
     number.value=String(Number(Number(slider.value).toFixed(5)));
     if(!jointDrag)startJoint(slider,link.object_id,i,spec,restore,'input');
     if(jointDrag?.input===slider){jointDrag.latest=Math.max(spec.range[0],Math.min(spec.range[1],slider.valueAsNumber*Math.PI/180));wakeJoint(jointDrag);}
    };
    slider.onchange=()=>{if(jointDrag?.input===slider&&jointDrag.source==='input')finishJoint(false);};
    slider.onpointerup=()=>{if(jointDrag?.input===slider)finishJoint(false);};
    slider.onpointercancel=()=>{if(jointDrag?.input===slider)finishJoint(true);};
    slider.onlostpointercapture=()=>{if(jointDrag?.input===slider&&!jointDrag.ending)finishJoint(true);};
    slider.onblur=()=>{if(jointDrag?.input===slider&&!jointDrag.ending)finishJoint(true);};
    const motionKeys=['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','PageUp','PageDown','Home','End'];
    slider.onkeydown=e=>{if(e.key==='Escape'){e.preventDefault();finishJoint(true);}else if(e.key==='Enter'){e.preventDefault();finishJoint(false);}else if(motionKeys.includes(e.key)&&!jointDrag)startJoint(slider,link.object_id,i,spec,restore,'keyboard');};
    slider.onkeyup=e=>{if(motionKeys.includes(e.key)&&jointDrag?.input===slider)finishJoint(false);};
    number.onchange=()=>commit(number);
    number.onkeydown=e=>{if(e.key==='Escape'){e.preventDefault();restore();number.blur();}else if(e.key==='Enter'){e.preventDefault();number.blur();}};
    reset.onclick=async()=>{await mutate({kind:'set_joint_angle',id:link.object_id,axis:i,angle:Math.max(spec.range[0],Math.min(spec.range[1],0))});restore();};
    row.append(label,slider,number,reset);
    const limits=document.createElement('div');limits.className='joint-limit-row';
    const boundInputs=[];
    const restoreBounds=()=>{
     const actual=current?.robots.flatMap(r=>r.links).find(l=>l.object_id===link.object_id);
     const range=i===0?actual?.range:actual?.extra_axes?.[i-1]?.range;
     if(!range)return;
     for(const [bound,input] of boundInputs.entries())input.value=String(Math.max(-180,Math.min(180,Number((range[bound]*180/Math.PI).toFixed(5)))));
    };
    const commitBounds=async()=>{
     if(boundInputs.some(input=>!input.reportValidity())||boundInputs[0].valueAsNumber>=boundInputs[1].valueAsNumber){status('invalid_project',true);restoreBounds();return;}
     await mutate({kind:'set_joint_range',id:link.object_id,axis:i,range:boundInputs.map(input=>input.valueAsNumber*Math.PI/180)});restoreBounds();
    };
    for(const bound of ['lower','upper']){
     const cell=document.createElement('label');cell.className='joint-bound';cell.dataset.bound=bound;
     const mark=document.createElement('span');mark.textContent=t(`joint_${bound}`);
     const input=document.createElement('input');input.type='number';input.min='-180';input.max='180';input.step='any';input.required=true;input.dataset.bound=bound;
     input.setAttribute('aria-label',`${t(`joint_${bound}`)} ${axisName} (°)`);input.onchange=commitBounds;
     input.onkeydown=e=>{if(e.key==='Escape'){e.preventDefault();restoreBounds();input.blur();}else if(e.key==='Enter'){e.preventDefault();input.blur();}};
     boundInputs.push(input);cell.append(mark,input);limits.append(cell);
    }
    restoreBounds();row.append(limits);fields.append(row);
   }
  }
 }
 for(const [i,row] of [...fields.children].entries()){
  const [slider,number]=row.querySelectorAll('input');
  if(document.activeElement===slider||document.activeElement===number)continue;
  const degrees=(link.angles?.[i]||0)*180/Math.PI;slider.value=String(degrees);number.value=String(Math.max(Number(number.min),Math.min(Number(number.max),Number(degrees.toFixed(5)))));
 }
}
for(const [language,labels] of Object.entries({
 en:{ik_position:'Position IK',ik_base:'Fixed chain base',ik_current:'Use current center as target',ik_solve:'Solve position IK',ik_target:'Target · m',ik_unreachable:'No IK solution found within the limits and budget. Pose unchanged.',ik_help:'Target: selected object center in world meters (Y up). The chosen ancestor stays fixed; joints on the path below it may rotate within their limits. Other descendants follow their parent. Position only, tolerance 1 mm. No collision, balance or orientation constraint. A bounded local search may fail even for a reachable target. Apply with the target icon or Enter; all solved angles form one Undo. Esc restores an uncommitted target field.'},
 ja:{ik_position:'位置IK',ik_base:'固定するチェーン基点',ik_current:'現在の中心位置を目標にする',ik_solve:'位置IKを適用',ik_target:'目標 · m',ik_unreachable:'制限・計算上限内でIK解が見つかりません。姿勢は変更していません。',ik_help:'選択部位の中心をワールド座標の目標へ移動（m、Y上）。選んだ祖先を固定し、その下から選択部位までの関節を制限内で回転。ほかの子孫も親に追従します。位置のみ、許容誤差1mm。衝突・バランス・向きの制約なし。局所探索のため、到達可能でも解が見つからない場合があります。目標アイコンまたはEnterで適用、全関節を1件のUndoに記録。Escで未確定の入力を復元。'},
 'zh-CN':{ik_position:'位置IK',ik_base:'固定链基点',ik_current:'以当前中心为目标',ik_solve:'应用位置IK',ik_target:'目标 · m',ik_unreachable:'在限位与计算预算内未找到IK解，姿态保持不变。',ik_help:'以世界坐标米为单位移动选中部位的中心（Y向上）。固定所选祖先，仅在限位内旋转下方路径上的关节；其他后代随父节点运动。仅约束位置，容差1mm，不含碰撞、平衡或朝向约束。局部搜索可能无法找到可达目标的解。目标图标或Enter应用，全部关节合为一次撤销；Esc恢复未提交的输入。'}
}))Object.assign(translations[language],labels);
let ikKey='';
const ikInputs=[];
function currentIkTarget(){return current?.objects.find(o=>o.id===camera?.selected)?.position;}
function resetIkTarget(){const target=currentIkTarget();if(target)ikInputs.forEach((input,i)=>{input.value=String(target[i]);input.dataset.committed=input.value;});}
async function solveIk(){
 if(ikInputs.some(input=>!input.reportValidity())||!$('ikBase').value)return;
 const target=ikInputs.map(input=>input.valueAsNumber);
 ikInputs.forEach(input=>input.dataset.committed=input.value);
 await mutate({kind:'solve_ik',id:camera.selected,base:Number($('ikBase').value),target});
}
for(let axis=0;axis<3;axis++){
 const cell=document.createElement('label');cell.className='transform-cell';const label=document.createElement('span');label.textContent='XYZ'[axis];label.dataset.axis=String(axis);
 const input=document.createElement('input');input.type='number';input.className='transform-number';input.step='any';input.min='-10000';input.max='10000';input.required=true;
 input.onkeydown=e=>{if(e.key==='Escape'){e.preventDefault();e.stopPropagation();input.value=input.dataset.committed;input.blur();}else if(e.key==='Enter'){e.preventDefault();solveIk();}};
 cell.append(label,input);ikInputs.push(input);$('ikTarget').append(cell);
}
$('ikCurrent').onclick=resetIkTarget;$('ikSolve').onclick=solveIk;
function renderIk(robot,link){
 $('ikEditor').hidden=link?.parent==null;
 if(!robot||link?.parent==null){ikKey='';return;}
 const key=`${current.project_id}:${link.object_id}:${locale}`;
 if(key!==ikKey){
  const oldBase=$('ikBase').value;const sameSelection=ikKey.split(':').slice(0,2).join(':')===key.split(':').slice(0,2).join(':');ikKey=key;
  const ancestors=[];let cursor=link.parent;
  while(cursor!=null){ancestors.push(cursor);cursor=robot.links.find(l=>l.object_id===cursor)?.parent;}
  $('ikBase').replaceChildren(...ancestors.reverse().map(id=>new Option(objectName(current.objects.find(o=>o.id===id)),String(id))));
  $('ikBase').value=sameSelection&&ancestors.includes(Number(oldBase))?oldBase:String(robot.root);
  if(!sameSelection)resetIkTarget();
  ikInputs.forEach((input,i)=>input.setAttribute('aria-label',`${t('ik_target')} ${'XYZ'[i]}`));
 }
}
const gravityInputs=[];
for(let axis=0;axis<3;axis++){
 const cell=document.createElement('label');cell.className='transform-cell';const letter=document.createElement('span');letter.textContent='XYZ'[axis];letter.dataset.axis=axis;cell.append(letter);
 const input=document.createElement('input');input.type='number';input.className='transform-number';input.min='-1000';input.max='1000';input.step='any';input.required=true;
 input.onchange=()=>{if(!input.reportValidity())return;const vector=[...current.physics.gravity.vector];vector[axis]=input.valueAsNumber;mutate({kind:'set_gravity',gravity:{preset:'custom',vector}});};
 input.onkeydown=e=>{if(e.key==='Escape'){input.value=current.physics.gravity.vector[axis];input.blur();}else if(e.key==='Enter')input.blur();};
 cell.append(input);gravityInputs.push(input);$('gravityVector').append(cell);
}
const frictionInputs=[];
for(const [axis,label] of ['sliding','torsional','rolling'].entries()){
 const row=document.createElement('label');row.className='setting-row';const title=document.createElement('span');title.dataset.text=label;row.append(title);
 const input=document.createElement('input');input.type='number';input.min='0';input.max='100';input.step='any';input.required=true;input.dataset.label=label;
 input.onchange=()=>editMaterialNumber(input,axis);input.onkeydown=e=>{if(e.key==='Escape'){input.value=selectedMaterial()?.friction[axis]??'';input.blur();}else if(e.key==='Enter')input.blur();};
 row.append(input);frictionInputs.push(input);$('frictionVector').append(row);
}
for(const name of ['wood','steel','aluminum','rubber','plastic','glass','concrete','ice','custom']){const option=document.createElement('option');option.value=name;option.dataset.text=name;$('materialPreset').append(option);}
function materialId(){return $('materialTarget').value==='floor'?null:(camera?.selected??undefined);}
function selectedMaterial(){return materialId()===null?current?.physics.floor:current?.objects.find(o=>o.id===materialId())?.material;}
function editMaterialNumber(input,axis){
 if(!input.reportValidity()||!Number.isFinite(input.valueAsNumber))return;
 const source=selectedMaterial();if(!source)return;const material=structuredClone(source);material.preset='custom';
 if(axis===null)material.density=input.valueAsNumber;else material.friction[axis]=input.valueAsNumber;
 mutate({kind:'set_material',id:materialId(),material});
}
$('density').onchange=()=>editMaterialNumber($('density'),null);
$('density').onkeydown=e=>{if(e.key==='Escape'){$('density').value=selectedMaterial()?.density??'';$('density').blur();}else if(e.key==='Enter')$('density').blur();};
$('gravityPreset').onchange=()=>{const gravity=physicsCatalog?.gravity.find(g=>g.preset===$('gravityPreset').value)||{preset:'custom',vector:[...current.physics.gravity.vector]};mutate({kind:'set_gravity',gravity});};
$('materialPreset').onchange=()=>{const material=physicsCatalog?.materials.find(m=>m.preset===$('materialPreset').value)||{...selectedMaterial(),preset:'custom'};mutate({kind:'set_material',id:materialId(),material});};
$('materialTarget').onchange=()=>{renderPhysics();controls();};
$('floorEnabled').onchange=()=>mutate({kind:'set_floor',enabled:$('floorEnabled').checked});
$('objectFixed').onchange=()=>mutate({kind:'set_fixed',id:materialId(),fixed:$('objectFixed').checked});
$('addRobot').onclick=()=>action(async()=>{
 const s=await api.core.invoke('edit',{request:{expected_revision:current.revision,project_id:current.project_id,command:{kind:'add_robot',template:$('robotTemplate').value}}});render(s);
 applyCamera(await api.core.invoke('camera_command',{command:{kind:'select',id:s.robots.at(-1).root},expectedRevision:s.revision}));
 applyCamera(await api.core.invoke('camera_command',{command:{kind:'focus'},expectedRevision:s.revision}));
});
$('simulate').onclick=()=>action(async()=>render(await api.core.invoke('simulate',{expectedRevision:current.revision,projectId:current.project_id,steps:Number($('simulationSteps').value)})));
$('resetSimulation').onclick=()=>action(async()=>render(await api.core.invoke('reset_simulation')));
function renderPhysics(){
 renderReach();
 renderAssets();
 if(!current?.physics)return;
 $('gravityPreset').value=current.physics.gravity.preset;
 gravityInputs.forEach((input,i)=>{input.setAttribute('aria-label',`${t('gravity')} ${'XYZ'[i]} (m/s², Z-up)`);if(document.activeElement!==input)input.value=current.physics.gravity.vector[i];});
 $('floorEnabled').checked=current.physics.floor_enabled;
 const material=selectedMaterial(),object=current.objects.find(o=>o.id===materialId());
 $('materialPreset').value=material?.preset||'custom';if(document.activeElement!==$('density'))$('density').value=material?.density??'';
 frictionInputs.forEach((input,i)=>{if(document.activeElement!==input)input.value=material?.friction[i]??'';});
 $('objectFixed').checked=object?.fixed??true;
 const mass=object?(object.mass_override??(object.mesh_ref?null:object.scale.reduce((a,b)=>a*b,1)*object.material.density)):null;
 $('bodyMass').textContent=object?(mass===null?'— kg':`${mass.toLocaleString(locale,{maximumFractionDigits:2})} kg`):'';
 $('simulationStatus').textContent=current.reach?`Reach · ${current.reach.time.toFixed(3)} s`:current.simulation_running?t('sim_running'):current.simulation?`${current.simulation.engine} · ${current.simulation.time.toFixed(3)} s`:t('sim_idle');
}

let assetCatalogData=null,assetWasRunning=false;
$('assetSource').onchange=()=>{$('hubFields').hidden=$('assetSource').value==='local';$('localFields').hidden=$('assetSource').value!=='local';};
function filterAssets(){
 const query=$('assetPackage').value.toLowerCase(),list=assetCatalogData?.packages||[];
 $('assetPackages').replaceChildren(...list.filter(p=>p.toLowerCase().includes(query)).slice(0,80).map(p=>{const o=document.createElement('option');o.value=p;return o;}));
 $('assetCatalogStatus').textContent=list.length?`${list.length} ${t('usd_catalog_count')}`:'';
}
$('assetPackage').oninput=filterAssets;
$('cancelAssetImport').onclick=()=>api.core.invoke('cancel_asset_import').catch(e=>status(e?.code||'error',true));
$('assetRepo').oninput=$('assetRevision').oninput=()=>{assetCatalogData=null;filterAssets();};
$('assetCatalog').onclick=()=>action(async()=>{
 $('assetStatus').textContent=t('usd_catalog_loading');
 try{assetCatalogData=await api.core.invoke('asset_catalog',{repo:$('assetRepo').value.trim(),revision:$('assetRevision').value.trim()||'main'});$('assetRevision').value=assetCatalogData.revision;filterAssets();}finally{$('assetStatus').textContent='';}
});
$('importAsset').onclick=()=>action(async()=>{
 const expectedRevision=current.revision,projectId=current.project_id;
 const source=$('assetSource').value==='local'?{kind:'local',path:$('assetPath').value.trim()}:{kind:'hugging_face',repo:$('assetRepo').value.trim(),package:$('assetPackage').value.trim(),revision:$('assetRevision').value.trim()||'main'};
 $('assetStatus').textContent=t('usd_importing');
 try{const s=await api.core.invoke('import_asset',{source,expectedRevision,projectId});render(s);const object=s.objects.at(-1);
 applyCamera(await api.core.invoke('camera_command',{command:{kind:'select',id:object.id},expectedRevision:s.revision}));
 applyCamera(await api.core.invoke('camera_command',{command:{kind:'focus'},expectedRevision:s.revision}));$('assetStatus').textContent=t('usd_imported');
 }catch(e){$('assetStatus').textContent='';throw e;}
});
function renderAssets(){
 if(current?.asset_importing)$('assetStatus').textContent=t('usd_importing');else if(assetWasRunning)$('assetStatus').textContent='';assetWasRunning=!!current?.asset_importing;
 const object=current?.objects.find(o=>o.id===camera?.selected),asset=current?.assets?.find(a=>a.id===object?.mesh_ref);
 $('assetSelection').hidden=!asset;
 if(asset){$('assetInfo').textContent=`${asset.triangles.toLocaleString(locale)} △ · ${asset.source.license}`;$('assetInfo').title=`${asset.name}\n${asset.source.author} · ${asset.source.repo}\n${asset.source.revision}\n${asset.source.entry}`;$('assetWarnings').title=asset.warnings.map(key=>t(key)).join('\n');$('assetWarnings').setAttribute('aria-label',$('assetWarnings').title);}
 if(assetCatalogData)filterAssets();
}

for(const [language,labels] of Object.entries({
 en:{reach_seed:'Seed',reach_horizon:'Steps',reach_robot:'Reach robot',reach_add:'Add dual-arm robot',reach_reset:'Reset Reach with this seed',reach_step:'Advance 0.02 s',reach_run:'Run to success or timeout',reach_save:'Save Episode checkpoint',reach_close:'Restore authored scene',reach_play:'Replay recorded poses',reach_pause:'Pause replay',reach_frame:'Recorded frame',reach_file:'Saved Episode',reach_refresh:'Refresh Episodes',reach_load:'Load Episode',reach_close_first:'Return to the editor scene before closing.',reach_recorded:'Recorded',reach_idle:'Ready',reach_running:'Running',reach_success:'Success',reach_timeout:'Timeout',reach_help:'Reach v1: isolated fixed-base dual arm in zero gravity. Add a dual-arm template first. Other scene objects are excluded; the authored scene stays unchanged. Seed determines a reference joint goal and target center (green marker). Each action holds 10 × 0.002 s; success needs 5 consecutive observations within 3.5 cm. Steps sets the timeout (1–500). Run uses a reference position-servo controller, not a learned policy. Save creates an immutable checkpoint in episodes/ beside the project; Running means a partial record. Replay/seek uses recorded poses without MuJoCo. Loaded Episodes are read-only; reset starts a new run. Exact rerun comparison requires the same model, engine, Python and platform; seed alone does not ensure cross-version equality.'},
 ja:{reach_seed:'Seed',reach_horizon:'上限',reach_robot:'Reachロボット',reach_add:'双腕ロボットを追加',reach_reset:'このseedでReachを初期化',reach_step:'0.02秒進める',reach_run:'成功またはtimeoutまで実行',reach_save:'Episodeのチェックポイントを保存',reach_close:'編集シーンに戻す',reach_play:'記録した姿勢を再生',reach_pause:'再生を一時停止',reach_frame:'記録フレーム',reach_file:'保存したEpisode',reach_refresh:'Episode一覧を更新',reach_load:'Episodeを読込',reach_close_first:'先に↩で編集シーンへ戻してください。',reach_recorded:'記録',reach_idle:'待機',reach_running:'実行中',reach_success:'成功',reach_timeout:'Timeout',reach_help:'Reach v1：選択した固定ベース双腕のみ、無重力の独立課題。先に双腕テンプレートを追加。他の物体は除外し、元の編集シーンは変更しません。seedで参照関節目標と中心位置の目標（緑マーカー）を決定。1step＝0.002秒×10、3.5cm以内に5回連続で留まると成功。上限1〜500stepでtimeout。実行は位置サーボの参照制御で、学習方策ではありません。保存先はプロジェクト横のepisodes/、上書きしないチェックポイントです。実行中の記録は部分記録。再生・シークは記録姿勢を使いMuJoCo不要。読込記録は読み取り専用で、resetすると新規実行になります。同一結果の再実行比較はモデル・MuJoCo・Python・実行環境を合わせて行い、seedのみで版を跨ぐ一致は保証しません。'},
 'zh-CN':{reach_seed:'Seed',reach_horizon:'上限',reach_robot:'Reach机器人',reach_add:'添加双臂机器人',reach_reset:'使用此seed重置Reach',reach_step:'前进0.02秒',reach_run:'运行至成功或超时',reach_save:'保存Episode检查点',reach_close:'返回编辑场景',reach_play:'回放记录姿态',reach_pause:'暂停回放',reach_frame:'记录帧',reach_file:'已保存Episode',reach_refresh:'刷新Episode列表',reach_load:'加载Episode',reach_close_first:'请先返回编辑场景再关闭。',reach_recorded:'记录',reach_idle:'就绪',reach_running:'运行中',reach_success:'成功',reach_timeout:'超时',reach_help:'Reach v1：仅使用所选固定基座双臂，零重力独立任务。先添加双臂模板；排除其他物体，保留原编辑场景。seed确定参考关节目标与中心目标（绿色标记）。每步0.002秒×10；连续5次处于3.5cm内即成功，上限1–500步。使用位置伺服参考控制器，不是学习策略。检查点保存于项目旁episodes/，不覆盖已有文件；运行中表示部分记录。回放与定位使用记录姿态，无需MuJoCo。加载记录只读，重置开始新的运行。精确重跑比较需相同模型、引擎、Python和平台，仅seed不能保证跨版本一致。'}
}))Object.assign(translations[language],labels);
let reachId=null;
function renderReach(){
 const r=current?.reach;
 if(!r){const robots=current?.robots.filter(r=>r.template==='dual_arm')||[];const previous=$('reachRobot').value;$('reachRobot').replaceChildren(...robots.map(r=>new Option(`#${r.root}`,String(r.root))));if(robots.some(r=>String(r.root)===previous))$('reachRobot').value=previous;reachId=null;}
 else if(reachId!==r.id){reachId=r.id;$('reachSeed').value=r.seed;$('reachHorizon').value=r.horizon;if(!$('reachRobot').querySelector(`option[value="${r.robot}"]`))$('reachRobot').append(new Option(`#${r.robot}`,String(r.robot)));$('reachRobot').value=String(r.robot);}
 $('reachStatus').textContent=r?`${r.step}/${r.horizon} · ${r.time.toFixed(2)} s · ${(r.distance*100).toFixed(1)} cm · ${t('reach_'+r.outcome)}${!r.live?' · '+t('reach_recorded'):''}`:t('reach_idle');
 $('reachTarget').textContent=r?`XYZ ${r.target.map(v=>v.toFixed(3)).join(' · ')} m`:'';
 $('reachFrame').max=String(Math.max(0,(r?.frames||1)-1));if(document.activeElement!==$('reachFrame'))$('reachFrame').value=String(r?.cursor||0);
 if(r?.saved)$('episodeFile').title=r.saved;
}
function reachControls(){
 const r=current?.reach,locked=busy||!current||!api,other=!!current?.previewing&&!r;
 $('reachFields').disabled=locked||other;
 for(const id of ['reachSeed','reachHorizon','reachRobot'])$(id).disabled=locked||other||!!r?.playing;
 $('reachAdd').disabled=locked||!!current?.previewing;
 $('reachReset').disabled=locked||other||!$('reachRobot').value;
 const cannotStep=locked||!r?.live||r.playing||r.cursor+1!==r.frames||r.final_outcome!=='running';
 $('reachStep').disabled=cannotStep;$('reachRun').disabled=cannotStep;
 $('reachSave').disabled=locked||!r||r.playing;$('reachClose').disabled=locked||!r;
 $('reachPlay').disabled=locked||!r||r.frames<2||r.playing;$('reachPause').disabled=locked||!r?.playing;$('reachFrame').disabled=locked||!r;
 $('episodeFile').disabled=locked||other;$('episodeRefresh').disabled=locked;$('episodeLoad').disabled=locked||other||!$('episodeFile').value;
}
function reachRequest(command){return action(async()=>{render(await api.core.invoke('reach_command',{request:{project_id:current.project_id,expected_revision:current.revision,expected_sequence:current.sequence,command}}));if(command.op==='save')await refreshEpisodeFiles();});}
async function refreshEpisodeFiles(){if(!api)return;const names=await api.core.invoke('episode_files');const previous=current?.reach?.saved||$('episodeFile').value;$('episodeFile').replaceChildren(...names.map(name=>new Option(name,name)));if(names.includes(previous))$('episodeFile').value=previous;controls();}
$('reachAdd').onclick=()=>mutate({kind:'add_robot',template:'dual_arm'});
$('reachReset').onclick=()=>{if(!$('reachSeed').reportValidity()||!$('reachHorizon').reportValidity())return;reachRequest({op:'reset',config:{seed:Number($('reachSeed').value),horizon:Number($('reachHorizon').value),robot:Number($('reachRobot').value)}});};
for(const [element,op] of Object.entries({reachStep:'step',reachRun:'run',reachSave:'save',reachClose:'close',reachPlay:'play',reachPause:'pause'}))$(element).onclick=()=>{const r=current?.reach;if(r)reachRequest({op,id:r.id,...(['step','run'].includes(op)?{expected_step:r.frames-1}:{} )});};
$('reachFrame').onchange=()=>{if(current?.reach)reachRequest({op:'seek',id:current.reach.id,index:Number($('reachFrame').value)});};
$('episodeRefresh').onclick=()=>action(refreshEpisodeFiles);$('episodeFile').onchange=controls;$('episodeLoad').onclick=()=>reachRequest({op:'load',name:$('episodeFile').value});
async function start(){
 localize();if(!api){status('desktop_required',true);return;}
 try{
  await api.event.listen('atom://layout',e=>applyLayout(e.payload));
  if(!isSplit){
   await api.event.listen('atom://changed',e=>render(e.payload));await api.event.listen('atom://close-blocked',()=>status(current?.asset_importing?'usd_importing':current?.reach?'reach_close_first':'unsaved_changes',true));await api.event.listen('atom://panel-error',e=>status(e.payload,true));await api.event.listen('atom://viewport',e=>{gpuKey=e.payload;$('gpu').textContent=t(gpuKey);});
   await api.event.listen('atom://dock-preview',e=>{const d=e.payload;$('dockPreview').textContent=d?`${d.target?t(d.target)+' · ':''}${t(d.zone==='floating'?'float':d.zone)}`:'';document.body.dataset.drop=d?.target===view?d.zone:'';});
   await api.event.listen('atom://camera',e=>applyCamera(e.payload));
   render(await api.core.invoke('snapshot'));applyCamera(await api.core.invoke('camera_state'));
  }
  if(!isSplit){physicsCatalog=await api.core.invoke('physics_catalog');await refreshEpisodeFiles();}applyLayout(await api.core.invoke('workspace_state'));status('ready');
 }catch(e){status(e?.code||'error',true);}
}
start();
