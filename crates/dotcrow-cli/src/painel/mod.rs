//! Painel visual do DotCrow: um servidor local com uma página e uma API JSON.
//!
//! A mesma API serve a página, os apps de celular e, mais tarde, a ponte com o
//! WhatsApp. Ela está documentada em `docs/painel.md`.
//!
//! Segurança:
//! - por padrão só escuta em 127.0.0.1; com `--rede`, aceita aparelhos da rede local;
//! - toda chamada a `/api/` exige o código de acesso (`Authorization: Bearer ...`);
//! - o cabeçalho `Host` precisa ser este computador, o que impede que um site aberto
//!   no navegador fale com o painel por trás (ataque de *DNS rebinding*);
//! - não há CORS: páginas de outras origens não conseguem chamar a API;
//! - nenhuma rota devolve uma chave de API.

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    convert::Infallible,
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use axum::{
    Json,
    extract::{Path as UrlPath, Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{
        Html, IntoResponse, Response,
        sse::{Event as SseEvent, KeepAlive, Sse},
    },
    routing::{delete, get, post},
};
use dotcrow_agents::{
    TaskKind,
    orchestrator::{self, Event},
};
use dotcrow_providers::catalog;
use dotcrow_vault::{KeyInfo, write_private};
use futures_util::{Stream, StreamExt, stream};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use zeroize::Zeroizing;

use crate::{
    maos::{self, Aprovador, Liberados, Maos, Modo, Resposta},
    memoria::Memoria,
    pacote::{self, Ficha},
    store::{Config, ProviderStatus, Store, TestResult, canonical_provider},
    treino::{self, Consentimento, Sessao},
};

mod civ;

const INDEX_HTML: &str = include_str!("index.html");
const MARK_SVG: &str = include_str!("../../../../assets/brand/dotcrow-mark-dark.svg");
const FAVICON_SVG: &str = include_str!("../../../../assets/brand/dotcrow-favicon.svg");
const APP_ICON_SVG: &str = include_str!("../../../../assets/brand/dotcrow-app-icon.svg");
/// Manifesto para "Adicionar à tela de início": o painel abre como um app no celular.
const MANIFEST: &str = r##"{
  "name": "DotCrow",
  "short_name": "DotCrow",
  "description": "Painel dos corvos do DotCrow",
  "start_url": "/",
  "display": "standalone",
  "background_color": "#0D1117",
  "theme_color": "#0D1117",
  "lang": "pt-BR",
  "icons": [{ "src": "/icone.svg", "sizes": "any", "type": "image/svg+xml", "purpose": "any" }]
}"##;

/// Quantas tarefas o painel guarda na memória.
const MAX_TASKS: usize = 50;
/// Tamanho máximo do texto de uma tarefa ou pergunta.
const MAX_TEXT: usize = 20_000;

/// Endereço padrão do relay (fica atrás do TLS da VPS em dotcrow.si).
pub const DEFAULT_RELAY: &str = "wss://dotcrow.si/t";

/// Opções do painel.
pub struct Options {
    /// Porta.
    pub port: u16,
    /// Libera o acesso pela rede local.
    pub lan: bool,
    /// Abre o navegador sozinho.
    pub open_browser: bool,
    /// Gera um código de acesso novo.
    pub new_code: bool,
    /// Liga o túnel remoto (celular pela internet).
    pub remoto: bool,
    /// Relay a usar; `None` usa [`DEFAULT_RELAY`].
    pub relay: Option<String>,
}

/// Estado de uma tarefa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TaskStatus {
    Running,
    Finished,
    Blocked,
    Failed,
}

struct TaskRecord {
    id: String,
    conversa: String,
    request: String,
    created_at: u64,
    status: TaskStatus,
    events: Vec<Event>,
    tx: broadcast::Sender<Event>,
    /// Mensagens que o dono manda durante a tarefa (conversa ao vivo); o executor recolhe.
    inbox: Arc<Mutex<Vec<String>>>,
}

/// Liga a caixa de mensagens da tarefa ao executor dos corvos.
struct PainelInbox(Arc<Mutex<Vec<String>>>);

impl orchestrator::Inbox for PainelInbox {
    fn drain(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

#[derive(Default)]
struct Board {
    order: VecDeque<String>,
    tasks: HashMap<String, TaskRecord>,
}

struct AppState {
    store: Arc<Store>,
    token: String,
    port: u16,
    lan: bool,
    own_ip: Option<IpAddr>,
    board: Mutex<Board>,
    /// Conversas abertas: cada uma tem a sua pasta e o seu histórico.
    conversas: Mutex<HashMap<String, Conversa>>,
    /// Confirmações esperando a resposta do dono.
    pendentes: Mutex<HashMap<String, tokio::sync::oneshot::Sender<Resposta>>>,
    /// Sessão de treino em andamento (gravação consentida).
    treino: Mutex<Option<TreinoAtivo>>,
    /// Túnel remoto ligado: relay e sala, para montar o QR rotativo a cada pedido.
    remoto: Option<(String, String)>,
    /// Avisado pelo botão "Fechar painel", para quem abriu pelo ícone e não tem terminal.
    shutdown: tokio::sync::Notify,
    /// A ponte com a extensão "DotCrow no Chrome".
    ponte: Arc<crate::ponte_chrome::Ponte>,
}

type Shared = Arc<AppState>;

/// A sessão de treino ativa e o sinal para parar os amostradores (mouse e atividade) e a
/// borda verde. O processo da borda não fica aqui: uma thread vigia-o e, se o dono clicar
/// em "Parar" na própria borda (a borda sai sozinha), encerra a gravação.
struct TreinoAtivo {
    sessao: Arc<Mutex<Sessao>>,
    parar: Arc<AtomicBool>,
}

/// Uma conversa: pedidos seguidos que dividem a mesma pasta.
struct Conversa {
    pasta: std::path::PathBuf,
    modo: Modo,
    historico: String,
    liberados: Liberados,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Grava o evento na tarefa e manda para quem está acompanhando.
fn push_event(st: &AppState, task: &str, e: Event) {
    let mut board = lock(&st.board);
    if let Some(t) = board.tasks.get_mut(task) {
        match &e {
            Event::Blocked { .. } => t.status = TaskStatus::Blocked,
            Event::Failed { .. } => t.status = TaskStatus::Failed,
            Event::Finished { .. } if t.status == TaskStatus::Running => t.status = TaskStatus::Finished,
            _ => {}
        }
        t.events.push(e.clone());
        let _ = t.tx.send(e);
    }
}

/// Pede a confirmação do dono no painel e espera (até 30 minutos; sem resposta = não).
struct PainelAprovador {
    st: Shared,
    task: String,
}

#[async_trait::async_trait]
impl Aprovador for PainelAprovador {
    async fn pedir(&self, n: usize, titulo: &str, motivo: &str) -> Resposta {
        let id = new_task_id();
        let (tx, rx) = tokio::sync::oneshot::channel();
        lock(&self.st.pendentes).insert(id.clone(), tx);
        push_event(
            &self.st,
            &self.task,
            Event::ApprovalNeeded { id: id.clone(), n, title: titulo.to_string(), reason: motivo.to_string() },
        );
        let r = tokio::time::timeout(std::time::Duration::from_secs(30 * 60), rx)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or(Resposta::Nao);
        lock(&self.st.pendentes).remove(&id);
        push_event(&self.st, &self.task, Event::ApprovalResolved { id, answer: r.as_str().to_string() });
        r
    }
}

/// Erro da API, sempre em JSON: `{"error": "..."}`.
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.to_string())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Alfabeto sem letras parecidas (sem l, o, 0, 1), com 32 símbolos.
const CODE_ALPHABET: &[u8; 32] = b"abcdefghijkmnpqrstuvwxyz23456789";

/// Código de acesso: 16 símbolos (80 bits), guardado em `painel.token` (0600).
fn access_code(dir: &Path, renew: bool) -> Result<String> {
    let path = dir.join("painel.token");
    if !renew && let Ok(saved) = std::fs::read_to_string(&path) {
        let saved = normalize_code(&saved);
        if saved.len() == 16 {
            return Ok(saved);
        }
    }
    let random = dotcrow_crypto::sym::generate_key()?;
    let code: String = random.iter().take(16).map(|b| CODE_ALPHABET[(b & 31) as usize] as char).collect();
    write_private(&path, code.as_bytes())?;
    Ok(code)
}

/// Aceita o código com ou sem hífens e em maiúsculas.
fn normalize_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

/// Código em grupos de 4, para ler e digitar.
fn pretty_code(code: &str) -> String {
    code.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).to_string()).collect::<Vec<_>>().join("-")
}

