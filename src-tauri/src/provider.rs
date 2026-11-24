use serde::Deserialize;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

const SERVICE: &str = "com.raxley.livecompanion";
const NVIDIA_ACCOUNT: &str = "nvidia-api-key";
const META_ACCOUNT: &str = "meta-model-api-key";
const GEMINI_ACCOUNT: &str = "gemini-api-key";
const TOGETHER_ACCOUNT: &str = "together-api-key";
const RAX_ACCOUNT: &str = "rax-api-key";
const ACTIVE_PROVIDER_ACCOUNT: &str = "active-ai-provider";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DialogueTurn {
    pub role: String,
    pub content: String,
    pub timestamp_ms: u64,
}

static SESSION_MEMORY: LazyLock<Mutex<VecDeque<DialogueTurn>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));

pub fn add_session_dialogue(role: &str, content: &str) {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return;
    }
    // Cap individual turn at 4000 characters to prevent runaway token explosion
    let capped = if trimmed.len() > 4000 {
        &trimmed[..4000]
    } else {
        trimmed
    };

    let mut mem = SESSION_MEMORY.lock().unwrap_or_else(|p| p.into_inner());
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    mem.push_back(DialogueTurn {
        role: role.to_string(),
        content: capped.to_string(),
        timestamp_ms: stamp,
    });
    while mem.len() > 10 {
        mem.pop_front();
    }
}

pub fn format_session_context() -> String {
    let mem = SESSION_MEMORY.lock().unwrap_or_else(|p| p.into_inner());
    if mem.is_empty() {
        return String::new();
    }
    let turns: Vec<String> = mem
        .iter()
        .map(|t| format!("{}: {}", t.role, t.content))
        .collect();
    format!("\n\n[Recent Interview History & Dialogue Memory]:\n{}", turns.join("\n\n"))
}

#[tauri::command]
pub fn clear_session_memory() -> Result<(), String> {
    let mut mem = SESSION_MEMORY.lock().unwrap_or_else(|p| p.into_inner());
    mem.clear();
    Ok(())
}

pub fn status_error(code: u16) -> String {
    match code {
        401 | 403 => "Your API key was rejected. Update it in settings.".into(),
        429 => "The AI provider's rate limit was reached. Wait a moment and try again.".into(),
        503 => "The AI service is temporarily unavailable. Try again shortly.".into(),
        _ => format!("The AI service returned an error ({code})."),
    }
}

fn get_entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|_| "Secure credential storage is unavailable.".into())
}

#[tauri::command]
pub fn save_provider_key(provider: String, key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() || key.len() > 4096 || key.chars().any(char::is_control) {
        return Err("Enter a valid API key.".into());
    }
    let account = match provider.as_str() {
        "gemini" => GEMINI_ACCOUNT,
        "together" => TOGETHER_ACCOUNT,
        "rax" => RAX_ACCOUNT,
        "meta" => META_ACCOUNT,
        _ => NVIDIA_ACCOUNT,
    };
    get_entry(account)?.set_password(key).map_err(|_| "Could not save the key in OS credential store.".into())
}

#[tauri::command]
pub fn has_provider_key(provider: String) -> Result<bool, String> {
    let account = match provider.as_str() {
        "gemini" => GEMINI_ACCOUNT,
        "together" => TOGETHER_ACCOUNT,
        "rax" => RAX_ACCOUNT,
        "meta" => META_ACCOUNT,
        _ => NVIDIA_ACCOUNT,
    };
    match get_entry(account)?.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(_) => Err("Could not access OS credential store.".into()),
    }
}

#[tauri::command]
pub fn set_active_provider(provider: String) -> Result<(), String> {
    get_entry(ACTIVE_PROVIDER_ACCOUNT)?.set_password(&provider).map_err(|_| "Could not save active provider.".into())
}

#[tauri::command]
pub fn get_active_provider() -> Result<String, String> {
    match get_entry(ACTIVE_PROVIDER_ACCOUNT)?.get_password() {
        Ok(p) => Ok(p),
        Err(_) => Ok("gemini".into()),
    }
}

