use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{CompletionRequest, CompletionResponse, LlmProvider, ProviderError};

/// Uma tentativa feita pelo roteador, para registro e para mostrar ao dono.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    /// Provedor tentado.
    pub provider: String,
    /// Erro que fez o roteador seguir adiante, ou `None` se foi pulado por estar de castigo.
    pub error: Option<ProviderError>,
}

/// Resposta bem-sucedida com o histórico de tentativas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routed {
    /// Provedor que respondeu.
    pub provider: String,
    /// A resposta.
    pub response: CompletionResponse,
    /// Tentativas anteriores que falharam ou foram puladas.
    pub attempts: Vec<Attempt>,
}

/// Erros do roteador.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouterError {
    /// Nenhum provedor configurado.
    #[error("nenhum provedor de LLM configurado; adicione uma chave com `dotcrow chaves adicionar`")]
    NoProviders,
    /// O pedido é inválido; não adianta trocar de provedor.
    #[error("pedido recusado por {provider}: {error}")]
    BadRequest {
        /// Provedor que recusou.
        provider: String,
        /// Erro retornado.
        error: ProviderError,
    },
    /// Todos os provedores falharam ou estão de castigo.
    #[error("todos os provedores falharam: {}", describe(attempts))]
    AllFailed {
        /// Tentativas feitas.
        attempts: Vec<Attempt>,
    },
}

fn describe(attempts: &[Attempt]) -> String {
    let parts: Vec<String> = attempts
        .iter()
        .map(|a| match &a.error {
            Some(e) => format!("{}: {e}", a.provider),
            None => format!("{}: de castigo depois de uma falha recente", a.provider),
        })
        .collect();
    if parts.is_empty() { "nenhuma tentativa".into() } else { parts.join("; ") }
}

/// Roteador com troca automática de provedor.
pub struct Router {
    providers: Vec<Arc<dyn LlmProvider>>,
    cooldown_until: Mutex<HashMap<String, Instant>>,
}

impl Router {
    /// Cria um roteador com os provedores na ordem de preferência.
    pub fn new(providers: Vec<Arc<dyn LlmProvider>>) -> Self {
        Self { providers, cooldown_until: Mutex::new(HashMap::new()) }
    }

    /// Ids dos provedores, na ordem de preferência.
    pub fn order(&self) -> Vec<String> {
        self.providers.iter().map(|p| p.id().to_string()).collect()
    }

    /// Provedores de castigo agora e quanto falta para voltarem.
    pub fn cooling_down(&self) -> Vec<(String, Duration)> {
        let now = Instant::now();
        let map = self.cooldown_until.lock().unwrap_or_else(|e| e.into_inner());
        let mut v: Vec<_> =
            map.iter().filter(|(_, until)| **until > now).map(|(id, until)| (id.clone(), *until - now)).collect();
        v.sort();
        v
    }

    fn is_cooling(&self, id: &str) -> bool {
        let map = self.cooldown_until.lock().unwrap_or_else(|e| e.into_inner());
        map.get(id).is_some_and(|until| *until > Instant::now())
    }

    fn set_cooldown(&self, id: &str, d: Duration) {
        if d.is_zero() {
            return;
        }
        let mut map = self.cooldown_until.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(id.to_string(), Instant::now() + d);
    }

    /// Envia o pedido ao primeiro provedor disponível, trocando sozinho quando um falha.
    ///
    /// Se todos estiverem de castigo, tenta mesmo assim o que volta mais cedo, para
    /// não deixar o dono sem resposta à toa.
    pub async fn complete(&self, request: &CompletionRequest) -> Result<Routed, RouterError> {
        if self.providers.is_empty() {
            return Err(RouterError::NoProviders);
        }
        let mut attempts = Vec::new();
        let mut candidates: Vec<&Arc<dyn LlmProvider>> = Vec::new();
        for p in &self.providers {
            if self.is_cooling(p.id()) {
                attempts.push(Attempt { provider: p.id().to_string(), error: None });
            } else {
                candidates.push(p);
            }
        }
        if candidates.is_empty() {
            let soonest = self.cooling_down().into_iter().min_by_key(|(_, d)| *d).map(|(id, _)| id);
            if let Some(p) = soonest.and_then(|id| self.providers.iter().find(|p| p.id() == id)) {
                candidates.push(p);
            }
        }

        for p in candidates {
            match p.complete(request).await {
                Ok(response) => {
                    return Ok(Routed { provider: p.id().to_string(), response, attempts });
                }
                Err(error) if !error.should_failover() => {
                    return Err(RouterError::BadRequest { provider: p.id().to_string(), error });
                }
                Err(error) => {
                    self.set_cooldown(p.id(), error.cooldown());
                    attempts.push(Attempt { provider: p.id().to_string(), error: Some(error) });
                }
            }
        }
        Err(RouterError::AllFailed { attempts })
    }

