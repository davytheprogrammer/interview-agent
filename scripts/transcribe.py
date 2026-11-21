#!/usr/bin/env python3
"""Local Vosk microphone worker. Audio is piped in memory and never saved."""
import json
import os
import subprocess
import sys
import threading
import time
from array import array

if sys.platform == "win32":
    # Keep the DLL search handle alive while Vosk loads its bundled native dependencies.
    _vosk_dll_directory = os.add_dll_directory(os.path.join(os.path.dirname(sys.executable), "Lib", "site-packages", "vosk"))

from vosk import KaldiRecognizer, Model, SetLogLevel

SetLogLevel(-1)
stop = threading.Event()
capture = None

def send(event, text=""):
    print(json.dumps({"event": event, "text": text}), flush=True)

def control():
    global capture
    sys.stdin.readline()
    stop.set()
    if capture is not None and capture.poll() is None:
        capture.terminate()

threading.Thread(target=control, daemon=True).start()
try:
    model = Model(sys.argv[1])
    recognizer = KaldiRecognizer(model, 16000)
    if stop.is_set():
        send("ended")
        sys.exit(0)
    if sys.platform == "win32":
        helper = os.path.join(os.path.dirname(__file__), "platform", "windows", "audio.py")
        command = [sys.executable, "-u", helper, "capture", sys.argv[2]]
        flags = subprocess.CREATE_NO_WINDOW
    else:
        command = ["parec", "--device=" + sys.argv[2], "--raw", "--format=s16le", "--rate=16000", "--channels=1", "--latency-msec=50"]
        flags = 0
    capture = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, creationflags=flags)
    if stop.is_set():
        capture.terminate()
    send("started")
    last_partial = ""
    last_activity = 0.0
    while not stop.is_set():
        data = os.read(capture.stdout.fileno(), 4096)
        if not data:
            break
        # Output activity keeps the meeting question open while recognition is
        # still catching up. Only a boolean is emitted; samples stay local.
        samples = array("h")
        samples.frombytes(data[:len(data) - len(data) % 2])
        if sys.byteorder != "little":
            samples.byteswap()
        now = time.monotonic()
        if samples and sum(int(sample) * sample for sample in samples) / len(samples) > 180 * 180 and now - last_activity >= 0.5:
            send("activity")
            last_activity = now
        if recognizer.AcceptWaveform(data):
            text = json.loads(recognizer.Result()).get("text", "")
            if text:
                send("final", text)
            last_partial = ""
        else:
            partial = json.loads(recognizer.PartialResult()).get("partial", "")
            if partial != last_partial:
                send("partial", partial)
                last_partial = partial
    final = json.loads(recognizer.FinalResult()).get("text", "")
    if final:
        send("final", final)
    if not stop.is_set():
        send("error", "Audio capture stopped unexpectedly. Check the selected system device and try again.")
except Exception:
    send("error", "Could not start local transcription. Check the model, default audio device and operating-system audio permissions.")
finally:
    if capture is not None:
        if capture.poll() is None:
            capture.terminate()
        try:
            capture.wait(timeout=2)
        except subprocess.TimeoutExpired:
            capture.kill()
            capture.wait()
    send("ended")