// Meta credentials compatibility
#[tauri::command]
pub fn save_meta_api_key(key: String) -> Result<(), String> { save_provider_key("meta".into(), key) }
#[tauri::command]
pub fn has_meta_api_key() -> Result<bool, String> { has_provider_key("meta".into()) }
#[tauri::command]
pub fn delete_meta_api_key() -> Result<(), String> { let _ = get_entry(META_ACCOUNT)?.delete_credential(); Ok(()) }
pub fn meta_api_key() -> Result<String, String> { get_entry(META_ACCOUNT)?.get_password().map_err(|_| "Save your Meta API key in Settings first.".into()) }

// NVIDIA credentials compatibility
#[tauri::command]
pub fn save_api_key(key: String) -> Result<(), String> { save_provider_key("nvidia".into(), key) }
#[tauri::command]
pub fn has_api_key() -> Result<bool, String> { has_provider_key("nvidia".into()) }
#[tauri::command]
pub fn delete_api_key() -> Result<(), String> { let _ = get_entry(NVIDIA_ACCOUNT)?.delete_credential(); Ok(()) }

#[derive(Deserialize)]
struct ResponsesReply {
    output: Option<Vec<ResponsesOutput>>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ResponsesOutput {
    content: Option<Vec<ResponsesContent>>,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ResponsesContent {
    text: Option<String>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
}

#[derive(Deserialize)]
struct GeminiContent {
    parts: Option<Vec<GeminiPart>>,
}

#[derive(Deserialize)]
struct GeminiPart {
    text: Option<String>,
}

const VISUAL_SYSTEM_PROMPT: &str = "\
You are an elite Staff-level technical interview co-pilot supporting a candidate in a live technical job interview. \
The user has captured their screen silently for your instant visual analysis. \
Understand the exact interview problem and deliver an authoritative, high-precision technical solution:
- Coding / LeetCode challenge: State the exact optimal algorithmic pattern (e.g. two pointers, monotonic stack, dynamic programming), rigorous Big-O Time & Space complexity (e.g. O(N) time, O(1) auxiliary space), followed by clean idiomatic production code with all edge cases handled, and a crisp 2-3 sentence technical rationale.
- Architecture / System Design diagram: State exact architectural components, data flow paths, specific consistency models (strong vs eventual), concrete protocols (gRPC, WebSocket), database access patterns, indexing strategies, and quantifiable latency/throughput trade-offs in 2-4 clear, articulate bullet points.
- Debugging & Stack Traces: Pinpoint the exact root cause in runtime internals, memory layout, or logic, cite the exact line/mechanism, and provide the concise fix.
- Technical Precision Standard: Never give hand-wavy or generic advice. Name specific diagnostics, internal tables, and exact mechanisms (e.g., in databases, operating systems, compilers, runtimes).
- Follow-ups & Refinements: When asked to refine, optimize, explain, or alter a previous answer, respond with the authoritative confidence and technical fluency of a senior tech lead. Zero filler, zero hedging.";

async fn try_gemini_vision(
    client: &reqwest::Client,
    key: &str,
    prompt: &str,
    mime: &str,
    raw_b64: &str,
) -> Result<String, String> {
    let models = ["gemini-flash-lite-latest", "gemini-2.5-flash", "gemini-3.8-flash"];
    let mut last_err = String::new();

    for model in models {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={}",
            key.trim()
        );
        let res = client
            .post(&url)
            .json(&json!({
                "contents": [{
                    "parts": [
                        {"text": prompt},
                        {"inline_data": {"mime_type": mime, "data": raw_b64}}
                    ]
                }],
                "generationConfig": {"temperature": 0.2, "maxOutputTokens": 2048}
            }))
            .send()
            .await;

        match res {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    if let Ok(body) = response.json::<GeminiResponse>().await {
                        if let Some(text) = body
                            .candidates
                            .unwrap_or_default()
                            .into_iter()
                            .flat_map(|c| c.content)
                            .flat_map(|co| co.parts.unwrap_or_default())
                            .filter_map(|p| p.text)
                            .find(|t| !t.trim().is_empty())
                        {
                            return Ok(text);
                        }
                    }
                } else {
                    let err_body = response.text().await.unwrap_or_default();
                    last_err = format!("Gemini ({model}) {status}: {err_body}");
                    if status.as_u16() == 429 || status.as_u16() == 404 {
                        continue;
                    }
                }
            }
            Err(e) => {
                last_err = format!("Gemini network error: {e}");
            }
        }
    }

    Err(last_err)
}

