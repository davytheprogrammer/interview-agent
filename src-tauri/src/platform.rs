use std::{path::PathBuf, process::Command};
use tauri::{AppHandle, Manager};

pub struct SpeechRuntime { pub python: PathBuf, pub worker: PathBuf, pub model: PathBuf, pub python_path: Option<PathBuf> }

pub fn speech_runtime(app: &AppHandle) -> Result<SpeechRuntime, String> {
 #[cfg(debug_assertions)]
 {
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().ok_or("Project path unavailable")?.to_path_buf();
  #[cfg(target_os = "linux")]
  { let _ = app; return Ok(SpeechRuntime { python: root.join(".venv/bin/python"), worker: root.join("scripts/transcribe.py"), model: root.join("models/vosk-model-small-en-us-0.15"), python_path: None }); }
  #[cfg(target_os = "windows")]
  { let _ = app; return Ok(SpeechRuntime { python: root.join("resources/windows/python/python.exe"), worker: root.join("scripts/transcribe.py"), model: root.join("models/vosk-model-small-en-us-0.15"), python_path: None }); }
 }
 #[allow(unreachable_code)]
 {
  let base = app.path().resource_dir().map_err(|_| "Application resources are unavailable")?.join("speech");
  #[cfg(target_os = "linux")]
  { return Ok(SpeechRuntime { python: PathBuf::from("/usr/bin/python3.10"), worker: base.join("scripts/transcribe.py"), model: base.join("models/vosk-model-small-en-us-0.15"), python_path: Some(base.join("python")) }); }
  #[cfg(target_os = "windows")]
  { return Ok(SpeechRuntime { python: base.join("python/python.exe"), worker: base.join("scripts/transcribe.py"), model: base.join("models/vosk-model-small-en-us-0.15"), python_path: None }); }
  #[allow(unreachable_code)]
  { let _ = base; Err("Speech transcription is not available on this platform".into()) }
 }
}

pub fn command(runtime: &SpeechRuntime) -> Command {
 let mut command = Command::new(&runtime.python);
 command.env("PYTHONNOUSERSITE", "1");
 if let Some(path) = &runtime.python_path { command.env("PYTHONPATH", path); }
 #[cfg(windows)]
 { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
 command
}

#[cfg(windows)]
pub fn windows_audio_check(app: &AppHandle, action: &str, mode: &str) -> Result<(), String> {
 use std::{process::Stdio, thread, time::{Duration, Instant}};
 let runtime = speech_runtime(app)?;
 let helper = runtime.worker.parent().ok_or("Audio helper unavailable")?.join("platform/windows/audio.py");
 if !runtime.python.is_file() || !helper.is_file() { return Err("Windows audio runtime is missing. Reinstall Live Companion.".into()); }
 let mut child = command(&runtime).arg(helper).args([action, mode]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_| "Could not start the Windows audio check")?;
 let deadline = Instant::now() + Duration::from_secs(8);
 loop {
  match child.try_wait() {
   Ok(Some(status)) if status.success() => return Ok(()),
   Ok(Some(_)) => return Err("The audio device could not be opened. Check the default device and Windows privacy settings.".into()),
   Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
   _ => { let _ = child.kill(); let _ = child.wait(); return Err("Audio check timed out. Check the default device and Windows privacy settings.".into()); }
  }
 }
}