/// Comparação em tempo constante.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// O `Host` precisa ser este computador. Com `--rede`, aceita IPs de rede local e o
/// IP deste computador (`own_ip`), mesmo que ele esteja fora das faixas privadas.
fn host_allowed(host: &str, port: u16, lan: bool, own_ip: Option<IpAddr>) -> bool {
    let (name, host_port) = match host.rsplit_once(':') {
        Some((n, p)) if !n.is_empty() => (n, p.parse::<u16>().ok()),
        _ => (host, Some(80)),
    };
    if host_port != Some(port) {
        return false;
    }
    match name {
        "localhost" | "127.0.0.1" | "[::1]" => true,
        _ if lan && own_ip.is_some_and(|own| own.to_string() == name.trim_matches(['[', ']'])) => true,
        _ if lan => name.parse::<Ipv4Addr>().is_ok_and(|ip| {
            let o = ip.octets();
            // Faixas privadas, loopback e a faixa de VPNs como o Tailscale (100.64.0.0/10).
            ip.is_private() || ip.is_loopback() || (o[0] == 100 && (o[1] & 0xC0) == 64)
        }),
        _ => false,
    }
}

/// IPv6 vai entre colchetes na URL.
fn host_for_url(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
    }
}

/// IP deste computador na rede local (não envia nenhum pacote).
fn lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.168.0.1:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
}

async fn guard(State(st): State<Shared>, req: Request, next: Next) -> Response {
    let host = req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
    if !host_allowed(host, st.port, st.lan, st.own_ip) {
        return ApiError(StatusCode::FORBIDDEN, "endereço não permitido".into()).into_response();
    }
    let is_api = req.uri().path().starts_with("/api/");
    if is_api {
        let given = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(normalize_code)
            .unwrap_or_default();
        if !same(given.as_bytes(), st.token.as_bytes()) {
            return ApiError(StatusCode::UNAUTHORIZED, "código de acesso inválido".into()).into_response();
        }
    }
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
    );
    if is_api {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    resp
}

fn svg(body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "max-age=86400")], body).into_response()
}

/// Monta as rotas do painel.
fn app(state: Shared) -> axum::Router {
    axum::Router::new()
        .route("/", get(|| async { Html(INDEX_HTML) }))
        .route("/civ", get(|| async { Html(civ::CIV_HTML) }))
        .route("/api/civ", get(civ::estado))
        .route("/marca.svg", get(|| async { svg(MARK_SVG) }))
        .route("/favicon.svg", get(|| async { svg(FAVICON_SVG) }))
        .route("/icone.svg", get(|| async { svg(APP_ICON_SVG) }))
        .route(
            "/manifest.webmanifest",
            get(|| async { ([(header::CONTENT_TYPE, "application/manifest+json")], MANIFEST) }),
        )
        .route("/extensao", get(extensao_ws))
        .route("/api/extensao", get(extensao_estado))
        .route("/api/extensao/instalar", post(extensao_instalar))
        .route("/api/extensao/codigo", post(extensao_codigo))
        .route("/api/estado", get(get_state))
        .route("/api/chaves", post(add_key))
        .route("/api/chaves/{id}", delete(delete_key))
        .route("/api/provedores", axum::routing::put(update_providers))
        .route("/api/testar", post(test_providers))
        .route("/api/perguntar", post(ask))
        .route("/api/tarefas", post(create_task).get(list_tasks))
        .route("/api/tarefas/{id}", get(get_task))
        .route("/api/tarefas/{id}/eventos", get(task_events))
        .route("/api/tarefas/{id}/mensagem", post(send_message))
        .route("/api/celular", get(phone_info))
        .route("/api/remoto", get(remote_info))
        .route("/api/desligar", post(shut_down))
        .route("/api/aprovacoes/{id}", post(answer_approval))
        .route("/api/conversas/{id}/arquivos", get(list_files))
        .route("/api/abrir", post(open_path))
        .route("/api/memoria", get(list_memories).post(add_memory))
        .route("/api/memoria/{id}", delete(forget_memory))
        .route("/api/memoria/exportar", post(export_rag))
        .route("/api/memoria/importar", post(import_rag))
        .route("/api/treino", get(treino_estado))
        .route("/api/treino/iniciar", post(treino_iniciar))
        .route("/api/treino/passo", post(treino_passo))
        .route("/api/treino/tela", post(treino_tela))
        .route("/api/treino/parar", post(treino_parar))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

#[derive(Serialize)]
struct StateView {
    version: &'static str,
    providers: Vec<ProviderStatus>,
    keys: Vec<KeyInfo>,
    order: Vec<String>,
    warnings: Vec<String>,
    lan: bool,
}

fn state_view(st: &AppState) -> StateView {
    StateView {
        version: env!("CARGO_PKG_VERSION"),
        providers: st.store.provider_status(),
        keys: st.store.list_keys(),
        order: st.store.active_order(),
        warnings: st.store.warnings(),
        lan: st.lan,
    }
}

/// A extensão "DotCrow no Chrome" se conecta aqui (WebSocket). Só aceita extensões do
/// Chrome (nunca um site) e só segue com o código de pareamento certo (ver ponte_chrome).
async fn extensao_ws(
    State(st): State<Shared>,
    headers: axum::http::HeaderMap,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> Response {
    let origem = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok()).unwrap_or("");
    if !origem.is_empty() && !origem.starts_with("chrome-extension://") {
        return ApiError(StatusCode::FORBIDDEN, "só a extensão do DotCrow fala com esta porta".into()).into_response();
    }
    let ponte = st.ponte.clone();
    ws.max_message_size(16 * 1024 * 1024).on_upgrade(move |socket| async move { ponte.atender(socket).await })
}

async fn extensao_estado(State(st): State<Shared>) -> Json<serde_json::Value> {
    let mut v = st.ponte.estado();
    v["pasta"] = serde_json::json!(crate::ponte_chrome::pasta_padrao().display().to_string());
    v["instalada"] = serde_json::json!(crate::ponte_chrome::pasta_padrao().join("manifest.json").exists());
    Json(v)
}

async fn extensao_instalar(State(st): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    let pasta = crate::ponte_chrome::pasta_padrao();
    crate::ponte_chrome::instalar(&pasta).map_err(|e| bad(format!("não consegui gravar a extensão: {e}")))?;
    Ok(extensao_estado(State(st)).await)
}

async fn extensao_codigo(State(st): State<Shared>) -> Json<serde_json::Value> {
    st.ponte.trocar_codigo();
    extensao_estado(State(st)).await
}

async fn get_state(State(st): State<Shared>) -> Json<StateView> {
    Json(state_view(&st))
}

#[derive(Deserialize)]
struct AddKey {
    provider: String,
    key: String,
    label: Option<String>,
}

async fn add_key(State(st): State<Shared>, Json(body): Json<AddKey>) -> Result<Json<KeyInfo>, ApiError> {
    let secret = Zeroizing::new(body.key);
    let provider = canonical_provider(&body.provider);
    if catalog::info(&provider).is_none() {
        return Err(bad(format!("provedor desconhecido: {provider}")));
    }
    st.store.add_key(&provider, secret, body.label).map(Json).map_err(bad)
}

async fn delete_key(State(st): State<Shared>, UrlPath(id): UrlPath<String>) -> Result<Json<KeyInfo>, ApiError> {
    st.store.delete_key(&id).map(Json).map_err(|e| ApiError(StatusCode::NOT_FOUND, e.to_string()))
}

#[derive(Deserialize)]
struct ProvidersUpdate {
    order: Option<Vec<String>>,
    /// Modelo por provedor; `null` ou vazio volta ao padrão.
    models: Option<BTreeMap<String, Option<String>>>,
    /// Endereço por provedor; `null` ou vazio volta ao padrão.
    urls: Option<BTreeMap<String, Option<String>>>,
}

async fn update_providers(
    State(st): State<Shared>,
    Json(body): Json<ProvidersUpdate>,
) -> Result<Json<StateView>, ApiError> {
    let known = |id: &str| catalog::info(&canonical_provider(id)).is_some();
    let ids = body.order.iter().flatten().chain(body.models.iter().flat_map(|m| m.keys()));
    if let Some(bad_id) = ids.chain(body.urls.iter().flat_map(|m| m.keys())).find(|id| !known(id)) {
        return Err(bad(format!("provedor desconhecido: {bad_id}")));
    }
    for url in body.urls.iter().flat_map(|m| m.values()).flatten() {
        let url = url.trim();
        if !url.is_empty() && !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(bad("o endereço precisa começar com http:// ou https://"));
        }
    }
    let apply = |map: &mut BTreeMap<String, String>, changes: &BTreeMap<String, Option<String>>| {
        for (id, value) in changes {
            let id = canonical_provider(id);
            match value.as_deref().map(str::trim) {
                Some(v) if !v.is_empty() => {
                    map.insert(id, v.to_string());
                }
                _ => {
                    map.remove(&id);
                }
            }
        }
    };
    st.store
        .update_config(|c: &mut Config| {
            if let Some(order) = &body.order {
                c.order = order.clone();
            }
            if let Some(m) = &body.models {
                apply(&mut c.models, m);
            }
            if let Some(u) = &body.urls {
                apply(&mut c.urls, u);
            }
        })
        .map_err(bad)?;
    Ok(Json(state_view(&st)))
}

