"""ATOM PyTorch BC v1 data adapter. Diagnose/materialize only; never optimizes weights."""
import contextlib
import hashlib
import io
import json
import math
import sys

MAX_BYTES = 64 * 1024 * 1024

def unique(pairs):
    result = {}
    for k, v in pairs:
        if k in result:
            raise ValueError("duplicate field")
        result[k] = v
    return result

def load(path, expected_sha):
    with open(path, "rb") as f:
        data = f.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES or hashlib.sha256(data).hexdigest() != expected_sha:
        raise ValueError("artifact hash")
    p = json.loads(data, object_pairs_hook=unique, parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite")))
    if set(p) != {"schema_version", "format", "summary", "feature_order", "contract", "normalization", "splits"} or p["schema_version"] != 1 or p["format"] != "atom-bc-dataset-v1":
        raise ValueError("format")
    s, norm = p["summary"], p["normalization"]
    contract = p["contract"]
    if contract["world_up"] != "Y" or contract["action_semantics"] != "absolute_joint_position_servo_target_rad" or contract["control_dt_seconds"] != 0.02 or len(contract["channels"]) != s["action_dim"]:
        raise ValueError("contract")
    n, dim = s["action_dim"], s["feature_dim"]
    if not 1 <= n <= 64 or dim != 3*n+6 or set(p["splits"]) != {"train", "validation", "test"} or norm["fit_split"] != "train" or norm["count"] != len(p["splits"]["train"]) or norm["count"] < 1:
        raise ValueError("shape")
    if p["feature_order"] != ["joint_position_rad", "joint_velocity_rad_s", "endpoint_world_m", "target_world_m", "previous_action_rad"]:
        raise ValueError("feature order")
    if len(norm["mean"]) != dim or len(norm["std"]) != dim or not all(math.isfinite(x) for x in norm["mean"]+norm["std"]) or not all(x > 0 for x in norm["std"]):
        raise ValueError("normalization")
    groups = {}
    for split, samples in p["splits"].items():
        if len(samples) != s["rows"][split] or len(samples) > 32000:
            raise ValueError("counts")
        for row in samples:
            if set(row) != {"episode_id", "group", "index", "features", "action"} or len(row["features"]) != dim or len(row["action"]) != n or not all(math.isfinite(x) for x in row["features"]+row["action"]):
                raise ValueError("sample")
            if row["group"] in groups and groups[row["group"]] != split:
                raise ValueError("group leakage")
            groups[row["group"]] = split
    # Recompute normalization strictly from train; held-out values cannot influence it.
    mean, m2 = [0.]*dim, [0.]*dim
    for count, row in enumerate(p["splits"]["train"], 1):
        for i, x in enumerate(row["features"]):
            delta = x-mean[i]
            mean[i] += delta/count
            m2[i] += delta*(x-mean[i])
    std = [math.sqrt(max(0., v/norm["count"])) for v in m2]
    std = [1. if x < 1e-8 else x for x in std]
    if any(not math.isclose(x, y, rel_tol=1e-10, abs_tol=1e-8) for x,y in zip(mean+std,norm["mean"]+norm["std"])):
        raise ValueError("normalization mismatch")
    return p

def fit_datasets(p, torch):
    """Only train and validation are exposed to a fitting consumer. Test is not a fit loader."""
    class BCDataset(torch.utils.data.Dataset):
        def __init__(self, rows):
            self.rows = rows
        def __len__(self):
            return len(self.rows)
        def __getitem__(self, i):
            row = self.rows[i]
            x = torch.tensor(row["features"], dtype=torch.float32)
            mean = torch.tensor(p["normalization"]["mean"], dtype=torch.float32)
            scale = torch.tensor(p["normalization"]["std"], dtype=torch.float32)
            return (x-mean)/scale, torch.tensor(row["action"], dtype=torch.float32)
    return {s: BCDataset(p["splits"][s]) for s in ("train", "validation")}

def diagnose(path, digest):
    p = load(path, digest)
    result = {"schema_version":1,"adapter":"pytorch_bc_v1","connected":True,"data_loaded":True,"framework_available":False,"tensor_batch_verified":False,"fit_ready":False,"trained":False,"artifact_sha256":digest,"source_sha256":p["summary"]["source_sha256"],"python":sys.version.split()[0],"framework_version":None,"rows":p["summary"]["rows"],"code":"learning_torch_unavailable"}
    try:
        # Library startup logs must not become part of the single JSON protocol response.
        with contextlib.redirect_stdout(io.StringIO()):
            import torch
        result["framework_available"] = True
        result["framework_version"] = str(torch.__version__)[:80]
        datasets = fit_datasets(p, torch)
        for name, dataset in datasets.items():
            if not len(dataset):
                continue
            x, y = next(iter(torch.utils.data.DataLoader(dataset, batch_size=1, shuffle=False, num_workers=0)))
            if list(x.shape) != [1,p["summary"]["feature_dim"]] or list(y.shape) != [1,p["summary"]["action_dim"]] or not bool(torch.isfinite(x).all()) or not bool(torch.isfinite(y).all()):
                raise ValueError("tensor batch")
        result["tensor_batch_verified"] = True
        result["fit_ready"] = p["summary"]["fit_data_ready"]
        result["code"] = "learning_ready" if result["fit_ready"] else "learning_missing_validation"
    except (ImportError, OSError):
        if result["framework_available"]:
            result["code"] = "learning_tensor_failed"
    except Exception:
        result["code"] = "learning_tensor_failed"
    return result

if __name__ == "__main__":
    try:
        if len(sys.argv) != 3:
            raise ValueError("arguments")
        result = diagnose(sys.argv[1], sys.argv[2])
    except Exception:
        result = {"schema_version":1,"code":"learning_worker_invalid_data"}
    print(json.dumps(result, allow_nan=False, separators=(",", ":")))