    /// Como [`Router::complete`], mas transmitindo o texto ao vivo por `on_delta`. Troca
    /// de provedor só enquanto nada foi transmitido; uma falha no meio do fluxo não pode
    /// ser desfeita, então vira erro em vez de recomeçar em outro provedor.
    pub async fn complete_streaming(
        &self,
        request: &CompletionRequest,
        on_delta: crate::OnDelta<'_>,
    ) -> Result<Routed, RouterError> {
        use std::sync::atomic::{AtomicBool, Ordering};
        if self.providers.is_empty() {
            return Err(RouterError::NoProviders);
        }
        let mut attempts = Vec::new();
        let mut candidates: Vec<&Arc<dyn LlmProvider>> = Vec::new();
        for p in &self.providers {
            if self.is_cooling(p.id()) {
                attempts.push(Attempt { provider: p.id().to_string(), error: None });
            } else {
                candidates.push(p);
            }
        }
        if candidates.is_empty()
            && let Some(id) = self.cooling_down().into_iter().min_by_key(|(_, d)| *d).map(|(id, _)| id)
            && let Some(p) = self.providers.iter().find(|p| p.id() == id)
        {
            candidates.push(p);
        }

        for p in candidates {
            let comecou = AtomicBool::new(false);
            let r = {
                let mut wrap = |s: &str| {
                    comecou.store(true, Ordering::Relaxed);
                    on_delta(s);
                };
                p.complete_streaming(request, &mut wrap).await
            };
            match r {
                Ok(response) => return Ok(Routed { provider: p.id().to_string(), response, attempts }),
                Err(error) if !error.should_failover() => {
                    return Err(RouterError::BadRequest { provider: p.id().to_string(), error });
                }
                Err(error) => {
                    let ja_transmitiu = comecou.load(Ordering::Relaxed);
                    let recusa = matches!(error, ProviderError::Refused(_));
                    self.set_cooldown(p.id(), error.cooldown());
                    attempts.push(Attempt { provider: p.id().to_string(), error: Some(error) });
                    if ja_transmitiu && !recusa {
                        return Err(RouterError::AllFailed { attempts });
                    }
                    if ja_transmitiu {
                        // Recusa no meio do texto: avisa e recomeça no próximo provedor,
                        // em vez de deixar a tarefa parada.
                        on_delta(&format!("\n\n[{} recusou; continuando com o próximo provedor]\n\n", p.id()));
                    }
                }
            }
        }
        Err(RouterError::AllFailed { attempts })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockProvider;
    use crate::{Message, Role};

    fn req() -> CompletionRequest {
        CompletionRequest {
            messages: vec![Message { role: Role::User, content: "oi".into() }],
            max_tokens: 64,
            tier: crate::Tier::Standard,
        }
    }

    #[tokio::test]
    async fn uses_first_provider_when_healthy() {
        let r = Router::new(vec![MockProvider::ok("anthropic"), MockProvider::ok("openai")]);
        let out = r.complete(&req()).await.unwrap();
        assert_eq!(out.provider, "anthropic");
        assert!(out.attempts.is_empty());
    }

    #[tokio::test]
    async fn fails_over_when_quota_runs_out_and_remembers() {
        let claude = MockProvider::failing("anthropic", ProviderError::QuotaExhausted);
        let gpt = MockProvider::ok("openai");
        let r = Router::new(vec![claude.clone(), gpt.clone()]);

        let first = r.complete(&req()).await.unwrap();
        assert_eq!(first.provider, "openai");
        assert_eq!(first.attempts[0].error, Some(ProviderError::QuotaExhausted));

        // No segundo pedido o Claude está de castigo e nem é chamado.
        let second = r.complete(&req()).await.unwrap();
        assert_eq!(second.provider, "openai");
        assert_eq!(second.attempts[0].error, None);
        assert_eq!(claude.calls(), 1);
        assert_eq!(gpt.calls(), 2);
    }

    #[tokio::test]
    async fn bad_request_stops_without_failover() {
        let a = MockProvider::failing("anthropic", ProviderError::BadRequest("muito longo".into()));
        let b = MockProvider::ok("openai");
        let r = Router::new(vec![a, b.clone()]);
        assert!(matches!(r.complete(&req()).await, Err(RouterError::BadRequest { .. })));
        assert_eq!(b.calls(), 0);
    }

    #[tokio::test]
    async fn refusal_passes_to_the_next_provider_without_cooldown() {
        let claude = MockProvider::failing("anthropic", ProviderError::Refused("filtro".into()));
        let gpt = MockProvider::ok("openai");
        let r = Router::new(vec![claude.clone(), gpt.clone()]);
        let out = r.complete(&req()).await.unwrap();
        assert_eq!(out.provider, "openai");
        // Sem castigo: o próximo pedido tenta o Claude de novo.
        let _ = r.complete(&req()).await;
        assert_eq!(claude.calls(), 2);
    }

    #[tokio::test]
    async fn all_failed_reports_every_attempt() {
        let r = Router::new(vec![
            MockProvider::failing("anthropic", ProviderError::Unauthorized),
            MockProvider::failing("openai", ProviderError::Unavailable("timeout".into())),
        ]);
        match r.complete(&req()).await {
            Err(RouterError::AllFailed { attempts }) => assert_eq!(attempts.len(), 2),
            other => panic!("esperava AllFailed, veio {other:?}"),
        }
    }

    #[tokio::test]
    async fn when_everyone_is_cooling_try_the_soonest() {
        let a = MockProvider::failing("anthropic", ProviderError::QuotaExhausted);
        let b =
            MockProvider::failing("openai", ProviderError::RateLimited { retry_after: Some(Duration::from_secs(1)) });
        let r = Router::new(vec![a.clone(), b.clone()]);
        let _ = r.complete(&req()).await;
        let _ = r.complete(&req()).await;
        assert_eq!(b.calls(), 2, "o provedor que volta mais cedo deve ser tentado");
        assert_eq!(a.calls(), 1);
    }

    #[tokio::test]
    async fn empty_router() {
        assert_eq!(Router::new(vec![]).complete(&req()).await.unwrap_err(), RouterError::NoProviders);
    }
}
