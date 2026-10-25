//! Adaptador da API da Anthropic (Claude), endpoint `POST /v1/messages`.

use std::sync::Arc;

use reqwest::header::{HeaderName, HeaderValue};
use serde_json::{Value, json};

use crate::http::{self, HttpFailure};
use crate::{CompletionRequest, CompletionResponse, KeySource, LlmProvider, ProviderError, Role};

/// Id usado no cofre.
pub const ID: &str = "anthropic";
/// Modelo usado quando o dono não escolhe outro.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
/// Modelo leve (faixa Fast).
pub const FAST_MODEL: &str = "claude-haiku-5-5";
/// Modelo forte (faixa Deep).
pub const DEEP_MODEL: &str = "claude-opus-5-5";
/// Endereço padrão da API.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";

/// Provedor Claude.
pub struct Anthropic {
    base_url: String,
    /// Modelo fixado pelo dono; `None` deixa a faixa (tier) escolher.
    model: Option<String>,
    keys: Arc<dyn KeySource>,
    client: reqwest::Client,
}

impl Anthropic {
    /// Cria o provedor. `model` e `base_url` vazios usam os padrões.
    pub fn new(
        keys: Arc<dyn KeySource>,
        model: Option<String>,
        base_url: Option<String>,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            base_url: http::check_base_url(base_url.as_deref().unwrap_or(DEFAULT_BASE_URL))?,
            model: model.filter(|m| !m.trim().is_empty()),
            keys,
            client: http::client()?,
        })
    }

    /// Modelo efetivo: o fixado pelo dono vence; senão, o Opus 5.5 em todas as faixas.
    /// O dono pediu o Opus em tudo, sem Haiku. Para variar por faixa de novo, é só
    /// trocar aqui (ou fixar outro modelo na aba Provedores).
    fn pick(&self, _tier: crate::Tier) -> &str {
        if let Some(m) = &self.model {
            return m;
        }
        DEEP_MODEL
    }
}

/// Corpo da requisição no formato da API de mensagens.
fn body(model: &str, req: &CompletionRequest) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|m| {
            let role = if m.role == Role::Assistant { "assistant" } else { "user" };
            json!({ "role": role, "content": m.content })
        })
        .collect();
    let mut b = json!({ "model": model, "max_tokens": req.max_tokens, "messages": messages });
    if let Some(system) = http::system_text(&req.messages) {
        b["system"] = Value::String(system);
    }
    b
}

fn parse(v: &Value, fallback_model: &str) -> Result<CompletionResponse, ProviderError> {
    let text: String = v
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    if text.trim().is_empty() {
        let reason = v.get("stop_reason").and_then(Value::as_str).unwrap_or("desconhecido");
        if reason == "refusal" {
            return Err(ProviderError::Refused("filtro de segurança da Anthropic (stop_reason: refusal)".into()));
        }
        return Err(ProviderError::Unavailable(format!("resposta sem texto (stop_reason: {reason})")));
    }
    let model = v.get("model").and_then(Value::as_str).unwrap_or(fallback_model).to_string();
    Ok(CompletionResponse { text, model })
}

/// Classifica erros da Anthropic. O tipo vem em `error.type`.
fn classify(status: u16, retry_after: Option<std::time::Duration>, body: &str, model: &str) -> ProviderError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let kind = parsed.as_ref().and_then(|v| v.pointer("/error/type")).and_then(Value::as_str).unwrap_or("");
    let detail = http::error_detail(body);
    let lower = detail.to_lowercase();
    match kind {
        "authentication_error" | "permission_error" => ProviderError::Unauthorized,
        "rate_limit_error" => ProviderError::RateLimited { retry_after },
        "overloaded_error" | "api_error" | "timeout_error" => ProviderError::Unavailable(detail),
        "billing_error" => ProviderError::QuotaExhausted,
        // A Anthropic avisa "credit balance is too low" com status 400.
        _ if lower.contains("credit balance") || lower.contains("billing") => ProviderError::QuotaExhausted,
        "not_found_error" => ProviderError::Misconfigured(format!("modelo \"{model}\" não encontrado: {detail}")),
        _ => ProviderError::from_http_status(status, retry_after, &detail),
    }
}

#[async_trait::async_trait]
impl LlmProvider for Anthropic {
    fn id(&self) -> &str {
        ID
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let key = http::secret_header(self.keys.as_ref(), ID, |k| k.to_string())?;
        let headers = vec![
            (HeaderName::from_static("x-api-key"), key),
            (HeaderName::from_static("anthropic-version"), HeaderValue::from_static(API_VERSION)),
        ];
        let url = format!("{}/v1/messages", self.base_url);
        let model = self.pick(req.tier);
        match http::post_json(&self.client, &url, headers, &body(model, req)).await {
            Ok(v) => parse(&v, model),
            Err(HttpFailure::Network(e)) => Err(e),
            Err(HttpFailure::Status { status, retry_after, body }) => {
                Err(classify(status, retry_after, &body, self.pick(req.tier)))
            }
        }
    }