async fn try_meta_vision(
    client: &reqwest::Client,
    key: &str,
    prompt: &str,
    image_data_url: &str,
) -> Result<String, String> {
    let res = client
        .post("https://api.meta.ai/v1/responses")
        .bearer_auth(key.trim())
        .json(&json!({
            "model": "muse-spark-1.3-contributor",
            "max_output_tokens": 2048,
            "reasoning": {"effort": "low"},
            "input": [{
                "type": "message",
                "role": "user",
                "content": [
                    {"type": "input_text", "text": prompt},
                    {"type": "input_image", "image_url": image_data_url}
                ]
            }]
        }))
        .send()
        .await;

    match res {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                if let Ok(body) = response.json::<ResponsesReply>().await {
                    for item in body.output.unwrap_or_default() {
                        if let Some(content_vec) = item.content {
                            for c in content_vec {
                                if let Some(t) = c.text {
                                    if !t.trim().is_empty() {
                                        return Ok(t);
                                    }
                                }
                            }
                        }
                        if let Some(t) = item.text {
                            if !t.trim().is_empty() {
                                return Ok(t);
                            }
                        }
                    }
                    Err("Meta returned an empty response body.".into())
                } else {
                    Err("Could not parse Meta API response format.".into())
                }
            } else {
                let err_text = response.text().await.unwrap_or_default();
                Err(format!("Meta API {status}: {err_text}"))
            }
        }
        Err(e) => {
            if e.is_timeout() {
                Err("Meta visual analysis timed out.".into())
            } else {
                Err(format!("Could not reach Meta API: {e}"))
            }
        }
    }
}

async fn try_meta_chat(
    client: &reqwest::Client,
    key: &str,
    system_prompt: &str,
    full_context: &str,
    question: &str,
) -> Result<String, String> {
    let prompt = format!("{}\n\nContext:\n{}\n\nInterviewer Question:\n{}", system_prompt, full_context.trim(), question);
    let res = client.post("https://api.meta.ai/v1/responses").bearer_auth(key.trim()).json(&json!({
        "model": "muse-spark-1.3-contributor",
        "max_output_tokens": 2048,
        "reasoning": {"effort": "low"},
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": prompt}]
        }]
    })).send().await;

    match res {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(body) = response.json::<ResponsesReply>().await {
                    for item in body.output.unwrap_or_default() {
                        if let Some(content_vec) = item.content {
                            for c in content_vec {
                                if let Some(t) = c.text {
                                    if !t.trim().is_empty() {
                                        return Ok(t);
                                    }
                                }
                            }
                        }
                        if let Some(t) = item.text {
                            if !t.trim().is_empty() {
                                return Ok(t);
                            }
                        }
                    }
                    Err("Meta returned an empty chat response.".into())
                } else {
                    Err("Could not parse Meta chat response.".into())
                }
            } else {
                let status = response.status();
                let err_text = response.text().await.unwrap_or_default();
                Err(format!("Meta status {}: {err_text}", status))
            }
        }
        Err(e) => Err(format!("Meta network error: {e}")),
    }
}

