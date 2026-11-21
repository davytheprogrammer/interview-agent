from pathlib import Path
import json
import unittest

root = Path(__file__).resolve().parents[1]

class PackageResourceTests(unittest.TestCase):
    def test_both_packagers_include_the_speech_model_and_worker(self):
        for platform in ("linux", "windows"):
            config = json.loads((root / f"src-tauri/tauri.{platform}.conf.json").read_text())
            resources = config["bundle"]["resources"]
            self.assertEqual(resources["../resources/models/"], "speech/models/")
            self.assertEqual(resources["../scripts/transcribe.py"], "speech/scripts/transcribe.py")
        self.assertTrue((root / "resources/models/vosk-model-small-en-us-0.15/am/final.mdl").is_file())

    def test_windows_runtime_and_wasapi_helper_are_staged(self):
        runtime = root / "resources/windows/python"
        self.assertTrue((runtime / "python.exe").is_file())
        self.assertTrue((runtime / "Lib/site-packages/vosk/libvosk.dll").is_file())
        self.assertTrue((runtime / "Lib/site-packages/soundcard/mediafoundation.py").is_file())
        self.assertTrue((runtime / "srt.py").is_file())
        self.assertIn("Lib/site-packages", (runtime / "python313._pth").read_text())
        config = json.loads((root / "src-tauri/tauri.windows.conf.json").read_text())
        self.assertIn("../scripts/platform/windows/audio.py", config["bundle"]["resources"])

    def test_linux_runtime_is_staged(self):
        runtime = root / "resources/linux/site-packages"
        self.assertTrue((runtime / "vosk/libvosk.so").is_file())
        self.assertTrue(any(runtime.glob("_cffi_backend*.so")))
        self.assertTrue((runtime / "srt.py").is_file())

if __name__ == "__main__":
    unittest.main()