#[derive(Deserialize)]
struct TestRequest {
    provider: Option<String>,
}

async fn test_providers(State(st): State<Shared>, Json(body): Json<TestRequest>) -> Json<Vec<TestResult>> {
    let targets = st.store.test_targets(body.provider.as_deref());
    let results = futures_util::future::join_all(targets.iter().map(|id| st.store.test_provider(id))).await;
    Json(results)
}

#[derive(Deserialize)]
struct AskRequest {
    text: String,
    provider: Option<String>,
    /// Modelo escolhido só para esta pergunta (precisa de um provedor escolhido).
    model: Option<String>,
}

#[derive(Serialize)]
struct AskResponse {
    text: String,
    provider: String,
    model: String,
    failovers: Vec<String>,
}

fn check_text(text: &str) -> Result<String, ApiError> {
    let t = text.trim();
    if t.is_empty() {
        return Err(bad("escreva o pedido"));
    }
    if t.chars().count() > MAX_TEXT {
        return Err(bad(format!("o pedido passou de {MAX_TEXT} caracteres")));
    }
    Ok(t.to_string())
}

fn router_for(
    st: &AppState,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<dotcrow_providers::Router, ApiError> {
    let provider = provider.filter(|p| !p.trim().is_empty());
    let model = model.filter(|m| !m.trim().is_empty());
    let router = st.store.router_choice(provider, model).map_err(bad)?;
    if router.order().is_empty() {
        return Err(bad("nenhum provedor ativo: adicione uma chave de API primeiro"));
    }
    Ok(router)
}

async fn ask(State(st): State<Shared>, Json(body): Json<AskRequest>) -> Result<Json<AskResponse>, ApiError> {
    let text = check_text(&body.text)?;
    let router = router_for(&st, body.provider.as_deref(), body.model.as_deref())?;
    let out = orchestrator::quick_answer(&router, &text)
        .await
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e.to_string()))?;
    let failovers =
        out.attempts.iter().filter_map(|a| a.error.as_ref().map(|e| format!("{}: {e}", a.provider))).collect();
    Ok(Json(AskResponse {
        text: out.response.text.trim().to_string(),
        provider: out.provider,
        model: out.response.model,
        failovers,
    }))
}

#[derive(Deserialize)]
struct TaskRequest {
    text: String,
    kind: Option<String>,
    provider: Option<String>,
    /// Modelo escolhido só para esta tarefa (precisa de um provedor escolhido).
    model: Option<String>,
    /// Continua esta conversa (mesma pasta e histórico); sem isso, abre uma nova.
    conversa: Option<String>,
    /// `sandbox`, `computador` ou `livre`.
    modo: Option<String>,
    /// Modo equipe: vários corvos em paralelo.
    #[serde(default)]
    equipe: bool,
}

fn new_task_id() -> String {
    let random = dotcrow_crypto::sym::generate_key().map(|k| k.iter().take(5).map(|b| format!("{b:02x}")).collect());
    random.unwrap_or_else(|_| format!("{:x}", now()))
}

async fn create_task(
    State(st): State<Shared>,
    Json(body): Json<TaskRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let text = check_text(&body.text)?;
    let kind = match body.kind.as_deref().map(str::trim).filter(|k| !k.is_empty() && *k != "auto") {
        Some(k) => {
            Some(TaskKind::parse(k).ok_or_else(|| bad("tipo desconhecido (use organizar, operar ou escrever)"))?)
        }
        None => None,
    };
    let router = router_for(&st, body.provider.as_deref(), body.model.as_deref())?;
    let modo = match body.modo.as_deref() {
        Some(m) => Some(Modo::parse(m).ok_or_else(|| bad("modo desconhecido (use sandbox, computador ou livre)"))?),
        None => None,
    };
    let equipe = body.equipe;
    let id = new_task_id();
    let (conversa, pasta, modo, historico, liberados) = {
        let mut conversas = lock(&st.conversas);
        let existente = body.conversa.as_deref().filter(|c| conversas.contains_key(*c)).map(String::from);
        let cid = match existente {
            Some(c) => c,
            None => {
                let pasta =
                    maos::nova_pasta(&text).map_err(|e| bad(format!("não consegui criar a pasta da conversa: {e}")))?;
                let cid = new_task_id();
                conversas.insert(
                    cid.clone(),
                    Conversa {
                        pasta,
                        modo: modo.unwrap_or_default(),
                        historico: String::new(),
                        liberados: Liberados::default(),
                    },
                );
                cid
            }
        };
        let Some(c) = conversas.get_mut(&cid) else { return Err(bad("conversa sumiu")) };
        if let Some(m) = modo {
            c.modo = m;
        }
        (cid, c.pasta.clone(), c.modo, c.historico.clone(), c.liberados.clone())
    };
    let inbox: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let mut board = st.board.lock().unwrap_or_else(|e| e.into_inner());
        let (tx, _) = broadcast::channel(1024);
        board.tasks.insert(
            id.clone(),
            TaskRecord {
                id: id.clone(),
                conversa: conversa.clone(),
                request: text.clone(),
                created_at: now(),
                status: TaskStatus::Running,
                events: Vec::new(),
                tx,
                inbox: inbox.clone(),
            },
        );
        board.order.push_back(id.clone());
        while board.order.len() > MAX_TASKS {
            if let Some(old) = board.order.pop_front() {
                board.tasks.remove(&old);
            }
        }
    }
    let st2 = st.clone();
    let id2 = id.clone();
    let conversa2 = conversa.clone();
    tokio::spawn(async move {
        let aprovador = Arc::new(PainelAprovador { st: st2.clone(), task: id2.clone() });
        let maos = Maos::new(modo, pasta.clone(), st2.store.dir().to_path_buf(), aprovador, liberados)
            .com_conectores(crate::conectores::Conexoes::do_store(&st2.store))
            .com_chrome(st2.ponte.clone());
        let memoria = st2.store.master_key().ok().and_then(|k| Memoria::abrir(st2.store.dir(), k).ok());
        let caixa = PainelInbox(inbox);
        let oraculo = st2.store.oraculo();
        let input = orchestrator::TaskInput {
            request: &text,
            kind,
            hands: Some(&maos),
            history: Some(&historico),
            memory: memoria.as_ref().map(|m| m as &dyn dotcrow_agents::Memory),
            // A conversa ao vivo vale no fluxo normal; no modo equipe os corvos trabalham
            // em paralelo e não há um executor único para recolher as mensagens.
            inbox: if equipe { None } else { Some(&caixa as &dyn orchestrator::Inbox) },
            // O Oráculo (Opus 5.5) decide quantos corvos trabalham; no modo equipe, o bando inteiro.
            oraculo: if equipe {
                orchestrator::Oraculo::Desligado
            } else {
                orchestrator::Oraculo::Router(oraculo.as_ref().unwrap_or(&router))
            },
        };
        let emit = |e: Event| push_event(&st2, &id2, e);
        let out = if equipe {
            orchestrator::run_team(&router, input, 7, &emit).await
        } else {
            orchestrator::run(&router, input, &emit).await
        };
        let entrega = match &out {
            Ok(o) => format!("{}\n\n{}", o.result.trim(), o.summary.trim()),
            Err(e) => format!("(a tarefa parou: {e})"),
        };
        let bloco = format!("## Pedido\n{text}\n\n## Entrega\n{entrega}\n\n");
        if let Some(c) = lock(&st2.conversas).get_mut(&conversa2) {
            c.historico.push_str(&bloco);
            // O histórico que vai para os corvos guarda só o fim, para não crescer sem limite.
            let total = c.historico.chars().count();
            if total > 16_000 {
                c.historico = c.historico.chars().skip(total - 16_000).collect();
            }
        }
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(pasta.join("conversa.md")) {
            let _ = f.write_all(bloco.as_bytes());
        }
    });
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id, "conversa": conversa }))))
}

