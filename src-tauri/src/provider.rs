use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

const SERVICE: &str = "com.raxley.livecompanion";
const NVIDIA_ACCOUNT: &str = "nvidia-api-key";
const META_ACCOUNT: &str = "meta-model-api-key";
const GEMINI_ACCOUNT: &str = "gemini-api-key";
const TOGETHER_ACCOUNT: &str = "together-api-key";
const RAX_ACCOUNT: &str = "rax-api-key";
const ACTIVE_PROVIDER_ACCOUNT: &str = "active-ai-provider";

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

#[tauri::command]
pub async fn analyze_image(question: String, image_data_url: String) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() || question.len() > 4000 { return Err("Enter a short question about this image.".into()); }
    if image_data_url.len() > 18_000_000 || !(image_data_url.starts_with("data:image/png;base64,") || image_data_url.starts_with("data:image/jpeg;base64,") || image_data_url.starts_with("data:image/webp;base64,")) {
        return Err("Choose a PNG, JPEG, or WebP image under 12 MB.".into());
    }
    let key = get_entry(META_ACCOUNT)?.get_password().map_err(|_| "Save your Meta API key in Settings first.".to_string())?;
    let client = reqwest::Client::builder().timeout(Duration::from_secs(60)).redirect(reqwest::redirect::Policy::none()).build().map_err(|_| "Could not create the network connection.")?;
    let concise_prompt = format!("Answer the user's question using only evidence visible in the screenshot. Be precise and concise: give the direct answer first, then at most two short supporting details. Do not guess obscured or unreadable content; say clearly when the screenshot does not provide enough information.\n\nUser question: {question}");
    let response = client.post("https://api.meta.ai/v1/responses").bearer_auth(key).json(&json!({
        "model": "muse-spark-1.3-contributor",
        "max_output_tokens": 500,
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": concise_prompt}, {"type": "input_image", "image_url": image_data_url}]
        }]
    })).send().await.map_err(|e| if e.is_timeout() { "Image analysis timed out." } else { "Could not reach Meta Model API." })?;

    if !response.status().is_success() {
        return Err("Meta rejected the image request. Check account and model access.".into());
    }
    let body: ResponsesReply = response.json().await.map_err(|_| "Meta returned an unexpected image-analysis response.")?;
    body.output.unwrap_or_default().into_iter().flat_map(|o| o.content.unwrap_or_default()).filter_map(|c| c.text).find(|t| !t.trim().is_empty()).ok_or_else(|| "Meta returned no image analysis.".into())
}

#[derive(Deserialize)] struct Completion { choices: Vec<Choice> }
#[derive(Deserialize)] struct Choice { message: Message }
#[derive(Deserialize)] struct Message { content: Option<String> }
#[derive(Deserialize)] #[serde(rename_all="lowercase")] pub enum AskMode { Meeting, Coding }

#[derive(Deserialize)] struct GeminiResponse { candidates: Option<Vec<GeminiCandidate>> }
#[derive(Deserialize)] struct GeminiCandidate { content: Option<GeminiContent> }
#[derive(Deserialize)] struct GeminiContent { parts: Option<Vec<GeminiPart>> }
#[derive(Deserialize)] struct GeminiPart { text: Option<String> }