    async fn complete_streaming(
        &self,
        req: &CompletionRequest,
        on_delta: crate::OnDelta<'_>,
    ) -> Result<CompletionResponse, ProviderError> {
        let key = http::secret_header(self.keys.as_ref(), ID, |k| k.to_string())?;
        let headers = vec![
            (HeaderName::from_static("x-api-key"), key),
            (HeaderName::from_static("anthropic-version"), HeaderValue::from_static(API_VERSION)),
        ];
        let url = format!("{}/v1/messages", self.base_url);
        let model = self.pick(req.tier).to_string();
        let mut corpo = body(&model, req);
        corpo["stream"] = Value::Bool(true);

        let mut acc = String::new();
        let mut modelo_visto = model.clone();
        let mut erro: Option<String> = None;
        let mut recusou = false;
        let resultado = {
            let mut on_event = |dados: &str| {
                let Ok(v) = serde_json::from_str::<Value>(dados) else { return };
                match v.get("type").and_then(Value::as_str) {
                    Some("content_block_delta") => {
                        if let Some(t) = v.pointer("/delta/text").and_then(Value::as_str) {
                            acc.push_str(t);
                            on_delta(t);
                        }
                    }
                    Some("message_delta") => {
                        if v.pointer("/delta/stop_reason").and_then(Value::as_str) == Some("refusal") {
                            recusou = true;
                        }
                    }
                    Some("message_start") => {
                        if let Some(m) = v.pointer("/message/model").and_then(Value::as_str) {
                            modelo_visto = m.to_string();
                        }
                    }
                    Some("error") => {
                        erro = Some(
                            v.pointer("/error/message").and_then(Value::as_str).unwrap_or("erro no stream").to_string(),
                        );
                    }
                    _ => {}
                }
            };
            http::post_sse(&self.client, &url, headers, &corpo, &mut on_event).await
        };
        match resultado {
            Ok(()) => {}
            Err(HttpFailure::Network(e)) => return Err(e),
            Err(HttpFailure::Status { status, retry_after, body }) => {
                return Err(classify(status, retry_after, &body, &model));
            }
        }
        if recusou {
            return Err(ProviderError::Refused("filtro de segurança da Anthropic (stop_reason: refusal)".into()));
        }
        if let Some(e) = erro {
            return Err(ProviderError::Unavailable(e));
        }
        if acc.trim().is_empty() {
            return Err(ProviderError::Unavailable("resposta vazia".into()));
        }
        Ok(CompletionResponse { text: acc, model: modelo_visto })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;

    fn req() -> CompletionRequest {
        CompletionRequest {
            tier: crate::Tier::Standard,
            messages: vec![
                Message { role: Role::System, content: "Você é Huginn.".into() },
                Message { role: Role::User, content: "organize".into() },
            ],
            max_tokens: 100,
        }
    }

    #[test]
    fn tier_picks_the_model_when_none_is_pinned() {
        use crate::{NoKeys, Tier};
        let auto = Anthropic::new(std::sync::Arc::new(NoKeys), None, None).unwrap();
        // O dono escolheu Opus 5.5 em todas as faixas (sem Haiku).
        assert_eq!(auto.pick(Tier::Fast), DEEP_MODEL);
        assert_eq!(auto.pick(Tier::Standard), DEEP_MODEL);
        assert_eq!(auto.pick(Tier::Deep), DEEP_MODEL);
        // Modelo fixado pelo dono vence qualquer faixa.
        let fixo = Anthropic::new(std::sync::Arc::new(NoKeys), Some("meu-modelo".into()), None).unwrap();
        assert_eq!(fixo.pick(Tier::Fast), "meu-modelo");
        assert_eq!(fixo.pick(Tier::Deep), "meu-modelo");
    }

    #[test]
    fn system_goes_to_its_own_field() {
        let b = body("claude-x", &req());
        assert_eq!(b["system"], "Você é Huginn.");
        assert_eq!(b["messages"].as_array().unwrap().len(), 1);
        assert_eq!(b["messages"][0]["role"], "user");
        assert_eq!(b["max_tokens"], 100);
    }

    #[test]
    fn parses_text_blocks() {
        let v =
            json!({"model":"claude-x-1","content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"Olá"}]});
        let r = parse(&v, "m").unwrap();
        assert_eq!(r.text, "Olá");
        assert_eq!(r.model, "claude-x-1");
        assert!(parse(&json!({"content":[]}), "m").is_err());
        // Recusa do filtro de segurança vira Refused (o roteador passa para o próximo).
        assert!(matches!(parse(&json!({"content":[],"stop_reason":"refusal"}), "m"), Err(ProviderError::Refused(_))));
    }

    #[test]
    fn classifies_errors() {
        let e = |t: &str, m: &str| format!(r#"{{"type":"error","error":{{"type":"{t}","message":"{m}"}}}}"#);
        assert_eq!(
            classify(401, None, &e("authentication_error", "invalid x-api-key"), "m"),
            ProviderError::Unauthorized
        );
        assert_eq!(
            classify(400, None, &e("invalid_request_error", "Your credit balance is too low"), "m"),
            ProviderError::QuotaExhausted
        );
        assert!(matches!(classify(429, None, &e("rate_limit_error", "x"), "m"), ProviderError::RateLimited { .. }));
        assert!(matches!(
            classify(529, None, &e("overloaded_error", "Overloaded"), "m"),
            ProviderError::Unavailable(_)
        ));
        assert!(matches!(classify(404, None, &e("not_found_error", "model: x"), "m"), ProviderError::Misconfigured(_)));
        assert!(matches!(
            classify(400, None, &e("invalid_request_error", "prompt is too long"), "m"),
            ProviderError::BadRequest(_)
        ));
    }
}
