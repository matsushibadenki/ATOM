"""ATOM Reach v1: persistent private MuJoCo process, bounded line protocol."""
import sys, json, math, platform
try:
    import mujoco
except ImportError:
    raise SystemExit(42)
model = mujoco.MjModel.from_xml_path(sys.argv[1])
data = mujoco.MjData(model)
config = json.load(open(sys.argv[2], encoding='utf-8'))
geom = mujoco.mj_name2id(model, mujoco.mjtObj.mjOBJ_GEOM, 'o'+str(config['endpoint']))
assert geom >= 0 and model.nu == len(config['channels'])
for i, channel in enumerate(config['channels']):
    j = model.actuator_trnid[i,0]
    assert model.jnt_qposadr[j] == channel['qpos_index'] and model.jnt_dofadr[j] == channel['qpos_index']
    name = mujoco.mj_id2name(model, mujoco.mjtObj.mjOBJ_JOINT, j)
    assert name in ('j'+str(channel['id']), 'j'+str(channel['id'])+'_'+str(channel['axis']))

def observation():
    mujoco.mj_forward(model,data)
    if any(w.number for w in data.warning):
        raise RuntimeError('unstable simulation')
    poses=[]
    for i in range(model.ngeom):
        name=mujoco.mj_id2name(model,mujoco.mjtObj.mjOBJ_GEOM,i)
        if not name or not name.startswith('o'): continue
        r=data.geom_xmat[i].reshape(3,3)
        y=math.asin(max(-1.,min(1.,float(r[0,2]))))
        if abs(math.cos(y))>1e-7: x,z=math.atan2(-r[1,2],r[2,2]),math.atan2(-r[0,1],r[0,0])
        else: x,z=math.atan2(r[2,1],r[1,1]),0.
        poses.append(dict(id=int(name[1:]),position=data.geom_xpos[i].tolist(),rotation=[x,y,z]))
    return dict(engine='MuJoCo '+mujoco.__version__,python=sys.version.split()[0],platform=platform.platform(),time=float(data.time),qpos=data.qpos.tolist(),qvel=data.qvel.tolist(),endpoint=data.geom_xpos[geom].tolist(),poses=poses)

for line in sys.stdin:
    try:
        request=json.loads(line)
        if request['op']=='reset':
            mujoco.mj_resetData(model,data)
            key=mujoco.mj_name2id(model,mujoco.mjtObj.mjOBJ_KEY,'authored')
            if key>=0: mujoco.mj_resetDataKeyframe(model,data,key)
            # Reset controls even for an all-zero pose without a keyframe.
            data.ctrl[:]=config['initial_action']
        elif request['op']=='step':
            action=request['action']
            assert len(action)==model.nu
            assert all(math.isfinite(v) and model.actuator_ctrlrange[i,0]<=v<=model.actuator_ctrlrange[i,1] for i,v in enumerate(action))
            data.ctrl[:]=action
            for _ in range(10): mujoco.mj_step(model,data)
        else: raise ValueError('operation')
        print(json.dumps(observation(),allow_nan=False,separators=(',',':')),flush=True)
    except Exception:
        raise SystemExit(43)
