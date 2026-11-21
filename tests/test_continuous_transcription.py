import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time

root = Path(__file__).resolve().parents[1]
env = os.environ.copy()
env["PATH"] = str(root / "tests/multi_fixture") + os.pathsep + env["PATH"]
worker = subprocess.Popen([str(root / ".venv/bin/python"), str(root / "scripts/transcribe.py"), str(root / "models/vosk-model-small-en-us-0.15"), "synthetic"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
events = queue.Queue()
threading.Thread(target=lambda: [events.put(line) for line in worker.stdout], daemon=True).start()
seen = []
try:
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        try:
            event = json.loads(events.get(timeout=1))
        except queue.Empty:
            continue
        seen.append(event)
        if len([e for e in seen if e["event"] == "final" and e["text"]]) >= 2:
            break
        if event["event"] in ("ended", "error"):
            break
    finals = [e["text"] for e in seen if e["event"] == "final" and e["text"]]
    assert len(finals) >= 2, f"Expected two separate utterances; got {seen}"
    assert worker.poll() is None, "worker stopped before being asked"
    worker.stdin.write("stop\n")
    worker.stdin.flush()
    worker.wait(timeout=5)
    print("Continuous fixture: two utterances recognized without restarting, then stopped")
finally:
    if worker.poll() is None:
        worker.kill()
        worker.wait()
