"""Windows WASAPI capture helper. stdout contains PCM or a small probe result."""
import os
import sys

def get_device(mode):
    import soundcard as sc
    if mode == "computer":
        speaker = sc.default_speaker()
        if speaker is None:
            raise RuntimeError("No default speaker is available")
        device = sc.get_microphone(id=speaker.id, include_loopback=True)
        if device is None or not device.isloopback or device.id != speaker.id:
            raise RuntimeError("No loopback source for the default speaker")
        return device
    if mode == "microphone":
        device = sc.default_microphone()
        if device is None or device.isloopback:
            raise RuntimeError("No default microphone is available")
        return device
    raise ValueError("Unknown source")

def main():
    action, mode = sys.argv[1:3]
    device = get_device(mode)
    if action == "probe":
        print("available", flush=True)
        return
    import numpy as np
    if action == "capture":
        import msvcrt
        msvcrt.setmode(sys.stdout.fileno(), os.O_BINARY)
    # Low latency streaming: record 512 frames (32ms) with 1024 blocksize
    with device.recorder(samplerate=16000, channels=2, blocksize=1024) as recorder:
        while True:
            data = recorder.record(numframes=512)
            if data.size == 0:
                continue
            mono = np.mean(data, axis=1) if data.ndim == 2 else data
            pcm = (np.clip(mono, -1.0, 1.0) * 32767).astype("<i2")
            if action == "test":
                print("samples", flush=True)
                return
            if action != "capture":
                raise ValueError("Unknown action")
            sys.stdout.buffer.write(pcm.tobytes())
            sys.stdout.buffer.flush()

if __name__ == "__main__":
    try:
        main()
    except Exception:
        # Do not expose device identifiers or captured content in logs.
        sys.exit(1)
