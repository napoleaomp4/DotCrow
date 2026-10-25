//! Adaptador para a API da OpenAI e todas as compatíveis com ela
//! (`POST /chat/completions`): OpenRouter, DeepSeek, Groq, Ollama e outras.

use std::sync::Arc;

use reqwest::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde_json::{Value, json};

use crate::http::{self, HttpFailure};
use crate::{CompletionRequest, CompletionResponse, KeySource, LlmProvider, ProviderError, Role};

/// Detalhes que mudam de um serviço compatível para outro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flavor {
    /// Id usado no cofre.
    pub id: &'static str,
    /// Endereço padrão, já com o `/v1` (ou equivalente) no fim.
    pub base_url: &'static str,
    /// Modelo padrão.
    pub default_model: &'static str,
    /// Se precisa de chave. O Ollama local não precisa.
    pub needs_key: bool,
    /// Nome do campo de limite de tokens. Os modelos novos da OpenAI só aceitam
    /// `max_completion_tokens`; os outros serviços usam `max_tokens`.
    pub max_tokens_field: &'static str,
    /// Esforço de raciocínio enviado em `reasoning_effort`, quando o serviço aceita.
    /// Baixo deixa a resposta mais rápida e sobra limite de tokens para o texto.
    pub reasoning_effort: Option<&'static str>,
}

/// OpenAI (GPT).
pub const OPENAI: Flavor = Flavor {
    id: "openai",
    base_url: "https://api.openai.com/v1",
    // Barato: os corvos trabalham com modelos de tokens baratos.
    default_model: "gpt-5-nano",
    needs_key: true,
    max_tokens_field: "max_completion_tokens",
    reasoning_effort: Some("low"),
};

/// OpenRouter: uma chave para centenas de modelos. `openrouter/auto` escolhe sozinho.
pub const OPENROUTER: Flavor = Flavor {
    id: "openrouter",
    base_url: "https://openrouter.ai/api/v1",
    default_model: "openrouter/auto",
    needs_key: true,
    max_tokens_field: "max_tokens",
    reasoning_effort: None,
};

/// DeepSeek.
pub const DEEPSEEK: Flavor = Flavor {
    id: "deepseek",
    base_url: "https://api.deepseek.com",
    default_model: "deepseek-flash",
    needs_key: true,
    max_tokens_field: "max_tokens",
    reasoning_effort: None,
};

/// Groq (modelos abertos, muito rápidos).
pub const GROQ: Flavor = Flavor {
    id: "groq",
    base_url: "https://api.groq.com/openai/v1",
    default_model: "openai/gpt-oss-120b",
    needs_key: true,
    max_tokens_field: "max_tokens",
    reasoning_effort: None,
};

/// Ollama: modelos rodando no seu próprio computador.
pub const OLLAMA: Flavor = Flavor {
    id: "ollama",
    base_url: "http://localhost:11434/v1",
    default_model: "llama3.2",
    needs_key: false,
    max_tokens_field: "max_tokens",
    reasoning_effort: None,
};

/// Provedor compatível com a API de chat da OpenAI.
pub struct OpenAiCompatible {
    flavor: Flavor,
    base_url: String,
    model: String,
    keys: Arc<dyn KeySource>,
    client: reqwest::Client,
}

impl OpenAiCompatible {
    /// Cria o provedor. `model` e `base_url` vazios usam os padrões do `flavor`.
    pub fn new(
        flavor: Flavor,
        keys: Arc<dyn KeySource>,
        model: Option<String>,
        base_url: Option<String>,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            flavor,
            base_url: http::check_base_url(base_url.as_deref().unwrap_or(flavor.base_url))?,
            model: model.filter(|m| !m.trim().is_empty()).unwrap_or_else(|| flavor.default_model.into()),
            keys,
            client: http::client()?,
        })
    }
}

fn body(flavor: &Flavor, model: &str, req: &CompletionRequest) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            json!({ "role": role, "content": m.content })
        })
        .collect();
    let mut b = json!({ "model": model, "messages": messages });
    b[flavor.max_tokens_field] = json!(req.max_tokens);
    if let Some(effort) = flavor.reasoning_effort {
        b["reasoning_effort"] = json!(effort);
    }
    b
}

fn parse(v: &Value, fallback_model: &str) -> Result<CompletionResponse, ProviderError> {
    let choice = v.pointer("/choices/0").ok_or_else(|| ProviderError::Unavailable("resposta sem `choices`".into()))?;
    let text = choice.pointer("/message/content").and_then(Value::as_str).unwrap_or("").to_string();
    if text.trim().is_empty() {
        let reason = choice.get("finish_reason").and_then(Value::as_str).unwrap_or("desconhecido");
        let refusal = choice.pointer("/message/refusal").and_then(Value::as_str);
        return Err(match refusal {
            Some(r) => ProviderError::Refused(http::redact_tokens(r)),
            None => ProviderError::Unavailable(format!("resposta vazia (motivo: {reason})")),
        });
    }
    let model = v.get("model").and_then(Value::as_str).unwrap_or(fallback_model).to_string();
    Ok(CompletionResponse { text, model })
}

