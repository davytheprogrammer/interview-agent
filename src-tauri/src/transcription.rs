use std::{collections::VecDeque, io::{BufRead, BufReader, Write}, process::{Child, Stdio}, sync::{Arc, Mutex}, thread, time::Instant};
#[cfg(target_os = "linux")]
use std::process::Command;
use tauri::{AppHandle, Emitter, State};
use crate::provider::{ask_nvidia, AskMode};

#[derive(Default)]
struct Inner { child: Option<Child>, meeting_active: bool, generation: u64, revision: u64, busy: bool, pending: VecDeque<String>, recent: VecDeque<String>, group: String, partial: String, last_preview: String, last_speech: Option<Instant>, task: Option<tauri::async_runtime::JoinHandle<()>> }
#[derive(Default)]
pub struct Transcription(Arc<Mutex<Inner>>);
fn cancel_meeting(inner: &mut Inner) {
 inner.meeting_active = false;
 inner.pending.clear();
 inner.recent.clear();
 inner.group.clear();
 inner.partial.clear();
 inner.last_preview.clear();
 inner.last_speech = None;
 inner.busy = false;
 if let Some(task) = inner.task.take() { task.abort(); }
}
#[derive(serde::Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMode { Microphone, Computer }
impl CaptureMode { fn label(self) -> &'static str { if self == Self::Microphone { "microphone" } else { "computer" } } }
fn remember_question(state: &mut Inner, text: String) {
 if !text.is_empty() {
  state.recent.push_back(text);
  while state.recent.len() > 4 { state.recent.pop_front(); }
 }
}
fn take_finished_group(state: &mut Inner, text: &str) -> Option<String> { let text=text.trim(); if text.is_empty() { return None; } state.last_speech=None; state.partial.clear(); state.last_preview.clear(); state.group.clear(); Some(text.to_owned()) }

fn bounded_text(text: &str, limit: usize) -> String { let mut result = String::new(); for ch in text.trim().chars() { if result.len() + ch.len_utf8() > limit { break; } result.push(ch); } result }
fn enqueue(state: &mut Inner, text: &str) -> Result<(), &'static str> {
 let text = text.trim().to_owned();
 if text.is_empty() { return Ok(()); }
 if text.len() > 16000 { return Err("The question is too long to send for an answer."); }
 if state.pending.len() >= 32 { return Err("Too many speech segments are waiting for NVIDIA. Stop and restart if the connection remains slow."); }
 state.pending.push_back(text); Ok(())
}
fn next_turn(state: &mut Inner) -> Option<(String, String)> {
 let text = state.pending.pop_front()?;
 let context = bounded_text(&state.recent.iter().cloned().collect::<Vec<_>>().join("\n"), 6000);
 Some((text, context))
}
fn meeting_question(text: &str) -> String { format!("This is automatically transcribed meeting speech and may lack punctuation or contain errors. If it is a question, an interview prompt, or plausibly requests a response, provide a brief useful draft answer. Only respond exactly NO_QUESTION when it clearly does not invite a response. Do not invent personal details. Latest speech: {text}") }

fn schedule_answer(shared: Arc<Mutex<Inner>>, app: AppHandle) {
 let Ok(mut state) = shared.lock() else { return; };
 if !state.meeting_active || state.busy || state.pending.is_empty() { return; }
 let Some((text, context)) = next_turn(&mut state) else { return; };
 let generation = state.generation;
 let revision = state.revision;
 state.busy = true;
 let _ = app.emit("meeting-suggestion", serde_json::json!({"event":"processing", "text":""}));
 let next_shared = Arc::clone(&shared); let next_app = app.clone();
 let task = tauri::async_runtime::spawn(async move {
  let question = meeting_question(&text);
  let result = ask_nvidia(question, context, AskMode::Meeting).await;
  let mut reschedule = false;
  if let Ok(mut state) = next_shared.lock() {
   if state.meeting_active && state.generation == generation && state.revision == revision {
    state.busy = false;
    match result {
     Ok(answer) if !answer.trim().trim_end_matches('.').eq_ignore_ascii_case("NO_QUESTION") => { let _ = next_app.emit("meeting-suggestion", serde_json::json!({"event":"answer", "text":answer, "utterance":text})); },
     Ok(_) => { let _ = next_app.emit("meeting-suggestion", serde_json::json!({"event":"no_question", "text":""})); },
     Err(error) => { let _ = next_app.emit("meeting-suggestion", serde_json::json!({"event":"error", "text":error})); }
    }
    reschedule = !state.pending.is_empty();
   }
  }
  if reschedule { schedule_answer(next_shared, next_app); }
 });
 state.task = Some(task);
}
fn state_schedule_after_turn(shared:&Arc<Mutex<Inner>>,app:&AppHandle) { let mut should_schedule=false; if let Ok(mut state)=shared.lock() { if state.meeting_active { if let Some(text)=state.recent.back().cloned() { match enqueue(&mut state,&text) { Ok(())=>should_schedule=true, Err(error)=>{let _=app.emit("meeting-suggestion",serde_json::json!({"event":"error","text":error}));} } } } } if should_schedule { schedule_answer(Arc::clone(shared),app.clone()); } }

