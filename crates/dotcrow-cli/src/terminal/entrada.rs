//! Leitura de linhas no terminal, com setas, histórico e Tab para completar.
//!
//! Tab completa os comandos (`/ver`, `/pasta`...) e caminhos de arquivo depois de `@`
//! (`@no` + Tab → `@notas.php`) ou depois dos comandos que recebem um caminho.
//!
//! Toda leitura do modo terminal passa por aqui, inclusive as confirmações no meio de
//! uma tarefa. Assim Ctrl+C numa pergunta vira "parar a tarefa" em vez de matar o
//! programa. Sem terminal de verdade (entrada por pipe), lê linha a linha do stdin.

use std::{
    borrow::Cow,
    io::{BufRead, IsTerminal},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

use rustyline::{
    CompletionType, Config, Context, Editor, Helper,
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    history::{DefaultHistory, History},
    validate::Validator,
};

/// Os comandos, para o Tab completar.
pub const COMANDOS: [&str; 26] = [
    "/ajuda",
    "/civ",
    "/audio",
    "/celular",
    "/conectores",
    "/arquivos",
    "/ver",
    "/buscar",
    "/pastas",
    "/pasta",
    "/anexos",
    "/alinhar",
    "/perfil",
    "/perguntas",
    "/modo",
    "/provedor",
    "/chave",
    "/testar",
    "/desfazer",
    "/nova",
    "/painel",
    "/sair",
    "/limpar",
    "/ls",
    "/help",
    "/exit",
];

/// O que veio do teclado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leitura {
    /// Uma linha (sem a quebra).
    Linha(String),
    /// Ctrl+C.
    Interrompido,
    /// Ctrl+D ou fim da entrada.
    Fim,
}

type Ed = Editor<Ajudante, DefaultHistory>;

/// A entrada do modo terminal.
pub struct Entrada {
    ed: Mutex<Option<Ed>>,
    historico: Option<PathBuf>,
}

impl Entrada {
    /// `raizes` são as pastas do projeto (a primeira é a principal): o Tab completa a
    /// partir delas. `historico` é o arquivo onde ficam as linhas digitadas.
    pub fn nova(raizes: Arc<RwLock<Vec<PathBuf>>>, historico: Option<PathBuf>) -> Arc<Self> {
        let ed = if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            let cfg = Config::builder()
                .auto_add_history(false)
                .completion_type(CompletionType::List)
                .max_history_size(2000)
                .ok()
                .map(|b| b.build())
                .unwrap_or_default();
            Ed::with_config(cfg).ok().map(|mut e| {
                e.set_helper(Some(Ajudante { raizes, prompt_cor: Mutex::new(String::new()) }));
                if let Some(h) = &historico {
                    let _ = e.load_history(h);
                }
                e
            })
        } else {
            None
        };
        Arc::new(Self { ed: Mutex::new(ed), historico })
    }

    /// Lê uma linha. `cor` é o mesmo prompt com cores (o editor mede o `simples`).
    /// Com `guardar`, a linha entra no histórico (as respostas às perguntas não entram).
    pub async fn ler(self: &Arc<Self>, simples: &str, cor: &str, guardar: bool) -> Leitura {
        let eu = self.clone();
        let (simples, cor) = (simples.to_string(), cor.to_string());
        tokio::task::spawn_blocking(move || eu.ler_agora(&simples, &cor, guardar)).await.unwrap_or(Leitura::Fim)
    }

    fn ler_agora(&self, simples: &str, cor: &str, guardar: bool) -> Leitura {
        let mut guarda = self.ed.lock().unwrap_or_else(|e| e.into_inner());
        let Some(ed) = guarda.as_mut() else {
            // Sem terminal: lê do stdin, mostrando o prompt mesmo assim (ajuda em logs).
            use std::io::Write;
            print!("{simples}");
            let _ = std::io::stdout().flush();
            let mut linha = String::new();
            return match std::io::stdin().lock().read_line(&mut linha) {
                Ok(0) | Err(_) => Leitura::Fim,
                Ok(_) => {
                    let l = linha.trim_end_matches(['\n', '\r']).to_string();
                    println!();
                    Leitura::Linha(l)
                }
            };
        };
        // O editor mede o prompt simples e desenha a versão colorida (highlight_prompt).
        if let Some(h) = ed.helper_mut()
            && let Ok(mut c) = h.prompt_cor.lock()
        {
            *c = cor.to_string();
        }
        match ed.readline(simples) {
            Ok(l) => {
                if guardar && !l.trim().is_empty() {
                    let _ = ed.history_mut().add(l.trim());
                    if let Some(h) = &self.historico {
                        let _ = ed.save_history(h);
                    }
                }
                Leitura::Linha(l)
            }
            Err(ReadlineError::Interrupted) => Leitura::Interrompido,
            Err(_) => Leitura::Fim,
        }
    }
}

