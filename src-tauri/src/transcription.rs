use std::{collections::VecDeque, io::{BufRead, BufReader, Write}, process::{Child, Stdio}, sync::{Arc, Mutex}, thread, time::{Duration, Instant}};
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
const QUESTION_PAUSE: Duration = Duration::from_secs(5);
const DRAFT_PAUSE: Duration = Duration::from_secs(4);
fn current_text(state: &Inner) -> String { format!("{} {}", state.group, state.partial).trim().to_owned() }
fn note_speech(state: &mut Inner, text: &str, final_segment: bool, now: Instant) -> Result<(), &'static str> {
 let text = text.trim();
 if text.is_empty() {
  if !final_segment && !state.partial.is_empty() { state.partial.clear(); state.revision = state.revision.wrapping_add(1); state.pending.clear(); if let Some(task) = state.task.take() { task.abort(); } state.busy = false; }
  return Ok(());
 }
 let before = current_text(state);
 state.last_speech = Some(now);
 if final_segment {
  if state.group.len() + text.len() + 1 > 16000 { return Err("The current question is too long to process. Stop listening and try a shorter question."); }
  if !state.group.is_empty() { state.group.push(' '); }
  state.group.push_str(text);
  state.partial.clear();
 } else if state.partial == text { return Ok(()); } else { state.partial = text.to_owned(); }
 if current_text(state) == before { return Ok(()); }
 state.revision = state.revision.wrapping_add(1);
 state.pending.clear();
 if let Some(task) = state.task.take() { task.abort(); }
 state.busy = false;
 Ok(())
}
fn draft_candidate(state: &mut Inner, now: Instant) -> Option<String> {
 if !state.last_speech.is_some_and(|last| now.saturating_duration_since(last) >= DRAFT_PAUSE) { return None; }
 let text = current_text(state);
 if text.is_empty() || text == state.last_preview { return None; }
 state.last_preview = text.clone();
 Some(text)
}
fn remember_question(state: &mut Inner, text: String) {
 if !text.is_empty() {
  state.recent.push_back(text);
  while state.recent.len() > 4 { state.recent.pop_front(); }
 }
}
fn take_finished_group(state: &mut Inner, now: Instant) -> Option<String> {
 if current_text(state).is_empty() || !state.last_speech.is_some_and(|last| now.saturating_duration_since(last) > QUESTION_PAUSE) { return None; }
 let text = current_text(state);
 state.last_speech = None;
 state.partial.clear();
 state.last_preview.clear();
 state.group.clear();
 Some(text)
}

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