/// Modelo de embeddings (vetores de ML) usado na memória dos corvos.
pub const EMBEDDING_MODEL: &str = "text-embedding-3-small";

/// Lê a resposta de `POST /embeddings` na ordem de `index`.
fn parse_embeddings(v: &Value) -> Result<Vec<Vec<f32>>, ProviderError> {
    let data = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::Unavailable("resposta de embeddings sem `data`".into()))?;
    let mut out: Vec<(i64, Vec<f32>)> = data
        .iter()
        .map(|d| {
            let i = d.get("index").and_then(Value::as_i64).unwrap_or(0);
            let vetor = d
                .get("embedding")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_f64).map(|x| x as f32).collect())
                .unwrap_or_default();
            (i, vetor)
        })
        .collect();
    out.sort_by_key(|(i, _)| *i);
    Ok(out.into_iter().map(|(_, v)| v).collect())
}

/// Gera embeddings (vetores) para `textos` com a API da OpenAI, usando a chave do cofre.
/// `dimensoes` encurta o vetor (o `text-embedding-3-small` aceita, por exemplo, 256), para
/// a memória dos corvos caber em poucos MB.
pub async fn embeddings(
    keys: &dyn KeySource,
    textos: &[String],
    dimensoes: u32,
) -> Result<Vec<Vec<f32>>, ProviderError> {
    let key = http::secret_header(keys, OPENAI.id, |k| format!("Bearer {k}"))?;
    let client = http::client()?;
    let body = json!({ "model": EMBEDDING_MODEL, "input": textos, "dimensions": dimensoes });
    let url = format!("{}/embeddings", OPENAI.base_url);
    match http::post_json(&client, &url, vec![(AUTHORIZATION, key)], &body).await {
        Ok(v) => parse_embeddings(&v),
        Err(HttpFailure::Network(e)) => Err(e),
        Err(HttpFailure::Status { status, retry_after, body }) => {
            Err(classify(status, retry_after, &body, EMBEDDING_MODEL))
        }
    }
}

/// Modelos de transcrição (voz → texto) da OpenAI, na ordem em que são tentados.
pub const TRANSCRICAO_OPENAI: &[&str] = &["gpt-4o-mini-transcribe", "whisper-1"];
/// Modelos de transcrição do Groq (Whisper aberto, rápido e barato).
pub const TRANSCRICAO_GROQ: &[&str] = &["whisper-large-v3-turbo", "whisper-large-v3"];

/// Tipo do arquivo de áudio pela extensão.
pub fn mime_de_audio(nome: &str) -> &'static str {
    match nome.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "m4a" | "mp4" | "aac" => "audio/mp4",
        "wav" => "audio/wav",
        "ogg" | "opus" | "oga" => "audio/ogg",
        "mp3" | "mpga" | "mpeg" => "audio/mpeg",
        "webm" => "audio/webm",
        "flac" => "audio/flac",
        "amr" => "audio/amr",
        _ => "application/octet-stream",
    }
}

