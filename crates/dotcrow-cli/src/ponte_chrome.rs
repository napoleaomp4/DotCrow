//! Ponte com a extensão **DotCrow no Chrome**.
//!
//! A extensão roda no Chrome do dono (com as contas dele já logadas) e abre um WebSocket
//! para o painel em `ws://127.0.0.1:PORTA/extensao`. Pela ponte, os corvos pedem ações
//! ("abra esta página", "leia", "clique no elemento 7") e a extensão responde.
//!
//! Segurança:
//! - a conexão só vale com o **código de pareamento** (mostrado no painel e digitado uma
//!   vez na extensão); o código fica num arquivo privado do cofre e pode ser trocado;
//! - só aceita origem `chrome-extension://` (um site aberto no navegador não consegue
//!   falar com a ponte), e o painel já recusa `Host` que não seja este computador;
//! - a extensão só mexe nas abas do grupo "DotCrow" (as que ela abriu ou que o dono
//!   entregou), mostra um cursor vermelho onde clica e pode ser pausada no ícone.
//!
//! Protocolo (JSON por mensagem de texto):
//! - extensão → ponte: `{"tipo":"ola","codigo":"...","versao":"...","navegador":"..."}`,
//!   depois `{"tipo":"resposta","id":N,"ok":true,"resultado":{...}}` ou `"erro":"..."`, e
//!   `{"tipo":"ping"}` de vez em quando (mantém a extensão acordada);
//! - ponte → extensão: `{"tipo":"bem_vindo"}` ou `{"tipo":"recusado","motivo":"..."}`,
//!   `{"tipo":"pedido","id":N,"acao":"ler","args":{...}}` e `{"tipo":"pong"}`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

/// Alfabeto do código: sem 0/O, 1/I/L, para ler e digitar sem errar.
const ALFABETO: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";

type Pendentes = HashMap<u64, oneshot::Sender<Result<Value, String>>>;

/// Quem está conectado agora.
#[derive(Clone, Debug)]
struct Conexao {
    id: u64,
    tx: mpsc::UnboundedSender<String>,
    versao: String,
    navegador: String,
}

/// A ponte: uma por painel. Os corvos de todas as tarefas pedem por ela.
pub struct Ponte {
    arquivo: PathBuf,
    codigo: Mutex<String>,
    conexao: Mutex<Option<Conexao>>,
    pendentes: Mutex<Pendentes>,
    proximo: AtomicU64,
}

impl Ponte {
    /// Abre a ponte; o código de pareamento fica em `cofre/extensao-chrome.codigo`
    /// (criado na primeira vez).
    pub fn carregar(cofre: &Path) -> Self {
        let arquivo = cofre.join("extensao-chrome.codigo");
        let codigo =
            std::fs::read_to_string(&arquivo).ok().map(|s| normalizar(&s)).filter(|s| s.len() == 8).unwrap_or_else(
                || {
                    let c = novo_codigo();
                    let _ = dotcrow_vault::write_private(&arquivo, c.as_bytes());
                    c
                },
            );
        Self {
            arquivo,
            codigo: Mutex::new(codigo),
            conexao: Mutex::new(None),
            pendentes: Mutex::new(HashMap::new()),
            proximo: AtomicU64::new(1),
        }
    }

    /// O código para digitar na extensão (ex.: `K7QF-29XM`).
    pub fn codigo(&self) -> String {
        let c = self.codigo.lock().map(|c| c.clone()).unwrap_or_default();
        format!("{}-{}", &c[..4.min(c.len())], &c[4.min(c.len())..])
    }

    /// Troca o código (a extensão conectada cai e precisa do código novo).
    pub fn trocar_codigo(&self) -> String {
        let c = novo_codigo();
        let _ = dotcrow_vault::write_private(&self.arquivo, c.as_bytes());
        if let Ok(mut g) = self.codigo.lock() {
            *g = c;
        }
        if let Ok(mut g) = self.conexao.lock() {
            *g = None;
        }
        self.codigo()
    }

    /// A extensão está conectada agora?
    pub fn conectada(&self) -> bool {
        self.conexao.lock().map(|c| c.as_ref().is_some_and(|c| !c.tx.is_closed())).unwrap_or(false)
    }

