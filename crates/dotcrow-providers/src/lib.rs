//! Provedores de LLM e o roteador com troca automática.
//!
//! Qualquer LLM com API pode virar um provedor implementando [`LlmProvider`].
//! O [`Router`] tenta os provedores na ordem de preferência do dono. Quando um
//! deles acaba a cota, bate no limite de uso, perde a autorização ou cai, o
//! roteador **troca para o próximo sozinho** e põe o que falhou de "castigo" por
//! um tempo, para não insistir nele a cada pedido.
//!
//! Erros que são culpa do pedido (por exemplo, um pedido mal formado) **não**
//! causam troca: o próximo provedor falharia do mesmo jeito.
//!
//! Adaptadores reais: [`anthropic`] (Claude), [`openai`] (OpenAI e todas as APIs
//! compatíveis: OpenRouter, DeepSeek, Groq, Ollama) e [`gemini`]. O [`catalog`]
//! lista os provedores conhecidos e monta cada um. Veja `docs/provedores.md`.
//!
//! Os adaptadores nunca guardam a chave de API: eles pedem ao [`KeySource`] (o
//! cofre) só na hora de montar o cabeçalho de cada requisição.

pub mod anthropic;
pub mod catalog;
pub mod gemini;
mod http;
pub mod mock;
pub mod openai;
mod router;
pub mod tls;

pub use router::{Attempt, Routed, Router, RouterError};

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Quem fala em uma mensagem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Instruções de sistema (perfil do agente).
    System,
    /// O dono do computador ou o orquestrador.
    User,
    /// A resposta do modelo.
    Assistant,
}

/// Uma mensagem da conversa com o modelo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Papel de quem fala.
    pub role: Role,
    /// Conteúdo em texto.
    pub content: String,
}

/// Porte do modelo pedido para uma etapa: do mais leve e barato ao mais forte.
/// Cada provedor escolhe o modelo da sua faixa (se o dono não fixou um modelo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Leve e rápido (ex.: Haiku), para passos pequenos e repetitivos.
    Fast,
    /// Equilíbrio (ex.: Sonnet), para o trabalho do dia a dia.
    #[default]
    Standard,
    /// O mais forte (ex.: Opus), para planejar e revisar o que é difícil.
    Deep,
}

/// Pedido de completamento, no formato comum a todos os provedores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Histórico da conversa.
    pub messages: Vec<Message>,
    /// Limite de tokens na resposta.
    pub max_tokens: u32,
    /// Porte do modelo desejado para esta etapa.
    #[serde(default)]
    pub tier: Tier,
}

/// Resposta de um provedor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Texto gerado.
    pub text: String,
    /// Modelo que respondeu, como informado pelo provedor.
    pub model: String,
}

/// Erros de provedor, já classificados para o roteador decidir se troca ou não.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// Cota ou créditos acabaram.
    #[error("cota ou créditos esgotados")]
    QuotaExhausted,
    /// Limite de requisições atingido; pode informar quanto esperar.
    #[error("limite de requisições atingido")]
    RateLimited {
        /// Tempo sugerido pelo provedor antes de tentar de novo.
        retry_after: Option<Duration>,
    },
    /// Chave inválida, revogada ou sem permissão.
    #[error("chave de API recusada")]
    Unauthorized,
    /// Provedor fora do ar, sobrecarregado ou rede indisponível.
    #[error("provedor indisponível: {0}")]
    Unavailable(String),
    /// O pedido é inválido; trocar de provedor não resolve.
    #[error("pedido inválido: {0}")]
    BadRequest(String),
    /// O modelo se recusou a responder (filtro de segurança do provedor). Outro
    /// provedor pode responder normalmente, então o roteador troca — sem castigo,
    /// porque o próximo pedido pode passar.
    #[error("o modelo recusou: {0}")]
    Refused(String),
    /// Não há chave configurada para este provedor.
    #[error("nenhuma chave configurada")]
    NotConfigured,
    /// Configuração do provedor errada (modelo que não existe, endereço inválido).
    /// Outro provedor pode responder, então o roteador troca.
    #[error("configuração inválida: {0}")]
    Misconfigured(String),
}

impl ProviderError {
    /// Se vale a pena tentar o próximo provedor.
    pub fn should_failover(&self) -> bool {
        !matches!(self, ProviderError::BadRequest(_))
    }

