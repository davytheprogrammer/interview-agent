mod storage;
mod provider;
mod transcription;
mod platform;
use storage::{Session, Store};
use tauri::{Manager, State};
use std::{sync::Mutex, time::{Duration, Instant}};

#[derive(Default)]
struct MicrophonePermission(Mutex<Option<Instant>>);
#[derive(Default)]
struct CaptureProtection(Mutex<bool>);
#[derive(serde::Serialize)]
struct CaptureProtectionStatus { supported: bool, requested: bool, description: &'static str }

#[tauri::command]
fn capture_protection_status(state: State<'_, CaptureProtection>) -> CaptureProtectionStatus {
 let requested = state.0.lock().map(|value| *value).unwrap_or(false);
 if cfg!(any(target_os = "windows", target_os = "macos")) {
  CaptureProtectionStatus { supported: true, requested, description: "OS capture exclusion can be requested, but its effect depends on the OS and sharing app. Verify before sharing." }
 } else {
  CaptureProtectionStatus { supported: false, requested: false, description: "Screen-share exclusion is not supported by this app on Linux. The window can appear in a screen share." }
 }
}
#[tauri::command]
fn ensure_capture_protection(window: tauri::WebviewWindow, state: State<'_, CaptureProtection>) -> Result<(), String> {
 #[cfg(any(target_os = "windows", target_os = "macos"))]
 {
  window.set_content_protected(true).map_err(|_| "Could not request screen-capture exclusion.")?;
  *state.0.lock().map_err(|_| "Could not update screen-capture exclusion.")? = true;
  Ok(())
 }
 #[cfg(not(any(target_os = "windows", target_os = "macos")))]
 { let _ = (window, state); Err("Screen-share exclusion is not supported by this app on Linux.".into()) }
}
#[tauri::command]
fn set_capture_protection(window: tauri::WebviewWindow, state: State<'_, CaptureProtection>, enabled: bool) -> Result<(), String> {
 if window.label() != "main" { return Err("Capture protection is unavailable for this window.".into()); }
 #[cfg(any(target_os = "windows", target_os = "macos"))]
 {
  window.set_content_protected(enabled).map_err(|_| "Could not request screen-capture exclusion.")?;
  *state.0.lock().map_err(|_| "Could not update screen-capture exclusion.")? = enabled;
  Ok(())
 }
 #[cfg(not(any(target_os = "windows", target_os = "macos")))]
 { let _ = (state, enabled); Err("Screen-share exclusion is not supported by this app on Linux.".into()) }
}
impl MicrophonePermission {
 fn take_valid(&self) -> bool { self.0.lock().ok().and_then(|mut pending| pending.take()).is_some_and(|deadline| Instant::now() <= deadline) }
}

#[tauri::command]
fn resize_window(window: tauri::WebviewWindow, landscape: bool) -> Result<(), String> {
    if landscape {
        // Wide landscape mode right beneath the camera: 800px wide x 340px high
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize { width: 840.0, height: 350.0 }));
    } else {
        // Standard compact portrait companion: 440px wide x 680px high
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize { width: 440.0, height: 680.0 }));
    }
    Ok(())
}

#[tauri::command]
fn arm_microphone_test(window: tauri::WebviewWindow, permission: State<'_, MicrophonePermission>) -> Result<(), String> {
 if window.label() != "main" { return Err("Microphone access is unavailable in this window.".into()); }
 *permission.0.lock().map_err(|_| "Microphone permission unavailable.")? = Some(Instant::now() + Duration::from_secs(30));
 Ok(())
}
#[tauri::command]
fn disarm_microphone_test(permission: State<'_, MicrophonePermission>) {
 if let Ok(mut pending) = permission.0.lock() { *pending = None; }
}

#[derive(serde::Serialize)]
struct AudioAvailability { available: bool, description: String, native_microphone: bool }

#[tauri::command]
async fn capture_screen_region() -> Result<String,String> {
 #[cfg(target_os="linux")]
 {
  tauri::async_runtime::spawn_blocking(|| {
   use std::{process::{Command,Stdio},time::{SystemTime,UNIX_EPOCH}};
   let stamp=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|"System clock unavailable.")?.as_millis();
   let path=std::env::temp_dir().join(format!("live-companion-snip-{}-{stamp}.png",std::process::id()));
   let result=Command::new("import").arg(&path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|_|"Region capture needs ImageMagick's `import` utility. Install ImageMagick and try again.")?;
   if !result.success() { return Err("Screenshot selection was cancelled.".to_string()); }
   let bytes=std::fs::read(&path).map_err(|_|"Could not read the selected screenshot.")?;
   let _=std::fs::remove_file(&path);
   if bytes.is_empty() || bytes.len()>12*1024*1024 { return Err("Selected screenshot is empty or larger than 12 MB.".into()); }
   use base64::Engine;
   Ok(format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)))
  }).await.map_err(|_|"Screenshot capture failed.")?
 }
 #[cfg(not(target_os="linux"))]
 { Err("Use the image chooser to select a screenshot on this platform.".into()) }
}

