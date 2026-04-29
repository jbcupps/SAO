use serde::{Deserialize, Serialize};

use super::{describe_transport_error, GenerateRequest, LlmError};

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "str::is_empty")]
    system: &'a str,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

pub async fn generate(api_key: &str, req: &GenerateRequest) -> Result<String, LlmError> {
    let body = MessagesRequest {
        model: &req.model,
        max_tokens: 1024,
        temperature: req.temperature,
        system: &req.system,
        messages: vec![Message {
            role: "user",
            content: &req.prompt,
        }],
    };

    let resp = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| LlmError::Http(describe_transport_error("Anthropic Messages API", &e)))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| LlmError::Http(describe_transport_error("Anthropic Messages API", &e)))?;

    if !status.is_success() {
        return Err(LlmError::ProviderError {
            status: status.as_u16(),
            body: text,
        });
    }

    let parsed: MessagesResponse =
        serde_json::from_str(&text).map_err(|e| LlmError::BadResponse(e.to_string()))?;

    parsed
        .content
        .into_iter()
        .find(|b| b.kind == "text")
        .and_then(|b| b.text)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| LlmError::BadResponse("no text content in response".into()))
}
