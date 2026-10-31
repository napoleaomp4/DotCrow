//! Navegador com **cursor próprio**, pelo Chrome DevTools Protocol (CDP).
//!
//! O ponto é este: os cliques e as teclas vão para a página por `Input.dispatchMouseEvent`
//! e `Input.dispatchKeyEvent`, que são eventos **sintéticos** entregues ao navegador. O
//! ponteiro de verdade do dono **não se mexe** — diferente do `xdotool`, que move o mouse
//! real e atrapalha quem está usando o computador. Assim o corvo navega numa janela própria
//! enquanto o dono continua mexendo no que quiser.
//!
//! A conexão é um WebSocket com a aba (page target) do Chrome, aberto com
//! `--remote-debugging-port`. Cada comando é um JSON `{"id":N,"method":...,"params":...}`;
//! a resposta tem o mesmo `id`. Eventos (sem `id`) são ignorados aqui.

use std::path::PathBuf;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Um navegador vivo, controlado pelo CDP. Enquanto existir, o Chrome fica aberto.
pub struct Navegador {
    _child: tokio::process::Child,
    ws: Ws,
    proximo_id: i64,
    perfil: PathBuf,
    /// Tamanho da janela, para mirar cliques por proporção quando preciso.
    pub largura: i64,
    pub altura: i64,
}

