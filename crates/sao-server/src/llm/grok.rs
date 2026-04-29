//! xAI Grok — OpenAI-compatible chat completions at api.x.ai.

use serde::{Deserialize, Serialize};

use super::{describe_transport_error, GenerateRequest, LlmError};

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

pub async fn generate(api_key: &str, req: &GenerateRequest) -> Result<String, LlmError> {
    let messages = if req.system.trim().is_empty() {
        vec![Message {
            role: "user",
            content: &req.prompt,
        }]
    } else {
        vec![
            Message {
                role: "system",
                content: &req.system,
            },
            Message {
                role: "user",
                content: &req.prompt,
            },
        ]
    };

    let body = ChatRequest {
        model: &req.model,
        messages,
        temperature: req.temperature,
    };

    let resp = reqwest::Client::new()
        .post("https://api.x.ai/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| LlmError::Http(describe_transport_error("xAI Chat Completions API", &e)))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| LlmError::Http(describe_transport_error("xAI Chat Completions API", &e)))?;

    if !status.is_success() {
        return Err(LlmError::ProviderError {
            status: status.as_u16(),
            body: text,
        });
    }

    let parsed: ChatResponse =
        serde_json::from_str(&text).map_err(|e| LlmError::BadResponse(e.to_string()))?;

    parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| LlmError::BadResponse("no choices in response".into()))
}