    /// Estado para o painel.
    pub fn estado(&self) -> Value {
        let c = self.conexao.lock().ok().and_then(|g| g.clone()).filter(|c| !c.tx.is_closed());
        json!({
            "conectada": c.is_some(),
            "codigo": self.codigo(),
            "versao": c.as_ref().map(|c| c.versao.clone()),
            "navegador": c.as_ref().map(|c| c.navegador.clone()),
        })
    }

    /// Pede uma ação à extensão e espera a resposta (até `segundos`).
    pub async fn pedir(&self, acao: &str, args: Value, segundos: u64) -> Result<Value, String> {
        let tx = self
            .conexao
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|c| c.tx.clone()))
            .filter(|tx| !tx.is_closed())
            .ok_or_else(desconectada)?;
        let id = self.proximo.fetch_add(1, Ordering::Relaxed);
        let (resp_tx, resp_rx) = oneshot::channel();
        self.pendentes.lock().map_err(|_| "ponte travada")?.insert(id, resp_tx);
        let msg = json!({"tipo": "pedido", "id": id, "acao": acao, "args": args}).to_string();
        if tx.send(msg).is_err() {
            self.tirar(id);
            return Err(desconectada());
        }
        match tokio::time::timeout(Duration::from_secs(segundos), resp_rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err("a extensão do Chrome desconectou no meio da ação".into()),
            Err(_) => {
                self.tirar(id);
                Err(format!("a extensão do Chrome não respondeu em {segundos} s"))
            }
        }
    }

    fn tirar(&self, id: u64) {
        if let Ok(mut p) = self.pendentes.lock() {
            p.remove(&id);
        }
    }

    /// Atende uma conexão da extensão (chamado pela rota `/extensao` do painel).
    pub async fn atender(&self, ws: WebSocket) {
        let (mut envio, mut recebe) = ws.split();
        // 1. A primeira mensagem tem de ser o "olá" com o código certo, em até 10 s.
        let ola = tokio::time::timeout(Duration::from_secs(10), recebe.next()).await;
        let Ok(Some(Ok(Message::Text(t)))) = ola else { return };
        let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { return };
        let dado = v.get("codigo").and_then(Value::as_str).map(normalizar).unwrap_or_default();
        let certo = self.codigo.lock().map(|c| c.clone()).unwrap_or_default();
        if v.get("tipo").and_then(Value::as_str) != Some("ola") || !mesmo(dado.as_bytes(), certo.as_bytes()) {
            // Um pouco de espera desanima quem tenta adivinhar o código.
            tokio::time::sleep(Duration::from_millis(800)).await;
            let _ = envio
                .send(Message::Text(
                    json!({"tipo": "recusado", "motivo": "código errado: confira no painel do DotCrow"})
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let id = self.proximo.fetch_add(1, Ordering::Relaxed);
        let texto = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").chars().take(80).collect::<String>();
        if let Ok(mut g) = self.conexao.lock() {
            // Uma extensão por vez: a nova substitui a antiga (o canal antigo fecha).
            *g = Some(Conexao { id, tx: tx.clone(), versao: texto("versao"), navegador: texto("navegador") });
        }
        if envio.send(Message::Text(json!({"tipo": "bem_vindo"}).to_string().into())).await.is_err() {
            return;
        }

        let mut batida = tokio::time::interval(Duration::from_secs(20));
        loop {
            tokio::select! {
                saida = rx.recv() => {
                    let Some(m) = saida else { break };
                    if envio.send(Message::Text(m.into())).await.is_err() { break }
                }
                entrada = recebe.next() => {
                    let Some(Ok(m)) = entrada else { break };
                    let t = match m {
                        Message::Text(t) => t.to_string(),
                        Message::Close(_) => break,
                        _ => continue,
                    };
                    let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                    match v.get("tipo").and_then(Value::as_str) {
                        Some("ping") => { let _ = tx.send(json!({"tipo": "pong"}).to_string()); }
                        Some("resposta") => self.responder(&v),
                        _ => {}
                    }
                }
                _ = batida.tick() => {
                    if envio.send(Message::Ping(Vec::new().into())).await.is_err() { break }
                }
            }
            // Outra extensão tomou o lugar (ou o dono trocou o código): esta sai.
            let ainda = self.conexao.lock().map(|g| g.as_ref().is_some_and(|c| c.id == id)).unwrap_or(false);
            if !ainda {
                break;
            }
        }
        if let Ok(mut g) = self.conexao.lock()
            && g.as_ref().is_some_and(|c| c.id == id)
        {
            *g = None;
        }
    }

    fn responder(&self, v: &Value) {
        let Some(id) = v.get("id").and_then(Value::as_u64) else { return };
        let Some(tx) = self.pendentes.lock().ok().and_then(|mut p| p.remove(&id)) else { return };
        let r = if v.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(v.get("resultado").cloned().unwrap_or(Value::Null))
        } else {
            Err(v.get("erro").and_then(Value::as_str).unwrap_or("a extensão não disse o motivo").to_string())
        };
        let _ = tx.send(r);
    }
}

fn desconectada() -> String {
    "a extensão DotCrow no Chrome não está conectada. Peça ao dono: no Chrome, clique no ícone do DotCrow, cole o \
código mostrado no painel (Extensão do Chrome) e toque em Conectar. Enquanto isso, use a ferramenta navegador."
        .into()
}

fn novo_codigo() -> String {
    let mut bytes = [0u8; 8];
    // Gerador do sistema operacional; se faltar (nunca deveria), o relógio desempata.
    if dotcrow_crypto::random_bytes(&mut bytes).is_err() {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(7);
        bytes = (t as u64).to_le_bytes();
    }
    bytes.iter().map(|b| ALFABETO[*b as usize % ALFABETO.len()] as char).collect()
}

/// Aceita o código com ou sem hífen, em minúsculas ou maiúsculas.
fn normalizar(c: &str) -> String {
    c.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

/// Comparação em tempo constante.
fn mesmo(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Os arquivos da extensão, embutidos no programa: `dotcrow extensao` (ou o painel)
/// grava tudo numa pasta para o dono carregar no Chrome.
pub const ARQUIVOS: &[(&str, &[u8])] = &[
    ("manifest.json", include_bytes!("../../../extensao/chrome/manifest.json")),
    ("fundo.js", include_bytes!("../../../extensao/chrome/fundo.js")),
    ("pagina.js", include_bytes!("../../../extensao/chrome/pagina.js")),
    ("janela.html", include_bytes!("../../../extensao/chrome/janela.html")),
    ("janela.js", include_bytes!("../../../extensao/chrome/janela.js")),
    ("LEIA-ME.md", include_bytes!("../../../extensao/chrome/LEIA-ME.md")),
    ("icones/16.png", include_bytes!("../../../extensao/chrome/icones/16.png")),
    ("icones/32.png", include_bytes!("../../../extensao/chrome/icones/32.png")),
    ("icones/48.png", include_bytes!("../../../extensao/chrome/icones/48.png")),
    ("icones/128.png", include_bytes!("../../../extensao/chrome/icones/128.png")),
];

/// Onde a extensão fica instalada: `~/DotCrow/extensao-chrome`.
pub fn pasta_padrao() -> PathBuf {
    crate::maos::pasta_das_conversas().parent().map(Path::to_path_buf).unwrap_or_default().join("extensao-chrome")
}

/// Grava (ou atualiza) os arquivos da extensão em `pasta`.
pub fn instalar(pasta: &Path) -> std::io::Result<()> {
    for (nome, dados) in ARQUIVOS {
        let alvo = pasta.join(nome);
        if let Some(p) = alvo.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(alvo, dados)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codigo_tem_8_letras_legiveis_e_persiste() {
        let d = std::env::temp_dir().join(format!("dotcrow-ponte-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = Ponte::carregar(&d);
        let c = p.codigo();
        assert_eq!(c.len(), 9);
        assert!(normalizar(&c).bytes().all(|b| ALFABETO.contains(&b)));
        assert_eq!(Ponte::carregar(&d).codigo(), c, "o código fica guardado");
        let novo = p.trocar_codigo();
        assert_ne!(novo, c);
        assert_eq!(Ponte::carregar(&d).codigo(), novo);
        assert!(!p.conectada());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn normaliza_e_compara_o_codigo() {
        assert_eq!(normalizar("k7qf-29xm"), "K7QF29XM");
        assert!(mesmo(b"ABC", b"ABC"));
        assert!(!mesmo(b"ABC", b"ABD"));
        assert!(!mesmo(b"AB", b"ABC"));
    }

    #[tokio::test]
    async fn pedir_sem_extensao_explica_como_conectar() {
        let d = std::env::temp_dir().join(format!("dotcrow-ponte2-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let e = Ponte::carregar(&d).pedir("ler", json!({}), 1).await.unwrap_err();
        assert!(e.contains("não está conectada"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