#[tauri::command]
pub fn start_transcription(app: AppHandle, state: State<'_, Transcription>, mode: CaptureMode) -> Result<(), String> {
 #[cfg(not(any(target_os = "linux", target_os = "windows")))]
 { let _ = (app, state, mode); return Err("Local transcription currently supports Linux only.".into()); }
 #[cfg(any(target_os = "linux", target_os = "windows"))]
 {
  let runtime = crate::platform::speech_runtime(&app)?;
  if !runtime.python.is_file() || !runtime.worker.is_file() { return Err("Speech runtime is missing. Reinstall or check setup instructions.".into()); }
  let meta_key=crate::provider::meta_api_key()?;
  if mode == CaptureMode::Computer && !crate::provider::has_api_key()? { return Err("Add your NVIDIA key in settings before starting meeting suggestions.".into()); }
  let mut slot = state.0.lock().map_err(|_| "Transcription is unavailable.")?;
  if slot.child.is_some() { return Err("Audio capture is already active or stopping.".into()); }
  #[cfg(target_os = "linux")]
  let source = if mode == CaptureMode::Computer { crate::default_monitor().ok_or("No computer-audio monitor found.")? } else {
   let output = Command::new("pactl").arg("get-default-source").output().map_err(|_| "Could not find the default microphone.")?;
   let source = String::from_utf8(output.stdout).map_err(|_| "Invalid microphone source.")?.trim().to_owned();
   if !output.status.success() || source.is_empty() || source.ends_with(".monitor") { return Err("Select a microphone in system sound settings first.".into()); } source
  };
  #[cfg(target_os = "windows")]
  let source = mode.label().to_owned();
  let mut worker=crate::platform::command(&runtime); worker.arg("-u").arg(&runtime.worker).arg(source).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).env("META_MODEL_API_KEY",meta_key);
  let mut child = worker.spawn().map_err(|_| "Could not start Muse Voice Transcribe.")?;
  let stdout = child.stdout.take().ok_or("Could not start transcription output.")?;
  slot.child = Some(child); slot.meeting_active = mode == CaptureMode::Computer; slot.generation = slot.generation.wrapping_add(1); slot.revision = 0; slot.pending.clear(); slot.recent.clear(); slot.group.clear(); slot.partial.clear(); slot.last_preview.clear(); slot.last_speech = None; slot.busy = false; slot.task = None;
  let generation = slot.generation;
  let shared = Arc::clone(&state.0);
  thread::spawn(move || {
   let mut ended = false;
   for line in BufReader::new(stdout).lines().map_while(Result::ok) {
    if let Ok(mut event) = serde_json::from_str::<serde_json::Value>(&line) {
     let kind = event.get("event").and_then(|v| v.as_str()).unwrap_or("").to_owned();
     if kind == "ended" { ended = true; }
     let mut grouped = None;
     if mode == CaptureMode::Computer && (kind == "turn_complete" || kind == "partial" || kind == "activity") {
      let text = event.get("text").and_then(|v| v.as_str()).unwrap_or("");
      if let Ok(mut state) = shared.lock() { if state.meeting_active && state.generation == generation { if kind == "partial" { state.partial=text.to_owned(); } else if kind == "turn_complete" { grouped=take_finished_group(&mut state,text); if let Some(question)=grouped.clone() { remember_question(&mut state,question); state.revision=state.revision.wrapping_add(1); } } } }
     }
     if let Some(object) = event.as_object_mut() { object.insert("mode".into(), mode.label().into()); }
     let _ = app.emit("microphone-transcription", event);
     if let Some(text) = grouped { let _ = app.emit("meeting-suggestion", serde_json::json!({"event":"buffering", "text":text})); state_schedule_after_turn(&shared,&app); }
    }
   }
   if let Ok(mut guard) = shared.lock() { cancel_meeting(&mut guard); if let Some(mut process) = guard.child.take() { let _ = process.wait(); } }
   if !ended { let _ = app.emit("microphone-transcription", serde_json::json!({"event":"error","text":"Audio capture stopped unexpectedly.","mode":mode.label()})); let _ = app.emit("microphone-transcription", serde_json::json!({"event":"ended","text":"","mode":mode.label()})); }
  });
  Ok(())
 }
}

#[tauri::command]
pub fn stop_transcription(state: State<'_, Transcription>) -> Result<(), String> {
 let mut slot = state.0.lock().map_err(|_| "Transcription is unavailable.")?;
 cancel_meeting(&mut slot);
 if let Some(child) = slot.child.as_mut() { if let Some(input) = child.stdin.as_mut() { input.write_all(b"stop\n").and_then(|_| input.flush()).map_err(|_| "Could not stop audio capture.")?; } }
 Ok(())
}
impl Drop for Transcription { fn drop(&mut self) { if let Ok(mut slot) = self.0.lock() { slot.meeting_active = false; if let Some(task) = slot.task.take() { task.abort(); } if let Some(child) = slot.child.as_mut() { if let Some(mut input) = child.stdin.take() { let _ = input.write_all(b"stop\n"); } } } } }

#[cfg(test)] mod tests {
 use super::*;
 #[test] fn completed_turn_is_used_as_the_boundary_without_silence_timers() { let mut state=Inner::default(); assert_eq!(take_finished_group(&mut state,"Recruiter question."),Some("Recruiter question.".into())); }
 #[test] fn empty_completed_turn_is_ignored() { let mut state=Inner::default(); assert!(take_finished_group(&mut state,"  ").is_none()); }
 #[test] fn privacy_stop_clears_pending_meeting_text() { let mut state=Inner { meeting_active:true, pending:VecDeque::from(["private speech".into()]), recent:VecDeque::from(["previous".into()]), busy:true, ..Default::default() }; cancel_meeting(&mut state); assert!(!state.meeting_active); assert!(!state.busy); assert!(state.pending.is_empty()); assert!(state.recent.is_empty()); }
 #[test] fn context_tracks_previous_completed_turns() { let mut state=Inner::default(); remember_question(&mut state,"first question".into()); enqueue(&mut state,"second question").unwrap(); let (question,context)=next_turn(&mut state).unwrap(); assert_eq!(question,"second question"); assert_eq!(context,"first question"); }
 #[test] fn provider_errors_are_not_sourced_from_transcript_payloads() { assert!(meeting_question("what happened?").contains("Latest speech")); }
}
