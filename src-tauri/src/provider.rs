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
    if let Ok(mut mem) = SESSION_MEMORY.lock() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        mem.push_back(DialogueTurn {
            role: role.to_string(),
            content: trimmed.to_string(),
            timestamp_ms: stamp,
        });
        while mem.len() > 10 {
            mem.pop_front();
        }
    }
}

pub fn format_session_context() -> String {
    if let Ok(mem) = SESSION_MEMORY.lock() {
        if mem.is_empty() {
            return String::new();
        }
        let turns: Vec<String> = mem
            .iter()
            .map(|t| format!("{}: {}", t.role, t.content))
            .collect();
        format!("\n\n[Recent Interview History & Dialogue Memory]:\n{}", turns.join("\n\n"))
    } else {
        String::new()
    }
}

#[tauri::command]
pub fn clear_session_memory() -> Result<(), String> {
    if let Ok(mut mem) = SESSION_MEMORY.lock() {
        mem.clear();
    }
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

#[derive(Deserialize)] struct ResponsesReply { output: Option<Vec<ResponsesOutput>> }
#[derive(Deserialize)] struct ResponsesOutput { content: Option<Vec<ResponsesContent>> }
#[derive(Deserialize)] struct ResponsesContent { text: Option<String> }

const VISUAL_SYSTEM_PROMPT: &str = "\
You are an expert technical co-pilot supporting a candidate during a live technical job interview. \
The user has captured their screen silently for your instant visual analysis. \
Understand the interview context from what is shown on screen and provide a high-IQ, practical, natural answer:
- Coding / LeetCode challenge: Identify the core algorithmic pattern (e.g. two pointers, monotonic stack, dynamic programming), state the optimal Time & Space complexity in Big-O, followed by clean, idiomatic code, and a brief 2-3 sentence natural explanation of the approach that the candidate can speak aloud to the interviewer.
- Architecture / System Design diagram: Identify key components and bottleneck points. State the data flow and practical scalability, caching, or consistency trade-offs in 2-4 clear, articulate bullet points.
- Debugging / Code Snippet: Pinpoint the exact root cause of the bug or edge-case failure, and provide the concise 1-3 line fix.
- Technical Question / Slides on screen: Answer the displayed prompt directly and professionally in the first person.
Keep your response focused, intelligent, and natural. Speak directly to the technical solution without fluff, preamble, or generic chatbot disclaimers.";

#[tauri::command]
pub async fn analyze_image(question: String, image_data_url: String) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() || question.len() > 8000 { return Err("Enter a short question about this screen.".into()); }
    if image_data_url.len() > 18_000_000 || !(image_data_url.starts_with("data:image/png;base64,") || image_data_url.starts_with("data:image/jpeg;base64,") || image_data_url.starts_with("data:image/webp;base64,")) {
        return Err("Invalid image data captured.".into());
    }

    let client = reqwest::Client::builder().timeout(Duration::from_secs(30)).redirect(reqwest::redirect::Policy::none()).build().map_err(|_| "Could not create the network connection.")?;
    let active = get_active_provider().unwrap_or_else(|_| "gemini".into());
    let session_context = format_session_context();

    let visual_prompt = format!(
        "{VISUAL_SYSTEM_PROMPT}{session_context}\n\n\
        Screen Analysis Request / Interview Question: {question}"
    );

    // 1. Try GEMINI VISION first if active is gemini or if gemini key exists
    let has_gemini = has_provider_key("gemini".into()).unwrap_or(false);
    let has_meta = has_meta_api_key().unwrap_or(false);

    if (active == "gemini" && has_gemini) || (has_gemini && !has_meta) {
        if let Ok(key) = get_entry(GEMINI_ACCOUNT)?.get_password() {
            if !key.trim().is_empty() {
                let (mime, raw_b64) = if let Some(s) = image_data_url.strip_prefix("data:image/jpeg;base64,") {
                    ("image/jpeg", s)
                } else if let Some(s) = image_data_url.strip_prefix("data:image/png;base64,") {
                    ("image/png", s)
                } else if let Some(s) = image_data_url.strip_prefix("data:image/webp;base64,") {
                    ("image/webp", s)
                } else {
                    ("image/jpeg", image_data_url.as_str())
                };

                let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}", key.trim());
                let res = client.post(&url).json(&json!({
                    "contents": [{
                        "parts": [
                            {"text": visual_prompt},
                            {"inline_data": {"mime_type": mime, "data": raw_b64}}
                        ]
                    }],
                    "generationConfig": {"temperature": 0.2, "maxOutputTokens": 1024}
                })).send().await;

                if let Ok(response) = res {
                    if response.status().is_success() {
                        if let Ok(body) = response.json::<GeminiResponse>().await {
                            if let Some(text) = body.candidates.unwrap_or_default().into_iter().flat_map(|c| c.content).flat_map(|co| co.parts.unwrap_or_default()).filter_map(|p| p.text).find(|t| !t.trim().is_empty()) {
                                add_session_dialogue("Interviewer / Screen Context", question);
                                add_session_dialogue("Candidate Solution", &text);
                                return Ok(text);
                            }
                        }
                    } else {
                        eprintln!("Gemini vision failed with status: {}", response.status());
                    }
                }
            }
        }
    }

    // 2. Try META MUSE SPARK VISION
    if let Ok(key) = get_entry(META_ACCOUNT)?.get_password() {
        if !key.trim().is_empty() {
            let response = client.post("https://api.meta.ai/v1/responses").bearer_auth(key).json(&json!({
                "model": "muse-spark-1.3-contributor",
                "max_output_tokens": 800,
                "input": [{
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": visual_prompt}, {"type": "input_image", "image_url": image_data_url}]
                }]
            })).send().await.map_err(|e| if e.is_timeout() { "Visual analysis timed out." } else { "Could not reach Meta Model API." })?;

            if response.status().is_success() {
                if let Ok(body) = response.json::<ResponsesReply>().await {
                    if let Some(text) = body.output.unwrap_or_default().into_iter().flat_map(|o| o.content.unwrap_or_default()).filter_map(|c| c.text).find(|t| !t.trim().is_empty()) {
                        add_session_dialogue("Interviewer / Screen Context", question);
                        add_session_dialogue("Candidate Solution", &text);
                        return Ok(text);
                    }
                }
            }
        }
    }

    Err("Add your Gemini or Meta API key in Settings to analyze your screen.".into())
}