/// Monta um corpo `multipart/form-data` com campos de texto e um arquivo.
pub fn multipart(fronteira: &str, campos: &[(&str, &str)], arquivo: (&str, &str, &[u8])) -> Vec<u8> {
    let mut b = Vec::new();
    for (nome, valor) in campos {
        b.extend_from_slice(
            format!("--{fronteira}\r\nContent-Disposition: form-data; name=\"{nome}\"\r\n\r\n{valor}\r\n").as_bytes(),
        );
    }
    let (nome, mime, dados) = arquivo;
    let nome: String = nome.chars().filter(|c| *c != '"' && *c != '\r' && *c != '\n').collect();
    b.extend_from_slice(
        format!(
            "--{fronteira}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{nome}\"\r\nContent-Type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    b.extend_from_slice(dados);
    b.extend_from_slice(format!("\r\n--{fronteira}--\r\n").as_bytes());
    b
}

/// Transcreve um áudio (voz → texto) com a API de `/audio/transcriptions` do serviço
/// (OpenAI ou Groq), tentando os modelos em ordem. `idioma` é o código ISO (ex.: "pt").
pub async fn transcrever(
    keys: &dyn KeySource,
    flavor: Flavor,
    modelos: &[&str],
    audio: &[u8],
    nome_arquivo: &str,
    idioma: Option<&str>,
) -> Result<String, ProviderError> {
    let key = http::secret_header(keys, flavor.id, |k| format!("Bearer {k}"))?;
    let client = http::client()?;
    let url = format!("{}/audio/transcriptions", flavor.base_url);
    let fronteira = format!(
        "dotcrow-{:x}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(7)
    );
    let mime = mime_de_audio(nome_arquivo);
    let mut ultimo = ProviderError::NotConfigured;
    for modelo in modelos {
        let mut campos = vec![("model", *modelo), ("response_format", "json")];
        if let Some(l) = idioma {
            campos.push(("language", l));
        }
        let corpo = multipart(&fronteira, &campos, (nome_arquivo, mime, audio));
        let tipo = format!("multipart/form-data; boundary={fronteira}");
        match http::post_bytes(&client, &url, vec![(AUTHORIZATION, key.clone())], &tipo, corpo).await {
            Ok(v) => {
                return v
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|t| t.trim().to_string())
                    .ok_or_else(|| ProviderError::Unavailable("transcrição sem `text`".into()));
            }
            Err(HttpFailure::Network(e)) => return Err(e),
            Err(HttpFailure::Status { status, retry_after, body }) => {
                let e = classify(status, retry_after, &body, modelo);
                // Modelo que este serviço não tem: tenta o próximo.
                if matches!(e, ProviderError::Misconfigured(_) | ProviderError::BadRequest(_)) {
                    ultimo = e;
                    continue;
                }
                return Err(e);
            }
        }
    }
    Err(ultimo)
}

fn classify(status: u16, retry_after: Option<std::time::Duration>, body: &str, model: &str) -> ProviderError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let code = parsed
        .as_ref()
        .and_then(|v| v.pointer("/error/code").or_else(|| v.pointer("/error/type")))
        .and_then(Value::as_str)
        .unwrap_or("");
    let detail = http::error_detail(body);
    let lower = detail.to_lowercase();
    // A OpenAI avisa cota esgotada com 429 e o código `insufficient_quota`;
    // o DeepSeek usa 402 "Insufficient Balance"; o OpenRouter, 402.
    if code == "insufficient_quota" || lower.contains("insufficient balance") || lower.contains("insufficient credits")
    {
        return ProviderError::QuotaExhausted;
    }
    if code == "model_not_found" || (status == 404 && lower.contains("model")) {
        return ProviderError::Misconfigured(format!("modelo \"{model}\" não encontrado: {detail}"));
    }
    if code == "invalid_api_key" {
        return ProviderError::Unauthorized;
    }
    ProviderError::from_http_status(status, retry_after, &detail)
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompatible {
    fn id(&self) -> &str {
        self.flavor.id
    }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let mut headers: Vec<(HeaderName, HeaderValue)> = Vec::new();
        match http::secret_header(self.keys.as_ref(), self.flavor.id, |k| format!("Bearer {k}")) {
            Ok(h) => headers.push((AUTHORIZATION, h)),
            // Provedores locais (Ollama) funcionam sem chave.
            Err(ProviderError::NotConfigured) if !self.flavor.needs_key => {}
            Err(e) => return Err(e),
        }
        if self.flavor.id == OPENROUTER.id {
            // O OpenRouter usa estes cabeçalhos para mostrar o app no painel dele.
            headers.push((HeaderName::from_static("http-referer"), HeaderValue::from_static("https://dotcrow.si")));
            headers.push((HeaderName::from_static("x-title"), HeaderValue::from_static("DotCrow")));
        }
        let url = format!("{}/chat/completions", self.base_url);
        match http::post_json(&self.client, &url, headers, &body(&self.flavor, &self.model, req)).await {
            Ok(v) => parse(&v, &self.model),
            Err(HttpFailure::Network(e)) => Err(e),
            Err(HttpFailure::Status { status, retry_after, body }) => {
                Err(classify(status, retry_after, &body, &self.model))
            }
        }
    }

    async fn complete_streaming(
        &self,
        req: &CompletionRequest,
        on_delta: crate::OnDelta<'_>,
    ) -> Result<CompletionResponse, ProviderError> {
        let mut headers: Vec<(HeaderName, HeaderValue)> = Vec::new();
        match http::secret_header(self.keys.as_ref(), self.flavor.id, |k| format!("Bearer {k}")) {
            Ok(h) => headers.push((AUTHORIZATION, h)),
            Err(ProviderError::NotConfigured) if !self.flavor.needs_key => {}
            Err(e) => return Err(e),
        }
        if self.flavor.id == OPENROUTER.id {
            headers.push((HeaderName::from_static("http-referer"), HeaderValue::from_static("https://dotcrow.si")));
            headers.push((HeaderName::from_static("x-title"), HeaderValue::from_static("DotCrow")));
        }
        let url = format!("{}/chat/completions", self.base_url);
        let mut corpo = body(&self.flavor, &self.model, req);
        corpo["stream"] = Value::Bool(true);

        let mut acc = String::new();
        let mut modelo_visto = self.model.clone();
        let resultado = {
            let mut on_event = |dados: &str| {
                if dados == "[DONE]" {
                    return;
                }
                let Ok(v) = serde_json::from_str::<Value>(dados) else { return };
                if let Some(m) = v.get("model").and_then(Value::as_str) {
                    modelo_visto = m.to_string();
                }
                if let Some(t) = v.pointer("/choices/0/delta/content").and_then(Value::as_str) {
                    acc.push_str(t);
                    on_delta(t);
                }
            };
            http::post_sse(&self.client, &url, headers, &corpo, &mut on_event).await
        };
        match resultado {
            Ok(()) => {}
            Err(HttpFailure::Network(e)) => return Err(e),
            Err(HttpFailure::Status { status, retry_after, body }) => {
                return Err(classify(status, retry_after, &body, &self.model));
            }
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

    #[test]
    fn monta_multipart_com_audio() {
        let b = multipart("XYZ", &[("model", "whisper-1")], ("fala.m4a", mime_de_audio("fala.m4a"), b"\x00\x01"));
        let t = String::from_utf8_lossy(&b);
        assert!(t.starts_with("--XYZ\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-1\r\n"));
        assert!(t.contains("filename=\"fala.m4a\"\r\nContent-Type: audio/mp4\r\n\r\n"));
        assert!(t.ends_with("\r\n--XYZ--\r\n"));
        assert_eq!(mime_de_audio("x.WAV"), "audio/wav");
    }

    #[test]
    fn reads_embeddings_in_index_order() {
        let v = json!({"data":[{"index":1,"embedding":[0.5,0.25]},{"index":0,"embedding":[1.0,0.0]}]});
        let e = parse_embeddings(&v).unwrap_or_default();
        assert_eq!(e, vec![vec![1.0, 0.0], vec![0.5, 0.25]]);
        assert!(parse_embeddings(&json!({})).is_err());
    }
    use crate::Message;

    fn req() -> CompletionRequest {
        CompletionRequest {
            messages: vec![
                Message { role: Role::System, content: "s".into() },
                Message { role: Role::User, content: "u".into() },
            ],
            max_tokens: 50,
            tier: crate::Tier::Standard,
        }
    }

    #[test]
    fn openai_uses_max_completion_tokens_and_low_effort() {
        let b = body(&OPENAI, "gpt-x", &req());
        assert_eq!(b["max_completion_tokens"], 50);
        assert!(b.get("max_tokens").is_none());
        assert_eq!(b["reasoning_effort"], "low");
        assert_eq!(b["messages"][0]["role"], "system");
    }

    #[test]
    fn compatible_services_use_max_tokens() {
        for f in [OPENROUTER, DEEPSEEK, GROQ, OLLAMA] {
            let b = body(&f, "m", &req());
            assert_eq!(b["max_tokens"], 50, "{}", f.id);
            assert!(b.get("reasoning_effort").is_none());
        }
    }

    #[test]
    fn parses_choices() {
        let v = json!({"model":"gpt-x-2026","choices":[{"message":{"role":"assistant","content":"Oi"},"finish_reason":"stop"}]});
        assert_eq!(parse(&v, "m").unwrap().text, "Oi");
        let empty = json!({"choices":[{"message":{"content":null},"finish_reason":"length"}]});
        assert!(matches!(parse(&empty, "m"), Err(ProviderError::Unavailable(_))));
        let refused = json!({"choices":[{"message":{"content":null,"refusal":"não posso"}}]});
        assert!(matches!(parse(&refused, "m"), Err(ProviderError::Refused(_))));
    }

    #[test]
    fn classifies_errors() {
        let quota = r#"{"error":{"message":"You exceeded your current quota","type":"insufficient_quota","code":"insufficient_quota"}}"#;
        assert_eq!(classify(429, None, quota, "m"), ProviderError::QuotaExhausted);
        let rate = r#"{"error":{"message":"Rate limit reached","type":"requests","code":"rate_limit_exceeded"}}"#;
        assert!(matches!(classify(429, None, rate, "m"), ProviderError::RateLimited { .. }));
        assert_eq!(
            classify(402, None, r#"{"error":{"message":"Insufficient Balance"}}"#, "m"),
            ProviderError::QuotaExhausted
        );
        let model = r#"{"error":{"message":"The model `x` does not exist","code":"model_not_found"}}"#;
        assert!(matches!(classify(404, None, model, "m"), ProviderError::Misconfigured(_)));
        let key = r#"{"error":{"message":"Incorrect API key provided: sk-abc","code":"invalid_api_key"}}"#;
        assert_eq!(classify(401, None, key, "m"), ProviderError::Unauthorized);
        assert!(matches!(classify(400, None, r#"{"error":{"message":"bad"}}"#, "m"), ProviderError::BadRequest(_)));
    }
}
