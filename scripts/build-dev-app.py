"""Build a local, unsigned macOS development app. No installation or model download."""
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
if sys.platform != "darwin":
    raise SystemExit("This development bundle helper is for macOS. Use cargo run elsewhere.")
subprocess.run(["cargo", "build", "--locked", "-p", "atom-desktop", "-p", "atom-cli", "-p", "atom-mcp"], cwd=root, check=True)
bundle = root / "target/debug/ATOM.app/Contents"
(bundle / "MacOS").mkdir(parents=True, exist_ok=True)
(bundle / "Resources").mkdir(exist_ok=True)
for executable in ("atom-desktop", "atom-cli", "atom-mcp"):
    destination = bundle / "MacOS" / executable
    temporary = destination.with_name(f".{executable}-next")
    shutil.copy2(root / "target/debug" / executable, temporary)
    temporary.replace(destination)
(bundle / "Info.plist").write_bytes(plistlib.dumps({
    "CFBundleName": "ATOM", "CFBundleDisplayName": "ATOM", "CFBundleIdentifier": "dev.atom.lab",
    "CFBundleExecutable": "atom-desktop", "CFBundlePackageType": "APPL",
    "CFBundleVersion": "0.1.0", "CFBundleShortVersionString": "0.1.0",
    "NSHighResolutionCapable": True, "NSPrincipalClass": "NSApplication",
}))
print(bundle.parent)