#[tauri::command]
pub fn start_transcription(app: AppHandle, state: State<'_, Transcription>, mode: CaptureMode) -> Result<(), String> {
 #[cfg(not(any(target_os = "linux", target_os = "windows")))]
 { let _ = (app, state, mode); return Err("Local transcription currently supports Linux only.".into()); }
 #[cfg(any(target_os = "linux", target_os = "windows"))]
 {
  let runtime = crate::platform::speech_runtime(&app)?;
  if !runtime.python.is_file() || !runtime.worker.is_file() || !runtime.model.join("am/final.mdl").is_file() { return Err("Local speech model or runtime is missing. Reinstall or check setup instructions.".into()); }
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
  let mut child = crate::platform::command(&runtime).arg("-u").arg(&runtime.worker).arg(&runtime.model).arg(source).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|_| "Could not start local transcription.")?;
  let stdout = child.stdout.take().ok_or("Could not start transcription output.")?;
  slot.child = Some(child); slot.meeting_active = mode == CaptureMode::Computer; slot.generation = slot.generation.wrapping_add(1); slot.revision = 0; slot.pending.clear(); slot.recent.clear(); slot.group.clear(); slot.partial.clear(); slot.last_preview.clear(); slot.last_speech = None; slot.busy = false; slot.task = None;
  let generation = slot.generation;
  let shared = Arc::clone(&state.0);
  if mode == CaptureMode::Computer {
   let timer_shared = Arc::clone(&shared); let timer_app = app.clone();
   thread::spawn(move || loop {
    thread::sleep(Duration::from_millis(200));
    let mut should_schedule = false;
    if let Ok(mut state) = timer_shared.lock() {
     if !state.meeting_active || state.generation != generation { break; }
     let now = Instant::now();
     if let Some(text) = draft_candidate(&mut state, now) {
      state.pending.clear();
      match enqueue(&mut state, &text) { Ok(()) => { should_schedule = true; }, Err(error) => { let _ = timer_app.emit("meeting-suggestion", serde_json::json!({"event":"error", "text":error})); } }
     }
     if let Some(text) = take_finished_group(&mut state, now) { remember_question(&mut state, text); }
    } else { break; }
    if should_schedule { schedule_answer(Arc::clone(&timer_shared), timer_app.clone()); }
   });
  }
  thread::spawn(move || {
   let mut ended = false;
   for line in BufReader::new(stdout).lines().map_while(Result::ok) {
    if let Ok(mut event) = serde_json::from_str::<serde_json::Value>(&line) {
     let kind = event.get("event").and_then(|v| v.as_str()).unwrap_or("").to_owned();
     if kind == "ended" { ended = true; }
     let mut grouped = None;
     if mode == CaptureMode::Computer && (kind == "final" || kind == "partial" || kind == "activity") {
      let text = event.get("text").and_then(|v| v.as_str()).unwrap_or("");
      if let Ok(mut state) = shared.lock() { if state.meeting_active && state.generation == generation { if kind == "activity" { state.last_speech = Some(Instant::now()); } else if let Err(error) = note_speech(&mut state, text, kind == "final", Instant::now()) { let _ = app.emit("meeting-suggestion", serde_json::json!({"event":"error", "text":error})); } if kind == "final" && !text.trim().is_empty() { grouped = Some(state.group.clone()); } } }
     }
     if let Some(object) = event.as_object_mut() { object.insert("mode".into(), mode.label().into()); }
     let _ = app.emit("microphone-transcription", event);
     if let Some(text) = grouped { let _ = app.emit("meeting-suggestion", serde_json::json!({"event":"buffering", "text":text})); }
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
 #[test] fn privacy_stop_clears_pending_meeting_text() { let mut state = Inner { meeting_active: true, pending: VecDeque::from(["private speech".into()]), recent: VecDeque::from(["previous".into()]), busy: true, ..Default::default() }; cancel_meeting(&mut state); assert!(!state.meeting_active); assert!(!state.busy); assert!(state.pending.is_empty()); assert!(state.recent.is_empty()); }
 #[test] fn separate_questions_are_processed_in_order_with_recent_context() { let mut state = Inner::default(); enqueue(&mut state,"first question").unwrap(); let (first, context) = next_turn(&mut state).unwrap(); assert_eq!(first,"first question"); assert!(context.is_empty()); remember_question(&mut state,first); enqueue(&mut state,"second question").unwrap(); let (second, context) = next_turn(&mut state).unwrap(); assert_eq!(second,"second question"); assert_eq!(context,"first question"); assert!(next_turn(&mut state).is_none()); }
 #[test] fn queue_is_bounded_and_reports_overflow() { let mut state = Inner::default(); for _ in 0..32 { enqueue(&mut state,"question").unwrap(); } assert!(enqueue(&mut state,"extra").is_err()); assert_eq!(state.pending.len(),32); }
 #[test] fn text_limit_preserves_unicode_boundaries() { let text = "😀".repeat(1000); let bounded = bounded_text(&text,2400); assert_eq!(bounded.len(),2400); }
 #[test] fn pauses_up_to_five_seconds_do_not_split_the_question() { let start = Instant::now(); let mut state = Inner::default(); note_speech(&mut state, "first part", true, start).unwrap(); assert!(take_finished_group(&mut state, start + Duration::from_secs(5)).is_none()); note_speech(&mut state, "second part", true, start + Duration::from_secs(5)).unwrap(); assert!(take_finished_group(&mut state, start + Duration::from_secs(10)).is_none()); assert_eq!(take_finished_group(&mut state, start + Duration::from_millis(10001)).as_deref(), Some("first part second part")); }
 #[test] fn ongoing_partial_speech_extends_the_boundary_and_stop_clears_it() { let start = Instant::now(); let mut state = Inner::default(); note_speech(&mut state, "one", true, start).unwrap(); note_speech(&mut state, "next", false, start + Duration::from_secs(4)).unwrap(); assert!(take_finished_group(&mut state, start + Duration::from_secs(6)).is_none()); cancel_meeting(&mut state); assert!(state.group.is_empty()); assert!(state.last_speech.is_none()); }
 #[test] fn draft_after_four_seconds_updates_after_speech_resumes() { let start = Instant::now(); let mut state = Inner::default(); note_speech(&mut state,"tell me",true,start).unwrap(); assert!(draft_candidate(&mut state,start+Duration::from_millis(3999)).is_none()); assert_eq!(draft_candidate(&mut state,start+Duration::from_secs(4)).as_deref(),Some("tell me")); assert!(draft_candidate(&mut state,start+Duration::from_millis(4500)).is_none()); note_speech(&mut state,"about your experience",true,start+Duration::from_millis(4500)).unwrap(); assert!(take_finished_group(&mut state,start+Duration::from_secs(9)).is_none()); assert_eq!(draft_candidate(&mut state,start+Duration::from_millis(8500)).as_deref(),Some("tell me about your experience")); assert_eq!(take_finished_group(&mut state,start+Duration::from_millis(9501)).as_deref(),Some("tell me about your experience")); }
 #[test] fn finalizing_same_partial_keeps_existing_draft_valid() { let start = Instant::now(); let mut state = Inner::default(); note_speech(&mut state,"hello there",false,start).unwrap(); let revision = state.revision; assert_eq!(draft_candidate(&mut state,start+Duration::from_secs(4)).as_deref(),Some("hello there")); note_speech(&mut state,"hello there",true,start+Duration::from_secs(5)).unwrap(); assert_eq!(state.revision,revision); assert!(draft_candidate(&mut state,start+Duration::from_secs(9)).is_none()); }
 #[test] fn resumed_speech_invalidates_older_draft_and_keeps_one_question() { let start = Instant::now(); let mut state = Inner::default(); note_speech(&mut state,"why did you",true,start).unwrap(); let revision = state.revision; assert_eq!(draft_candidate(&mut state,start+Duration::from_secs(4)).as_deref(),Some("why did you")); note_speech(&mut state,"choose this role",false,start+Duration::from_millis(4500)).unwrap(); assert_ne!(state.revision, revision); assert_eq!(draft_candidate(&mut state,start+Duration::from_millis(8500)).as_deref(),Some("why did you choose this role")); assert_eq!(take_finished_group(&mut state,start+Duration::from_millis(9501)).as_deref(),Some("why did you choose this role")); }
 #[test] #[ignore = "requires saved NVIDIA key and sends two synthetic questions"] fn live_two_turns() { let mut state = Inner::default(); enqueue(&mut state,"What is two plus two?").unwrap(); enqueue(&mut state,"What is three plus three?").unwrap(); for _ in 0..2 { let (text, context) = next_turn(&mut state).unwrap(); let answer = tauri::async_runtime::block_on(ask_nvidia(meeting_question(&text),context,AskMode::Meeting)).expect("Synthetic NVIDIA request failed"); assert!(!answer.trim().is_empty()); assert!(!answer.trim().trim_end_matches('.').eq_ignore_ascii_case("NO_QUESTION")); } }
}
