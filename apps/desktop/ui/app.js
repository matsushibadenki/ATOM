const translations = {
  en: { project:'Project', scene:'Scene', workflow:'Workflow', tasks:'Task specifications', validation:'Validate specifications', foundation:'Foundation', rust_owned:'Document · State · Renderer / Rust', save:'Save', reload:'Reload saved project', rename:'Rename project', add:'Add box', remove:'Remove box', viewport:'Open viewport', validate:'Validate tasks', closed:'Closed', opening:'Opening…', ready:'Ready', not_run:'Not run', connecting:'Connecting…', saved:'Saved', unsaved:'Unsaved', box:'Box', scope:'Validates eight task specifications. Physics, LLM inference and training are not connected yet.', validated:'8 specifications validated · 0 episodes', desktop_required:'Open this panel in the ATOM desktop app.', revision_conflict:'The project changed. Review the latest state and try again.', unsupported_version:'Unsupported project version.', invalid_project:'Check the project name and values.', project_locked:'The project is open in another process.', external_change:'The saved file changed externally. Save was cancelled.', unsaved_changes:'Save your changes before reloading or closing.', io_error:'Could not read or save the project.', invalid_json:'Invalid project file.', internal_error:'An internal error occurred.', forbidden:'This window is not authorized.', missing_project:'The saved project was not found.', gpu_unavailable:'GPU unavailable', error:'Operation failed.' },
  ja: { project:'プロジェクト', scene:'シーン', workflow:'ワークフロー', tasks:'タスク設定', validation:'設定を検証', foundation:'ベース実装', rust_owned:'Document · State · Renderer / Rust', save:'保存', reload:'保存済みプロジェクトを再読込', rename:'プロジェクト名を変更', add:'ボックスを追加', remove:'ボックスを削除', viewport:'描画ウインドウを開く', validate:'タスクを検証', closed:'未起動', opening:'起動中…', ready:'準備完了', not_run:'未実行', connecting:'接続中…', saved:'保存済み', unsaved:'未保存', box:'ボックス', scope:'8件のタスク設定を検証します。物理、LLM推論、学習はまだ接続していません。', validated:'設定8件を検証 · 実行0件', desktop_required:'ATOMデスクトップアプリで開いてください。', revision_conflict:'別の操作で更新されました。最新の状態を確認してください。', unsupported_version:'未対応のプロジェクト形式です。', invalid_project:'名前と入力値を確認してください。', project_locked:'別のプロセスが使用中です。', external_change:'保存先が外部で変更されました。保存を中止しました。', unsaved_changes:'再読込・終了の前に変更を保存してください。', io_error:'プロジェクトを読み書きできませんでした。', invalid_json:'プロジェクトファイルが不正です。', internal_error:'内部エラーが発生しました。', forbidden:'このウインドウには操作権限がありません。', missing_project:'保存済みプロジェクトが見つかりません。', gpu_unavailable:'GPU利用不可', error:'操作に失敗しました。' },
  'zh-CN': { project:'项目', scene:'场景', workflow:'工作流', tasks:'任务配置', validation:'验证配置', foundation:'基础实现', rust_owned:'Document · State · Renderer / Rust', save:'保存', reload:'重新加载已保存的项目', rename:'重命名项目', add:'添加方块', remove:'删除方块', viewport:'打开渲染窗口', validate:'验证任务', closed:'未启动', opening:'启动中…', ready:'就绪', not_run:'未运行', connecting:'连接中…', saved:'已保存', unsaved:'未保存', box:'方块', scope:'验证8个任务配置。物理、LLM推理和训练尚未连接。', validated:'已验证8个配置 · 运行0个回合', desktop_required:'请在ATOM桌面应用中打开。', revision_conflict:'项目已更新，请检查最新状态。', unsupported_version:'不支持的项目版本。', invalid_project:'请检查名称和输入值。', project_locked:'另一个进程正在使用此项目。', external_change:'文件已被外部修改，保存已取消。', unsaved_changes:'重新加载或关闭前请保存更改。', io_error:'无法读取或保存项目。', invalid_json:'无效的项目文件。', internal_error:'发生内部错误。', forbidden:'此窗口没有操作权限。', missing_project:'找不到已保存的项目。', gpu_unavailable:'GPU不可用', error:'操作失败。' }
};
const dockTranslations = {
 en:{float_group:'Float tab group',return_group:'Return tab group to dock',viewport:'3D View',drag:'Drag a tab to a panel: center joins tabs; edges split. Outside floats. Esc cancels.',float:'Float active panel',target:'Dock target',placement:'Placement',center:'Tab',left:'Left',right:'Right',top:'Above',bottom:'Below',dock_apply:'Apply docking',return_panel:'Return to previous dock',workspace:'Workspace',reset_layout:'Reset layout',resize:'Resize split; arrow keys adjust, Esc cancels',layout_conflict:'The layout changed. Try again.',invalid_layout:'Invalid layout.',panel_busy:'A layout gesture is in progress.',panel_error:'Could not move the panel.',layout_save_failed:'Layout changed, but could not be saved.',gpu_waiting:'Waiting for GPU',objects:'objects'},
 ja:{float_group:'タブグループを切り離す',return_group:'タブグループをドックへ戻す',viewport:'3Dビュー',drag:'タブをドラッグ：中央でタブ結合、四辺で分割、ウインドウ外で切り離し。Escで中止。',float:'アクティブなパネルを切り離す',target:'移動先のパネル',placement:'配置方法',center:'タブ',left:'左',right:'右',top:'上',bottom:'下',dock_apply:'配置を適用',return_panel:'元のドックへ戻す',workspace:'ワークスペース',reset_layout:'配置をリセット',resize:'分割を調整。矢印キーで微調整、Escで中止。',layout_conflict:'配置が更新されました。もう一度操作してください。',invalid_layout:'配置が不正です。',panel_busy:'配置を操作中です。',panel_error:'パネルを移動できませんでした。',layout_save_failed:'配置は変更されましたが、保存できませんでした。',gpu_waiting:'GPU待機中',objects:'オブジェクト'},
 'zh-CN':{float_group:'浮动标签组',return_group:'将标签组返回停靠区',viewport:'3D视图',drag:'拖动标签：中央合并，边缘拆分，窗口外浮动。Esc取消。',float:'浮动当前面板',target:'目标面板',placement:'位置',center:'标签',left:'左侧',right:'右侧',top:'上方',bottom:'下方',dock_apply:'应用布局',return_panel:'返回原停靠位置',workspace:'工作区',reset_layout:'重置布局',resize:'调整分隔线。方向键微调，Esc取消。',layout_conflict:'布局已更新，请重试。',invalid_layout:'无效的布局。',panel_busy:'正在调整布局。',panel_error:'无法移动面板。',layout_save_failed:'布局已更改，但无法保存。',gpu_waiting:'等待GPU',objects:'个对象'}
};
for(const language of Object.keys(translations)) Object.assign(translations[language],dockTranslations[language]);
const cameraTranslations={en:{show_grid:'XZ grid (Y = 0); spacing follows zoom',show_axes:'World axes: X red, Y green (up), Z blue',pan_mode:'Pan with left drag',selected_object:'Selected object',focus_selection:'Focus selected object (F)',reset_camera:'Reset camera (0)',select_object:'Select object',no_selection:'Select an object first.',camera_help:'Left drag: orbit. Right/middle drag or Shift + left drag: pan. Wheel: zoom. Click an object to select. F: focus selection. 0: reset. Esc: cancel drag.'},ja:{show_grid:'XZグリッド（Y＝0）。ズームに応じて間隔を調整',show_axes:'ワールド座標軸：Xは赤、Yは緑（上）、Zは青',pan_mode:'左ドラッグをパンに切り替える',selected_object:'選択中のオブジェクト',focus_selection:'選択オブジェクトにフォーカス（F）',reset_camera:'視点をリセット（0）',select_object:'オブジェクトを選択',no_selection:'オブジェクトを選択してください。',camera_help:'左ドラッグで回転。右・中ドラッグ、またはShift＋左ドラッグでパン。ホイールでズーム。クリックで選択。Fでフォーカス、0で視点リセット、Escでドラッグを中止。'},'zh-CN':{show_grid:'XZ网格（Y＝0），间距随缩放调整',show_axes:'世界坐标轴：X红色、Y绿色（向上）、Z蓝色',pan_mode:'左键拖动平移',selected_object:'选中对象',focus_selection:'聚焦选中对象（F）',reset_camera:'重置视角（0）',select_object:'选择对象',no_selection:'请先选择对象。',camera_help:'左键拖动旋转；右键、中键或Shift＋左键拖动平移；滚轮缩放。单击对象选择，F聚焦，0重置，Esc取消拖动。'}};
for(const language of Object.keys(translations))Object.assign(translations[language],cameraTranslations[language]);
const paths={grid:'M3 3h14v14H3z M3 8h14 M3 12h14 M8 3v14 M12 3v14',axes:'M5 15h12 M5 15V3 M5 15l8-8 M14 12l3 3-3 3 M2 6l3-3 3 3 M9 7h4v4',pan:'M10 2v16 M2 10h16 M7 5l3-3 3 3 M7 15l3 3 3-3 M5 7l-3 3 3 3 M15 7l3 3-3 3',focus:'M3 7V3h4 M13 3h4v4 M17 13v4h-4 M7 17H3v-4 M6 10h8 M10 6v8',group:'M6 2h12v12H6z M2 6v12h12 M6 6h12 M10 2v4',return_group:'M2 3h16v14H2z M2 7h16 M7 3v4 M12 9l-3 3 3 3 M9 12h6',save:'M3 3h11l3 3v11H3z M6 3v5h8V3 M6 17v-6h8v6',reload:'M16 6a7 7 0 1 0 1 7 M16 2v5h-5',check:'m4 10 4 4 8-9',plus:'M10 3v14 M3 10h14',remove:'M4 5h12 M8 5V3h4v2 M6 5l1 12h6l1-12 M9 8v6 M11 8v6',play:'m6 3 10 7-10 7z',cube:'m10 2 7 4v8l-7 4-7-4V6z M3 6l7 4 7-4 M10 10v8',float:'M7 3h10v10H7z M3 7v10h10',return:'M3 4h14v13H3z M11 7l-3 3 3 3 M8 10h7',layout:'M3 3h14v14H3z M7 3v14 M13 3v14',scene:'m10 2 7 4v8l-7 4-7-4V6z M3 6l7 4 7-4 M10 10v8',workflow:'M3 3h5v5H3z M12 12h5v5h-5z M8 5h6v7',viewport:'M2 3h16v12H2z M7 18h6 M10 15v3'};
function icon(name){const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 20 20');svg.setAttribute('aria-hidden','true');const p=document.createElementNS(svg.namespaceURI,'path');p.setAttribute('d',paths[name]);svg.append(p);return svg;}
document.querySelectorAll('[data-icon]').forEach(el=>el.append(icon(el.dataset.icon)));
const $=id=>document.getElementById(id),view=new URLSearchParams(location.search).get('view')||'toolbar',isSplit=view.startsWith('split-');
document.body.dataset.view=view;if(isSplit)document.body.dataset.split='true';
let camera=null;
let locale='ja',current=null,layout=null,busy=false,statusKey='connecting',gpuKey='opening',reportKey='not_run',drag=null,splitDrag=null;
const t=key=>translations[locale][key]||translations[locale].error,api=window.__TAURI__;
function nodes(root,kind){if(!root)return [];if(root.kind===kind)return [root,...(root.kind==='split'?[...nodes(root.first,kind),...nodes(root.second,kind)]:[])];return root.kind==='split'?[...nodes(root.first,kind),...nodes(root.second,kind)]:[];}
function groupFor(s,p){return nodes(s?.root,'tabs').find(g=>g.panels.includes(p))||s?.floating_groups.find(g=>g.panels.includes(p));}
function controls(){
 document.querySelectorAll('button').forEach(b=>b.disabled=busy||!current||!api);
 for(const id of ['cameraFocus','sceneFocus'])$(id).disabled=busy||!api||!camera?.selected;
 $('reload').disabled=busy||!current||current.dirty||!api;$('locale').disabled=busy||!layout||!api;
 $('dockApply').disabled=busy||!layout||!$('dockTarget').options.length;$('floatPanel').disabled=busy||!layout||(layout.floating.includes(view)&&groupFor(layout,view)?.panels.length===1);
 $('floatGroup').hidden=!!layout?.floating.includes(view);$('floatGroup').disabled=busy||!layout||(groupFor(layout,view)?.panels.length||0)<2;
 $('returnGroup').hidden=!layout?.floating.includes(view)||(groupFor(layout,view)?.panels.length||0)<2;
 $('returnPanel').hidden=!layout?.floating.includes(view);$('dockTarget').disabled=busy||!layout;$('dockZone').disabled=busy||!layout;
}
function localize(){
 document.documentElement.lang=locale;
 document.querySelectorAll('[data-text]').forEach(el=>el.textContent=t(el.dataset.text));document.querySelectorAll('[data-label]').forEach(el=>{el.title=t(el.dataset.label);el.setAttribute('aria-label',t(el.dataset.label));});
 $('status').textContent=t(statusKey);$('gpu').textContent=t(gpuKey);$('report').textContent=t(reportKey);$('locale').value=locale;
 if(current)render(current);else controls();
}
function render(s){
 if(current&&s.revision<current.revision)return;if(!current||current.revision!==s.revision)reportKey='not_run';current=s;
 if(document.activeElement!==$('name'))$('name').value=s.name;
 $('dirty').textContent=t(s.dirty?'unsaved':'saved');$('revision').textContent=`rev ${s.revision}`;$('report').textContent=t(reportKey);$('objectCount').textContent=`${s.objects.length} ${t('objects')}`;
 if(view==='scene')$('objects').replaceChildren(...s.objects.map(o=>{const row=document.createElement('li');row.dataset.selected=String(camera?.selected===o.id);row.append(icon('cube'));const name=document.createElement('button');name.className='object-name';name.textContent=`${t('box')} ${o.id}`;name.setAttribute('aria-label',`${t('select_object')} ${o.id}`);name.setAttribute('aria-pressed',String(camera?.selected===o.id));name.onclick=()=>cameraCommand({kind:'select',id:o.id});row.append(name);const pos=document.createElement('span');pos.className='position';pos.textContent=o.position.map(n=>n.toFixed(1)).join(' / ');row.append(pos);const remove=document.createElement('button');remove.title=t('remove');remove.setAttribute('aria-label',`${t('remove')} ${o.id}`);remove.append(icon('remove'));remove.onclick=()=>mutate({kind:'remove_box',id:o.id});row.append(remove);return row;}));controls();
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
async function action(fn){if(busy||!api)return;busy=true;controls();try{await fn();status('ready');}catch(e){status(e?.code||'error',true);try{if(e?.code==='revision_conflict')render(await api.core.invoke('snapshot'));if(['layout_conflict','panel_error','layout_save_failed'].includes(e?.code))applyLayout(await api.core.invoke('workspace_state'));}catch{/* Keep original failure visible. */}}finally{busy=false;controls();}}
function mutate(command){return action(async()=>render(await api.core.invoke('edit',{request:{expected_revision:current.revision,project_id:current.project_id,command}})));}
function command(command){return action(async()=>applyLayout(await api.core.invoke('workspace_command',{command,expectedRevision:layout.revision})));}
function applyCamera(s){if(camera&&s.revision<camera.revision)return;const selectionChanged=camera?.selected!==s.selected;camera=s;$('cameraGrid').setAttribute('aria-pressed',String(s.show_grid));$('cameraAxes').setAttribute('aria-pressed',String(s.show_axes));$('cameraPan').setAttribute('aria-pressed',String(s.pan_mode));$('cameraSelection').textContent=s.selected?`#${s.selected}`:'';$('cameraSelection').title=s.selected?`${t('selected_object')} · ${t('box')} ${s.selected}`:'';if(current&&selectionChanged)render(current);else controls();}
function cameraCommand(command){return action(async()=>applyCamera(await api.core.invoke('camera_command',{command,expectedRevision:current.revision})));}
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
async function start(){
 localize();if(!api){status('desktop_required',true);return;}
 try{
  await api.event.listen('atom://layout',e=>applyLayout(e.payload));
  if(!isSplit){
   await api.event.listen('atom://changed',e=>render(e.payload));await api.event.listen('atom://close-blocked',()=>status('unsaved_changes',true));await api.event.listen('atom://panel-error',e=>status(e.payload,true));await api.event.listen('atom://viewport',e=>{gpuKey=e.payload;$('gpu').textContent=t(gpuKey);});
   await api.event.listen('atom://dock-preview',e=>{const d=e.payload;$('dockPreview').textContent=d?`${d.target?t(d.target)+' · ':''}${t(d.zone==='floating'?'float':d.zone)}`:'';document.body.dataset.drop=d?.target===view?d.zone:'';});
   await api.event.listen('atom://camera',e=>applyCamera(e.payload));
   render(await api.core.invoke('snapshot'));applyCamera(await api.core.invoke('camera_state'));
  }
  applyLayout(await api.core.invoke('workspace_state'));status('ready');
 }catch(e){status(e?.code||'error',true);}
}
start();