#[derive(Deserialize)] struct Completion { choices: Vec<Choice> }
#[derive(Deserialize)] struct Choice { message: Message }
#[derive(Deserialize)] struct Message { content: Option<String> }
#[derive(Deserialize)] #[serde(rename_all="lowercase")] pub enum AskMode { Meeting, Coding }

#[derive(Deserialize)] struct GeminiResponse { candidates: Option<Vec<GeminiCandidate>> }
#[derive(Deserialize)] struct GeminiCandidate { content: Option<GeminiContent> }
#[derive(Deserialize)] struct GeminiContent { parts: Option<Vec<GeminiPart>> }
#[derive(Deserialize)] struct GeminiPart { text: Option<String> }

const MEETING_SYSTEM_PROMPT: &str = "\
You are an expert interview co-pilot whispering spoken answers directly to a candidate in a live technical job interview. \
You speak in the first person ('I', 'In my experience', 'My approach is...') as the candidate answering the interviewer. \
Guidelines:
- Natural, conversational flow: Sound like a sharp, articulate senior engineer having a real discussion—never sound robotic, canned, or like an AI assistant.
- Balanced, reasonable depth: Do NOT clip your answer into a robotic one-sentence fragment, and do NOT deliver a 500-word lecture. Aim for 2 to 4 fluid, well-structured sentences (~20-30 seconds of spoken delivery).
- Core structure: Give the direct answer/approach first, mention the practical real-world nuance or trade-off, and conclude cleanly.
- Strict prohibitions: NEVER say 'Sure!', 'Certainly', 'As an AI', or echo the interviewer's question. Jump straight into the candidate's spoken response.";

const CODING_SYSTEM_PROMPT: &str = "\
You are an expert coding interview co-pilot assisting a candidate. Provide a clean, optimal solution with correct syntax, precise time and space complexity, and a natural 2-sentence walk-through of the key insight.";

#[tauri::command]
pub async fn ask_nvidia(question: String, context: String, mode: AskMode) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() || question.len() > 20000 || context.len() > 16000 {
        return Err("Enter a question of at most 20,000 bytes and context of at most 16,000 bytes.".into());
    }

    let active = get_active_provider().unwrap_or_else(|_| "gemini".into());
    let client = reqwest::Client::builder().timeout(Duration::from_secs(20)).redirect(reqwest::redirect::Policy::none()).build().map_err(|_| "Could not create network client.".to_string())?;

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
        if let Ok(key) = get_entry(GEMINI_ACCOUNT)?.get_password() {
            if !key.trim().is_empty() {
                let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}", key.trim());
                let prompt = format!("{}\n\nContext:\n{}\n\nInterviewer Question:\n{}", system_prompt, full_context.trim(), question);
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

    // 2. TOGETHER AI (arize-ai/qwen-2-1.5b-instruct)
    if active == "together" {
        if let Ok(key) = get_entry(TOGETHER_ACCOUNT)?.get_password() {
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

    // 3. RAX AI (Official Endpoint & rax-4.0 model)
    if active == "rax" {
        if let Ok(key) = get_entry(RAX_ACCOUNT)?.get_password() {
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

    // 4. META MUSE SPARK
    if active == "meta" {
        if let Ok(key) = get_entry(META_ACCOUNT)?.get_password() {
            if !key.trim().is_empty() {
                let prompt = format!("{}\n\nContext:\n{}\n\nInterviewer Question:\n{}", system_prompt, full_context.trim(), question);
                let res = client.post("https://api.meta.ai/v1/responses").bearer_auth(key.trim()).json(&json!({
                    "model": "muse-spark-1.3-contributor",
                    "max_output_tokens": 1024,
                    "input": [{
                        "type": "message",
                        "role": "user",
                        "content": [{"type": "input_text", "text": prompt}]
                    }]
                })).send().await;

                if let Ok(response) = res {
                    if response.status().is_success() {
                        if let Ok(body) = response.json::<ResponsesReply>().await {
                            if let Some(text) = body.output.unwrap_or_default().into_iter().flat_map(|o| o.content.unwrap_or_default()).filter_map(|c| c.text).find(|t| !t.trim().is_empty()) {
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

    // 5. NVIDIA FALLBACK
    if let Ok(key) = get_entry(NVIDIA_ACCOUNT)?.get_password() {
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
    #[ignore = "requires a saved NVIDIA key and sends a synthetic prompt"]
    fn live_connection() {
        let result = tauri::async_runtime::block_on(ask_nvidia("What is two plus two?".into(), String::new(), AskMode::Meeting));
        assert!(result.is_ok(), "Synthetic connection test failed: {}", result.err().unwrap_or_default());
    }
}
