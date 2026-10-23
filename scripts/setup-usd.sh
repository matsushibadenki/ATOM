#!/bin/sh
# Dedicated SDK environment; run explicitly, never from the application startup.
set -eu
cd "$(dirname "$0")/.."
python3 -m venv .venv-usd
.venv-usd/bin/python -m pip install -r scripts/requirements-usd.txt