#[tauri::command]
pub async fn analyze_image(question: String, image_data_url: String) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() || question.len() > 8000 {
        return Err("Enter a short question about this screen.".into());
    }
    if image_data_url.len() > 18_000_000
        || !(image_data_url.starts_with("data:image/png;base64,")
            || image_data_url.starts_with("data:image/jpeg;base64,")
            || image_data_url.starts_with("data:image/webp;base64,"))
    {
        return Err("Invalid image data captured.".into());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(35))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Could not create network connection.".to_string())?;

    let active = get_active_provider().unwrap_or_else(|_| "gemini".into());
    let session_context = format_session_context();

    let visual_prompt = format!(
        "{VISUAL_SYSTEM_PROMPT}{session_context}\n\n\
        Screen Analysis Request / Interview Question: {question}"
    );

    let (mime, raw_b64) = if let Some(s) = image_data_url.strip_prefix("data:image/jpeg;base64,") {
        ("image/jpeg", s)
    } else if let Some(s) = image_data_url.strip_prefix("data:image/png;base64,") {
        ("image/png", s)
    } else if let Some(s) = image_data_url.strip_prefix("data:image/webp;base64,") {
        ("image/webp", s)
    } else {
        ("image/jpeg", image_data_url.as_str())
    };

    let gemini_key = get_entry(GEMINI_ACCOUNT)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|k| !k.trim().is_empty());
    let meta_key = get_entry(META_ACCOUNT)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|k| !k.trim().is_empty());

    if gemini_key.is_none() && meta_key.is_none() {
        return Err("Save your Gemini or Meta API key in Settings to analyze your screen.".into());
    }

    let mut errors: Vec<String> = Vec::new();
    // Prioritize Meta Muse first for vision whenever active is meta or any other provider,
    // since user has abundant credits on Meta Muse and it is a stronger reasoning model.
    let try_meta_first = active != "gemini";

    if try_meta_first {
        if let Some(ref m_key) = meta_key {
            match try_meta_vision(&client, m_key, &visual_prompt, &image_data_url).await {
                Ok(text) => {
                    add_session_dialogue("Interviewer / Screen Context", question);
                    add_session_dialogue("Candidate Solution", &text);
                    return Ok(text);
                }
                Err(e) => errors.push(format!("Meta: {e}")),
            }
        }
        if let Some(ref g_key) = gemini_key {
            match try_gemini_vision(&client, g_key, &visual_prompt, mime, raw_b64).await {
                Ok(text) => {
                    add_session_dialogue("Interviewer / Screen Context", question);
                    add_session_dialogue("Candidate Solution", &text);
                    return Ok(text);
                }
                Err(e) => errors.push(format!("Gemini fallback: {e}")),
            }
        }
    } else {
        if let Some(ref g_key) = gemini_key {
            match try_gemini_vision(&client, g_key, &visual_prompt, mime, raw_b64).await {
                Ok(text) => {
                    add_session_dialogue("Interviewer / Screen Context", question);
                    add_session_dialogue("Candidate Solution", &text);
                    return Ok(text);
                }
                Err(e) => errors.push(format!("Gemini: {e}")),
            }
        }
        if let Some(ref m_key) = meta_key {
            match try_meta_vision(&client, m_key, &visual_prompt, &image_data_url).await {
                Ok(text) => {
                    add_session_dialogue("Interviewer / Screen Context", question);
                    add_session_dialogue("Candidate Solution", &text);
                    return Ok(text);
                }
                Err(e) => errors.push(format!("Meta fallback: {e}")),
            }
        }
    }

    Err(format!("Visual analysis failed. {}", errors.join(" | ")))
}

#[derive(Deserialize)] struct Completion { choices: Vec<Choice> }
#[derive(Deserialize)] struct Choice { message: Message }
#[derive(Deserialize)] struct Message { content: Option<String> }
#[derive(Deserialize)] #[serde(rename_all="lowercase")] pub enum AskMode { Meeting, Coding }

const MEETING_SYSTEM_PROMPT: &str = "\
You are an expert spoken interview co-pilot whispering real-time speech drafts to a candidate during a live interview. \
The candidate must speak your draft aloud naturally to the interviewer right now.