#[cfg(target_os = "linux")]
fn default_monitor() -> Option<String> {
 use std::process::Command;
 let sink = Command::new("pactl").arg("get-default-sink").output().ok()?;
 let sources = Command::new("pactl").args(["list", "short", "sources"]).output().ok()?;
 if !sink.status.success() || !sources.status.success() { return None; }
 let name = String::from_utf8(sink.stdout).ok()?.trim().to_string();
 if name.is_empty() { return None; }
 let monitor = format!("{name}.monitor");
 let text = String::from_utf8(sources.stdout).ok()?;
 text.lines().any(|line| line.split('\t').nth(1) == Some(monitor.as_str())).then_some(monitor)
}

#[tauri::command]
async fn system_audio_availability(app: tauri::AppHandle) -> AudioAvailability {
 #[cfg(target_os = "linux")]
 {
  let _ = app;
  if let Some(name) = default_monitor() { return AudioAvailability { available: true, description: format!("Default output monitor: {name}"), native_microphone: true }; }
  AudioAvailability { available: false, description: "No default output monitor found. Check PipeWire/PulseAudio and your output device.".into(), native_microphone: true }
 }
 #[cfg(target_os = "windows")]
 {
  let found = tauri::async_runtime::spawn_blocking(move || platform::windows_audio_check(&app, "probe", "computer")).await.ok().is_some_and(|r| r.is_ok());
  AudioAvailability { available: found, description: if found { "Default Windows output loopback is available." } else { "No Windows output loopback found. Check your default speaker." }.into(), native_microphone: true }
 }
 #[cfg(not(any(target_os = "linux", target_os = "windows")))]
 { let _ = app; AudioAvailability { available: false, description: "System audio discovery is not implemented on this platform.".into(), native_microphone: false } }
}

