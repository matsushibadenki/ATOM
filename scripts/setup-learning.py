"""Create ATOM's isolated Python 3.12 learning/test environment from the macOS lock."""
from pathlib import Path
import shutil
import platform
import sys
import subprocess
if sys.platform != "darwin" or platform.machine() != "arm64":
    raise SystemExit("This environment lock is verified on macOS arm64. Configure ATOM_LEARNING_PYTHON for other platforms.")
root = Path(__file__).resolve().parents[1]
python = shutil.which("python3.12")
if not python:
    raise SystemExit("Python 3.12 is required. Install it, then rerun this script.")
env = root / ".venv-learning"
if not env.exists():
    subprocess.run([python, "-m", "venv", str(env)], check=True)
interpreter = env / "bin/python"
version = subprocess.check_output([str(interpreter), "-I", "-c", "import sys; print('%s.%s'%sys.version_info[:2])"], text=True).strip()
if version != "3.12":
    raise SystemExit("Existing .venv-learning must use Python 3.12; it was preserved without modification.")
subprocess.run([str(interpreter), "-m", "pip", "install", "--only-binary=:all:", "--index-url", "https://pypi.org/simple", "-r", str(root / "scripts/requirements-learning.lock")], check=True)
subprocess.run([str(interpreter), "-I", "-c", "import torch,mujoco; print('PyTorch',torch.__version__,'MuJoCo',mujoco.__version__); print(torch.tensor([1.,2.]).tolist())"], check=True)
