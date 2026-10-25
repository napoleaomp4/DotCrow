//! Dados locais do DotCrow: cofre de chaves, configuração dos provedores e o roteador
//! montado a partir deles. Usado pela CLI e pelo painel.

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use anyhow::{Context, Result};
use dotcrow_crypto::sym;
use dotcrow_providers::{
    KeySource, LlmProvider, ProviderError, Router,
    catalog::{self, PROVIDERS, ProviderSettings},
};
use dotcrow_vault::{KeyInfo, Vault, write_private};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Pasta de dados padrão do usuário.
pub fn default_data_dir() -> Result<PathBuf> {
    directories::ProjectDirs::from("si", "dotcrow", "dotcrow")
        .map(|d| d.data_dir().to_path_buf())
        .context("não encontrei a pasta de dados do usuário; use --home")
}

/// Chave mestra do cofre.
///
/// TEMPORÁRIO: fica num arquivo 0600 ao lado do cofre. A issue "Guardar a chave mestra
/// no chaveiro do sistema" troca isso pelo Keychain (macOS), Credential Manager
/// (Windows) e Secret Service (Linux).
fn master_key(dir: &Path) -> Result<sym::SymKey> {
    let path = dir.join("master.key");
    match std::fs::read(&path) {
        Ok(bytes) => {
            let arr: [u8; 32] = bytes.as_slice().try_into().context("master.key corrompida")?;
            Ok(Zeroizing::new(arr))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let k = sym::generate_key()?;
            write_private(&path, k.as_ref())?;
            Ok(k)
        }
        Err(e) => Err(e.into()),
    }
}

/// Configuração dos provedores escolhida pelo dono (`config.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Ordem de preferência. Provedores com chave que não estão aqui entram depois.
    #[serde(default)]
    pub order: Vec<String>,
    /// Modelo escolhido por provedor.
    #[serde(default)]
    pub models: BTreeMap<String, String>,
    /// Endereço da API por provedor (por exemplo, Ollama em outra máquina).
    #[serde(default)]
    pub urls: BTreeMap<String, String>,
    /// Conta (domínio) de cada conector que muda de endereço por dono (Shopify, Jira...).
    /// Não é segredo: a chave fica no cofre.
    #[serde(default)]
    pub conectores: BTreeMap<String, String>,
}

/// Situação de um provedor, para listar na CLI e no painel.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderStatus {
    /// Id.
    pub id: &'static str,
    /// Nome.
    pub name: &'static str,
    /// Modelo em uso.
    pub model: String,
    /// Se o modelo é o padrão.
    pub default_model: bool,
    /// Modelo padrão do catálogo.
    pub catalog_model: &'static str,
    /// Endereço em uso, se o dono trocou.
    pub custom_url: Option<String>,
    /// Se precisa de chave.
    pub needs_key: bool,
    /// Se já tem chave no cofre.
    pub has_key: bool,
    /// Posição na fila do roteador (1 = primeiro), se estiver ativo.
    pub position: Option<usize>,
    /// Onde conseguir a chave.
    pub key_url: &'static str,
}

/// Nomes populares que as pessoas usam no lugar do id.
pub fn canonical_provider(name: &str) -> String {
    let n = name.trim().to_lowercase();
    match n.as_str() {
        "claude" => "anthropic",
        "gpt" | "chatgpt" => "openai",
        "google" => "gemini",
        other => other,
    }
    .to_string()
}

/// O cofre e a configuração, prontos para uso concorrente.
pub struct Store {
    dir: PathBuf,
    vault: Arc<RwLock<Vault>>,
    config: RwLock<Config>,
}

/// Ponte entre o cofre e os adaptadores: empresta a chave só na hora da requisição.
struct VaultKeys(Arc<RwLock<Vault>>);

impl KeySource for VaultKeys {
    fn with_key(&self, provider: &str, f: &mut dyn FnMut(&str)) -> bool {
        let vault = self.0.read().unwrap_or_else(|e| e.into_inner());
        match vault.lease(provider) {
            Some(lease) => {
                lease.with_secret(|k| f(k));
                true
            }
            None => false,
        }
    }
}

