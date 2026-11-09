"""Private persistent CPU inference worker. Input contains observations, never teacher goals."""
import json
from pathlib import Path
import sys
import platform
import torch

torch.set_num_threads(1)
torch.set_num_interop_threads(1)
torch.use_deterministic_algorithms(True)
root = Path(sys.argv[1])
expected = json.loads((root / "expected.json").read_text())
checkpoint = torch.load(root / "best.pt", map_location="cpu", weights_only=True)
s = checkpoint["signature"]
if (checkpoint["schema_version"] != 1 or s["artifact_sha256"] != expected["artifact_sha256"]
    or s["source_sha256"] != expected["source_sha256"] or s["contract"] != expected["contract"]
    or s["normalization"] != expected["normalization"] or s["architecture"] != "mlp-64-64-relu-v1"
    or s["preprocessing"] != "torch-f32-normalize-v1" or s["torch"] != str(torch.__version__)
    or s["python"] != sys.version.split()[0] or s["device"] != "cpu" or s["threads"] != 1):
    raise ValueError("policy contract")
n = len(s["contract"]["channels"])
dim = n * 3 + 6
model = torch.nn.Sequential(torch.nn.Linear(dim,64),torch.nn.ReLU(),torch.nn.Linear(64,64),torch.nn.ReLU(),torch.nn.Linear(64,n))
model.load_state_dict(checkpoint["model"],strict=True)
model.eval()
if not all(bool(torch.isfinite(v).all()) for v in model.state_dict().values()):
    raise ValueError("nonfinite weights")
mean = torch.tensor(s["normalization"]["mean"],dtype=torch.float32)
scale = torch.tensor(s["normalization"]["std"],dtype=torch.float32)
print(json.dumps({"ready":True,"torch":str(torch.__version__),"python":sys.version.split()[0],"platform":platform.platform(),"device":"cpu","threads":1}),flush=True)
for line in sys.stdin:
    if len(line) > 32768:
        raise ValueError("request size")
    features = json.loads(line)
    if len(features) != dim:
        raise ValueError("feature shape")
    x = torch.tensor(features,dtype=torch.float32)
    if not bool(torch.isfinite(x).all()):
        raise ValueError("nonfinite input")
    with torch.inference_mode():
        action = model((x-mean)/scale)
    if not bool(torch.isfinite(action).all()):
        raise ValueError("nonfinite action")
    print(json.dumps(action.tolist(),allow_nan=False),flush=True)
