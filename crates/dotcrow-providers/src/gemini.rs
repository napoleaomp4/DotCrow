//! Adaptador da API do Google Gemini, endpoint `models/{modelo}:generateContent`.

use std::{sync::Arc, time::Duration};

use reqwest::header::HeaderName;
use serde_json::{Value, json};

use crate::http::{self, HttpFailure};
use crate::{CompletionRequest, CompletionResponse, KeySource, LlmProvider, ProviderError, Role};

/// Id usado no cofre.
pub const ID: &str = "gemini";
/// Modelo usado quando o dono não escolhe outro.
pub const DEFAULT_MODEL: &str = "gemini-3.8-flash";
/// Endereço padrão da API.
pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Provedor Gemini.
pub struct Gemini {
    base_url: String,
    model: String,
    keys: Arc<dyn KeySource>,
    client: reqwest::Client,
}

impl Gemini {
    /// Cria o provedor. `model` e `base_url` vazios usam os padrões.
    pub fn new(
        keys: Arc<dyn KeySource>,
        model: Option<String>,
        base_url: Option<String>,
    ) -> Result<Self, ProviderError> {
        let model = model.filter(|m| !m.trim().is_empty()).unwrap_or_else(|| DEFAULT_MODEL.into());
        // O modelo vai no caminho da URL; só aceitamos nomes simples.
        if !model.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_')) {
            return Err(ProviderError::Misconfigured(format!("nome de modelo inválido: {model}")));
        }
        Ok(Self {
            base_url: http::check_base_url(base_url.as_deref().unwrap_or(DEFAULT_BASE_URL))?,
            model,
            keys,
            client: http::client()?,
        })
    }
}

fn body(req: &CompletionRequest) -> Value {
    let contents: Vec<Value> = req
        .messages
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|m| {
            let role = if m.role == Role::Assistant { "model" } else { "user" };
            json!({ "role": role, "parts": [{ "text": m.content }] })
        })
        .collect();
    let mut b = json!({ "contents": contents, "generationConfig": { "maxOutputTokens": req.max_tokens } });
    if let Some(system) = http::system_text(&req.messages) {
        b["systemInstruction"] = json!({ "parts": [{ "text": system }] });
    }
    b
}

fn parse(v: &Value, fallback_model: &str) -> Result<CompletionResponse, ProviderError> {
    let Some(candidate) = v.pointer("/candidates/0") else {
        let reason = v.pointer("/promptFeedback/blockReason").and_then(Value::as_str).unwrap_or("sem candidatos");
        return Err(ProviderError::Refused(format!("o Gemini bloqueou o pedido ({reason})")));
    };
    let text: String = candidate
        .pointer("/content/parts")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                // Partes de "pensamento" não são a resposta.
                .filter(|p| p.get("thought").and_then(Value::as_bool) != Some(true))
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    if text.trim().is_empty() {
        let reason = candidate.get("finishReason").and_then(Value::as_str).unwrap_or("desconhecido");
        return Err(ProviderError::Unavailable(format!("resposta vazia (motivo: {reason})")));
    }
    let model = v.get("modelVersion").and_then(Value::as_str).unwrap_or(fallback_model).to_string();
    Ok(CompletionResponse { text, model })
}

/// Lê o `retryDelay` ("37s") que o Gemini manda nos detalhes de um 429.
fn retry_delay(v: &Value) -> Option<Duration> {
    v.pointer("/error/details")?
        .as_array()?
        .iter()
        .filter_map(|d| d.get("retryDelay").and_then(Value::as_str))
        .find_map(|s| s.trim_end_matches('s').parse::<f64>().ok())
        .map(|secs| Duration::from_secs(secs.ceil() as u64))
}