impl Navegador {
    /// Sobe um Chrome dedicado (janela própria) e conecta na aba por CDP.
    pub async fn abrir(bin: &std::path::Path, largura: i64, altura: i64) -> Result<Self, String> {
        let porta = porta_livre().await?;
        let perfil = crate::trava::temporaria().join(format!("dotcrow-cdp-{}-{}", std::process::id(), porta));
        let _ = std::fs::remove_dir_all(&perfil);

        let mut c = tokio::process::Command::new(bin);
        c.arg(format!("--remote-debugging-port={porta}"))
            .arg("--remote-allow-origins=*")
            .arg(format!("--user-data-dir={}", perfil.display()))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking")
            .arg("--disable-features=Translate")
            .arg(format!("--window-size={largura},{altura}"))
            .arg("about:blank")
            // Sem janela na tela (servidores, testes): DOTCROW_SEM_JANELA=1.
            .args(std::env::var_os("DOTCROW_SEM_JANELA").map(|_| "--headless=new"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        let child = c.spawn().map_err(|e| format!("não consegui abrir o Chrome: {e}"))?;

        let ws_url = esperar_ws(porta).await?;
        let (ws, _) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|e| format!("não consegui falar com o navegador (CDP): {e}"))?;

        let mut nav = Self { _child: child, ws, proximo_id: 0, perfil, largura, altura };
        nav.comando("Page.enable", json!({})).await?;
        nav.comando("Runtime.enable", json!({})).await?;
        Ok(nav)
    }

    /// Manda um comando CDP e espera a resposta de mesmo `id`, pulando eventos.
    pub async fn comando(&mut self, metodo: &str, params: Value) -> Result<Value, String> {
        self.proximo_id += 1;
        let id = self.proximo_id;
        let msg = json!({"id": id, "method": metodo, "params": params});
        self.ws.send(Message::Text(msg.to_string())).await.map_err(|e| format!("falha ao enviar {metodo}: {e}"))?;

        let prazo = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let Some(item) = self.ws.next().await else {
                    return Err("o navegador fechou a conexão".to_string());
                };
                let texto = match item.map_err(|e| e.to_string())? {
                    Message::Text(t) => t.to_string(),
                    Message::Binary(b) => String::from_utf8_lossy(&b).into_owned(),
                    Message::Close(_) => return Err("o navegador fechou a conexão".to_string()),
                    _ => continue,
                };
                let Ok(v) = serde_json::from_str::<Value>(&texto) else { continue };
                if v.get("id").and_then(Value::as_i64) == Some(id) {
                    if let Some(err) = v.get("error") {
                        let m = err.get("message").and_then(Value::as_str).unwrap_or("erro CDP");
                        return Err(m.to_string());
                    }
                    return Ok(v.get("result").cloned().unwrap_or(Value::Null));
                }
            }
        })
        .await;
        prazo.map_err(|_| format!("{metodo} passou do tempo"))?
    }

    /// Roda JavaScript na página e devolve o valor (por `returnByValue`).
    pub async fn evaluar(&mut self, expr: &str) -> Result<Value, String> {
        let r = self
            .comando("Runtime.evaluate", json!({"expression": expr, "returnByValue": true, "awaitPromise": true}))
            .await?;
        Ok(r.get("result").and_then(|o| o.get("value")).cloned().unwrap_or(Value::Null))
    }

    /// Abre uma URL e espera a página carregar (readyState = complete).
    pub async fn ir(&mut self, url: &str) -> Result<(), String> {
        self.comando("Page.navigate", json!({"url": url})).await?;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            if self.evaluar("document.readyState").await.ok().and_then(|v| v.as_str().map(str::to_string))
                == Some("complete".into())
            {
                return Ok(());
            }
        }
        Ok(())
    }

    /// O texto visível da página.
    pub async fn texto(&mut self) -> Result<String, String> {
        Ok(self.evaluar("document.body ? document.body.innerText : ''").await?.as_str().unwrap_or("").to_string())
    }

    /// Clica em `(x, y)` com o **cursor sintético** (o mouse real não se move).
    pub async fn clicar(&mut self, x: f64, y: f64) -> Result<(), String> {
        let base = |tipo: &str| json!({"type": tipo, "x": x, "y": y, "button": "left", "buttons": 1, "clickCount": 1});
        self.comando("Input.dispatchMouseEvent", json!({"type":"mouseMoved","x":x,"y":y})).await?;
        self.comando("Input.dispatchMouseEvent", base("mousePressed")).await?;
        self.comando("Input.dispatchMouseEvent", base("mouseReleased")).await?;
        Ok(())
    }

    /// Acha um elemento pelo seletor CSS, rola até ele e clica no centro dele.
    pub async fn clicar_seletor(&mut self, seletor: &str) -> Result<(), String> {
        let centro = self.evaluar(&js_centro_do_seletor(seletor)).await?;
        let (Some(x), Some(y)) = (centro.get("x").and_then(Value::as_f64), centro.get("y").and_then(Value::as_f64))
        else {
            return Err(format!("não achei o elemento \"{seletor}\" na página"));
        };
        self.clicar(x, y).await
    }

    /// Digita um texto no elemento em foco (evento sintético de teclado).
    pub async fn digitar(&mut self, texto: &str) -> Result<(), String> {
        self.comando("Input.insertText", json!({"text": texto})).await?;
        Ok(())
    }

    /// Rola a página `dy` pixels (positivo = para baixo), no centro da janela.
    pub async fn rolar(&mut self, dy: f64) -> Result<(), String> {
        let (x, y) = (self.largura as f64 / 2.0, self.altura as f64 / 2.0);
        self.comando("Input.dispatchMouseEvent", json!({"type":"mouseWheel","x":x,"y":y,"deltaX":0,"deltaY":dy}))
            .await?;
        Ok(())
    }

    /// Captura a tela da página (PNG) e salva no caminho dado.
    pub async fn captura(&mut self, caminho: &std::path::Path) -> Result<(), String> {
        let r = self.comando("Page.captureScreenshot", json!({"format": "png"})).await?;
        let b64 = r.get("data").and_then(Value::as_str).ok_or("o navegador não devolveu a imagem")?;
        let bytes = base64_decode(b64).ok_or("imagem em base64 inválida")?;
        std::fs::write(caminho, bytes).map_err(|e| format!("não consegui salvar a captura: {e}"))?;
        Ok(())
    }
}

/// Os "olhos" do corvo (o mesmo arquivo que a extensão do Chrome usa): numera os
/// elementos da página e descreve tudo em texto curto.
pub const OLHOS_JS: &str = include_str!("../../../extensao/chrome/pagina.js");

impl Navegador {
    /// Garante os olhos na página atual (cada navegação começa sem eles).
    async fn olhos(&mut self) -> Result<(), String> {
        self.evaluar(OLHOS_JS).await.map(|_| ())
    }

    /// A página descrita em texto, com os elementos usáveis numerados.
    pub async fn ler_numerado(&mut self) -> Result<String, String> {
        self.olhos().await?;
        Ok(self.evaluar("window.__dotcrow.ler()").await?.as_str().unwrap_or("").to_string())
    }

    /// Centro do elemento `n` (rolado para a vista), com o rótulo e se é sensível.
    pub async fn alvo(&mut self, n: i64) -> Result<Value, String> {
        self.olhos().await?;
        let v = self.evaluar(&format!("window.__dotcrow.alvo({n})")).await?;
        if v.is_null() {
            return Err(format!("o elemento [{n}] não existe mais: use ler de novo (a página mudou)"));
        }
        Ok(v)
    }

