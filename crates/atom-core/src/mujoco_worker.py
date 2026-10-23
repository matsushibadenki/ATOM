"""Private headless worker; no renderer, networking, or user-provided Python."""
import json
import math
import sys
try:
    import mujoco
except ImportError:
    raise SystemExit(42)

model = mujoco.MjModel.from_xml_path(sys.argv[1])
data = mujoco.MjData(model)
steps = int(sys.argv[3])
assert 1 <= steps <= 1000
for _ in range(steps):
    mujoco.mj_step(model, data)
# Synchronize derived geometry transforms with the final integrated qpos.
mujoco.mj_forward(model, data)
if any(w.number for w in data.warning):
    raise RuntimeError("MuJoCo warning: unstable or capacity-limited simulation")
poses = []
for i in range(model.ngeom):
    name = mujoco.mj_id2name(model, mujoco.mjtObj.mjOBJ_GEOM, i)
    if not name or not name.startswith("o"):
        continue
    # Invert the renderer's intrinsic XYZ = Rx * Ry * Rz convention.
    position = data.geom_xpos[i].tolist()
    r = data.geom_xmat[i].reshape(3, 3)
    if model.nuser_geom >= 4 and model.geom_user[i, 0] == 1:
        # MuJoCo recenters/rotates mesh vertices during compilation. Return the
        # authored mesh frame, not its compiler-generated inertia frame.
        import numpy as np
        x, y, z = model.geom_user[i, 1:4]
        sx, cx, sy, cy, sz, cz = math.sin(x), math.cos(x), math.sin(y), math.cos(y), math.sin(z), math.cos(z)
        rx = np.array([[1,0,0],[0,cx,-sx],[0,sx,cx]])
        ry = np.array([[cy,0,sy],[0,1,0],[-sy,0,cy]])
        rz = np.array([[cz,-sz,0],[sz,cz,0],[0,0,1]])
        body = model.geom_bodyid[i]
        r = data.xmat[body].reshape(3,3) @ rx @ ry @ rz
        position = data.xpos[body].tolist()
    y = math.asin(max(-1.0, min(1.0, float(r[0, 2]))))
    if abs(math.cos(y)) > 1e-7:
        x, z = math.atan2(-r[1, 2], r[2, 2]), math.atan2(-r[0, 1], r[0, 0])
    else:
        x, z = math.atan2(r[2, 1], r[1, 1]), 0.0
    poses.append({"id": int(name[1:]), "position": position, "rotation": [x, y, z]})
with open(sys.argv[2], "w", encoding="utf-8") as f:
    json.dump({"engine": "MuJoCo " + mujoco.__version__, "time": float(data.time), "contacts": int(data.ncon), "poses": poses}, f, allow_nan=False)