SPOKEN DELIVERY RULES:
1. Sound Natural and Conversational (Like a Real Engineer, Not an LLM or Textbook):
   - Speak in the first person ('I typically...', 'In my experience...', 'My approach here is...').
   - Use natural conversational flow. A real senior engineer explains concepts clearly and casually, with confidence and poise.
   - Do NOT sound like an encyclopedia dumping raw specs unprompted. If the interviewer asks 'What's the difference between a list and a tuple?', answer with clean clarity first ('Tuples are immutable and fixed-size, while lists are dynamic mutable arrays...'). Only dive into deep internals (like CPython dense entry arrays or hash caching) if the question specifically asks for internals, performance optimizations, or how it works under the hood.

2. Match the Question's Depth:
   - High-level question? Give a clear, articulate, high-level answer with the right core insight.
   - Deep/system-level question? Give the exact diagnostic, metric, or internal structure cleanly without rambling.
   - Never confuse mutability or core concepts: always get fundamentals 100% correct on the first sentence before adding nuance.

3. Spoken Brevity (2 to 3 Natural Sentences):
   - Keep answers between 2 and 3 fluid sentences (~15-25 seconds of speaking time).
   - Easy to read off a teleprompter and speak aloud without tripping over words.
   - Absolutely no preamble, throat-clearing ('Sure!', 'Great question'), or echoing the prompt.";

const CODING_SYSTEM_PROMPT: &str = "\
You are an expert coding interview co-pilot assisting a candidate. Provide a clean, mathematically optimal solution with correct syntax, precise Big-O time and space complexity, comprehensive edge-case handling (null, empty, negative, overflow), and a natural 2-sentence walk-through of the key insight.";