#[tauri::command]
async fn test_system_audio_access(app: tauri::AppHandle) -> Result<String, String> {
 #[cfg(target_os = "linux")]
 {
  let _ = app;
  tauri::async_runtime::spawn_blocking(|| {
   use std::{process::{Command, Stdio}, thread, time::Duration};
   let monitor = default_monitor().ok_or("No computer-audio monitor found. Check your output device and PipeWire/PulseAudio.")?;
   let mut child = Command::new("parec").arg(format!("--device={monitor}")).args(["--raw", "--format=s16le", "--rate=16000", "--channels=1"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_| "Could not start the audio access check. Install the PulseAudio recording utilities and try again.")?;
   thread::sleep(Duration::from_millis(800));
   match child.try_wait() {
    Ok(Some(_)) => Err("The audio source could not be opened. Check PipeWire/PulseAudio access and your output device.".to_string()),
    Ok(None) => { child.kill().map_err(|_| "Could not stop the audio access check.")?; child.wait().map_err(|_| "Could not finish the audio access check.")?; Ok("Computer-audio source opened and stopped. No audio was saved. This does not test transcription.".to_string()) },
    Err(_) => { let _ = child.kill(); let _ = child.wait(); Err("Could not verify audio access.".to_string()) }
   }
  }).await.map_err(|_| "Audio access check failed.".to_string())?
 }
 #[cfg(target_os = "windows")]
 { tauri::async_runtime::spawn_blocking(move || platform::windows_audio_check(&app, "test", "computer")).await.map_err(|_| "Audio check failed.")??; Ok("Computer audio opened and delivered samples. Capture stopped; no audio was saved.".into()) }
 #[cfg(not(any(target_os = "linux", target_os = "windows")))]
 { let _ = app; Err("Computer-audio access checks are not implemented on this platform.".into()) }
}

#[tauri::command]
async fn test_native_microphone(app: tauri::AppHandle) -> Result<String, String> {
 #[cfg(target_os = "linux")]
 {
  let _ = app;
  tauri::async_runtime::spawn_blocking(|| {
   use std::{io::Read, process::{Command, Stdio}, sync::{Arc, atomic::{AtomicUsize, Ordering}}, thread};
   let output = Command::new("pactl").arg("get-default-source").output().map_err(|_| "Could not find the default microphone. Check PipeWire/PulseAudio.")?;
   if !output.status.success() { return Err("Could not find the default microphone. Check PipeWire/PulseAudio."); }
   let source = String::from_utf8(output.stdout).map_err(|_| "The microphone source name is invalid.")?.trim().to_string();
   if source.is_empty() || source.ends_with(".monitor") { return Err("The default input is not a microphone. Select a microphone in your system sound settings."); }
   let mut child = Command::new("parec").arg(format!("--device={source}")).args(["--raw", "--format=s16le", "--rate=16000", "--channels=1", "--latency-msec=50"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|_| "Could not open the microphone. Check PulseAudio recording utilities.")?;
   let mut stdout = child.stdout.take().ok_or("Could not read microphone audio.")?;
   let bytes = Arc::new(AtomicUsize::new(0)); let count = Arc::clone(&bytes);
   let reader = thread::spawn(move || { let mut buffer = [0u8; 4096]; while let Ok(n) = stdout.read(&mut buffer) { if n == 0 { break; } count.fetch_add(n, Ordering::Relaxed); } });
   thread::sleep(Duration::from_millis(1200));
   let exited = child.try_wait().map_err(|_| "Could not check microphone access.")?;
   if exited.is_none() { let _ = child.kill(); }
   let _ = child.wait(); let _ = reader.join();
   if bytes.load(Ordering::Relaxed) == 0 { return Err("The microphone opened but no audio data arrived. Check the input device and mute settings."); }
   Ok("Microphone opened and delivered audio data. Capture stopped and samples were discarded. Speech-to-text is not connected.")
  }).await.map_err(|_| "Microphone test failed.")?.map(str::to_string).map_err(str::to_string)
 }
 #[cfg(target_os = "windows")]
 { tauri::async_runtime::spawn_blocking(move || platform::windows_audio_check(&app, "test", "microphone")).await.map_err(|_| "Microphone check failed.")??; Ok("Microphone opened and delivered samples. Capture stopped; no audio was saved.".into()) }
 #[cfg(not(any(target_os = "linux", target_os = "windows")))]
 { let _ = app; Err("Native microphone checking is not implemented on this platform.".into()) }
}

#[tauri::command]
fn history_enabled(store: State<'_, Store>) -> Result<bool, String> { store.history_enabled() }
#[tauri::command]
fn set_history(store: State<'_, Store>, enabled: bool) -> Result<(), String> { store.set_history(enabled) }
#[tauri::command]
fn list_sessions(store: State<'_, Store>) -> Result<Vec<Session>, String> { store.list() }
#[tauri::command]
fn save_session(store: State<'_, Store>, session: Session) -> Result<(), String> { store.save(&session) }
#[tauri::command]
fn delete_session(store: State<'_, Store>, id: String) -> Result<(), String> { store.delete(&id) }
#[tauri::command]
fn delete_all_data(store: State<'_, Store>) -> Result<(), String> { store.delete_all() }

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
 tauri::Builder::default().manage(MicrophonePermission::default()).manage(CaptureProtection::default()).manage(transcription::Transcription::default())
 .on_permission_request(|webview, kind| {
  use tauri::webview::{PermissionKind, PermissionResponse};
  if kind == PermissionKind::Microphone && webview.label() == "main" {
   let permission = webview.state::<MicrophonePermission>();
   if permission.take_valid() { return PermissionResponse::Allow; }
  }
  PermissionResponse::Deny
 })
 .setup(|app| {
  #[cfg(any(target_os = "windows", target_os = "macos"))]
  if let Some(window) = app.get_webview_window("main") { if window.set_content_protected(true).is_ok() { if let Ok(mut requested) = app.state::<CaptureProtection>().0.lock() { *requested = true; } } }
  let path = app.path().app_local_data_dir()?;
  std::fs::create_dir_all(&path)?;
  app.manage(Store::open(&path.join("sessions.sqlite3")).map_err(std::io::Error::other)?);
  Ok(())
 }).invoke_handler(tauri::generate_handler![resize_window,capture_screen_region,capture_protection_status,set_capture_protection,ensure_capture_protection,transcription::start_transcription,transcription::stop_transcription,test_native_microphone,arm_microphone_test,disarm_microphone_test,provider::save_api_key,provider::has_api_key,provider::delete_api_key,provider::save_meta_api_key,provider::has_meta_api_key,provider::delete_meta_api_key,provider::save_provider_key,provider::has_provider_key,provider::set_active_provider,provider::get_active_provider,provider::analyze_image,provider::ask_nvidia,system_audio_availability,test_system_audio_access,history_enabled,set_history,list_sessions,save_session,delete_session,delete_all_data])
 .run(tauri::generate_context!()).expect("failed to run Live Companion");
}

#[cfg(test)]
mod permission_tests {
 use super::*;
 #[test] fn microphone_grant_is_one_time_and_expires() {
  let permission = MicrophonePermission::default();
  assert!(!permission.take_valid());
  *permission.0.lock().unwrap() = Some(Instant::now() + Duration::from_secs(1));
  assert!(permission.take_valid());
  assert!(!permission.take_valid());
  *permission.0.lock().unwrap() = Some(Instant::now() - Duration::from_secs(1));
  assert!(!permission.take_valid());
 }
}