/// Completa comandos e caminhos, e pinta o prompt.
pub struct Ajudante {
    raizes: Arc<RwLock<Vec<PathBuf>>>,
    prompt_cor: Mutex<String>,
}

impl Ajudante {
    fn base(&self) -> PathBuf {
        self.raizes
            .read()
            .ok()
            .and_then(|r| r.first().cloned())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
}

/// Comandos depois dos quais vem um caminho.
const COM_CAMINHO: [&str; 7] =
    ["/ver ", "/arquivos ", "/ls ", "/pasta + ", "/pasta adicionar ", "/pasta - ", "/buscar "];

impl Completer for Ajudante {
    type Candidate = Pair;

    fn complete(&self, linha: &str, pos: usize, _ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        let antes = &linha[..pos];
        let inicio = antes.rfind(char::is_whitespace).map(|i| i + 1).unwrap_or(0);
        let palavra = &antes[inicio..];
        // Comando no começo da linha.
        if inicio == 0 && palavra.starts_with('/') {
            let lista = COMANDOS
                .iter()
                .filter(|c| c.starts_with(palavra))
                .map(|c| Pair { display: (*c).to_string(), replacement: format!("{c} ") })
                .collect();
            return Ok((0, lista));
        }
        // @caminho em qualquer lugar da linha.
        if let Some(parcial) = palavra.strip_prefix('@') {
            let lista = completar_caminho(parcial, &self.base())
                .into_iter()
                .map(|(mostrar, trocar)| Pair { display: mostrar, replacement: format!("@{trocar}") })
                .collect();
            return Ok((inicio, lista));
        }
        // Caminho depois de um comando que recebe caminho.
        if COM_CAMINHO.iter().any(|c| antes.starts_with(c)) {
            let lista = completar_caminho(palavra, &self.base())
                .into_iter()
                .map(|(mostrar, trocar)| Pair { display: mostrar, replacement: trocar })
                .collect();
            return Ok((inicio, lista));
        }
        Ok((pos, Vec::new()))
    }
}

/// Possíveis caminhos para `parcial` (relativo a `base`, ou com `~/` ou `/`).
/// Devolve (o que mostrar, o texto que entra na linha). Pastas terminam em `/`.
pub fn completar_caminho(parcial: &str, base: &Path) -> Vec<(String, String)> {
    let (dir_txt, prefixo) = match parcial.rfind('/') {
        Some(i) => (&parcial[..=i], &parcial[i + 1..]),
        None => ("", parcial),
    };
    let dir = if dir_txt.is_empty() { base.to_path_buf() } else { super::projeto::expandir(dir_txt, base) };
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let prefixo_min = prefixo.to_lowercase();
    let mut lista: Vec<(String, String)> = rd
        .flatten()
        .filter_map(|e| {
            let nome = e.file_name().to_string_lossy().to_string();
            if nome.starts_with('.') && !prefixo.starts_with('.') {
                return None;
            }
            if !nome.to_lowercase().starts_with(&prefixo_min) {
                return None;
            }
            let pasta = e.path().is_dir();
            let mostrar = if pasta { format!("{nome}/") } else { nome.clone() };
            Some((mostrar.clone(), format!("{dir_txt}{mostrar}")))
        })
        .collect();
    lista.sort();
    lista.truncate(200);
    lista
}

impl Hinter for Ajudante {
    type Hint = String;
}
impl Highlighter for Ajudante {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, default: bool) -> Cow<'b, str> {
        match self.prompt_cor.lock() {
            Ok(c) if default && !c.is_empty() => Cow::Owned(c.clone()),
            _ => Cow::Borrowed(prompt),
        }
    }
}
impl Validator for Ajudante {}
impl Helper for Ajudante {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completa_caminhos_e_pastas() {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("dotcrow-entrada-{}-{n}", std::process::id()));
        std::fs::create_dir_all(d.join("admin")).unwrap();
        std::fs::write(d.join("notas.php"), "").unwrap();
        std::fs::write(d.join("Notas-velhas.txt"), "").unwrap();
        std::fs::write(d.join(".env"), "").unwrap();
        std::fs::write(d.join("admin/painel.php"), "").unwrap();

        let r = completar_caminho("no", &d);
        assert_eq!(r.len(), 2, "sem diferenciar maiúsculas: {r:?}");
        assert!(r.iter().any(|(_, t)| t == "notas.php"));
        assert_eq!(completar_caminho("ad", &d), vec![("admin/".to_string(), "admin/".to_string())]);
        assert_eq!(completar_caminho("admin/p", &d), vec![("painel.php".to_string(), "admin/painel.php".to_string())]);
        // Ocultos só quando o dono começa com ponto.
        assert!(!completar_caminho("", &d).iter().any(|(m, _)| m == ".env"));
        assert!(completar_caminho(".", &d).iter().any(|(m, _)| m == ".env"));
        let _ = std::fs::remove_dir_all(d);
    }
}
