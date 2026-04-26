//! Google Gemini — Generative Language API (v1beta generateContent).

use serde::{Deserialize, Serialize};

use super::{GenerateRequest, LlmError};

#[derive(Serialize)]
struct GenerateContentRequest<'a> {
    contents: Vec<Content<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<Content<'a>>,
    generation_config: GenerationConfig,
}

#[derive(Serialize)]
struct Content<'a> {
    role: &'a str,
    parts: Vec<Part<'a>>,
}

#[derive(Serialize)]
struct Part<'a> {
    text: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    temperature: f32,
    max_output_tokens: u32,
}

#[derive(Deserialize)]
struct GenerateContentResponse {
    candidates: Option<Vec<Candidate>>,
    #[serde(rename = "promptFeedback")]
    prompt_feedback: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<CandidateContent>,
}

#[derive(Deserialize)]
struct CandidateContent {
    parts: Option<Vec<CandidatePart>>,
}

#[derive(Deserialize)]
struct CandidatePart {
    text: Option<String>,
}

pub async fn generate(api_key: &str, req: &GenerateRequest) -> Result<String, LlmError> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        req.model, api_key
    );

    let system_instruction = if req.system.trim().is_empty() {
        None
    } else {
        Some(Content {
            role: "system",
            parts: vec![Part { text: &req.system }],
        })
    };

    let body = GenerateContentRequest {
        contents: vec![Content {
            role: "user",
            parts: vec![Part { text: &req.prompt }],
        }],
        system_instruction,
        generation_config: GenerationConfig {
            temperature: req.temperature,
            max_output_tokens: 1024,
        },
    };

    let resp = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| LlmError::Http(e.to_string()))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| LlmError::Http(e.to_string()))?;

    if !status.is_success() {
        return Err(LlmError::ProviderError {
            status: status.as_u16(),
            body: text,
        });
    }

    let parsed: GenerateContentResponse =
        serde_json::from_str(&text).map_err(|e| LlmError::BadResponse(e.to_string()))?;

    if parsed.candidates.as_ref().map(|c| c.is_empty()).unwrap_or(true) {
        let reason = parsed
            .prompt_feedback
            .map(|v| v.to_string())
            .unwrap_or_else(|| "no candidates".to_string());
        return Err(LlmError::BadResponse(format!("Gemini returned no candidates: {reason}")));
    }

    parsed
        .candidates
        .and_then(|cands| cands.into_iter().next())
        .and_then(|c| c.content)
        .and_then(|c| c.parts)
        .and_then(|parts| parts.into_iter().filter_map(|p| p.text).next())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| LlmError::BadResponse("Gemini candidate had no text part".into()))
}