#[derive(Deserialize)]
struct ApprovalAnswer {
    resposta: String,
}

async fn answer_approval(
    State(st): State<Shared>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<ApprovalAnswer>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let r = Resposta::parse(&body.resposta).ok_or_else(|| bad("resposta: sim, sempre ou nao"))?;
    let tx = lock(&st.pendentes)
        .remove(&id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "essa confirmação já foi respondida".into()))?;
    let _ = tx.send(r);
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn conversa_pasta(st: &AppState, id: &str) -> Result<(std::path::PathBuf, Modo), ApiError> {
    lock(&st.conversas)
        .get(id)
        .map(|c| (c.pasta.clone(), c.modo))
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "conversa não encontrada".into()))
}

#[derive(Serialize)]
struct FileEntry {
    caminho: String,
    tamanho: u64,
    pasta: bool,
}

/// Os arquivos da pasta da conversa (sem `.git`, `node_modules` e `target`).
async fn list_files(
    State(st): State<Shared>,
    UrlPath(id): UrlPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (raiz, modo) = conversa_pasta(&st, &id)?;
    let mut lista = Vec::new();
    let mut fila = vec![(raiz.clone(), 0usize)];
    while let Some((dir, nivel)) = fila.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            if lista.len() >= 400 {
                break;
            }
            let p = e.path();
            let nome = e.file_name().to_string_lossy().to_string();
            let Ok(m) = e.metadata() else { continue };
            let rel = p.strip_prefix(&raiz).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or(nome.clone());
            if m.is_dir() {
                lista.push(FileEntry { caminho: rel, tamanho: 0, pasta: true });
                if nivel < 4 && !matches!(nome.as_str(), ".git" | "node_modules" | "target" | "__pycache__" | ".venv") {
                    fila.push((p, nivel + 1));
                }
            } else {
                lista.push(FileEntry { caminho: rel, tamanho: m.len(), pasta: false });
            }
        }
    }
    lista.sort_by(|a, b| a.caminho.cmp(&b.caminho));
    Ok(Json(serde_json::json!({
        "pasta": raiz.display().to_string(),
        "modo": modo.as_str(),
        "arquivos": lista,
    })))
}

#[derive(Deserialize)]
struct OpenRequest {
    conversa: String,
    caminho: Option<String>,
}

/// Abre a pasta da conversa (ou um arquivo dela) nos programas do computador.
async fn open_path(
    State(st): State<Shared>,
    Json(body): Json<OpenRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (raiz, _) = conversa_pasta(&st, &body.conversa)?;
    let alvo = match body.caminho.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => maos::normalizar(&raiz.join(c)),
        None => raiz.clone(),
    };
    if !alvo.starts_with(&raiz) || !alvo.exists() {
        return Err(bad("arquivo fora da conversa ou inexistente"));
    }
    if st.lan {
        // Pelo celular, abrir arquivos na tela do computador não faz sentido; mostra o caminho.
        return Ok(Json(serde_json::json!({ "ok": false, "caminho": alvo.display().to_string() })));
    }
    maos::abrir_no_sistema(&alvo.display().to_string()).map_err(bad)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Abre a memória cifrada deste computador.
fn memoria(st: &AppState) -> Result<Memoria, ApiError> {
    let key = st.store.master_key().map_err(bad)?;
    Memoria::abrir(st.store.dir(), key).map_err(bad)
}

/// O que o DotCrow lembra (mais novo primeiro). Mostra só um trecho de cada.
async fn list_memories(State(st): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    let m = memoria(&st)?;
    let itens: Vec<serde_json::Value> = m
        .todas()
        .into_iter()
        .map(|l| {
            let trecho: String = l.texto.chars().take(280).collect();
            serde_json::json!({ "id": l.id, "ts": l.ts, "fonte": l.fonte, "trecho": trecho })
        })
        .collect();
    Ok(Json(serde_json::json!({ "total": itens.len(), "itens": itens })))
}

#[derive(Deserialize)]
struct AddMemory {
    texto: String,
}

/// Guarda uma nota escrita pelo dono (passa pela remoção de segredos).
async fn add_memory(
    State(st): State<Shared>,
    Json(body): Json<AddMemory>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let texto = body.texto.trim();
    if texto.is_empty() {
        return Err(bad("escreva algo para o DotCrow lembrar"));
    }
    let id = memoria(&st)?.guardar(texto, "você").map_err(bad)?;
    match id {
        Some(id) => Ok(Json(serde_json::json!({ "ok": true, "id": id }))),
        None => Err(bad("isso parecia só um segredo (chave/senha), então não guardei")),
    }
}

/// Esquece uma lembrança.
async fn forget_memory(
    State(st): State<Shared>,
    UrlPath(id): UrlPath<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let achou = memoria(&st)?.esquecer(&id).map_err(bad)?;
    Ok(Json(serde_json::json!({ "ok": achou })))
}

/// Exporta a memória como um RAG (arquivo .dotrag). O navegador baixa o conteúdo.
async fn export_rag(State(st): State<Shared>, Json(ficha): Json<Ficha>) -> Result<Json<serde_json::Value>, ApiError> {
    if ficha.nome.trim().is_empty() {
        return Err(bad("dê um nome ao RAG"));
    }
    let m = memoria(&st)?;
    let pct = pacote::exportar(m.todas(), ficha);
    if pct.itens.is_empty() {
        return Err(bad("não há lembranças para exportar"));
    }
    let nome_arq = format!("{}.dotrag", slug(&pct.ficha.nome));
    let conteudo = String::from_utf8(pacote::para_bytes(&pct)).unwrap_or_default();
    Ok(Json(serde_json::json!({ "arquivo": nome_arq, "itens": pct.itens.len(), "conteudo": conteudo })))
}