    /// Por quanto tempo deixar o provedor de lado depois deste erro.
    pub fn cooldown(&self) -> Duration {
        match self {
            ProviderError::QuotaExhausted => Duration::from_secs(60 * 60),
            ProviderError::RateLimited { retry_after } => retry_after.unwrap_or(Duration::from_secs(30)),
            ProviderError::Unauthorized | ProviderError::NotConfigured | ProviderError::Misconfigured(_) => {
                Duration::from_secs(10 * 60)
            }
            ProviderError::Unavailable(_) => Duration::from_secs(20),
            ProviderError::BadRequest(_) | ProviderError::Refused(_) => Duration::ZERO,
        }
    }

    /// Classificação genérica a partir do status HTTP. Cada adaptador pode refinar
    /// olhando o corpo da resposta (alguns provedores avisam "sem créditos" com 400).
    pub fn from_http_status(status: u16, retry_after: Option<Duration>, detail: &str) -> Self {
        match status {
            401 | 403 => ProviderError::Unauthorized,
            402 => ProviderError::QuotaExhausted,
            404 => ProviderError::Misconfigured(format!("HTTP 404: {detail}")),
            429 => ProviderError::RateLimited { retry_after },
            408 | 500..=599 => ProviderError::Unavailable(format!("HTTP {status}")),
            _ => ProviderError::BadRequest(format!("HTTP {status}: {detail}")),
        }
    }
}

/// De onde os adaptadores pegam a chave de API, **só na hora** de montar a requisição.
///
/// O cofre implementa isto. A chave passa para `f` e não deve ser guardada em
/// campos, logs ou mensagens de erro.
pub trait KeySource: Send + Sync {
    /// Chama `f` com a chave do `provider`. Devolve `false` se não houver chave.
    fn with_key(&self, provider: &str, f: &mut dyn FnMut(&str)) -> bool;
}

/// Fonte sem nenhuma chave, para provedores locais e testes.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoKeys;

impl KeySource for NoKeys {
    fn with_key(&self, _provider: &str, _f: &mut dyn FnMut(&str)) -> bool {
        false
    }
}

/// Recebe pedaços do texto conforme o modelo vai gerando (streaming ao vivo).
pub type OnDelta<'a> = &'a mut (dyn FnMut(&str) + Send);

/// Um provedor de LLM (Claude, GPT, Gemini, modelos locais...).
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Identificador estável, igual ao usado no cofre (`anthropic`, `openai`...).
    fn id(&self) -> &str;

    /// Gera uma resposta.
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, ProviderError>;

    /// Gera uma resposta transmitindo o texto ao vivo: `on_delta` recebe cada pedaço
    /// assim que chega. O padrão não transmite de verdade — chama [`complete`] e entrega
    /// o texto inteiro de uma vez; provedores que suportam SSE sobrescrevem isto.
    async fn complete_streaming(
        &self,
        request: &CompletionRequest,
        on_delta: OnDelta<'_>,
    ) -> Result<CompletionResponse, ProviderError> {
        let r = self.complete(request).await?;
        on_delta(&r.text);
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_classification() {
        assert_eq!(ProviderError::from_http_status(401, None, ""), ProviderError::Unauthorized);
        assert_eq!(ProviderError::from_http_status(402, None, ""), ProviderError::QuotaExhausted);
        assert!(matches!(ProviderError::from_http_status(429, None, ""), ProviderError::RateLimited { .. }));
        assert!(matches!(ProviderError::from_http_status(529, None, ""), ProviderError::Unavailable(_)));
        assert!(matches!(ProviderError::from_http_status(400, None, "x"), ProviderError::BadRequest(_)));
        assert!(matches!(ProviderError::from_http_status(404, None, "x"), ProviderError::Misconfigured(_)));
    }

    #[test]
    fn bad_request_does_not_failover() {
        assert!(!ProviderError::BadRequest("x".into()).should_failover());
        assert!(ProviderError::QuotaExhausted.should_failover());
        assert!(ProviderError::Misconfigured("modelo".into()).should_failover());
    }

    #[test]
    fn refusal_fails_over_without_cooldown() {
        let e = ProviderError::Refused("filtro".into());
        assert!(e.should_failover());
        assert_eq!(e.cooldown(), Duration::ZERO);
    }
}
