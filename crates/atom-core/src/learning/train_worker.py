"""Fixed CPU BC trainer. No arbitrary model/code arguments, no test-based selection."""
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import runpy
import sys
import time

adapter = runpy.run_path(str(Path(__file__).with_name("adapter.py")))

def digest(path):
    with open(path, "rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()

def write_json(path, value):
    with open(path, "x", encoding="utf-8") as f:
        json.dump(value, f, allow_nan=False, separators=(",", ":"))
        f.flush()
        os.fsync(f.fileno())

def checkpoint(path, value, torch):
    with open(path, "xb") as f:
        torch.save(value, f)
        f.flush()
        os.fsync(f.fileno())
    if path.stat().st_size > 8*1024*1024:
        raise ValueError("checkpoint size")

def run():
    import torch
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    data_path, cfg_path, stage, resume, cancel_path = sys.argv[1:]
    stage = Path(stage)
    cfg = json.loads(Path(cfg_path).read_text())
    p = adapter["load"](data_path, cfg["artifact_sha256"])
    c = cfg["config"]
    source = p["summary"]["source_sha256"]
    if not p["summary"]["fit_data_ready"] or not p["summary"]["test_data_ready"]:
        raise ValueError("all three partitions required")
    start = time.monotonic()
    end = start+c["max_seconds"]
    fit = adapter["fit_datasets"](p, torch)
    def tensors(rows):
        # Same preprocessing as the diagnosed map-style Dataset, without test exposure to fit.
        x = torch.tensor([r["features"] for r in rows], dtype=torch.float32)
        y = torch.tensor([r["action"] for r in rows], dtype=torch.float32)
        mean = torch.tensor(p["normalization"]["mean"], dtype=torch.float32)
        scale = torch.tensor(p["normalization"]["std"], dtype=torch.float32)
        return (x-mean)/scale, y
    train_x, train_y = tensors(p["splits"]["train"])
    val_x, val_y = tensors(p["splits"]["validation"])
    # Exercise the actual DataLoader independently of the vectorized training batch path.
    batch = next(iter(torch.utils.data.DataLoader(fit["train"], batch_size=min(c["batch_size"],len(fit["train"])),shuffle=False,num_workers=0)))
    if batch[0].shape[1] != p["summary"]["feature_dim"] or not torch.equal(batch[0][0], train_x[0]):
        raise ValueError("loader shape")
    torch.manual_seed(c["seed"])
    dim, n = p["summary"]["feature_dim"], p["summary"]["action_dim"]
    model = torch.nn.Sequential(torch.nn.Linear(dim,64),torch.nn.ReLU(),torch.nn.Linear(64,64),torch.nn.ReLU(),torch.nn.Linear(64,n))
    optimizer = torch.optim.Adam(model.parameters(), lr=c["learning_rate"])
    order = torch.randperm(len(train_x), generator=torch.Generator().manual_seed(c["seed"]))
    signature = {"artifact_sha256":cfg["artifact_sha256"],"source_sha256":source,"contract":p["contract"],"normalization":p["normalization"],"architecture":"mlp-64-64-relu-v1","preprocessing":"torch-f32-normalize-v1","seed":c["seed"],"batch_size":c["batch_size"],"learning_rate":c["learning_rate"],"torch":str(torch.__version__),"python":sys.version.split()[0],"device":"cpu","threads":1}
    def metrics(x,y):
        model.eval()
        se, ae, count = 0.,0.,0
        with torch.no_grad():
            for i in range(0,len(x),256):
                pred = model(x[i:i+256])
                delta = pred-y[i:i+256]
                if not bool(torch.isfinite(delta).all()):
                    raise ValueError("nonfinite metrics")
                se += float(delta.double().square().sum())
                ae += float(delta.double().abs().sum())
                count += delta.numel()
        return {"mse_rad2":se/count,"rmse_rad":math.sqrt(se/count),"mae_rad":ae/count,"rows":len(x)}
    step, best_step = 0,0
    initial_val = metrics(val_x,val_y)
    best_val = initial_val["mse_rad2"]
    best_weights = copy.deepcopy(model.state_dict())
    parent = None
    if resume != "-":
        resume = Path(resume)
        manifest = json.loads((resume/"manifest.json").read_text())
        for name in ("last.pt","best.pt","report.json"):
            if digest(resume/name) != manifest["files"][name]:
                raise ValueError("resume hash")
        last = torch.load(resume/"last.pt", map_location="cpu", weights_only=True)
        best = torch.load(resume/"best.pt", map_location="cpu", weights_only=True)
        if last["signature"] != signature or best["signature"] != signature or last["schema_version"] != 1 or best["schema_version"] != 1:
            raise ValueError("resume contract")
        model.load_state_dict(last["model"],strict=True)
        optimizer.load_state_dict(last["optimizer"])
        step, best_step = last["step"],last["best_step"]
        best_val, initial_val = last["best_val"],last["initial_validation"]
        best_weights = best["model"]
        if best["step"] != best_step or not 0 <= best_step <= step <= 10000 or (cfg["op"] != "evaluate" and c["max_steps"] <= step):
            raise ValueError("resume step")
        parent = digest(resume/"manifest.json")
    def save_best():
        checkpoint(stage/"best.pt",{"schema_version":1,"signature":signature,"model":best_weights,"step":best_step,"validation_mse_rad2":best_val},torch)
    if cfg["op"] == "evaluate":
        # A fixed final-evaluation operation never calls optimizer.step or selects another model.
        if resume == "-":
            raise ValueError("evaluation checkpoint")
        model.load_state_dict(best_weights,strict=True)
        test_x,test_y = tensors(p["splits"]["test"])
        result={"schema_version":1,"kind":"evaluation","config":c,"source_sha256":source,"artifact_sha256":cfg["artifact_sha256"],"checkpoint_sha256":digest(resume/"best.pt"),"best_step":best_step,"test":metrics(test_x,test_y),"trained":False,"torch":str(torch.__version__)}
        write_json(stage/"evaluation.json",result)
        return result
    initial_weights = copy.deepcopy(model.state_dict())
    start_step=step
    status="completed"
    history=[]
    while step<c["max_steps"]:
        if Path(cancel_path).exists():
            status="cancelled";break
        if time.monotonic()>=end:
            status="time_budget";break
        indices=order[(torch.arange(c["batch_size"])+step*c["batch_size"])%len(order)]
        model.train();optimizer.zero_grad(set_to_none=True)
        loss=torch.nn.functional.mse_loss(model(train_x[indices]),train_y[indices])
        if not bool(torch.isfinite(loss)):
            raise ValueError("nonfinite loss")
        loss.backward();torch.nn.utils.clip_grad_norm_(model.parameters(),10.,error_if_nonfinite=True);optimizer.step();step+=1
        if step==1 or step%10==0:
            value=metrics(val_x,val_y)["mse_rad2"]
            history.append({"step":step,"train_mse_rad2":float(loss.detach()),"validation_mse_rad2":value})
            if value<best_val:
                best_val,best_step,best_weights=value,step,copy.deepcopy(model.state_dict())
    if step==start_step:
        raise ValueError("no optimizer step")
    if not all(bool(torch.isfinite(v).all()) for v in list(model.state_dict().values())+list(best_weights.values())):
        raise ValueError("nonfinite weights")
    updated=any(not torch.equal(initial_weights[k],v) for k,v in model.state_dict().items())
    checkpoint(stage/"last.pt",{"schema_version":1,"signature":signature,"model":model.state_dict(),"optimizer":optimizer.state_dict(),"step":step,"best_step":best_step,"best_val":best_val,"initial_validation":initial_val},torch)
    save_best()
    # Reload the published best payload using weights_only and strict loading before test.
    reloaded=torch.load(stage/"best.pt",map_location="cpu",weights_only=True)
    model.load_state_dict(reloaded["model"],strict=True)
    verified_val=metrics(val_x,val_y)
    if abs(verified_val["mse_rad2"]-best_val)>1e-12:
        raise ValueError("checkpoint reload")
    # Test tensors are constructed only after validation selection and checkpoint freezing.
    test_x,test_y=tensors(p["splits"]["test"])
    result={"schema_version":1,"kind":"training","status":status,"source_sha256":source,"artifact_sha256":cfg["artifact_sha256"],"config":c,"start_step":start_step,"step":step,"best_step":best_step,"weights_updated":updated,"tensor_batch_verified":True,"checkpoint_reload_verified":True,"initial_validation":initial_val,"best_validation":verified_val,"test":metrics(test_x,test_y),"history":history,"elapsed_seconds":time.monotonic()-start,"rows":p["summary"]["rows"],"parent_manifest_sha256":parent,"torch":str(torch.__version__),"python":sys.version.split()[0],"device":"cpu","threads":1,"evaluation_semantics":"offline_recorded_action_prediction_not_physics_success"}
    write_json(stage/"report.json",result)
    files={name:digest(stage/name) for name in ("last.pt","best.pt","report.json")}
    write_json(stage/"manifest.json",{"schema_version":1,"format":"atom-bc-job-v1","source_sha256":source,"artifact_sha256":cfg["artifact_sha256"],"files":files})
    return result

if __name__ == "__main__":
    try:
        result=run()
        print(json.dumps({"ok":True,"result":result},allow_nan=False,separators=(",", ":")))
    except Exception as error:
        message=str(error)
        code={"resume contract":"learning_checkpoint_config","resume step":"learning_checkpoint_step","nonfinite weights":"learning_nonfinite","nonfinite loss":"learning_nonfinite","nonfinite metrics":"learning_nonfinite"}.get(message,"learning_training_failed")
        if message=="no optimizer step":
            code="learning_job_cancelled" if Path(sys.argv[-1]).exists() else "learning_job_budget"
        print(json.dumps({"ok":False,"code":code}))
        sys.exit(1)
