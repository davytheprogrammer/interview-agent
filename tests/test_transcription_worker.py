import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time

root = Path(__file__).resolve().parents[1]
env = os.environ.copy()
env["PATH"] = str(root / "tests/fixtures") + os.pathsep + env["PATH"]
worker = subprocess.Popen([str(root / ".venv/bin/python"), str(root / "scripts/transcribe.py"), str(root / "models/vosk-model-small-en-us-0.15"), "synthetic"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
events = queue.Queue()
threading.Thread(target=lambda: [events.put(line) for line in worker.stdout], daemon=True).start()
seen = []
deadline = time.monotonic() + 20
stopped = False
try:
    while time.monotonic() < deadline:
        try:
            event = json.loads(events.get(timeout=1))
        except queue.Empty:
            continue
        seen.append(event)
        if event["event"] == "partial" and event["text"] and not stopped:
            worker.stdin.write("stop\n")
            worker.stdin.flush()
            stopped = True
        if event["event"] == "ended":
            break
    worker.wait(timeout=5)
    assert any(e["event"] == "started" for e in seen), seen
    assert any(e["event"] == "final" and e["text"] for e in seen), seen
    assert seen[-1]["event"] == "ended", seen
    assert not any(e["event"] == "error" for e in seen), seen
    print("Synthetic speech: started, produced final text, stopped cleanly")
finally:
    if worker.poll() is None:
        worker.kill()
        worker.wait()
