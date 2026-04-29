use serde::{Deserialize, Serialize};

use super::{describe_transport_error, GenerateRequest, LlmError};

#[derive(Serialize)]
struct OllamaGenerateBody<'a> {
    model: &'a str,
    system: &'a str,
    prompt: &'a str,
    stream: bool,
    options: Options,
}

#[derive(Serialize)]
struct Options {
    temperature: f32,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: Option<String>,
    error: Option<String>,
}

pub async fn generate(base_url: &str, req: &GenerateRequest) -> Result<String, LlmError> {
    let url = format!("{}/api/generate", base_url.trim_end_matches('/'));
    let body = OllamaGenerateBody {
        model: &req.model,
        system: &req.system,
        prompt: &req.prompt,
        stream: false,
        options: Options {
            temperature: req.temperature,
        },
    };

    let resp = reqwest::Client::new()
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| LlmError::Http(describe_transport_error("Ollama generate endpoint", &e)))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| LlmError::Http(describe_transport_error("Ollama generate endpoint", &e)))?;

    if !status.is_success() {
        return Err(LlmError::ProviderError {
            status: status.as_u16(),
            body: text,
        });
    }

    let parsed: OllamaGenerateResponse =
        serde_json::from_str(&text).map_err(|e| LlmError::BadResponse(e.to_string()))?;

    if let Some(err) = parsed.error {
        return Err(LlmError::BadResponse(err));
    }

    parsed
        .response
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| LlmError::BadResponse("empty response field".into()))
}

pub async fn list_models(base_url: &str) -> Result<Vec<String>, LlmError> {
    #[derive(Deserialize)]
    struct TagsResp {
        models: Option<Vec<TagsEntry>>,
    }
    #[derive(Deserialize)]
    struct TagsEntry {
        name: String,
    }

    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| LlmError::Http(describe_transport_error("Ollama tags endpoint", &e)))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| LlmError::Http(describe_transport_error("Ollama tags endpoint", &e)))?;
    if !status.is_success() {
        return Err(LlmError::ProviderError {
            status: status.as_u16(),
            body: text,
        });
    }
    let parsed: TagsResp =
        serde_json::from_str(&text).map_err(|e| LlmError::BadResponse(e.to_string()))?;
    Ok(parsed
        .models
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.name)
        .collect())
}