#[tauri::command]
pub async fn ask_nvidia(question: String, context: String, mode: AskMode) -> Result<String, String> {
    let question = question.trim();
    if question.is_empty() || question.len() > 20000 || context.len() > 16000 {
        return Err("Enter a question of at most 20,000 bytes and context of at most 16,000 bytes.".into());
    }

    let active = get_active_provider().unwrap_or_else(|_| "gemini".into());
    let client = reqwest::Client::builder().timeout(Duration::from_secs(20)).redirect(reqwest::redirect::Policy::none()).build().map_err(|_| "Could not create network client.".to_string())?;

    let system_prompt = match mode {
        AskMode::Meeting => "You are an expert real-time interview co-pilot whispering answers in real-time to the candidate. Output ONLY the direct first-person answer the candidate should speak aloud immediately. Do NOT repeat or quote the question. Do NOT include greetings or preamble like 'Sure' or 'Here is'. Jump straight into the answer. Sound articulate, natural, confident, and professional.",
        AskMode::Coding => "You are an expert coding interview assistant. Provide a clean, accurate, and direct solution with code examples where relevant. No fluff.",
    };

    // 1. GEMINI (Fastest, ultra-low latency)
    if active == "gemini" {
        if let Ok(key) = get_entry(GEMINI_ACCOUNT)?.get_password() {
            if !key.trim().is_empty() {
                let url = format!("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}", key.trim());
                let prompt = format!("{}\n\nContext:\n{}\n\nQuestion:\n{}", system_prompt, context.trim(), question);
                let res = client.post(&url).json(&json!({
                    "contents": [{"parts": [{"text": prompt}]}],
                    "generationConfig": {"temperature": 0.4, "maxOutputTokens": 1024}
                })).send().await;

                if let Ok(response) = res {
                    if response.status().is_success() {
                        if let Ok(body) = response.json::<GeminiResponse>().await {
                            if let Some(text) = body.candidates.unwrap_or_default().into_iter().flat_map(|c| c.content).flat_map(|co| co.parts.unwrap_or_default()).filter_map(|p| p.text).find(|t| !t.trim().is_empty()) {
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
                        {"role": "user", "content": format!("Context:\n{}\n\nQuestion:\n{}", context.trim(), question)}
                    ],
                    "temperature": 0.3,
                    "max_tokens": 1024
                })).send().await;

                if let Ok(response) = res {
                    if response.status().is_success() {
                        if let Ok(body) = response.json::<Completion>().await {
                            if let Some(text) = body.choices.into_iter().next().and_then(|c| c.message.content).filter(|s| !s.trim().is_empty()) {
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
                        {"role": "user", "content": format!("Context:\n{}\n\nQuestion:\n{}", context.trim(), question)}
                    ],
                    "temperature": 0.3,
                    "max_tokens": 1024
                })).send().await;

                if let Ok(response) = res {
                    if response.status().is_success() {
                        if let Ok(body) = response.json::<Completion>().await {
                            if let Some(text) = body.choices.into_iter().next().and_then(|c| c.message.content).filter(|s| !s.trim().is_empty()) {
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
                let prompt = format!("{}\n\nContext:\n{}\n\nQuestion:\n{}", system_prompt, context.trim(), question);
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
                    {"role": "user", "content": format!("Context:\n{}\n\nQuestion:\n{}", context.trim(), question)}
                ],
                "temperature": 0.3,
                "max_tokens": 1024
            })).send().await;

            if let Ok(response) = res {
                if response.status().is_success() {
                    if let Ok(body) = response.json::<Completion>().await {
                        if let Some(text) = body.choices.into_iter().next().and_then(|c| c.message.content).filter(|s| !s.trim().is_empty()) {
                            return Ok(text);
                        }
                    }
                }
            }
        }
    }

    Err(format!("Selected provider ({}) failed or key not configured in Settings.", active))
}

#[cfg(test)] mod tests { use super::*; #[test] fn provider_errors_are_clear_and_do_not_include_payloads() { assert!(status_error(401).contains("key")); assert!(status_error(429).contains("rate limit")); assert!(status_error(503).contains("temporarily unavailable")); } #[test] fn validates_assistant_mode() { assert!(matches!(serde_json::from_str::<AskMode>("\"meeting\""), Ok(AskMode::Meeting))); assert!(matches!(serde_json::from_str::<AskMode>("\"coding\""), Ok(AskMode::Coding))); assert!(serde_json::from_str::<AskMode>("\"other\"").is_err()); } #[test] #[ignore = "requires a saved NVIDIA key and sends a synthetic prompt"] fn live_connection() { let result = tauri::async_runtime::block_on(ask_nvidia("What is two plus two?".into(), String::new(), AskMode::Meeting)); assert!(result.is_ok(), "Synthetic connection test failed: {}", result.err().unwrap_or_default()); } }