    /// Clica no elemento `n` com o cursor sintético (o vermelho aparece onde o corvo clica).
    pub async fn clicar_n(&mut self, n: i64) -> Result<String, String> {
        let a = self.alvo(n).await?;
        let (x, y) = (a["x"].as_f64().unwrap_or(0.0), a["y"].as_f64().unwrap_or(0.0));
        let _ = self.evaluar(&format!("window.__dotcrow.cursor({x},{y})")).await;
        tokio::time::sleep(Duration::from_millis(380)).await;
        self.clicar(x, y).await?;
        Ok(format!("Cliquei em [{n}] {} \"{}\".", a["tipo"].as_str().unwrap_or(""), a["rotulo"].as_str().unwrap_or("")))
    }

    /// Põe o foco no elemento `n` (para digitar); `limpar` seleciona o que já estava escrito.
    pub async fn focar(&mut self, n: i64, limpar: bool) -> Result<Value, String> {
        self.olhos().await?;
        let v = self.evaluar(&format!("window.__dotcrow.focar({n}, {limpar})")).await?;
        if v.is_null() {
            return Err(format!("o elemento [{n}] não existe mais: use ler de novo"));
        }
        Ok(v)
    }

    /// Escolhe uma opção de um menu `<select>` pelo texto.
    pub async fn escolher(&mut self, n: i64, opcao: &str) -> Result<String, String> {
        self.olhos().await?;
        let op = serde_json::to_string(opcao).unwrap_or_default();
        let v = self.evaluar(&format!("window.__dotcrow.escolher({n}, {op})")).await?;
        if let Some(e) = v.get("erro").and_then(Value::as_str) {
            return Err(format!("{e}. Opções: {}", v.get("opcoes").map(|o| o.to_string()).unwrap_or_default()));
        }
        match v.get("escolhida").and_then(Value::as_str) {
            Some(t) => Ok(format!("Escolhi \"{t}\" no menu [{n}].")),
            None => Err(format!("[{n}] não é um menu de opções: clique nele e escolha na lista")),
        }
    }

    /// Aperta uma tecla especial (Enter, Tab, Escape, setas, Backspace...).
    pub async fn tecla(&mut self, tecla: &str) -> Result<(), String> {
        let (key, code, vk) = tecla_cdp(tecla).ok_or_else(|| format!("não conheço a tecla \"{tecla}\""))?;
        let texto = if key == "Enter" { "\r" } else { "" };
        for tipo in ["keyDown", "keyUp"] {
            let mut p = json!({"type": tipo, "key": key, "code": code, "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk});
            if tipo == "keyDown" && !texto.is_empty() {
                p["text"] = json!(texto);
            }
            self.comando("Input.dispatchKeyEvent", p).await?;
        }
        Ok(())
    }

    /// Passa o mouse sintético por cima de `(x, y)` (menus que abrem ao passar o mouse).
    pub async fn passar(&mut self, x: f64, y: f64) -> Result<(), String> {
        let _ = self.evaluar(&format!("window.__dotcrow && window.__dotcrow.cursor({x},{y})")).await;
        self.comando("Input.dispatchMouseEvent", json!({"type":"mouseMoved","x":x,"y":y})).await.map(|_| ())
    }
}

/// Teclas especiais: (key, code, windowsVirtualKeyCode).
pub fn tecla_cdp(nome: &str) -> Option<(&'static str, &'static str, i64)> {
    Some(match nome.trim().to_lowercase().as_str() {
        "enter" | "return" | "entrar" => ("Enter", "Enter", 13),
        "tab" => ("Tab", "Tab", 9),
        "escape" | "esc" => ("Escape", "Escape", 27),
        "backspace" | "apagar" => ("Backspace", "Backspace", 8),
        "delete" | "del" => ("Delete", "Delete", 46),
        "space" | "espaco" | "espaço" => (" ", "Space", 32),
        "arrowup" | "up" | "cima" | "seta_cima" => ("ArrowUp", "ArrowUp", 38),
        "arrowdown" | "down" | "baixo" | "seta_baixo" => ("ArrowDown", "ArrowDown", 40),
        "arrowleft" | "left" | "esquerda" => ("ArrowLeft", "ArrowLeft", 37),
        "arrowright" | "right" | "direita" => ("ArrowRight", "ArrowRight", 39),
        "home" | "inicio" | "início" => ("Home", "Home", 36),
        "end" | "fim" => ("End", "End", 35),
        "pageup" => ("PageUp", "PageUp", 33),
        "pagedown" => ("PageDown", "PageDown", 34),
        _ => return None,
    })
}

impl Drop for Navegador {
    fn drop(&mut self) {
        // O Chrome morre junto (kill_on_drop); limpamos o perfil descartável.
        let _ = std::fs::remove_dir_all(&self.perfil);
    }
}

/// JS que devolve `{x, y}` do centro de um elemento (rolando até ele antes), ou `null`.
fn js_centro_do_seletor(seletor: &str) -> String {
    let s = seletor.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"(() => {{
            const e = document.querySelector("{s}");
            if (!e) return null;
            e.scrollIntoView({{block:"center",inline:"center"}});
            const r = e.getBoundingClientRect();
            return {{x: r.x + r.width/2, y: r.y + r.height/2}};
        }})()"#
    )
}