impl Store {
    /// Abre (ou cria) os dados em `dir`.
    pub fn open(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir).with_context(|| format!("não consegui criar {}", dir.display()))?;
        let key = master_key(&dir)?;
        let vault = Vault::open_file(dir.join("vault.bin"), key)?;
        let config = match std::fs::read(dir.join("config.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("config.json inválido")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self { dir, vault: Arc::new(RwLock::new(vault)), config: RwLock::new(config) })
    }

    /// Pasta de dados.
    /// A chave-mestra deste computador (para cifrar a memória, por exemplo).
    pub fn master_key(&self) -> Result<sym::SymKey> {
        master_key(&self.dir)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Fonte de chaves para os adaptadores.
    pub fn keys(&self) -> Arc<dyn KeySource> {
        Arc::new(VaultKeys(self.vault.clone()))
    }

    /// Chaves guardadas (só metadados).
    pub fn list_keys(&self) -> Vec<KeyInfo> {
        self.vault.read().unwrap_or_else(|e| e.into_inner()).list()
    }

    /// Guarda uma chave. Devolve os metadados e, se o nome foi corrigido, o id usado.
    pub fn add_key(&self, provider: &str, secret: Zeroizing<String>, label: Option<String>) -> Result<KeyInfo> {
        let provider = canonical_provider(provider);
        let mut vault = self.vault.write().unwrap_or_else(|e| e.into_inner());
        Ok(vault.add(&provider, secret, label)?)
    }

    /// Apaga uma chave pelo id.
    pub fn delete_key(&self, id: &str) -> Result<KeyInfo> {
        let mut vault = self.vault.write().unwrap_or_else(|e| e.into_inner());
        Ok(vault.delete(id)?)
    }

    /// Cópia da configuração atual.
    pub fn config(&self) -> Config {
        self.config.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Altera e grava a configuração.
    pub fn update_config(&self, change: impl FnOnce(&mut Config)) -> Result<Config> {
        let mut cfg = self.config.write().unwrap_or_else(|e| e.into_inner());
        let mut next = cfg.clone();
        change(&mut next);
        next.order = next.order.iter().map(|p| canonical_provider(p)).collect();
        let mut seen = HashSet::new();
        next.order.retain(|p| seen.insert(p.clone()));
        next.models.retain(|_, v| !v.trim().is_empty());
        next.urls.retain(|_, v| !v.trim().is_empty());
        write_private(&self.dir.join("config.json"), &serde_json::to_vec_pretty(&next)?)?;
        *cfg = next.clone();
        Ok(next)
    }

    fn providers_with_key(&self) -> HashSet<String> {
        self.list_keys().into_iter().map(|k| k.provider).collect()
    }

    /// Fila de provedores que o roteador vai usar, em ordem.
    pub fn active_order(&self) -> Vec<String> {
        let cfg = self.config();
        let with_key = self.providers_with_key();
        let usable = |id: &str| catalog::info(id).is_some_and(|p| !p.needs_key || with_key.contains(id));
        let mut order: Vec<String> = cfg.order.iter().filter(|id| usable(id)).cloned().collect();
        for p in &PROVIDERS {
            // Provedores sem chave (Ollama) só entram se o dono puser na ordem.
            if p.needs_key && with_key.contains(p.id) && !order.iter().any(|o| o == p.id) {
                order.push(p.id.to_string());
            }
        }
        order
    }

    /// Ajustes de um provedor vindos da configuração.
    pub fn settings(&self, id: &str) -> ProviderSettings {
        let cfg = self.config();
        ProviderSettings { model: cfg.models.get(id).cloned(), base_url: cfg.urls.get(id).cloned() }
    }

    /// Monta um provedor.
    pub fn provider(&self, id: &str) -> Result<Arc<dyn LlmProvider>, ProviderError> {
        catalog::build(id, self.keys(), &self.settings(id))
    }

    /// Monta um provedor, trocando o modelo só para esta chamada (sem gravar na config).
    pub fn provider_with(&self, id: &str, model: Option<&str>) -> Result<Arc<dyn LlmProvider>, ProviderError> {
        let mut s = self.settings(id);
        if let Some(m) = model.map(str::trim).filter(|m| !m.is_empty()) {
            s.model = Some(m.to_string());
        }
        catalog::build(id, self.keys(), &s)
    }

    /// Monta o roteador. Com `only`, usa só aquele provedor (mesmo sem estar na fila).
    pub fn router(&self, only: Option<&str>) -> Result<Router> {
        self.router_choice(only, None)
    }

    /// Como `router`, mas com um modelo escolhido só para esta tarefa. O modelo só
    /// é aplicado quando um provedor foi escolhido (`only`), para não misturar o
    /// modelo de um provedor com os outros da fila.
    pub fn router_choice(&self, only: Option<&str>, model: Option<&str>) -> Result<Router> {
        let ids = match only {
            Some(id) => {
                let id = canonical_provider(id);
                if catalog::info(&id).is_none() {
                    anyhow::bail!("provedor desconhecido: {id}. Veja a lista com: dotcrow provedores");
                }
                vec![id]
            }
            None => self.active_order(),
        };
        let model = only.and(model);
        let mut providers = Vec::new();
        for id in &ids {
            providers.push(self.provider_with(id, model).map_err(|e| anyhow::anyhow!("{id}: {e}"))?);
        }
        Ok(Router::new(providers))
    }

    /// O Oráculo: o Opus 5.5 da Anthropic, quando há chave. Ele lê cada pedido e decide
    /// quantos corvos trabalham. Sem a chave, `None` (quem chama usa a fila de sempre).
    pub fn oraculo(&self) -> Option<Router> {
        if !self.list_keys().iter().any(|k| k.provider == dotcrow_providers::anthropic::ID) {
            return None;
        }
        self.provider_with(dotcrow_providers::anthropic::ID, Some(dotcrow_providers::anthropic::DEEP_MODEL))
            .ok()
            .map(|p| Router::new(vec![p]))
    }

    /// Avisos úteis: chaves de provedores que o DotCrow não conhece, falta de chaves.
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        for k in self.list_keys() {
            if catalog::info(&k.provider).is_none() {
                out.push(format!(
                    "A chave {} é de \"{}\", que o DotCrow ainda não conhece. Ela fica guardada, mas não é usada.",
                    k.id, k.provider
                ));
            }
        }
        if self.active_order().is_empty() {
            out.push("Nenhum provedor ativo. Adicione uma chave de API para os corvos poderem pensar.".into());
        }
        out
    }

    /// Situação de todos os provedores do catálogo.
    pub fn provider_status(&self) -> Vec<ProviderStatus> {
        let cfg = self.config();
        let with_key = self.providers_with_key();
        let order = self.active_order();
        PROVIDERS
            .iter()
            .map(|p| {
                let model = cfg.models.get(p.id).cloned();
                ProviderStatus {
                    id: p.id,
                    name: p.name,
                    default_model: model.is_none(),
                    catalog_model: p.default_model,
                    model: model.unwrap_or_else(|| p.default_model.to_string()),
                    custom_url: cfg.urls.get(p.id).cloned(),
                    needs_key: p.needs_key,
                    has_key: with_key.contains(p.id),
                    position: order.iter().position(|o| o == p.id).map(|i| i + 1),
                    key_url: p.key_url,
                }
            })
            .collect()
    }
}

/// Resultado do teste de um provedor.
#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    /// Provedor testado.
    pub provider: String,
    /// Se respondeu.
    pub ok: bool,
    /// Modelo que respondeu (ou o configurado, se falhou).
    pub model: String,
    /// Tempo da resposta em milissegundos.
    pub ms: u128,
    /// Erro, se falhou.
    pub error: Option<String>,
    /// O que fazer, se falhou.
    pub hint: Option<String>,
}

impl Store {
    /// Provedores para testar: os ativos, ou só `only`.
    pub fn test_targets(&self, only: Option<&str>) -> Vec<String> {
        match only {
            Some(id) => vec![canonical_provider(id)],
            None => {
                let mut ids = self.active_order();
                // Mostra também quem tem chave mas está fora da fila (provedor sem chave não entra).
                for k in self.list_keys() {
                    if catalog::info(&k.provider).is_some() && !ids.contains(&k.provider) {
                        ids.push(k.provider);
                    }
                }
                ids
            }
        }
    }

