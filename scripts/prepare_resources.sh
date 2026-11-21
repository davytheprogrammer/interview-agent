#!/usr/bin/env bash
set -euo pipefail
project_root=$(cd "$(dirname "$0")/.." && pwd)
cd "$project_root"
target=${1:-all}
case "$target" in linux|windows|all) ;; *) printf 'Usage: %s [linux|windows|all]\n' "$0" >&2; exit 2;; esac
mkdir -p build-tools/downloads resources/models
if [ ! -x .venv/bin/python ]; then python3.10 -m venv .venv; fi
.venv/bin/pip install -r scripts/requirements-linux.txt
model_zip=build-tools/downloads/vosk-model-small-en-us-0.15.zip
if [ ! -f "$model_zip" ]; then curl -fL --retry 2 -o "$model_zip" https://alphacephei.com/vosk/models/vosk-model-small-en-us-0.15.zip; fi
printf '%s  %s\n' 30f26242c4eb449f948e42cb302dd7a686cb29a3423a8367f99ff41780942498 "$model_zip" | sha256sum -c -
unzip -oq "$model_zip" -d resources/models
if [ "$target" = linux ] || [ "$target" = all ]; then
 mkdir -p resources/linux/site-packages
 cp -a .venv/lib/python3.10/site-packages/. resources/linux/site-packages/
fi
if [ "$target" = windows ] || [ "$target" = all ]; then
 python_zip=build-tools/downloads/python-3.13.17-embed-amd64.zip
 if [ ! -f "$python_zip" ]; then curl -fL --retry 2 -o "$python_zip" https://www.python.org/ftp/python/3.13.17/python-3.13.17-embed-amd64.zip; fi
 printf '%s  %s\n' 9e35319bffedab8b36ad44d8f377cb4f1733bcec257a0b6d04d94f01486ddb38 "$python_zip" | sha256sum -c -
 mkdir -p resources/windows/python/Lib/site-packages
 unzip -oq "$python_zip" -d resources/windows/python
 cp scripts/platform/windows/python313._pth resources/windows/python/python313._pth
 cp .venv/lib/python3.10/site-packages/srt.py resources/windows/python/srt.py
 .venv/bin/pip install --upgrade --target resources/windows/python/Lib/site-packages --platform win_amd64 --python-version 3.13 --implementation cp --abi cp313 --only-binary=:all: --no-deps vosk==0.3.45 soundcard==0.4.6 numpy==2.2.6 cffi==1.17.1 pycparser==2.23 requests==2.32.5 urllib3==2.5.0 idna==3.10 charset-normalizer==3.4.4 certifi==2026.11.20 tqdm==4.67.1 websockets==15.0.1 colorama==0.4.6
fi