#[derive(Deserialize)]
struct ImportRag {
    conteudo: String,
}

/// Importa um RAG (conteúdo de um .dotrag) para a memória.
async fn import_rag(
    State(st): State<Shared>,
    Json(body): Json<ImportRag>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pct = pacote::ler(body.conteudo.as_bytes()).map_err(bad)?;
    let n = pacote::importar(&memoria(&st)?, &pct).map_err(bad)?;
    Ok(Json(serde_json::json!({ "ok": true, "nome": pct.ficha.nome, "guardadas": n })))
}

fn slug(nome: &str) -> String {
    let s: String = nome.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "rag".into() } else { s.chars().take(40).collect() }
}

/// Janela em foco agora (classe e título), pelo xdotool. Serve para registrar o que
/// está acontecendo nos navegadores e no sistema durante o treino.
fn janela_ativa() -> Option<(String, String)> {
    let saida = |args: &[&str]| {
        std::process::Command::new("xdotool")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let titulo = saida(&["getactivewindow", "getwindowname"])?;
    if titulo.is_empty() {
        return None;
    }
    let classe = saida(&["getactivewindow", "getwindowclassname"]).unwrap_or_default();
    Some((classe, titulo))
}

/// Script GTK3 da borda verde do treino. A janela ocupa **só** os pixels da borda e de um
/// selo no canto — o centro da tela não faz parte da janela, então nada trava e o dono
/// segue usando o computador normalmente. O selo "● Em treinamento de RAG — ■ Parar" é
/// clicável: ao tocar, o processo sai, e o DotCrow encerra a gravação e guarda na memória.
/// Precisa de python3 + GTK3 (gir1.2-gtk-3.0), que o Linux Mint já traz. Sem isso, o treino
/// roda sem a borda (mas o botão Parar do painel continua valendo).
const OVERLAY_PY: &str = r#"import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, Gdk, GLib
import cairo

LABEL = "● Em treinamento de RAG   ■ Parar"
B = 6  # espessura da borda

win = Gtk.Window(type=Gtk.WindowType.POPUP)
screen = win.get_screen()
visual = screen.get_rgba_visual()
if visual is not None:
    win.set_visual(visual)
win.set_app_paintable(True)
win.set_decorated(False)
win.set_accept_focus(False)
win.set_focus_on_map(False)
win.set_keep_above(True)
win.set_skip_taskbar_hint(True)
win.set_skip_pager_hint(True)
win.set_type_hint(Gdk.WindowTypeHint.NOTIFICATION)
geo = screen.get_root_window().get_geometry()
W, H = geo[2], geo[3]
win.move(0, 0)
win.set_default_size(W, H)

# Geometria do selo (canto inferior direito). Tamanho fixo e generoso para caber o texto.
BW, BH = 300, 46
BADGE = (W - BW - 22, H - BH - 22, BW, BH)

def regiao_moldura():
    # Só as 4 tiras da borda + o retângulo do selo compõem a janela. O miolo fica de fora.
    reg = cairo.Region(cairo.RectangleInt(0, 0, W, B))
    reg.union(cairo.RectangleInt(0, H - B, W, B))
    reg.union(cairo.RectangleInt(0, 0, B, H))
    reg.union(cairo.RectangleInt(W - B, 0, B, H))
    reg.union(cairo.RectangleInt(*[int(v) for v in BADGE]))
    return reg

def on_realize(w):
    gdkwin = w.get_window()
    if gdkwin is None:
        return
    # A janela é só a moldura + o selo (o centro nem existe para o gerenciador de janelas).
    gdkwin.shape_combine_region(regiao_moldura(), 0, 0)
    # A entrada vale só no selo: as bordas deixam o clique passar; o selo é o botão Parar.
    gdkwin.input_shape_combine_region(cairo.Region(cairo.RectangleInt(*[int(v) for v in BADGE])), 0, 0)
win.connect("realize", on_realize)

def on_draw(w, cr):
    cr.set_operator(cairo.OPERATOR_SOURCE)
    cr.set_source_rgba(0, 0, 0, 0)
    cr.paint()
    cr.set_operator(cairo.OPERATOR_OVER)
    cr.set_source_rgba(0.13, 0.78, 0.37, 0.95)
    for (x, y, bw, bh) in [(0, 0, W, B), (0, H - B, W, B), (0, 0, B, H), (W - B, 0, B, H)]:
        cr.rectangle(x, y, bw, bh)
        cr.fill()
    bx, by, bw, bh = BADGE
    cr.set_source_rgba(0.05, 0.07, 0.09, 0.95)
    cr.rectangle(bx, by, bw, bh)
    cr.fill()
    cr.set_source_rgba(0.13, 0.78, 0.37, 1)
    cr.rectangle(bx, by, bw, 3)
    cr.fill()
    cr.select_font_face("sans-serif", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
    cr.set_font_size(16)
    cr.set_source_rgba(0.94, 0.97, 0.99, 1)
    ext = cr.text_extents(LABEL)
    cr.move_to(bx + (bw - ext.width) / 2, by + (bh + ext.height) / 2)
    cr.show_text(LABEL)
    return False
win.connect("draw", on_draw)

def on_click(w, ev):
    # Qualquer clique chega só no selo (a entrada está limitada a ele): parar o treino.
    Gtk.main_quit()
    return True
win.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
win.connect("button-press-event", on_click)

def tick():
    win.queue_draw()
    return True
GLib.timeout_add(1000, tick)

win.show_all()
Gtk.main()
"#;

/// Abre a borda verde do treino. Devolve o processo (para fechar depois) ou `None` se
/// não deu (sem python3/GTK, ou fora do Linux) — nesse caso o treino segue sem a borda.
fn abrir_overlay_treino(dir: &Path) -> Option<std::process::Child> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let script = dir.join("overlay-treino.py");
    std::fs::write(&script, OVERLAY_PY).ok()?;
    std::process::Command::new("python3")
        .arg(&script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
}

/// Lê a posição do mouse com o xdotool (Linux). `None` se não der.
fn posicao_mouse() -> Option<(i64, i64)> {
    let saida = std::process::Command::new("xdotool").arg("getmouselocation").output().ok()?;
    let txt = String::from_utf8_lossy(&saida.stdout);
    let pega = |campo: &str| txt.split_whitespace().find_map(|p| p.strip_prefix(campo)?.parse::<i64>().ok());
    Some((pega("x:")?, pega("y:")?))
}

#[derive(Serialize)]
struct TreinoView {
    ativa: bool,
    id: Option<String>,
    titulo: Option<String>,
    passos: u64,
    capturas: u64,
    amostras: u64,
    atividades: u64,
    sessoes: Vec<serde_json::Value>,
}

async fn treino_estado(State(st): State<Shared>) -> Json<TreinoView> {
    let (ativa, id, titulo, passos, capturas, amostras, atividades) = {
        let guard = lock(&st.treino);
        match guard.as_ref() {
            Some(t) => {
                let s = t.sessao.lock().unwrap_or_else(|e| e.into_inner());
                let m = s.meta();
                (true, Some(m.id.clone()), Some(m.titulo.clone()), m.passos, m.capturas, m.amostras_mouse, m.atividades)
            }
            None => (false, None, None, 0, 0, 0, 0),
        }
    };
    let sessoes = treino::listar(st.store.dir())
        .into_iter()
        .map(|m| {
            serde_json::json!({ "id": m.id, "titulo": m.titulo, "por": m.consentimento.nome, "inicio": m.inicio, "passos": m.passos, "capturas": m.capturas })
        })
        .collect();
    Json(TreinoView { ativa, id, titulo, passos, capturas, amostras, atividades, sessoes })
}

#[derive(Deserialize)]
struct IniciarTreino {
    titulo: String,
    nome: String,
    #[serde(default)]
    papel: String,
    autorizo: bool,
    #[serde(default)]
    mouse: bool,
}

async fn treino_iniciar(
    State(st): State<Shared>,
    Json(b): Json<IniciarTreino>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if lock(&st.treino).is_some() {
        return Err(bad("já existe uma gravação em andamento"));
    }
    let consent = Consentimento { nome: b.nome.clone(), autorizado: b.autorizo, papel: b.papel, ts: 0 };
    let sessao = Sessao::iniciar(st.store.dir(), &b.titulo, consent).map_err(bad)?;
    let id = sessao.id().to_string();
    let sessao = Arc::new(Mutex::new(sessao));
    let parar = Arc::new(AtomicBool::new(false));
    if b.mouse {
        let sessao2 = sessao.clone();
        let parar2 = parar.clone();
        tokio::spawn(async move {
            while !parar2.load(Ordering::Relaxed) {
                if let Some((x, y)) = posicao_mouse() {
                    sessao2.lock().unwrap_or_else(|e| e.into_inner()).amostrar_mouse(x, y);
                }
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
        });
    }
    // Registra o que está em foco (navegadores e outros programas), sem repetir a mesma janela.
    {
        let sessao2 = sessao.clone();
        let parar2 = parar.clone();
        tokio::spawn(async move {
            let mut ultimo = String::new();
            while !parar2.load(Ordering::Relaxed) {
                if let Some((classe, titulo)) = janela_ativa() {
                    let chave = format!("{classe}\t{titulo}");
                    if chave != ultimo {
                        ultimo = chave;
                        sessao2.lock().unwrap_or_else(|e| e.into_inner()).registrar_atividade(&classe, &titulo);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            }
        });
    }
    *lock(&st.treino) = Some(TreinoAtivo { sessao, parar: parar.clone() });

    // Borda verde na tela com o selo clicável "Parar". Uma thread vigia o processo: se o
    // dono clicar em Parar (a borda sai sozinha), encerra a gravação e guarda na memória;
    // se o painel parar antes, o sinal `parar` fecha a borda.
    if let Some(mut overlay) = abrir_overlay_treino(st.store.dir()) {
        let st2 = st.clone();
        std::thread::spawn(move || {
            loop {
                if parar.load(Ordering::Relaxed) {
                    let _ = overlay.kill();
                    break;
                }
                match overlay.try_wait() {
                    Ok(Some(_)) => {
                        // A borda saiu sozinha: o dono clicou em "Parar".
                        finalizar_treino(&st2, true);
                        break;
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(300)),
                    Err(_) => break,
                }
            }
        });
    }
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

/// Encerra a gravação em andamento (se houver): para os amostradores, fecha a sessão e,
/// se `virar`, guarda na memória. Devolve `(título, virou_memória)`. Usado tanto pelo
/// botão do painel quanto pela borda verde (quando o dono clica em "Parar" nela).
fn finalizar_treino(st: &AppState, virar: bool) -> Option<(String, bool)> {
    let ativo = lock(&st.treino).take()?;
    ativo.parar.store(true, Ordering::Relaxed);
    let mut s = ativo.sessao.lock().unwrap_or_else(|e| e.into_inner());
    s.parar();
    let guardada =
        if virar { memoria(st).ok().and_then(|m| s.virar_memoria(&m).ok().flatten()).is_some() } else { false };
    Some((s.meta().titulo.clone(), guardada))
}

#[derive(Deserialize)]
struct Passo {
    rotulo: String,
}

async fn treino_passo(State(st): State<Shared>, Json(b): Json<Passo>) -> Result<Json<serde_json::Value>, ApiError> {
    let guard = lock(&st.treino);
    let t = guard.as_ref().ok_or_else(|| bad("não há gravação em andamento"))?;
    t.sessao.lock().unwrap_or_else(|e| e.into_inner()).registrar_passo(&b.rotulo);
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn treino_tela(State(st): State<Shared>) -> Result<Json<serde_json::Value>, ApiError> {
    let destino = {
        let guard = lock(&st.treino);
        let t = guard.as_ref().ok_or_else(|| bad("não há gravação em andamento"))?;
        t.sessao.lock().unwrap_or_else(|e| e.into_inner()).nova_captura()
    };
    let alvo = destino.display().to_string();
    let ok = ["gnome-screenshot", "scrot", "spectacle"].iter().any(|prog| {
        let args: Vec<&str> = match *prog {
            "gnome-screenshot" => vec!["-f", &alvo],
            "spectacle" => vec!["-b", "-n", "-o", &alvo],
            _ => vec![&alvo],
        };
        std::process::Command::new(prog)
            .args(args)
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }) || std::process::Command::new("import")
        .args(["-window", "root", &alvo])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok && destino.exists() {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err(bad("não consegui capturar a tela (instale gnome-screenshot ou scrot)"))
    }
}

#[derive(Deserialize)]
struct PararTreino {
    #[serde(default)]
    virar_memoria: bool,
}

async fn treino_parar(
    State(st): State<Shared>,
    Json(b): Json<PararTreino>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // A thread que vigia a borda vê o sinal `parar` e fecha a borda verde.
    let (titulo, guardada) =
        finalizar_treino(&st, b.virar_memoria).ok_or_else(|| bad("não há gravação em andamento"))?;
    Ok(Json(serde_json::json!({ "ok": true, "titulo": titulo, "virou_memoria": guardada })))
}

#[derive(Serialize)]
struct TaskSummary {
    id: String,
    conversa: String,
    request: String,
    created_at: u64,
    status: TaskStatus,
}

async fn list_tasks(State(st): State<Shared>) -> Json<Vec<TaskSummary>> {
    let board = st.board.lock().unwrap_or_else(|e| e.into_inner());
    let list = board
        .order
        .iter()
        .rev()
        .filter_map(|id| board.tasks.get(id))
        .map(|t| TaskSummary {
            id: t.id.clone(),
            conversa: t.conversa.clone(),
            request: t.request.clone(),
            created_at: t.created_at,
            status: t.status,
        })
        .collect();
    Json(list)
}

#[derive(Serialize)]
struct TaskView {
    id: String,
    conversa: String,
    request: String,
    created_at: u64,
    status: TaskStatus,
    events: Vec<Event>,
}

async fn get_task(State(st): State<Shared>, UrlPath(id): UrlPath<String>) -> Result<Json<TaskView>, ApiError> {
    let board = st.board.lock().unwrap_or_else(|e| e.into_inner());
    let t = board.tasks.get(&id).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "tarefa não encontrada".into()))?;
    Ok(Json(TaskView {
        id: t.id.clone(),
        conversa: t.conversa.clone(),
        request: t.request.clone(),
        created_at: t.created_at,
        status: t.status,
        events: t.events.clone(),
    }))
}

#[derive(Deserialize)]
struct TaskMessage {
    texto: String,
}

/// O dono manda uma mensagem para os corvos durante a tarefa (conversa ao vivo).
async fn send_message(
    State(st): State<Shared>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<TaskMessage>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let texto = body.texto.trim();
    if texto.is_empty() {
        return Err(bad("escreva a mensagem"));
    }
    if texto.chars().count() > MAX_TEXT {
        return Err(bad(format!("a mensagem passou de {MAX_TEXT} caracteres")));
    }
    let (inbox, running) = {
        let board = st.board.lock().unwrap_or_else(|e| e.into_inner());
        let t = board.tasks.get(&id).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "tarefa não encontrada".into()))?;
        (t.inbox.clone(), t.status == TaskStatus::Running)
    };
    if !running {
        return Err(bad("a tarefa já terminou; abra uma nova para continuar"));
    }
    inbox.lock().unwrap_or_else(|e| e.into_inner()).push(texto.to_string());
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn is_terminal(e: &Event) -> bool {
    matches!(e, Event::Finished { .. } | Event::Failed { .. })
}

fn to_sse(e: &Event) -> Result<SseEvent, Infallible> {
    Ok(SseEvent::default().data(serde_json::to_string(e).unwrap_or_default()))
}

/// Eventos da tarefa como *Server-Sent Events*: primeiro os que já aconteceram,
/// depois os novos, ao vivo, até o fim.
async fn task_events(
    State(st): State<Shared>,
    UrlPath(id): UrlPath<String>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    let (past, rx) = {
        let board = st.board.lock().unwrap_or_else(|e| e.into_inner());
        let t = board.tasks.get(&id).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "tarefa não encontrada".into()))?;
        // Copiar o passado e assinar o futuro sob a mesma trava: nenhum evento se perde.
        (t.events.clone(), t.tx.subscribe())
    };
    let done = past.iter().any(is_terminal);
    let replay = stream::iter(past.iter().map(to_sse).collect::<Vec<_>>());
    let live = stream::unfold((!done).then_some(rx), |rx| async move {
        let mut rx = rx?;
        loop {
            match rx.recv().await {
                Ok(e) => {
                    let end = is_terminal(&e);
                    return Some((to_sse(&e), (!end).then_some(rx)));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Ok(Sse::new(replay.chain(live)).keep_alive(KeepAlive::default()))
}

#[derive(Serialize)]
struct PhoneInfo {
    lan: bool,
    url: Option<String>,
    qr_svg: Option<String>,
}

/// Endereço e QR code para abrir o painel no celular.
async fn phone_info(State(st): State<Shared>) -> Json<PhoneInfo> {
    if !st.lan {
        return Json(PhoneInfo { lan: false, url: None, qr_svg: None });
    }
    let url = st.own_ip.map(|ip| format!("http://{}:{}/#t={}", host_for_url(ip), st.port, st.token));
    let qr_svg = url.as_deref().and_then(|u| qrcode::QrCode::new(u).ok()).map(|code| {
        code.render::<qrcode::render::svg::Color<'_>>()
            .min_dimensions(240, 240)
            .dark_color(qrcode::render::svg::Color("#0D1117"))
            .light_color(qrcode::render::svg::Color("#F0F6FC"))
            .build()
    });
    Json(PhoneInfo { lan: true, url, qr_svg })
}

#[derive(Serialize)]
struct RemoteInfo {
    ativo: bool,
    link: Option<String>,
    qr_svg: Option<String>,
}

/// Dados do túnel remoto para a aba Celular: o link de pareamento (da janela atual) e seu
/// QR. O link muda a cada 10 s, então a página recarrega isto nesse ritmo.
async fn remote_info(State(st): State<Shared>) -> Json<RemoteInfo> {
    let link = st
        .remoto
        .as_ref()
        .map(|(relay, sala)| crate::remoto::link_pareamento(relay, sala, &st.token, crate::remoto::janela_atual()));
    let qr_svg = link.as_deref().and_then(|u| qrcode::QrCode::new(u).ok()).map(|code| {
        code.render::<qrcode::render::svg::Color<'_>>()
            .min_dimensions(240, 240)
            .dark_color(qrcode::render::svg::Color("#0D1117"))
            .light_color(qrcode::render::svg::Color("#F0F6FC"))
            .build()
    });
    Json(RemoteInfo { ativo: link.is_some(), link, qr_svg })
}

fn terminal_qr(text: &str) -> Option<String> {
    let code = qrcode::QrCode::new(text).ok()?;
    Some(
        code.render::<qrcode::render::unicode::Dense1x2>()
            .dark_color(qrcode::render::unicode::Dense1x2::Light)
            .light_color(qrcode::render::unicode::Dense1x2::Dark)
            .build(),
    )
}

fn open_in_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let cmd = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    // Termux: abre no navegador do celular.
    #[cfg(target_os = "android")]
    let cmd = std::process::Command::new("termux-open-url")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "android")))]
    let cmd = std::process::Command::new("xdg-open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let _ = cmd;
}

async fn shut_down(State(st): State<Shared>) -> Json<serde_json::Value> {
    st.shutdown.notify_one();
    Json(serde_json::json!({ "ok": true }))
}

/// Abre o painel numa janela própria (Chrome/Edge em modo aplicativo: sem abas nem
/// barra de endereço). Sem esses navegadores, abre no navegador padrão.
fn open_window(url: &str, dir: &Path) {
    if let Some(bin) = maos::chrome() {
        let ok = std::process::Command::new(bin)
            .arg(format!("--app={url}"))
            .arg(format!("--user-data-dir={}", dir.join("janela").display()))
            .arg("--class=DotCrow")
            .arg("--window-size=1320,880")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok();
        if ok {
            return;
        }
    }
    open_in_browser(url);
}

/// Já tem um painel do DotCrow nesta porta? (o ícone foi clicado de novo)
fn painel_running(port: u16) -> bool {
    use std::io::{Read, Write};
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut conn) = TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(800)) else {
        return false;
    };
    let _ = conn.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let req = format!("GET /manifest.webmanifest HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n\r\n");
    if conn.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut body = String::new();
    let _ = conn.take(64 * 1024).read_to_string(&mut body);
    body.contains("\"name\": \"DotCrow\"")
}

/// Sobe o painel e fica rodando até o Ctrl+C ou o botão "Fechar painel".
pub async fn serve(store: Arc<Store>, opts: Options) -> Result<()> {
    let token = access_code(store.dir(), opts.new_code)?;
    let bind = if opts.lan { Ipv4Addr::UNSPECIFIED } else { Ipv4Addr::LOCALHOST };
    let listener = match tokio::net::TcpListener::bind((bind, opts.port)).await {
        Ok(l) => l,
        // Clicou no ícone com o painel já aberto: só mostra a janela de novo.
        Err(_) if !opts.new_code && !opts.lan && painel_running(opts.port) => {
            println!("  O painel do DotCrow já está aberto em http://127.0.0.1:{}", opts.port);
            if opts.open_browser {
                open_window(&format!("http://127.0.0.1:{}/#t={token}", opts.port), store.dir());
            }
            return Ok(());
        }
        Err(e) => {
            return Err(e).with_context(|| {
                format!("a porta {} está ocupada; tente: dotcrow painel --porta {}", opts.port, opts.port + 1)
            });
        }
    };
    let own_ip = if opts.lan { lan_ip() } else { None };

    // Túnel remoto (celular pela internet): identidade, sala e a ponte.
    let relay = opts.relay.clone().unwrap_or_else(|| DEFAULT_RELAY.to_string());
    let mut remoto_task = None;
    let remoto = if opts.remoto {
        match crate::remoto::carregar_identidade(store.dir()) {
            Ok(id) => {
                let id = Arc::new(id);
                let sala = crate::remoto::sala_de(&id.public());
                remoto_task = Some((
                    id,
                    crate::remoto::ponte::Config {
                        relay: relay.clone(),
                        sala: sala.clone(),
                        porta: opts.port,
                        codigo: token.clone(),
                    },
                ));
                Some((relay.clone(), sala))
            }
            Err(e) => {
                eprintln!("  (remoto) não consegui preparar a identidade do computador: {e}");
                None
            }
        }
    } else {
        None
    };

    let dir_da_ponte = store.dir().to_path_buf();
    let state = Arc::new(AppState {
        store,
        token: token.clone(),
        port: opts.port,
        lan: opts.lan,
        own_ip,
        board: Mutex::default(),
        conversas: Mutex::default(),
        pendentes: Mutex::default(),
        treino: Mutex::default(),
        remoto: remoto.clone(),
        shutdown: tokio::sync::Notify::new(),
        ponte: Arc::new(crate::ponte_chrome::Ponte::carregar(&dir_da_ponte)),
    });

    let local = format!("http://127.0.0.1:{}/#t={token}", opts.port);
    println!("\n  DotCrow · painel aberto\n");
    println!("  No computador:     http://127.0.0.1:{}", opts.port);
    println!("  Código de acesso:  {}", pretty_code(&token));
    if opts.lan {
        match own_ip {
            Some(ip) => {
                let host = host_for_url(ip);
                let phone = format!("http://{host}:{}/#t={token}", opts.port);
                println!("  No celular:        http://{host}:{}  (mesmo Wi-Fi)\n", opts.port);
                println!("  Aponte a câmera do celular para o QR code:\n");
                if let Some(qr) = terminal_qr(&phone) {
                    for line in qr.lines() {
                        println!("    {line}");
                    }
                }
                println!(
                    "\n  Atenção: na rede, o tráfego ainda não é cifrado. Use só em Wi-Fi de confiança (casa, escritório)."
                );
            }
            None => println!("\n  Não achei o IP deste computador na rede. Confira se está conectado ao Wi-Fi."),
        }
    } else {
        println!("\n  Para abrir no celular, rode: dotcrow painel --rede");
    }
    if let (Some((relay_u, sala)), Some((id, cfg))) = (&remoto, remoto_task) {
        let link = crate::remoto::link_pareamento(relay_u, sala, &token, crate::remoto::janela_atual());
        println!("\n  Celular pela internet (fora do Wi-Fi): ligado, pelo relay {relay}");
        println!("  Pareie o app lendo o QR na aba Celular do painel (ele troca a cada 10 s).");
        println!("  QR de agora (vale uns segundos):\n");
        if let Some(qr) = terminal_qr(&link) {
            for line in qr.lines() {
                println!("    {line}");
            }
        }
        println!("\n  Impressão digital deste computador: {}", id.public().fingerprint());
        tokio::spawn(async move { crate::remoto::ponte::servir(id, cfg).await });
    }
    println!("\n  Para fechar: botão \"Fechar painel\" na página, ou Ctrl+C aqui.\n");
    if opts.open_browser {
        open_window(&local, state.store.dir());
    }

    let stop = Arc::clone(&state);
    axum::serve(listener, app(state))
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                () = stop.shutdown.notified() => {}
            }
            println!("\n  Painel fechado.");
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert!(host_allowed("127.0.0.1:7777", 7777, false, None));
        assert!(host_allowed("localhost:7777", 7777, false, None));
        assert!(host_allowed("[::1]:7777", 7777, false, None));
        assert!(!host_allowed("127.0.0.1:8000", 7777, false, None));
        assert!(!host_allowed("evil.example:7777", 7777, true, None), "nomes de domínio nunca, mesmo com --rede");
        assert!(!host_allowed("192.168.0.10:7777", 7777, false, None));
        assert!(host_allowed("192.168.0.10:7777", 7777, true, None));
        assert!(host_allowed("100.101.1.2:7777", 7777, true, None));
        assert!(!host_allowed("8.8.8.8:7777", 7777, true, None));
        assert!(!host_allowed("", 7777, false, None));
        let own = Some(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2)));
        assert!(host_allowed("192.0.2.2:7777", 7777, true, own), "o IP do próprio computador vale com --rede");
        assert!(!host_allowed("192.0.2.2:7777", 7777, false, own), "mas nunca sem --rede");
        assert!(!host_allowed("192.0.2.3:7777", 7777, true, own));
    }

    /// Sobe o painel numa porta livre com dados num diretório temporário.
    async fn spawn() -> (String, String, std::path::PathBuf) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = std::env::temp_dir().join(format!("dotcrow-painel-{}-{}", std::process::id(), new_task_id()));
        let store = Arc::new(Store::open(dir.clone()).unwrap());
        let token = access_code(&dir, false).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(AppState {
            store,
            token: token.clone(),
            port,
            lan: false,
            own_ip: None,
            board: Mutex::default(),
            conversas: Mutex::default(),
            pendentes: Mutex::default(),
            treino: Mutex::default(),
            remoto: None,
            shutdown: tokio::sync::Notify::new(),
            ponte: Arc::new(crate::ponte_chrome::Ponte::carregar(&dir)),
        });
        tokio::spawn(async move { axum::serve(listener, app(state)).await });
        (format!("http://127.0.0.1:{port}"), token, dir)
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[tokio::test]
    async fn api_requires_code_and_right_host() {
        let (url, token, dir) = spawn().await;
        let c = client();
        assert_eq!(c.get(format!("{url}/")).send().await.unwrap().status(), 200, "a página abre sem código");
        assert_eq!(c.get(format!("{url}/api/estado")).send().await.unwrap().status(), 401);
        let wrong = c.get(format!("{url}/api/estado")).bearer_auth("errado").send().await.unwrap();
        assert_eq!(wrong.status(), 401);
        let rebinding =
            c.get(format!("{url}/api/estado")).bearer_auth(&token).header("host", "evil.example").send().await.unwrap();
        assert_eq!(rebinding.status(), 403);
        let ok =
            c.get(format!("{url}/api/estado")).bearer_auth(pretty_code(&token).to_uppercase()).send().await.unwrap();
        assert_eq!(ok.status(), 200);
        assert!(ok.headers().get("content-security-policy").is_some());
        assert_eq!(ok.headers().get("cache-control").unwrap(), "no-store");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn keys_go_in_but_never_come_out() {
        let (url, token, dir) = spawn().await;
        let c = client();
        let secret = "sk-ant-segredo-que-nunca-volta-123456";
        let unknown = c
            .post(format!("{url}/api/chaves"))
            .bearer_auth(&token)
            .json(&serde_json::json!({ "provider": "skynet", "key": secret }))
            .send()
            .await
            .unwrap();
        assert_eq!(unknown.status(), 400);
        let added = c
            .post(format!("{url}/api/chaves"))
            .bearer_auth(&token)
            .json(&serde_json::json!({ "provider": "claude", "key": secret, "label": "teste" }))
            .send()
            .await
            .unwrap();
        assert_eq!(added.status(), 200);
        let body = added.text().await.unwrap();
        assert!(!body.contains(secret));
        assert!(body.contains("\"provider\":\"anthropic\""), "apelido vira o id do catálogo");
        let state = c.get(format!("{url}/api/estado")).bearer_auth(&token).send().await.unwrap().text().await.unwrap();
        assert!(!state.contains(secret));
        assert!(state.contains("\"order\":[\"anthropic\"]"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn tasks_need_text_and_a_provider() {
        let (url, token, dir) = spawn().await;
        let c = client();
        let post =
            |body: serde_json::Value| c.post(format!("{url}/api/tarefas")).bearer_auth(&token).json(&body).send();
        let empty = post(serde_json::json!({ "text": "   " })).await.unwrap();
        assert_eq!(empty.status(), 400);
        let no_provider = post(serde_json::json!({ "text": "organize" })).await.unwrap();
        assert_eq!(no_provider.status(), 400);
        assert!(no_provider.text().await.unwrap().contains("nenhum provedor ativo"));
        let bad_kind = post(serde_json::json!({ "text": "x", "kind": "dançar", "provider": "ollama" })).await.unwrap();
        assert_eq!(bad_kind.status(), 400);
        let missing = c.get(format!("{url}/api/tarefas/nao-existe/eventos")).bearer_auth(&token).send().await.unwrap();
        assert_eq!(missing.status(), 404);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codes() {
        assert_eq!(normalize_code("ABCD-efgh-2345-6789"), "abcdefgh23456789");
        assert_eq!(pretty_code("abcdefgh23456789"), "abcd-efgh-2345-6789");
        assert!(same(b"abc", b"abc"));
        assert!(!same(b"abc", b"abd"));
        assert!(!same(b"abc", b"abcd"));
    }
}