/// Acha uma porta TCP livre no localhost (pede ao SO uma porta efêmera e a devolve).
async fn porta_livre() -> Result<u16, String> {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.map_err(|e| e.to_string())?;
    let p = l.local_addr().map_err(|e| e.to_string())?.port();
    drop(l);
    Ok(p)
}

/// Espera o Chrome subir e devolve a URL WebSocket da primeira aba (page target).
async fn esperar_ws(porta: u16) -> Result<String, String> {
    let cliente =
        reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(2)).build().map_err(|e| e.to_string())?;
    let url = format!("http://127.0.0.1:{porta}/json");
    for _ in 0..50 {
        if let Ok(resp) = cliente.get(&url).send().await
            && let Ok(corpo) = resp.text().await
            && let Some(ws) = ws_url_do_json(&corpo)
        {
            return Ok(ws);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err("o navegador não abriu a porta de depuração a tempo".into())
}

/// Lê o JSON de `/json` do Chrome e devolve a `webSocketDebuggerUrl` da 1ª aba (`type:page`).
fn ws_url_do_json(corpo: &str) -> Option<String> {
    let v: Value = serde_json::from_str(corpo).ok()?;
    let lista = v.as_array()?;
    lista
        .iter()
        .find(|t| t.get("type").and_then(Value::as_str) == Some("page"))
        .or_else(|| lista.first())?
        .get("webSocketDebuggerUrl")?
        .as_str()
        .map(str::to_string)
}

/// Decodifica base64 padrão (o que o CDP usa na captura de tela).
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let limpo: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(limpo.len() / 4 * 3);
    for bloco in limpo.chunks(4) {
        let mut acc = 0u32;
        let mut n = 0;
        for &c in bloco {
            if c == b'=' {
                break;
            }
            acc = (acc << 6) | val(c)?;
            n += 1;
        }
        match n {
            4 => {
                out.push((acc >> 16) as u8);
                out.push((acc >> 8) as u8);
                out.push(acc as u8);
            }
            3 => {
                let acc = acc << 6;
                out.push((acc >> 16) as u8);
                out.push((acc >> 8) as u8);
            }
            2 => {
                let acc = acc << 12;
                out.push((acc >> 16) as u8);
            }
            _ => break,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acha_a_ws_da_aba() {
        let corpo = r#"[
            {"type":"background_page","webSocketDebuggerUrl":"ws://x/bg"},
            {"type":"page","title":"t","webSocketDebuggerUrl":"ws://127.0.0.1:9/devtools/page/ABC"}
        ]"#;
        assert_eq!(ws_url_do_json(corpo).unwrap(), "ws://127.0.0.1:9/devtools/page/ABC");
        assert!(ws_url_do_json("[]").is_none());
        assert!(ws_url_do_json("não é json").is_none());
    }

    #[test]
    fn seletor_vira_js_seguro() {
        let js = js_centro_do_seletor("a.botão\"x");
        assert!(js.contains("querySelector"));
        assert!(js.contains("a.botão\\\"x"), "aspas do seletor têm de ser escapadas");
    }

    #[test]
    fn base64_vai_e_volta() {
        // "DotCrow" em base64 é "RG90Q3Jvdw==".
        assert_eq!(base64_decode("RG90Q3Jvdw==").unwrap(), b"DotCrow");
        assert_eq!(base64_decode("").unwrap(), b"");
        // tolera quebras de linha (o CDP às vezes manda a imagem quebrada)
        assert_eq!(base64_decode("RG90\nQ3Jvdw==").unwrap(), b"DotCrow");
    }
}
