import importlib.util
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import patch

path = Path(__file__).resolve().parents[1] / "scripts/platform/windows/audio.py"
spec = importlib.util.spec_from_file_location("windows_audio", path)
audio = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audio)

class Device:
    def __init__(self, device_id, loopback):
        self.id = device_id
        self.isloopback = loopback

class WindowsAudioRoutingTests(unittest.TestCase):
    def test_computer_uses_default_speaker_loopback_not_microphone(self):
        speaker = Device("speaker-id", False)
        loopback = Device("speaker-id", True)
        def microphone(id, include_loopback=False):
            self.assertEqual(id, speaker.id)
            self.assertTrue(include_loopback)
            return loopback
        fake = types.SimpleNamespace(default_speaker=lambda: speaker, get_microphone=microphone)
        with patch.dict(sys.modules, {"soundcard": fake}):
            self.assertIs(audio.get_device("computer"), loopback)

    def test_rejects_mismatched_loopback(self):
        fake = types.SimpleNamespace(default_speaker=lambda: Device("speaker", False), get_microphone=lambda **kwargs: Device("other", True))
        with patch.dict(sys.modules, {"soundcard": fake}):
            with self.assertRaises(RuntimeError):
                audio.get_device("computer")

    def test_microphone_cannot_be_a_loopback(self):
        fake = types.SimpleNamespace(default_microphone=lambda: Device("speaker", True))
        with patch.dict(sys.modules, {"soundcard": fake}):
            with self.assertRaises(RuntimeError):
                audio.get_device("microphone")

if __name__ == "__main__":
    unittest.main()