#[tauri::command]
pub async fn ask_nvidia(question: String, context: String, mode: AskMode) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() || question.len() > 20000 || context.len() > 16000 {
        return Err("Enter a question of at most 20,000 bytes and context of at most 16,000 bytes.".into());
    }

    let active = get_active_provider().unwrap_or_else(|_| "gemini".into());
    let timeout_secs = match mode {
        AskMode::Meeting => 8,
        AskMode::Coding => 15,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Could not create network client.".to_string())?;

    let system_prompt = match mode {
        AskMode::Meeting => MEETING_SYSTEM_PROMPT,
        AskMode::Coding => CODING_SYSTEM_PROMPT,
    };

    let session_context = format_session_context();
    let full_context = if context.trim().is_empty() {
        session_context
    } else {
        format!("{}\n{}", context.trim(), session_context)
    };

    // 1. GEMINI (Fastest, ultra-low latency)
    if active == "gemini" {
        if let Ok(entry) = get_entry(GEMINI_ACCOUNT) {
            if let Ok(key) = entry.get_password() {
                if !key.trim().is_empty() {
                    let prompt = format!("{}\n\nContext:\n{}\n\nInterviewer Question:\n{}", system_prompt, full_context.trim(), question);
                    let models = ["gemini-flash-lite-latest", "gemini-2.5-flash", "gemini-3.8-flash"];
                    for model in models {
                        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={}", key.trim());
                        let res = client.post(&url).json(&json!({
                            "contents": [{"parts": [{"text": prompt}]}],
                            "generationConfig": {"temperature": 0.4, "maxOutputTokens": 1024}
                        })).send().await;

                        if let Ok(response) = res {
                            if response.status().is_success() {
                                if let Ok(body) = response.json::<GeminiResponse>().await {
                                    if let Some(text) = body.candidates.unwrap_or_default().into_iter().flat_map(|c| c.content).flat_map(|co| co.parts.unwrap_or_default()).filter_map(|p| p.text).find(|t| !t.trim().is_empty()) {
                                        add_session_dialogue("Interviewer", question);
                                        add_session_dialogue("Candidate", &text);
                                        return Ok(text);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. TOGETHER AI (arize-ai/qwen-2-1.5b-instruct)
    if active == "together" {
        if let Ok(entry) = get_entry(TOGETHER_ACCOUNT) {
            if let Ok(key) = entry.get_password() {
                if !key.trim().is_empty() {
                    let res = client.post("https://api.together.xyz/v1/chat/completions").bearer_auth(key.trim()).json(&json!({
                        "model": "arize-ai/qwen-2-1.5b-instruct",
                        "messages": [
                            {"role": "system", "content": system_prompt},
                            {"role": "user", "content": format!("Context:\n{}\n\nInterviewer Question:\n{}", full_context.trim(), question)}
                        ],
                        "temperature": 0.3,
                        "max_tokens": 1024
                    })).send().await;

                    if let Ok(response) = res {
                        if response.status().is_success() {
                            if let Ok(body) = response.json::<Completion>().await {
                                if let Some(text) = body.choices.into_iter().next().and_then(|c| c.message.content).filter(|s| !s.trim().is_empty()) {
                                    add_session_dialogue("Interviewer", question);
                                    add_session_dialogue("Candidate", &text);
                                    return Ok(text);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. RAX AI (Official Endpoint & rax-4.0 model)
    if active == "rax" {
        if let Ok(entry) = get_entry(RAX_ACCOUNT) {
            if let Ok(key) = entry.get_password() {
                if !key.trim().is_empty() {
                    let res = client.post("https://www.ai.raxcore.dev/api/v1/chat/completions").bearer_auth(key.trim()).json(&json!({
                        "model": "rax-4.0",
                        "messages": [
                            {"role": "system", "content": system_prompt},
                            {"role": "user", "content": format!("Context:\n{}\n\nInterviewer Question:\n{}", full_context.trim(), question)}
                        ],
                        "temperature": 0.3,
                        "max_tokens": 1024
                    })).send().await;

                    if let Ok(response) = res {
                        if response.status().is_success() {
                            if let Ok(body) = response.json::<Completion>().await {
                                if let Some(text) = body.choices.into_iter().next().and_then(|c| c.message.content).filter(|s| !s.trim().is_empty()) {
                                    add_session_dialogue("Interviewer", question);
                                    add_session_dialogue("Candidate", &text);
                                    return Ok(text);
                                }
                            }
                        } else {
                            let err_text = response.text().await.unwrap_or_default();
                            eprintln!("Rax AI error: {}", err_text);
                        }
                    }
                }
            }
        }
    }

    // 4. META MUSE SPARK (Primary if active == "meta")
    if active == "meta" {
        if let Ok(entry) = get_entry(META_ACCOUNT) {
            if let Ok(key) = entry.get_password() {
                if !key.trim().is_empty() {
                    if let Ok(text) = try_meta_chat(&client, &key, system_prompt, &full_context, question).await {
                        add_session_dialogue("Interviewer", question);
                        add_session_dialogue("Candidate", &text);
                        return Ok(text);
                    }
                }
            }
        }
    }

    // 5. NVIDIA
    if active == "nvidia" {
        if let Ok(entry) = get_entry(NVIDIA_ACCOUNT) {
            if let Ok(key) = entry.get_password() {
                if !key.trim().is_empty() {
                    let res = client.post("https://integrate.api.nvidia.com/v1/chat/completions").bearer_auth(key.trim()).json(&json!({
                        "model": "poolside/laguna-xs-2.1",
                        "messages": [
                            {"role": "system", "content": system_prompt},
                            {"role": "user", "content": format!("Context:\n{}\n\nInterviewer Question:\n{}", full_context.trim(), question)}
                        ],
                        "temperature": 0.3,
                        "max_tokens": 1024
                    })).send().await;

                    if let Ok(response) = res {
                        if response.status().is_success() {
                            if let Ok(body) = response.json::<Completion>().await {
                                if let Some(text) = body.choices.into_iter().next().and_then(|c| c.message.content).filter(|s| !s.trim().is_empty()) {
                                    add_session_dialogue("Interviewer", question);
                                    add_session_dialogue("Candidate", &text);
                                    return Ok(text);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 6. AUTOMATED FALLBACK TO META MUSE SPARK (If primary provider failed)
    if active != "meta" {
        if let Ok(entry) = get_entry(META_ACCOUNT) {
            if let Ok(meta_key) = entry.get_password() {
                if !meta_key.trim().is_empty() {
                    if let Ok(text) = try_meta_chat(&client, &meta_key, system_prompt, &full_context, question).await {
                        add_session_dialogue("Interviewer", question);
                        add_session_dialogue("Candidate", &text);
                        return Ok(text);
                    }
                }
            }
        }
    }

    // 7. FINAL FALLBACK TO GEMINI (If Meta was active and failed)
    if active == "meta" {
        if let Ok(entry) = get_entry(GEMINI_ACCOUNT) {
            if let Ok(key) = entry.get_password() {
                if !key.trim().is_empty() {
                    let prompt = format!("{}\n\nContext:\n{}\n\nInterviewer Question:\n{}", system_prompt, full_context.trim(), question);
                    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-flash-lite-latest:generateContent?key={}", key.trim());
                    if let Ok(res) = client.post(&url).json(&json!({
                        "contents": [{"parts": [{"text": prompt}]}],
                        "generationConfig": {"temperature": 0.4, "maxOutputTokens": 1024}
                    })).send().await {
                        if res.status().is_success() {
                            if let Ok(body) = res.json::<GeminiResponse>().await {
                                if let Some(text) = body.candidates.unwrap_or_default().into_iter().flat_map(|c| c.content).flat_map(|co| co.parts.unwrap_or_default()).filter_map(|p| p.text).find(|t| !t.trim().is_empty()) {
                                    add_session_dialogue("Interviewer", question);
                                    add_session_dialogue("Candidate", &text);
                                    return Ok(text);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err(format!("Selected provider ({}) failed or key not configured in Settings.", active))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_errors_are_clear_and_do_not_include_payloads() {
        assert!(status_error(401).contains("key"));
        assert!(status_error(429).contains("rate limit"));
        assert!(status_error(503).contains("temporarily unavailable"));
    }

    #[test]
    fn validates_assistant_mode() {
        assert!(matches!(serde_json::from_str::<AskMode>("\"meeting\""), Ok(AskMode::Meeting)));
        assert!(matches!(serde_json::from_str::<AskMode>("\"coding\""), Ok(AskMode::Coding)));
        assert!(serde_json::from_str::<AskMode>("\"other\"").is_err());
    }

    #[test]
    fn session_dialogue_records_and_formats_memory() {
        clear_session_memory().unwrap();
        assert_eq!(format_session_context(), "");

        add_session_dialogue("Interviewer", "How do you handle state management?");
        add_session_dialogue("Candidate", "I generally lean towards Zustand for minimal boilerplate.");

        let context = format_session_context();
        assert!(context.contains("Recent Interview History & Dialogue Memory"));
        assert!(context.contains("Interviewer: How do you handle state management?"));
        assert!(context.contains("Candidate: I generally lean towards Zustand for minimal boilerplate."));
    }

    #[test]
    fn session_dialogue_caps_at_ten_turns() {
        clear_session_memory().unwrap();
        for i in 1..=15 {
            add_session_dialogue("Turn", &format!("Question {i}"));
        }
        let context = format_session_context();
        assert!(!context.contains("Question 1\n"));
        assert!(context.contains("Question 15"));
    }

    #[test]
    fn live_gemini_vision_works() {
        let tiny_png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";
        let res = tauri::async_runtime::block_on(analyze_image("What color is this tiny pixel?".into(), tiny_png.into()));
        assert!(res.is_ok(), "Vision test failed: {}", res.err().unwrap_or_default());
    }

    #[test]
    #[ignore = "requires a saved NVIDIA key and sends a synthetic prompt"]
    fn live_connection() {
        let result = tauri::async_runtime::block_on(ask_nvidia("What is two plus two?".into(), String::new(), AskMode::Meeting));
        assert!(result.is_ok(), "Synthetic connection test failed: {}", result.err().unwrap_or_default());
    }
}