    /// Manda um pedido mínimo para `id` e mede o tempo.
    pub async fn test_provider(&self, id: &str) -> TestResult {
        let configured = self
            .settings(id)
            .model
            .unwrap_or_else(|| catalog::info(id).map(|p| p.default_model.to_string()).unwrap_or_default());
        let fail = |e: ProviderError, ms| TestResult {
            provider: id.to_string(),
            ok: false,
            model: configured.clone(),
            ms,
            error: Some(e.to_string()),
            hint: Some(hint(id, &e)),
        };
        let provider = match self.provider(id) {
            Ok(p) => p,
            Err(e) => return fail(e, 0),
        };
        let req = dotcrow_providers::CompletionRequest {
            messages: vec![dotcrow_providers::Message {
                role: dotcrow_providers::Role::User,
                content: "Responda apenas com a palavra: funcionando".into(),
            }],
            max_tokens: 400,
            tier: dotcrow_providers::Tier::Fast,
        };
        let start = std::time::Instant::now();
        match provider.complete(&req).await {
            Ok(r) => TestResult {
                provider: id.to_string(),
                ok: true,
                model: r.model,
                ms: start.elapsed().as_millis(),
                error: None,
                hint: None,
            },
            Err(e) => fail(e, start.elapsed().as_millis()),
        }
    }
}

/// Dica do que fazer depois de um erro de provedor.
pub fn hint(provider: &str, e: &ProviderError) -> String {
    match e {
        ProviderError::Unauthorized => {
            format!(
                "A chave foi recusada. Confira se copiou inteira; se precisar, apague e adicione de novo para {provider}."
            )
        }
        ProviderError::NotConfigured => format!("Falta a chave. Adicione uma para {provider}."),
        ProviderError::QuotaExhausted => "A conta está sem créditos ou passou do limite do plano.".into(),
        ProviderError::RateLimited { .. } => "Muitos pedidos em pouco tempo. Espere um pouco e tente de novo.".into(),
        ProviderError::Misconfigured(_) => {
            format!("Troque o modelo ou o endereço de {provider} (ou volte para o padrão).")
        }
        ProviderError::Unavailable(_) if provider == "ollama" => {
            "O Ollama não respondeu. Ele está instalado e aberto? Baixou o modelo com `ollama pull`?".into()
        }
        ProviderError::Unavailable(_) => "Sem resposta do provedor. Confira a internet e tente de novo.".into(),
        ProviderError::BadRequest(_) => "O provedor recusou o pedido.".into(),
        ProviderError::Refused(_) => {
            "O modelo se recusou a responder (filtro de segurança do provedor). Com outro provedor na fila, o DotCrow passa para ele sozinho.".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Store, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("dotcrow-test-{}", std::process::id() as u64 * 1000 + rand_suffix()));
        (Store::open(dir.clone()).unwrap(), dir)
    }

    fn rand_suffix() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::SeqCst)
    }

    fn secret(s: &str) -> Zeroizing<String> {
        Zeroizing::new(s.to_string())
    }

    #[test]
    fn order_follows_config_then_catalog_and_aliases_work() {
        let (s, dir) = store();
        assert!(s.active_order().is_empty());
        s.add_key("gemini", secret("AIzaSyTESTKEY-0000000000"), None).unwrap();
        let k = s.add_key("Claude", secret("sk-ant-TESTKEY-000000000000"), None).unwrap();
        assert_eq!(k.provider, "anthropic");
        assert_eq!(s.active_order(), vec!["anthropic", "gemini"]);
        s.update_config(|c| c.order = vec!["google".into(), "ollama".into()]).unwrap();
        assert_eq!(s.active_order(), vec!["gemini", "ollama", "anthropic"]);
        // A configuração é lida de volta do disco.
        let again = Store::open(dir.clone()).unwrap();
        assert_eq!(again.config().order, vec!["gemini", "ollama"]);
        assert_eq!(again.router(None).unwrap().order(), vec!["gemini", "ollama", "anthropic"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn status_and_warnings() {
        let (s, dir) = store();
        assert!(!s.warnings().is_empty());
        s.add_key("xai", secret("xai-TESTKEY-0000000000000"), None).unwrap();
        s.update_config(|c| {
            c.models.insert("anthropic".into(), "claude-haiku-5-5".into());
        })
        .unwrap();
        let st = s.provider_status();
        let a = st.iter().find(|p| p.id == "anthropic").unwrap();
        assert_eq!(a.model, "claude-haiku-5-5");
        assert!(!a.default_model);
        assert!(s.warnings().iter().any(|w| w.contains("xai")));
        assert!(s.router(Some("nao-existe")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