fn classify(status: u16, retry_after: Option<Duration>, body: &str, model: &str) -> ProviderError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let state = parsed.as_ref().and_then(|v| v.pointer("/error/status")).and_then(Value::as_str).unwrap_or("");
    let detail = http::error_detail(body);
    let lower = detail.to_lowercase();
    match (status, state) {
        // Chave inválida chega como 400 INVALID_ARGUMENT com "API key not valid".
        (400, _) if lower.contains("api key") => ProviderError::Unauthorized,
        (403, _) | (_, "PERMISSION_DENIED") | (_, "UNAUTHENTICATED") => ProviderError::Unauthorized,
        (429, _) | (_, "RESOURCE_EXHAUSTED") => {
            let delay = parsed.as_ref().and_then(retry_delay).or(retry_after);
            if lower.contains("per day") || lower.contains("perday") || lower.contains("billing") {
                ProviderError::QuotaExhausted
            } else {
                ProviderError::RateLimited { retry_after: delay }
            }
        }
        (404, _) | (_, "NOT_FOUND") => {
            ProviderError::Misconfigured(format!("modelo \"{model}\" não encontrado: {detail}"))
        }
        _ => ProviderError::from_http_status(status, retry_after, &detail),
    }
}

#[async_trait::async_trait]
impl LlmProvider for Gemini {
    fn id(&self) -> &str {
        ID
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        // A chave vai no cabeçalho, nunca na URL (URLs acabam em logs).
        let key = http::secret_header(self.keys.as_ref(), ID, |k| k.to_string())?;
        let headers = vec![(HeaderName::from_static("x-goog-api-key"), key)];
        let url = format!("{}/models/{}:generateContent", self.base_url, self.model);
        match http::post_json(&self.client, &url, headers, &body(req)).await {
            Ok(v) => parse(&v, &self.model),
            Err(HttpFailure::Network(e)) => Err(e),
            Err(HttpFailure::Status { status, retry_after, body }) => {
                Err(classify(status, retry_after, &body, &self.model))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;

    #[test]
    fn builds_contents_and_system_instruction() {
        let req = CompletionRequest {
            messages: vec![
                Message { role: Role::System, content: "s".into() },
                Message { role: Role::User, content: "u".into() },
                Message { role: Role::Assistant, content: "a".into() },
            ],
            max_tokens: 10,
            tier: crate::Tier::Standard,
        };
        let b = body(&req);
        assert_eq!(b["systemInstruction"]["parts"][0]["text"], "s");
        assert_eq!(b["contents"][1]["role"], "model");
        assert_eq!(b["generationConfig"]["maxOutputTokens"], 10);
    }

    #[test]
    fn parses_and_skips_thoughts() {
        let v = json!({"modelVersion":"gemini-x","candidates":[{"content":{"parts":[{"text":"pensando","thought":true},{"text":"Oi"}]},"finishReason":"STOP"}]});
        assert_eq!(parse(&v, "m").unwrap().text, "Oi");
        let blocked = json!({"promptFeedback":{"blockReason":"SAFETY"}});
        assert!(matches!(parse(&blocked, "m"), Err(ProviderError::Refused(_))));
        let empty = json!({"candidates":[{"content":{"parts":[]},"finishReason":"MAX_TOKENS"}]});
        assert!(matches!(parse(&empty, "m"), Err(ProviderError::Unavailable(_))));
    }

    #[test]
    fn classifies_errors() {
        let bad_key = r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT"}}"#;
        assert_eq!(classify(400, None, bad_key, "m"), ProviderError::Unauthorized);
        let rate = r#"{"error":{"code":429,"message":"Quota exceeded for metric: requests per minute","status":"RESOURCE_EXHAUSTED","details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"37s"}]}}"#;
        assert_eq!(
            classify(429, None, rate, "m"),
            ProviderError::RateLimited { retry_after: Some(Duration::from_secs(37)) }
        );
        let daily =
            r#"{"error":{"code":429,"message":"Quota exceeded: requests per day","status":"RESOURCE_EXHAUSTED"}}"#;
        assert_eq!(classify(429, None, daily, "m"), ProviderError::QuotaExhausted);
        let missing = r#"{"error":{"code":404,"message":"models/x is not found","status":"NOT_FOUND"}}"#;
        assert!(matches!(classify(404, None, missing, "m"), ProviderError::Misconfigured(_)));
        assert!(matches!(classify(503, None, "{}", "m"), ProviderError::Unavailable(_)));
    }

    #[test]
    fn rejects_strange_model_names() {
        assert!(Gemini::new(Arc::new(crate::NoKeys), Some("../x?y".into()), None).is_err());
    }
}
