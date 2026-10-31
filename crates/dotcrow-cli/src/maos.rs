//! As mãos dos corvos: terminal, arquivos, internet, navegador, mouse e teclado.
//!
//! Regra: o que fica dentro da pasta da conversa e não sai do computador roda
//! direto. O que mexe no sistema, apaga fora da pasta, publica algo, toca em
//! segredos ou assume o mouse e o teclado pede a confirmação do dono (o "1%").
//! "Permitir sempre" vale só para aquele tipo de ação, só naquela conversa.
//! O cofre de chaves do DotCrow nunca é lido, nem com confirmação.

use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use dotcrow_agents::hands::{Hands, ToolCall, ToolOutput, clip};

/// Saída máxima de um comando.
const MAX_OUT: usize = 30_000;
/// Tamanho máximo de um arquivo lido de uma vez.
const MAX_READ: usize = 120_000;
/// Tamanho máximo de uma página baixada pela ferramenta `web`.
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
/// Tempo padrão e máximo de um comando.
const CMD_TIMEOUT: u64 = 120;
const CMD_TIMEOUT_MAX: u64 = 900;

/// Resposta do dono a um pedido de confirmação.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resposta {
    /// Só desta vez.
    Sim,
    /// Este tipo de ação, até o fim da conversa.
    Sempre,
    /// Não.
    Nao,
}

impl Resposta {
    /// Lê `sim`, `sempre` ou `nao`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "sim" | "s" | "yes" | "y" => Some(Self::Sim),
            "sempre" | "a" | "always" => Some(Self::Sempre),
            "nao" | "não" | "n" | "no" => Some(Self::Nao),
            _ => None,
        }
    }

    /// Nome usado na API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sim => "sim",
            Self::Sempre => "sempre",
            Self::Nao => "nao",
        }
    }
}

/// Quem pergunta ao dono (o painel ou o terminal).
#[async_trait::async_trait]
pub trait Aprovador: Send + Sync {
    /// Pergunta e espera a resposta.
    async fn pedir(&self, n: usize, titulo: &str, motivo: &str) -> Resposta;

    /// Faz uma pergunta aberta ao dono no meio da tarefa, com respostas sugeridas.
    /// `None` = não há ninguém para responder agora (o corvo decide sozinho).
    async fn perguntar(&self, _pergunta: &str, _opcoes: &[String]) -> Option<String> {
        None
    }
}

/// Uma pasta a mais que participa do projeto, além da principal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PastaExtra {
    /// Caminho absoluto.
    pub caminho: PathBuf,
    /// Se os corvos podem gravar nela sem perguntar (falso = só leitura).
    pub escrita: bool,
}

/// Tipos de ação que pedem confirmação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Categoria {
    Sistema,
    Apagar,
    ApagarProjeto,
    Publicar,
    Segredos,
    ForaDaPasta,
    Controle,
    Envio,
    /// Ligar, SMS, tocar em publicar/enviar/pagar/apagar: não tem volta, pergunta sempre.
    Mensagem,
    /// Liga o microfone: pergunta até no modo Livre.
    Microfone,
}

impl Categoria {
    fn motivo(self) -> &'static str {
        match self {
            Self::Sistema => "mexe no sistema (administrador, instalar programas ou serviços)",
            Self::Apagar => "apaga ou sobrescreve coisas fora da pasta da conversa",
            Self::ApagarProjeto => "apaga arquivos do projeto (o que um comando apaga não volta com /desfazer)",
            Self::Publicar => "publica ou envia algo para fora deste computador",
            Self::Segredos => "toca em senhas, chaves ou certificados",
            Self::ForaDaPasta => "grava fora da pasta da conversa",
            Self::Controle => "assume a tela: o mouse e o teclado do computador ou os toques no celular",
            Self::Envio => "envia dados para um site (formulário, POST)",
            Self::Mensagem => "liga, manda mensagem ou publica em nome do dono (não tem volta)",
            Self::Microfone => "liga o microfone para ouvir",
        }
    }
}

/// Quanto os corvos podem fazer numa conversa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Modo {
    /// Caixa de areia: o terminal roda isolado e só grava na pasta da conversa; sem mouse e teclado.
    Sandbox,
    /// O computador inteiro, perguntando antes do que é sensível.
    #[default]
    Computador,
    /// O computador inteiro, sem perguntar (só o que pode destruir o sistema ainda pergunta).
    Livre,
}

impl Modo {
    /// Lê `sandbox`, `computador` ou `livre`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "sandbox" | "caixa" => Some(Self::Sandbox),
            "computador" | "pc" | "" => Some(Self::Computador),
            "livre" | "autonomo" | "autônomo" => Some(Self::Livre),
            _ => None,
        }
    }

    /// Nome usado na API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sandbox => "sandbox",
            Self::Computador => "computador",
            Self::Livre => "livre",
        }
    }
}

/// Liberações dadas com "Permitir sempre" numa conversa.
pub type Liberados = Arc<Mutex<HashSet<Categoria>>>;

/// As mãos de uma tarefa.
pub struct Maos {
    modo: Modo,
    pasta: PathBuf,
    cofre: PathBuf,
    aprovador: Arc<dyn Aprovador>,
    liberados: Liberados,
    http: Option<reqwest::Client>,
    /// O navegador de cursor próprio (CDP), aberto sob demanda e reaproveitado na tarefa.
    nav: Arc<tokio::sync::Mutex<Option<crate::cdp::Navegador>>>,
    /// Pastas que também fazem parte do projeto, além da principal.
    extras: Vec<PastaExtra>,
    /// A pasta principal é um projeto do dono (arquivos de verdade), não uma pasta de conversa.
    projeto: bool,
    /// Onde anotar o original de cada arquivo antes de mudar, para poder desfazer.
    diario: Option<Arc<crate::desfazer::Diario>>,
    /// Os corvos podem fazer perguntas ao dono no meio da tarefa.
    pode_perguntar: bool,
    /// Onde vão as entregas que não fazem parte do projeto (a pasta Downloads).
    saidas: Option<PathBuf>,
    /// As chaves e contas dos conectores (X, GitHub, Google Drive...).
    conexoes: Option<Arc<crate::conectores::Conexoes>>,
    /// A ponte com a extensão "DotCrow no Chrome" (só quando a tarefa vem do painel).
    chrome: Option<Arc<crate::ponte_chrome::Ponte>>,
    /// O dono já liberou o Chrome dele nesta tarefa.
    chrome_liberado: std::sync::atomic::AtomicBool,
}

impl Maos {
    /// Mãos que trabalham em `pasta` (a pasta da conversa). `cofre` é a pasta de dados do DotCrow.
    pub fn new(
        modo: Modo,
        pasta: PathBuf,
        cofre: PathBuf,
        aprovador: Arc<dyn Aprovador>,
        liberados: Liberados,
    ) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = dotcrow_providers::tls::preparar(
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(60))
                .user_agent(concat!(
                    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140 Safari/537.36 dotcrow/",
                    env!("CARGO_PKG_VERSION")
                )),
        )
        .build()
        .ok();
        Self {
            modo,
            pasta,
            cofre,
            aprovador,
            liberados,
            http,
            nav: Arc::new(tokio::sync::Mutex::new(None)),
            extras: Vec::new(),
            projeto: false,
            diario: None,
            pode_perguntar: false,
            saidas: None,
            conexoes: None,
            chrome: None,
            chrome_liberado: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Liga os conectores (serviços com API, chaves no cofre).
    /// Liga a ferramenta `chrome`: os corvos usam o Chrome do dono pela extensão.
    pub fn com_chrome(mut self, ponte: Arc<crate::ponte_chrome::Ponte>) -> Self {
        self.chrome = Some(ponte);
        self
    }

    pub fn com_conectores(mut self, cx: crate::conectores::Conexoes) -> Self {
        self.conexoes = Some(Arc::new(cx));
        self
    }

    /// Entregas que não são do projeto (relatórios, PDFs, imagens, exportações) vão para
    /// `pasta` (a Downloads do dono), gravadas sem perguntar.
    pub fn com_saidas(mut self, pasta: PathBuf) -> Self {
        self.saidas = Some(normalizar(&pasta));
        self
    }

    /// Modo projeto: a pasta principal são os arquivos de verdade do dono, `extras` são as
    /// outras pastas que participam e `diario` guarda o original de cada arquivo alterado.
    pub fn com_projeto(mut self, extras: Vec<PastaExtra>, diario: Option<Arc<crate::desfazer::Diario>>) -> Self {
        self.projeto = true;
        self.extras = extras.into_iter().map(|e| PastaExtra { caminho: normalizar(&e.caminho), ..e }).collect();
        self.diario = diario;
        self
    }

    /// Liga a ferramenta `perguntar` (há alguém no terminal para responder).
    pub fn com_perguntas(mut self) -> Self {
        self.pode_perguntar = true;
        self
    }

    /// Anota o original no diário antes de mudar um arquivo.
    fn anotar(&self, p: &Path) {
        if let Some(d) = &self.diario {
            d.antes_de_mudar(p);
        }
    }

    /// A pasta principal e as extras, todas (para buscar e listar).
    fn raizes(&self) -> Vec<PathBuf> {
        std::iter::once(self.pasta.clone()).chain(self.extras.iter().map(|e| e.caminho.clone())).collect()
    }

    /// Pergunta ao dono, a menos que a categoria já esteja liberada nesta conversa.
    /// Ligar, SMS e publicar ([`Categoria::Mensagem`]) perguntam sempre, mesmo no modo Livre
    /// e depois de um "Permitir sempre"; o microfone pergunta mesmo no modo Livre.
    async fn autorizar(&self, call: &ToolCall, titulo: &str, cat: Categoria, extremo: bool) -> bool {
        let sempre_pergunta = extremo || cat == Categoria::Mensagem;
        if !sempre_pergunta && cat != Categoria::Microfone && self.modo == Modo::Livre {
            return true;
        }
        if self.modo == Modo::Sandbox
            && matches!(cat, Categoria::ForaDaPasta | Categoria::Controle | Categoria::Mensagem | Categoria::Microfone)
        {
            return false;
        }
        if !sempre_pergunta && self.liberados.lock().map(|l| l.contains(&cat)).unwrap_or(false) {
            return true;
        }
        let motivo = if extremo { "pode destruir dados ou o sistema e não tem volta" } else { cat.motivo() };
        match self.aprovador.pedir(call.n, titulo, motivo).await {
            Resposta::Sim => true,
            Resposta::Sempre => {
                if !sempre_pergunta && let Ok(mut l) = self.liberados.lock() {
                    l.insert(cat);
                }
                true
            }
            Resposta::Nao => false,
        }
    }

    /// Caminho absoluto: `~` vira a pasta pessoal, relativo vira a pasta da conversa.
    fn resolver(&self, caminho: &str) -> PathBuf {
        let c = caminho.trim();
        let p = if c == "~" {
            home()
        } else if let Some(rest) = c.strip_prefix("~/") {
            home().join(rest)
        } else if c.is_empty() || c == "." {
            self.pasta.clone()
        } else {
            let p = PathBuf::from(c);
            if p.is_absolute() { p } else { self.pasta.join(p) }
        };
        normalizar(&p)
    }

    /// Pode gravar sem perguntar: a pasta principal ou uma extra com escrita liberada.
    /// Compara também os caminhos reais, porque no Termux `~/storage/shared` é um link
    /// para `/storage/emulated/0` e o mesmo arquivo aparece pelos dois caminhos.
    fn dentro_da_pasta(&self, p: &Path) -> bool {
        let livres = || {
            std::iter::once(&self.pasta)
                .chain(self.saidas.iter())
                .chain(self.extras.iter().filter(|e| e.escrita).map(|e| &e.caminho))
        };
        if livres().any(|d| p.starts_with(d)) {
            return true;
        }
        let rp = real(p);
        livres().any(|d| rp.starts_with(real(d)))
    }

    fn no_cofre(&self, p: &Path) -> bool {
        p.starts_with(&self.cofre)
    }

    async fn terminal(&self, call: &ToolCall, titulo: &str) -> ToolOutput {
        let Some(cmd) = call.text("comando").map(str::trim).filter(|c| !c.is_empty()) else {
            return ToolOutput::err("faltou args.comando");
        };
        if cmd.contains(&*self.cofre.to_string_lossy()) {
            return ToolOutput::err("o cofre de chaves do DotCrow não pode ser lido nem alterado por comandos");
        }
        // Destruição irreversível do sistema nunca roda, em modo nenhum.
        if perigo_extremo(cmd) {
            return ToolOutput::err(
                "comando recusado: isto apagaria o sistema ou um disco inteiro e não tem volta. \
O DotCrow nunca roda isso. Se você precisa mesmo, faça à mão fora do DotCrow.",
            );
        }
        // No modo projeto a pasta tem os arquivos de verdade do dono: apagar ali também pergunta.
        let categoria =
            classificar_comando(cmd).or_else(|| (self.projeto && apaga_algo(cmd)).then_some(Categoria::ApagarProjeto));
        if let Some(cat) = categoria
            && !self.autorizar(call, titulo, cat, false).await
        {
            return ToolOutput::err("o dono não autorizou este comando");
        }
        let cwd = call.text("pasta").map(|p| self.resolver(p)).unwrap_or_else(|| self.pasta.clone());
        let secs = call.number("segundos").map(|s| s.clamp(1, CMD_TIMEOUT_MAX as i64) as u64).unwrap_or(CMD_TIMEOUT);
        let mut c = if self.modo == Modo::Sandbox {
            if !self.dentro_da_pasta(&cwd) {
                return ToolOutput::err("no modo Sandbox o terminal só roda dentro das pastas do projeto");
            }
            match self.caixa(cmd, &cwd) {
                Some(c) => c,
                None if cfg!(target_os = "android") => {
                    return ToolOutput::err(
                        "o modo Sandbox não existe no Android (Termux). Troque para o modo Computador.",
                    );
                }
                None => {
                    return ToolOutput::err(
                        "o modo Sandbox precisa do bubblewrap (Linux: sudo apt install bubblewrap). \
Troque para o modo Computador ou instale.",
                    );
                }
            }
        } else {
            let mut c = shell(cmd);
            c.current_dir(&cwd);
            c
        };
        c.env("DOTCROW_PASTA", &self.pasta);
        rodar(c, secs).await
    }

    /// Comando dentro da caixa de areia (bubblewrap): o sistema todo só para leitura,
    /// a pasta da conversa gravável, o cofre e as chaves SSH escondidos.
    fn caixa(&self, cmd: &str, cwd: &Path) -> Option<tokio::process::Command> {
        if !cfg!(target_os = "linux") {
            return None;
        }
        let bwrap = ["/usr/bin/bwrap", "/bin/bwrap"].into_iter().find(|p| Path::new(p).exists())?;
        let mut c = tokio::process::Command::new(bwrap);
        c.args(["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp"]);
        c.arg("--bind").arg(&self.pasta).arg(&self.pasta);
        for extra in self.extras.iter().filter(|e| e.escrita && e.caminho.is_dir()) {
            c.arg("--bind").arg(&extra.caminho).arg(&extra.caminho);
        }
        for escondido in [self.cofre.clone(), home().join(".ssh"), home().join(".gnupg"), home().join(".aws")] {
            if escondido.is_dir() {
                c.arg("--tmpfs").arg(escondido);
            }
        }
        c.args(["--unshare-pid", "--unshare-ipc", "--die-with-parent", "--new-session"]);
        c.arg("--chdir").arg(cwd);
        c.args(["/bin/bash", "-c", cmd]);
        Some(c)
    }

    async fn ler(&self, call: &ToolCall, titulo: &str) -> ToolOutput {
        let Some(c) = call.text("caminho") else { return ToolOutput::err("faltou args.caminho") };
        let p = self.resolver(c);
        if self.no_cofre(&p) {
            return ToolOutput::err("o cofre de chaves do DotCrow não pode ser lido");
        }
        if caminho_sensivel(&p) && !self.autorizar(call, titulo, Categoria::Segredos, false).await {
            return ToolOutput::err("o dono não autorizou ler este arquivo");
        }
        match tokio::fs::read(&p).await {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let inicio = call.number("linha_inicial").unwrap_or(1).max(1) as usize;
                let linhas = call.number("linhas").map(|l| l.max(1) as usize);
                let mut out = String::new();
                for (i, l) in text.lines().enumerate().skip(inicio - 1).take(linhas.unwrap_or(usize::MAX)) {
                    out.push_str(&format!("{:>5}  {l}\n", i + 1));
                    if out.len() > MAX_READ {
                        out.push_str("[... arquivo grande: leia o resto com linha_inicial]\n");
                        break;
                    }
                }
                if out.is_empty() {
                    out = "(arquivo vazio)".into();
                }
                ToolOutput::ok(out)
            }
            Err(e) => ToolOutput::err(format!("não consegui ler {}: {e}", p.display())),
        }
    }

    async fn escrever(&self, call: &ToolCall, titulo: &str) -> ToolOutput {
        let Some(c) = call.text("caminho") else { return ToolOutput::err("faltou args.caminho") };
        let conteudo = call.text("conteudo").unwrap_or("");
        let p = self.resolver(c);
        if self.no_cofre(&p) {
            return ToolOutput::err("o cofre de chaves do DotCrow não pode ser alterado");
        }
        if !self.dentro_da_pasta(&p) {
            let cat = if caminho_sensivel(&p) { Categoria::Segredos } else { Categoria::ForaDaPasta };
            if !self.autorizar(call, titulo, cat, false).await {
                return ToolOutput::err("o dono não autorizou gravar fora da pasta da conversa");
            }
        }
        if let Some(dir) = p.parent()
            && let Err(e) = tokio::fs::create_dir_all(dir).await
        {
            return ToolOutput::err(format!("não consegui criar a pasta {}: {e}", dir.display()));
        }
        let anexar = call.args.get("anexar").and_then(|v| v.as_bool()).unwrap_or(false);
        self.anotar(&p);
        let res = if anexar {
            use tokio::io::AsyncWriteExt;
            match tokio::fs::OpenOptions::new().create(true).append(true).open(&p).await {
                Ok(mut f) => f.write_all(conteudo.as_bytes()).await,
                Err(e) => Err(e),
            }
        } else {
            tokio::fs::write(&p, conteudo).await
        };
        match res {
            Ok(()) => ToolOutput::ok(format!(
                "{} {} ({} bytes)",
                if anexar { "Acrescentado em" } else { "Gravado" },
                p.display(),
                conteudo.len()
            )),
            Err(e) => ToolOutput::err(format!("não consegui gravar {}: {e}", p.display())),
        }
    }

    /// Troca um trecho exato de um arquivo, sem reescrever o resto (bom para mexer em
    /// arquivos grandes do dono, como um `notas.php`).
    async fn editar(&self, call: &ToolCall, titulo: &str) -> ToolOutput {
        let Some(c) = call.text("caminho") else { return ToolOutput::err("faltou args.caminho") };
        let Some(trocar) = call.text("trocar").filter(|t| !t.is_empty()) else {
            return ToolOutput::err("faltou args.trocar (o trecho exato que vai sair)");
        };
        let por = call.text("por").unwrap_or("");
        let todas = call.args.get("todas").and_then(|v| v.as_bool()).unwrap_or(false);
        let p = self.resolver(c);
        if self.no_cofre(&p) {
            return ToolOutput::err("o cofre de chaves do DotCrow não pode ser alterado");
        }
        if !self.dentro_da_pasta(&p) {
            let cat = if caminho_sensivel(&p) { Categoria::Segredos } else { Categoria::ForaDaPasta };
            if !self.autorizar(call, titulo, cat, false).await {
                return ToolOutput::err("o dono não autorizou mexer fora das pastas do projeto");
            }
        }
        let original = match tokio::fs::read(&p).await {
            Ok(b) => match String::from_utf8(b) {
                Ok(s) => s,
                Err(_) => return ToolOutput::err("o arquivo não é texto (UTF-8); use o terminal para ele"),
            },
            Err(e) => return ToolOutput::err(format!("não consegui ler {}: {e}", p.display())),
        };
        let (novo, n) = match trocar_trecho(&original, trocar, por, todas) {
            Ok(r) => r,
            Err(e) => return ToolOutput::err(e),
        };
        self.anotar(&p);
        match tokio::fs::write(&p, &novo).await {
            Ok(()) => {
                let linha = original.find(trocar).map(|i| original[..i].matches('\n').count() + 1).unwrap_or(1);
                ToolOutput::ok(format!(
                    "Editado {} ({n} trecho(s) trocado(s), a partir da linha {linha}). Releia o trecho se precisar conferir.",
                    p.display()
                ))
            }
            Err(e) => ToolOutput::err(format!("não consegui gravar {}: {e}", p.display())),
        }
    }

    /// Procura arquivos por nome e/ou conteúdo nas pastas do projeto (ou numa pasta dada).
    async fn buscar(&self, call: &ToolCall) -> ToolOutput {
        let consulta = crate::busca::Consulta {
            nome: call.text("nome").map(String::from),
            texto: call.text("texto").map(String::from),
            ocultos: call.args.get("ocultos").and_then(|v| v.as_bool()).unwrap_or(false),
        };
        if consulta.nome.as_deref().is_none_or(|n| n.trim().is_empty())
            && consulta.texto.as_deref().is_none_or(|t| t.trim().is_empty())
        {
            return ToolOutput::err("informe args.nome (parte do nome, aceita *) e/ou args.texto (conteúdo)");
        }
        let raizes = match call.text("pasta").map(str::trim).filter(|p| !p.is_empty()) {
            Some(p) => vec![self.resolver(p)],
            None => self.raizes(),
        };
        let cofre = self.cofre.clone();
        let raizes2 = raizes.clone();
        let r = tokio::task::spawn_blocking(move || crate::busca::buscar(&raizes2, &consulta, 200))
            .await
            .unwrap_or_default();
        let achados: Vec<_> = r.achados.into_iter().filter(|a| !a.caminho.starts_with(&cofre)).collect();
        if achados.is_empty() {
            return ToolOutput::ok("Nada encontrado (pastas como .git, node_modules e vendor ficam de fora).");
        }
        let mut out = format!("{} resultado(s):\n", achados.len());
        for a in &achados {
            let nome = crate::busca::exibir(&a.caminho, &raizes);
            match &a.linha {
                Some((n, l)) => out.push_str(&format!("{nome}:{n}: {l}\n")),
                None => out.push_str(&format!("{nome}\n")),
            }
        }
        if r.cortado {
            out.push_str("[... há mais: refine a busca]\n");
        }
        ToolOutput::ok(clip(&out, MAX_OUT))
    }

    /// Pergunta ao dono no meio da tarefa.
    async fn perguntar(&self, call: &ToolCall) -> ToolOutput {
        const SOZINHO: &str = "Siga com a opção mais razoável e diga na entrega o que você decidiu e por quê.";
        let Some(pergunta) = call.text("pergunta").map(str::trim).filter(|p| !p.is_empty()) else {
            return ToolOutput::err("faltou args.pergunta");
        };
        if !self.pode_perguntar {
            return ToolOutput::ok(format!("O dono não pode responder agora. {SOZINHO}"));
        }
        let opcoes: Vec<String> = call
            .args
            .get("opcoes")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).map(String::from).take(5).collect())
            .unwrap_or_default();
        match self.aprovador.perguntar(pergunta, &opcoes).await {
            Some(r) if !r.trim().is_empty() => ToolOutput::ok(format!("O dono respondeu: {}", r.trim())),
            Some(_) => ToolOutput::ok(format!("O dono deixou a decisão com você. {SOZINHO}")),
            None => ToolOutput::ok(format!("O dono não respondeu. {SOZINHO}")),
        }
    }

    async fn listar(&self, call: &ToolCall) -> ToolOutput {
        let p = self.resolver(call.text("caminho").unwrap_or("."));
        if self.no_cofre(&p) {
            return ToolOutput::err("o cofre de chaves do DotCrow fica de fora");
        }
        let mut rd = match tokio::fs::read_dir(&p).await {
            Ok(r) => r,
            Err(e) => return ToolOutput::err(format!("não consegui listar {}: {e}", p.display())),
        };
        let mut itens = Vec::new();
        while let Ok(Some(e)) = rd.next_entry().await {
            let nome = e.file_name().to_string_lossy().to_string();
            match e.metadata().await {
                Ok(m) if m.is_dir() => itens.push(format!("{nome}/")),
                Ok(m) => itens.push(format!("{nome}  ({})", tamanho(m.len()))),
                Err(_) => itens.push(nome),
            }
        }
        itens.sort();
        let total = itens.len();
        itens.truncate(400);
        let mut out = format!("{} ({} itens)\n{}", p.display(), total, itens.join("\n"));
        if total > 400 {
            out.push_str("\n[... mais itens]");
        }
        ToolOutput::ok(out)
    }

    async fn web(&self, call: &ToolCall, titulo: &str) -> ToolOutput {
        let Some(url) = call.text("url").map(str::trim) else { return ToolOutput::err("faltou args.url") };
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return ToolOutput::err("a url precisa começar com http:// ou https://");
        }
        let Some(http) = &self.http else { return ToolOutput::err("cliente HTTP indisponível") };
        let metodo = call.text("metodo").unwrap_or("GET").trim().to_uppercase();
        let mut req = match metodo.as_str() {
            "GET" => http.get(url),
            "HEAD" => http.head(url),
            "POST" | "PUT" | "PATCH" | "DELETE" => {
                if !self.autorizar(call, titulo, Categoria::Envio, false).await {
                    return ToolOutput::err("o dono não autorizou enviar dados para este site");
                }
                http.request(reqwest::Method::from_bytes(metodo.as_bytes()).unwrap_or(reqwest::Method::POST), url)
            }
            _ => return ToolOutput::err("metodo deve ser GET, HEAD, POST, PUT, PATCH ou DELETE"),
        };
        if let Some(h) = call.args.get("cabecalhos").and_then(|v| v.as_object()) {
            for (k, v) in h {
                if let Some(v) = v.as_str() {
                    req = req.header(k.as_str(), v);
                }
            }
        }
        if let Some(corpo) = call.args.get("corpo") {
            req = match corpo {
                serde_json::Value::String(s) => req.body(s.clone()),
                other => req.json(other),
            };
        }
        let res = match req.send().await {
            Ok(r) => r,
            Err(e) => return ToolOutput::err(format!("falha ao acessar {url}: {e}")),
        };
        let status = res.status();
        let final_url = res.url().to_string();
        let tipo =
            res.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let bytes = match res.bytes().await {
            Ok(b) => b,
            Err(e) => return ToolOutput::err(format!("falha ao ler a resposta: {e}")),
        };
        let corpo = &bytes[..bytes.len().min(MAX_PAGE_BYTES)];
        let texto = if tipo.contains("html") {
            html_para_texto(&String::from_utf8_lossy(corpo), &final_url)
        } else if tipo.starts_with("text/") || tipo.contains("json") || tipo.contains("xml") || tipo.is_empty() {
            String::from_utf8_lossy(corpo).to_string()
        } else {
            format!("(conteúdo binário {tipo}, {}; use a ferramenta baixar para salvar)", tamanho(bytes.len() as u64))
        };
        let head = format!("HTTP {} · {final_url} · {tipo}\n\n", status.as_u16());
        let out = format!("{head}{}", clip(&texto, MAX_OUT));
        if status.is_success() { ToolOutput::ok(out) } else { ToolOutput::err(out) }
    }

    async fn baixar(&self, call: &ToolCall, titulo: &str) -> ToolOutput {
        let (Some(url), Some(c)) = (call.text("url"), call.text("caminho")) else {
            return ToolOutput::err("faltou args.url ou args.caminho");
        };
        let p = self.resolver(c);
        if self.no_cofre(&p) {
            return ToolOutput::err("o cofre de chaves do DotCrow não pode ser alterado");
        }
        if !self.dentro_da_pasta(&p) && !self.autorizar(call, titulo, Categoria::ForaDaPasta, false).await {
            return ToolOutput::err("o dono não autorizou gravar fora da pasta da conversa");
        }
        let Some(http) = &self.http else { return ToolOutput::err("cliente HTTP indisponível") };
        let res = match http.get(url).timeout(Duration::from_secs(600)).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => return ToolOutput::err(format!("HTTP {} ao baixar {url}", r.status().as_u16())),
            Err(e) => return ToolOutput::err(format!("falha ao baixar {url}: {e}")),
        };
        let bytes = match res.bytes().await {
            Ok(b) => b,
            Err(e) => return ToolOutput::err(format!("falha ao baixar {url}: {e}")),
        };
        if let Some(dir) = p.parent() {
            let _ = tokio::fs::create_dir_all(dir).await;
        }
        self.anotar(&p);
        match tokio::fs::write(&p, &bytes).await {
            Ok(()) => ToolOutput::ok(format!("Baixado em {} ({})", p.display(), tamanho(bytes.len() as u64))),
            Err(e) => ToolOutput::err(format!("não consegui gravar {}: {e}", p.display())),
        }
    }

    /// Abre a página num Chrome invisível, que roda o JavaScript, e devolve o texto.
    async fn navegador(&self, call: &ToolCall) -> ToolOutput {
        // Com `acao`, usa o navegador de cursor próprio (CDP); sem, o modo antigo de uma
        // tacada só (abre, lê o HTML e fecha).
        if call.text("acao").is_some() {
            return self.navegador_cdp(call).await;
        }
        let Some(url) = call.text("url").map(str::trim) else { return ToolOutput::err("faltou args.url") };
        if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")) {
            return ToolOutput::err("a url precisa começar com http://, https:// ou file://");
        }
        let Some(bin) = chrome() else {
            return ToolOutput::err("não achei Chrome, Chromium, Edge ou Brave; use a ferramenta web");
        };
        // Perfil novo e descartável a cada chamada: um perfil compartilhado deixa um
        // SingletonLock para trás quando o navegador anterior não fecha direito, e aí
        // toda chamada seguinte falha. Com um perfil por chamada isso nunca acontece.
        let perfil = crate::trava::temporaria().join(format!("dotcrow-nav-{}-{}", std::process::id(), call.n));
        let _ = std::fs::remove_dir_all(&perfil);
        let base = |c: &mut tokio::process::Command| {
            c.arg("--headless=new")
                .arg("--disable-gpu")
                .arg("--no-first-run")
                .arg("--hide-scrollbars")
                .arg("--no-default-browser-check")
                .arg(format!("--user-data-dir={}", perfil.display()))
                .arg("--window-size=1366,900")
                .arg("--virtual-time-budget=10000");
        };
        let mut extra = String::new();
        if call.args.get("captura").and_then(|v| v.as_bool()).unwrap_or(false) {
            let arq = self.pasta.join(format!("captura-{}.png", call.n));
            let mut c = tokio::process::Command::new(&bin);
            base(&mut c);
            c.arg(format!("--screenshot={}", arq.display())).arg(url);
            let r = rodar(c, 90).await;
            extra = if arq.exists() {
                format!("\n\nCaptura da página salva em {}", arq.display())
            } else {
                format!("\n\nNão consegui salvar a captura: {}", clip(&r.text, 400))
            };
        }
        let mut c = tokio::process::Command::new(&bin);
        base(&mut c);
        c.arg("--dump-dom").arg(url);
        let r = rodar(c, 90).await;
        let _ = std::fs::remove_dir_all(&perfil);
        if !r.ok && r.text.len() < 200 {
            return ToolOutput::err(format!("o navegador falhou: {}", r.text));
        }
        let html = r.text.split_once("\n--- erros ---").map(|(a, _)| a).unwrap_or(&r.text);
        ToolOutput::ok(format!("{}{extra}", clip(&html_para_texto(html, url), MAX_OUT)))
    }

    /// Navegador com cursor próprio: abre numa janela dedicada e age por eventos sintéticos
    /// do CDP — o mouse de verdade do dono nunca se mexe.
    async fn navegador_cdp(&self, call: &ToolCall) -> ToolOutput {
        if self.modo == Modo::Sandbox {
            return ToolOutput::err("no modo Sandbox, navegue com a ferramenta `web` (sem janela na tela do dono)");
        }
        let acao = call.text("acao").unwrap_or("").trim().to_lowercase();

        // Abre o navegador na 1ª vez e reaproveita nas ações seguintes da tarefa.
        let mut guarda = self.nav.lock().await;
        if guarda.is_none() {
            let Some(bin) = chrome() else {
                return ToolOutput::err("não achei Chrome, Chromium, Edge ou Brave; use a ferramenta web");
            };
            match crate::cdp::Navegador::abrir(&bin, 1280, 900).await {
                Ok(n) => *guarda = Some(n),
                Err(e) => return ToolOutput::err(e),
            }
        }
        let Some(nav) = guarda.as_mut() else { return ToolOutput::err("navegador indisponível") };

        let r: Result<String, String> = match acao.as_str() {
            "ir" | "abrir" | "navegar" => {
                let Some(url) = call.text("url").map(str::trim) else { return ToolOutput::err("faltou args.url") };
                if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://")) {
                    return ToolOutput::err("a url precisa começar com http://, https:// ou file://");
                }
                nav.ir(url).await.map(|_| format!("Abri {url}."))
            }
            "ler" | "olhar" | "ver" => nav.ler_numerado().await.map(|t| clip(&t, MAX_OUT)),
            "texto" => nav.texto().await.map(|t| clip(&t, MAX_OUT)),
            "clicar" => {
                if let Some(n) = call.number("n").or_else(|| call.number("elemento")) {
                    nav.clicar_n(n).await
                } else if let Some(sel) = call.text("seletor") {
                    nav.clicar_seletor(sel).await.map(|_| format!("Cliquei em {sel}."))
                } else if let (Some(x), Some(y)) = (call.number("x"), call.number("y")) {
                    nav.clicar(x as f64, y as f64).await.map(|_| format!("Cliquei em {x},{y}."))
                } else {
                    return ToolOutput::err("clicar precisa de `seletor` ou de `x` e `y`");
                }
            }
            "digitar" => {
                let Some(texto) = call.text("texto") else { return ToolOutput::err("faltou args.texto") };
                if let Some(n) = call.number("n").or_else(|| call.number("elemento")) {
                    let limpar = call.args.get("limpar").and_then(|v| v.as_bool()).unwrap_or(true);
                    match nav.focar(n, limpar).await {
                        Ok(info) if info["sensivel"].as_bool() == Some(true) => {
                            let titulo = format!(
                                "navegador: digitar num campo sensível ({})",
                                info["rotulo"].as_str().unwrap_or("")
                            );
                            if !self.autorizar(call, &titulo, Categoria::Segredos, true).await {
                                return ToolOutput::err("o dono não autorizou digitar nesse campo (senha ou cartão)");
                            }
                        }
                        Ok(_) => {}
                        Err(e) => return ToolOutput::err(e),
                    }
                }
                nav.digitar(texto).await.map(|_| format!("Digitei \"{}\".", clip(texto, 120)))
            }
            "tecla" | "apertar" => {
                let Some(t) = call.text("tecla") else {
                    return ToolOutput::err("faltou args.tecla (ex.: Enter, Tab, Escape)");
                };
                nav.tecla(t).await.map(|_| format!("Apertei {t}."))
            }
            "escolher" | "selecionar" => {
                let (Some(n), Some(op)) = (call.number("n"), call.text("opcao")) else {
                    return ToolOutput::err("escolher precisa de n (o menu) e opcao (o texto da opção)");
                };
                nav.escolher(n, op).await
            }
            "passar_mouse" | "passar" | "hover" => {
                if let Some(n) = call.number("n") {
                    match nav.alvo(n).await {
                        Ok(a) => {
                            let (x, y) = (a["x"].as_f64().unwrap_or(0.0), a["y"].as_f64().unwrap_or(0.0));
                            nav.passar(x, y).await.map(|_| format!("Passei o mouse por [{n}]."))
                        }
                        Err(e) => Err(e),
                    }
                } else if let (Some(x), Some(y)) = (call.number("x"), call.number("y")) {
                    nav.passar(x as f64, y as f64).await.map(|_| format!("Passei o mouse por {x},{y}."))
                } else {
                    return ToolOutput::err("passar_mouse precisa de n ou de x e y");
                }
            }
            "voltar" => nav.evaluar("history.back()").await.map(|_| "Voltei uma página.".to_string()),
            "esperar" => {
                let s = call.number("segundos").unwrap_or(2).clamp(1, 30) as u64;
                tokio::time::sleep(std::time::Duration::from_secs(s)).await;
                Ok(format!("Esperei {s} s."))
            }
            "rolar" => {
                let dy = call.number("quantidade").unwrap_or(600) as f64;
                nav.rolar(dy).await.map(|_| format!("Rolei {dy} px."))
            }
            "captura" | "tela" => {
                let arq = self.pasta.join(format!("navegador-{}.png", call.n));
                nav.captura(&arq).await.map(|_| format!("Captura salva em {}", arq.display()))
            }
            outra => Err(format!(
                "ação de navegador \"{outra}\" não existe. Use: ir, ler, clicar, digitar, tecla, escolher, \
passar_mouse, rolar, voltar, esperar, captura."
            )),
        };
        match r {
            Ok(txt) => ToolOutput::ok(txt),
            Err(e) => ToolOutput::err(e),
        }
    }

    /// O Chrome do dono, pela extensão: as mesmas ações do navegador, nas abas do grupo
    /// DotCrow. A primeira ação da tarefa pede a confirmação do dono (o Chrome dele tem as
    /// contas logadas); senha e cartão pedem sempre.
    async fn chrome(&self, call: &ToolCall) -> ToolOutput {
        use std::sync::atomic::Ordering;
        if self.modo == Modo::Sandbox {
            return ToolOutput::err("no modo Sandbox os corvos não usam o Chrome do dono; use web ou navegador");
        }
        let Some(ponte) = &self.chrome else {
            return ToolOutput::err(
                "a extensão do Chrome só funciona com o painel (dotcrow painel); aqui, use a ferramenta navegador",
            );
        };
        let acao = call.text("acao").unwrap_or("").trim().to_lowercase();
        if acao.is_empty() {
            return ToolOutput::err("faltou args.acao (ir, ler, clicar, digitar, tecla, abas, nova_aba...)");
        }
        if !self.chrome_liberado.load(Ordering::Relaxed) {
            let titulo = format!("usar o seu Chrome (com as suas contas): {}", self.title(call));
            if !self.autorizar(call, &titulo, Categoria::Controle, false).await {
                return ToolOutput::err("o dono não liberou o Chrome dele nesta tarefa; use a ferramenta navegador");
            }
            self.chrome_liberado.store(true, Ordering::Relaxed);
        }
        let mut args = call.args.clone();
        let prazo = if acao == "esperar" { 40 } else { 60 };
        let mut r = ponte.pedir(&acao, args.clone(), prazo).await;
        // Campo de senha ou cartão: a extensão para e o dono decide.
        if let Ok(v) = &r
            && v.get("precisa_confirmar").and_then(|b| b.as_bool()) == Some(true)
        {
            let motivo = v.get("motivo").and_then(|m| m.as_str()).unwrap_or("campo sensível").to_string();
            if !self.autorizar(call, &format!("Chrome: {motivo}"), Categoria::Segredos, true).await {
                return ToolOutput::err("o dono não autorizou digitar nesse campo (senha ou cartão)");
            }
            args["confirmado"] = serde_json::json!(true);
            r = ponte.pedir(&acao, args, prazo).await;
        }
        match r {
            Ok(v) => {
                if let Some(b64) = v.get("png_base64").and_then(|b| b.as_str()) {
                    let arq = self.pasta.join(format!("chrome-{}.png", call.n));
                    return match crate::cdp::base64_decode(b64).map(|bytes| std::fs::write(&arq, bytes)) {
                        Some(Ok(())) => ToolOutput::ok(format!("Captura do Chrome salva em {}", arq.display())),
                        _ => ToolOutput::err("não consegui salvar a captura do Chrome"),
                    };
                }
                let texto =
                    v.get("texto").and_then(|t| t.as_str()).map(str::to_string).unwrap_or_else(|| v.to_string());
                ToolOutput::ok(clip(&texto, MAX_OUT))
            }
            Err(e) => ToolOutput::err(e),
        }
    }

    async fn abrir(&self, call: &ToolCall) -> ToolOutput {
        if self.modo == Modo::Sandbox {
            return ToolOutput::err("no modo Sandbox os corvos não abrem programas na tela do dono");
        }
        let Some(alvo) = call.text("alvo").map(str::trim) else { return ToolOutput::err("faltou args.alvo") };
        let alvo = if alvo.contains("://") { alvo.to_string() } else { self.resolver(alvo).display().to_string() };
        match abrir_no_sistema(&alvo) {
            Ok(()) => ToolOutput::ok(format!("Aberto: {alvo}")),
            Err(e) => ToolOutput::err(format!("não consegui abrir {alvo}: {e}")),
        }
    }

    async fn controle(&self, call: &ToolCall, titulo: &str) -> ToolOutput {
        if !cfg!(target_os = "linux") {
            return ToolOutput::err("mouse, teclado e janelas, por enquanto, só no Linux");
        }
        if self.modo == Modo::Sandbox {
            return ToolOutput::err("no modo Sandbox os corvos não usam mouse, teclado nem janelas");
        }
        if call.tool != "janelas" && !self.autorizar(call, titulo, Categoria::Controle, false).await {
            return ToolOutput::err("o dono não autorizou usar o mouse e o teclado");
        }
        let xdo = |args: Vec<String>| {
            let mut c = tokio::process::Command::new("xdotool");
            c.args(args);
            c
        };
        let s = |v: &str| v.to_string();
        match call.tool.as_str() {
            "mouse" => {
                let acao = call.text("acao").unwrap_or("clicar").to_lowercase();
                let mut args = Vec::new();
                if let (Some(x), Some(y)) = (call.number("x"), call.number("y")) {
                    args.extend([s("mousemove"), s("--sync"), x.to_string(), y.to_string()]);
                }
                let vezes = call.number("quantidade").unwrap_or(3).clamp(1, 50).to_string();
                match acao.as_str() {
                    "mover" => {}
                    "clicar" => args.extend([s("click"), s("1")]),
                    "duplo" => args.extend([s("click"), s("--repeat"), s("2"), s("1")]),
                    "direito" => args.extend([s("click"), s("3")]),
                    "rolar_cima" => args.extend([s("click"), s("--repeat"), vezes, s("4")]),
                    "rolar_baixo" => args.extend([s("click"), s("--repeat"), vezes, s("5")]),
                    "posicao" => args.extend([s("getmouselocation")]),
                    _ => {
                        return ToolOutput::err(
                            "acao: mover, clicar, duplo, direito, rolar_cima, rolar_baixo ou posicao",
                        );
                    }
                }
                if args.is_empty() {
                    return ToolOutput::err("mover precisa de x e y");
                }
                rodar(xdo(args), 20).await
            }
            "teclado" => {
                if let Some(t) = call.text("texto") {
                    rodar(xdo(vec![s("type"), s("--delay"), s("15"), s("--"), t.to_string()]), 120).await
                } else if let Some(k) = call.text("teclas") {
                    let mut a = vec![s("key"), s("--")];
                    a.extend(k.split_whitespace().map(String::from));
                    rodar(xdo(a), 20).await
                } else {
                    ToolOutput::err("use args.texto (digitar) ou args.teclas (ex.: \"ctrl+l\", \"Return\")")
                }
            }
            "janelas" => {
                let mut c = tokio::process::Command::new("wmctrl");
                c.arg("-lG");
                let mut r = rodar(c, 10).await;
                if !r.ok {
                    r = rodar(
                        xdo(vec![s("search"), s("--onlyvisible"), s("--name"), s("."), s("getwindowname"), s("%@")]),
                        10,
                    )
                    .await;
                }
                let tela = rodar(xdo(vec![s("getdisplaygeometry")]), 5).await;
                ToolOutput { ok: r.ok, text: format!("Tela: {}\n{}", tela.text.trim(), r.text) }
            }
            "focar" => {
                let Some(nome) = call.text("janela") else { return ToolOutput::err("faltou args.janela") };
                let mut c = tokio::process::Command::new("wmctrl");
                c.args(["-a", nome]);
                rodar(c, 10).await
            }
            "tela" => {
                let arq = self.pasta.join(format!("tela-{}.png", call.n));
                let a = arq.display().to_string();
                let tentativas: [(&str, Vec<String>); 4] = [
                    ("gnome-screenshot", vec![s("-f"), a.clone()]),
                    ("import", vec![s("-window"), s("root"), a.clone()]),
                    ("scrot", vec![s("-o"), a.clone()]),
                    ("xfce4-screenshooter", vec![s("-f"), s("-s"), a.clone()]),
                ];
                for (prog, args) in tentativas {
                    let mut c = tokio::process::Command::new(prog);
                    c.args(args);
                    if rodar(c, 30).await.ok && arq.exists() {
                        let tela = rodar(xdo(vec![s("getdisplaygeometry")]), 5).await;
                        return ToolOutput::ok(format!(
                            "Captura salva em {a}. Tela: {}. (Você não vê a imagem; use janelas e posições para se orientar.)",
                            tela.text.trim()
                        ));
                    }
                }
                ToolOutput::err("não achei programa de captura (gnome-screenshot, import, scrot)")
            }
            _ => ToolOutput::err("ferramenta desconhecida"),
        }
    }

    /// Um serviço pela API oficial, com a chave do cofre. Ler é livre; escrever pergunta
    /// sempre (é publicar, enviar ou apagar em nome do dono).
    async fn conector(&self, call: &ToolCall, titulo: &str) -> ToolOutput {
        let Some(cx) = &self.conexoes else {
            return ToolOutput::err("os conectores não estão ligados nesta tarefa");
        };
        let Some(nome) = call.text("servico").or_else(|| call.text("conector")).or_else(|| call.text("nome")) else {
            return ToolOutput::err("faltou args.servico (ex.: \"github\"). Veja a lista com conectores {busca}.");
        };
        let Some(c) = crate::conectores::achar(nome) else {
            let parecidos: Vec<&str> = crate::conectores::buscar(nome).iter().take(8).map(|c| c.id).collect();
            return ToolOutput::err(format!(
                "não conheço o conector \"{nome}\". Parecidos: {}. Veja todos com conectores {{busca}}.",
                if parecidos.is_empty() { "nenhum".to_string() } else { parecidos.join(", ") }
            ));
        };
        let metodo = call.text("metodo").unwrap_or("GET").trim().to_uppercase();
        if !crate::conectores::eh_leitura(&metodo) {
            if self.modo == Modo::Sandbox {
                return ToolOutput::err("no modo Sandbox os conectores só leem (GET)");
            }
            if !self.autorizar(call, titulo, Categoria::Mensagem, metodo == "DELETE").await {
                return ToolOutput::err(format!("o dono não autorizou: {titulo}"));
            }
        }
        let consulta: Vec<(String, String)> = call
            .args
            .get("consulta")
            .and_then(serde_json::Value::as_object)
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().map(String::from).unwrap_or_else(|| v.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let Some(http) = &self.http else { return ToolOutput::err("sem cliente HTTP") };
        let caminho = call.text("caminho").unwrap_or("");
        crate::conectores::chamar(http, cx, c, &metodo, caminho, &consulta, call.args.get("corpo")).await
    }

    /// O celular Android (ADB e Termux:API): planeja, pergunta quando precisa e executa.
    async fn celular(&self, call: &ToolCall) -> ToolOutput {
        if self.modo == Modo::Sandbox {
            return ToolOutput::err("no modo Sandbox os corvos não mexem no celular");
        }
        let plano = match crate::celular::planejar(call).await {
            Ok(p) => p,
            Err(e) => return e,
        };
        let cat = match plano.permissao {
            crate::celular::Permissao::Nenhuma => None,
            crate::celular::Permissao::Controle => Some(Categoria::Controle),
            crate::celular::Permissao::Microfone => Some(Categoria::Microfone),
            crate::celular::Permissao::Irreversivel => Some(Categoria::Mensagem),
        };
        if let Some(cat) = cat
            && !self.autorizar(call, &plano.titulo, cat, false).await
        {
            return ToolOutput::err(format!("o dono não autorizou: {}", plano.titulo));
        }
        crate::celular::executar(plano, &self.pasta, call.n).await
    }
}

#[async_trait::async_trait]
impl Hands for Maos {
    fn manual(&self) -> String {
        let pasta = self.pasta.display();
        let so = if cfg!(target_os = "windows") {
            "Windows (o terminal é o PowerShell)"
        } else if cfg!(target_os = "macos") {
            "macOS (o terminal é o bash)"
        } else if cfg!(target_os = "android") {
            "Android, dentro do Termux (o terminal é o bash; instale programas com `pkg install`, sem sudo; não há \
mouse, teclado nem janelas; os arquivos do celular ficam em ~/storage/shared quando o dono rodou termux-setup-storage)"
        } else {
            "Linux (o terminal é o bash)"
        };
        let pastas = if self.projeto {
            let mut t = format!(
                "Pasta do projeto: {pasta} (é o diretório atual do terminal e o destino dos caminhos relativos). São os \
ARQUIVOS DE VERDADE do dono: leia antes de mudar, mude só o necessário, prefira editar_arquivo a reescrever o arquivo \
inteiro e não apague nada sem motivo. Cada arquivo que você altera com as ferramentas ganha uma cópia, para o dono \
poder desfazer."
            );
            if !self.extras.is_empty() {
                t.push_str(" Outras pastas que fazem parte do projeto:");
                for e in &self.extras {
                    let acesso =
                        if e.escrita { "leitura e escrita" } else { "só leitura: gravar ali pede confirmação" };
                    t.push_str(&format!("\n  - {} ({acesso})", e.caminho.display()));
                }
            }
            t
        } else {
            format!(
                "Pasta desta conversa: {pasta} (é o diretório atual do terminal e o destino dos caminhos relativos). \
Guarde ali tudo o que criar."
            )
        };
        let pastas = match &self.saidas {
            Some(s) if *s != self.pasta => format!(
                "{pastas}\nPasta de ENTREGAS: {} (a Downloads do dono). Arquivos que são entregas para ele e não fazem \
parte do projeto (relatórios, PDFs, planilhas, imagens geradas, exportações, coisas baixadas para ele) vão para lá, \
gravados sem perguntar. O que é do projeto (código, páginas, configurações) fica no projeto. Na entrega final, diga \
onde cada arquivo ficou.",
                s.display()
            ),
            _ => pastas,
        };
        let perguntar = if self.pode_perguntar {
            "- perguntar {pergunta, opcoes?}: pergunta algo ao dono e espera a resposta (ele está no terminal). Use quando \
uma decisão mudaria o resultado e você não consegue descobrir sozinho (qual de dois caminhos, um dado que só ele sabe, \
apagar ou manter). Dê 2 a 4 opções curtas quando houver respostas prováveis. Não pergunte o que dá para descobrir.\n"
        } else {
            ""
        };
        let modo = match self.modo {
            Modo::Sandbox => {
                "MODO SANDBOX: o terminal roda numa caixa isolada; só a pasta da conversa pode ser alterada; \
mouse, teclado, janelas e abrir estão desligados. Faça tudo pelo terminal, arquivos e internet."
            }
            Modo::Computador => {
                "MODO COMPUTADOR: você usa o computador inteiro. Ações sensíveis pedem a confirmação do \
dono na hora (ele responde na tela); siga trabalhando depois da resposta."
            }
            Modo::Livre => {
                "MODO LIVRE: o dono liberou o computador inteiro, sem perguntar. Aja sozinho até terminar: \
use terminal, arquivos, internet, navegador, mouse e teclado. Só o que pode destruir o sistema pede confirmação."
            }
        };
        format!(
            "# Ferramentas\n{modo}\n\
Sistema: {so}. Pasta pessoal: {home}. {pastas}\n\n\
- terminal {{comando, pasta?, segundos?}}: roda um comando (padrão 120 s, máx. 900). Use para programar, compilar, \
testar, instalar dependências do projeto, git, curl, etc. Para algo que fica rodando, use `nohup ... > log.txt 2>&1 &`.\n\
- ler_arquivo {{caminho, linha_inicial?, linhas?}}: lê um arquivo de texto (com números de linha).\n\
- editar_arquivo {{caminho, trocar, por, todas?}}: troca um trecho EXATO do arquivo por outro, sem mexer no resto. \
`trocar` precisa aparecer uma vez só (inclua linhas em volta para ficar único) ou use todas=true. Copie o trecho do \
ler_arquivo sem os números de linha. É o jeito certo de mudar arquivos que já existem.\n\
- escrever_arquivo {{caminho, conteudo, anexar?}}: cria ou substitui um arquivo inteiro (anexar=true acrescenta no fim). \
Use para arquivos novos ou quando for reescrever tudo.\n\
- listar {{caminho?}}: lista uma pasta.\n\
- buscar {{nome?, texto?, pasta?}}: acha arquivos pelo nome (aceita *, ex.: \"*.php\") e/ou pelo conteúdo, nas pastas \
do projeto (ou na pasta dada), mostrando arquivo:linha. Use antes de sair abrindo arquivo por arquivo.\n\
{perguntar}\
- web {{url, metodo?, cabecalhos?, corpo?}}: acessa a internet direto (rápido, sem navegador) e devolve o texto da \
página com os links em [texto](url). Para pesquisar, prefira https://lite.duckduckgo.com/lite/?q=termos (quase nunca pede \
CAPTCHA). Se uma busca devolver um desafio anti-robô, NÃO tente burlar: troque de fonte (outro buscador, o site oficial, \
OpenStreetMap/Nominatim) ou abra a página com o `navegador`.\n\
- navegador {{url, captura?}}: lê uma página de uma vez, num navegador invisível que roda JavaScript (sites que a \
ferramenta web não mostra direito, como o Google Maps). Use-o quando a ferramenta web vier vazia ou com CAPTCHA; \
captura=true salva uma imagem da página na pasta.\n\
- navegador {{acao, ...}}: navega numa janela própria com um **cursor próprio** (vermelho) — os cliques e as teclas vão \
para a página sem mexer no mouse do dono, então ele pode usar o computador ao mesmo tempo. Ações: `ir {{url}}` abre a \
página; `ler` descreve a página e NUMERA os elementos usáveis ([1] botão \"Entrar\", [2] campo email...); \
`clicar {{n}}` clica no elemento n (prefira o número; `seletor` ou `x, y` só se faltar); `digitar {{n, texto}}` clica \
no campo n, limpa e digita; `tecla {{tecla}}` aperta Enter, Tab, Escape, setas...; `escolher {{n, opcao}}` escolhe \
num menu; `passar_mouse {{n}}` abre menus de passar o mouse; `rolar {{quantidade}}` (positivo desce); `voltar`; \
`esperar {{segundos}}`; `captura` salva uma imagem na pasta. Ciclo: ler → agir pelo número → ler de novo para ver \
o que mudou (os números mudam quando a página muda). A janela fica aberta entre as ações da tarefa.\n\
{chrome}\
- baixar {{url, caminho}}: baixa um arquivo.\n\
- abrir {{alvo}}: abre um arquivo, pasta ou site nos programas do dono, na tela dele.\n\
- janelas {{}}: lista as janelas abertas e o tamanho da tela.\n\
- focar {{janela}}: traz para a frente a janela com esse título.\n\
- mouse {{acao, x?, y?, quantidade?}}: acao = mover, clicar, duplo, direito, rolar_cima, rolar_baixo ou posicao.\n\
- teclado {{texto}} digita um texto; teclado {{teclas}} aperta atalhos, ex.: \"ctrl+l\", \"Return\", \"ctrl+s\".\n\
- tela {{}}: salva uma captura da tela na pasta.\n\
{celular}\
{conectores}\
- concluir {{texto}}: termina a tarefa com a entrega final para o dono.\n\n\
Prefira terminal, web e arquivos (mais rápidos e confiáveis). Mouse e teclado são o último recurso, para \
programas que só funcionam pela tela.",
            home = home().display(),
            celular =
                if crate::celular::disponivel() || cfg!(target_os = "android") { crate::celular::manual() } else { "" },
            conectores = self.conexoes.as_deref().map(crate::conectores::manual).unwrap_or_default(),
            chrome = match &self.chrome {
                Some(p) if self.modo != Modo::Sandbox => format!(
                    "- chrome {{{{acao, ...}}}}: usa o CHROME DO DONO pela extensão DotCrow no Chrome ({}), com as contas \
que ele já tem logadas — use quando a tarefa precisa dessas contas (e-mail, redes, sistemas da empresa) ou quando o dono \
pedir o Chrome dele. Mesmas ações do navegador (ir, ler, clicar {{{{n}}}}, digitar {{{{n, texto}}}}, tecla, escolher, \
passar_mouse, rolar, voltar, esperar, captura) e mais: `abas` lista as abas do DotCrow, `nova_aba {{{{url}}}}`, \
`fechar_aba`; aba? escolhe a aba pelo número. O corvo só mexe nas abas do grupo DotCrow. Na primeira ação da tarefa \
o dono confirma; senha e cartão sempre pedem confirmação.\n",
                    if p.conectada() {
                        "conectada agora"
                    } else {
                        "ainda desconectada: se usar, o erro explica como conectar"
                    }
                ),
                _ => String::new(),
            },
        )
    }

    fn title(&self, call: &ToolCall) -> String {
        let t = |k: &str| call.text(k).unwrap_or("").trim().to_string();
        let resumo = match call.tool.as_str() {
            "terminal" => t("comando"),
            "ler_arquivo" | "escrever_arquivo" | "editar_arquivo" | "listar" => {
                let c = t("caminho");
                if c.is_empty() { ".".into() } else { c }
            }
            "buscar" => [("nome", t("nome")), ("texto", t("texto"))]
                .into_iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, v)| format!("{k} \"{v}\""))
                .collect::<Vec<_>>()
                .join(" e "),
            "perguntar" => t("pergunta"),
            "web" => format!("{} {}", call.text("metodo").unwrap_or("GET").to_uppercase(), t("url")),
            "baixar" => t("url"),
            "navegador" | "chrome" => {
                let acao = t("acao");
                let alvo = [t("url"), call.number("n").map(|n| format!("[{n}]")).unwrap_or_default(), t("tecla")]
                    .into_iter()
                    .find(|v| !v.is_empty())
                    .unwrap_or_default();
                format!("{acao} {alvo}").trim().to_string()
            }
            "abrir" => t("alvo"),
            "focar" => t("janela"),
            "mouse" => {
                let pos = match (call.number("x"), call.number("y")) {
                    (Some(x), Some(y)) => format!(" em {x},{y}"),
                    _ => String::new(),
                };
                format!("{}{pos}", call.text("acao").unwrap_or("clicar"))
            }
            "teclado" => call.text("teclas").map(String::from).unwrap_or_else(|| format!("digitar \"{}\"", t("texto"))),
            "celular" => {
                let alvo = [t("texto"), t("app"), t("tecla"), t("direcao"), t("url")]
                    .into_iter()
                    .find(|v| !v.is_empty())
                    .or_else(|| call.number("x").zip(call.number("y")).map(|(x, y)| format!("{x},{y}")))
                    .unwrap_or_default();
                format!("{} {alvo}", call.text("acao").unwrap_or("tela")).trim().to_string()
            }
            "conector" | "api" => format!(
                "{} {} {}",
                call.text("metodo").unwrap_or("GET").to_uppercase(),
                call.text("servico").or_else(|| call.text("conector")).unwrap_or("?"),
                t("caminho")
            ),
            "conectores" => t("busca"),
            "ligar" => t("numero"),
            "sms" => format!("{}: {}", t("numero"), t("texto")),
            "falar" | "notificar" => t("texto"),
            _ => String::new(),
        };
        let resumo: String = resumo.chars().take(300).collect();
        if resumo.is_empty() { call.tool.clone() } else { format!("{}: {resumo}", call.tool) }
    }

    async fn run(&self, call: &ToolCall) -> ToolOutput {
        let titulo = self.title(call);
        match call.tool.as_str() {
            "terminal" | "comando" | "shell" => self.terminal(call, &titulo).await,
            "ler_arquivo" | "ler" => self.ler(call, &titulo).await,
            "escrever_arquivo" | "escrever" => self.escrever(call, &titulo).await,
            "editar_arquivo" | "editar" => self.editar(call, &titulo).await,
            "buscar" | "procurar" => self.buscar(call).await,
            "perguntar" => self.perguntar(call).await,
            "listar" => self.listar(call).await,
            "web" | "curl" => self.web(call, &titulo).await,
            "navegador" => self.navegador(call).await,
            "chrome" => self.chrome(call).await,
            "baixar" => self.baixar(call, &titulo).await,
            "abrir" => self.abrir(call).await,
            "mouse" | "teclado" | "janelas" | "focar" | "tela" => self.controle(call, &titulo).await,
            t if crate::celular::FERRAMENTAS.contains(&t) => self.celular(call).await,
            "conector" | "api" => self.conector(call, &titulo).await,
            "conectores" => match &self.conexoes {
                Some(cx) => ToolOutput::ok(crate::conectores::listar_para_corvo(cx, call.text("busca").unwrap_or(""))),
                None => ToolOutput::err("os conectores não estão ligados nesta tarefa"),
            },
            other => ToolOutput::err(format!(
                "ferramenta \"{other}\" não existe. Use: terminal, ler_arquivo, editar_arquivo, escrever_arquivo, \
listar, buscar, perguntar, web, navegador, chrome, baixar, abrir, janelas, focar, mouse, teclado, tela, celular, ligar, sms, \
falar, ouvir, notificar, conectores, conector ou concluir."
            )),
        }
    }
}

fn home() -> PathBuf {
    directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
}

/// A pasta Downloads do dono (a localizada, quando o sistema diz qual é). No Termux,
/// a pasta Download do celular (`~/storage/downloads`), quando o dono já rodou
/// `termux-setup-storage`.
pub fn pasta_de_downloads() -> PathBuf {
    // Descoberta uma vez só: com a trava da Civilização, o arquivo que diz o nome da
    // pasta (Downloads, Transferências...) deixa de ser legível.
    static PASTA: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    PASTA
        .get_or_init(|| {
            if cfg!(target_os = "android") || std::env::var_os("TERMUX_VERSION").is_some() {
                let celular = home().join("storage").join("downloads");
                if celular.is_dir() {
                    return celular;
                }
            }
            directories::UserDirs::new()
                .and_then(|d| d.download_dir().map(Path::to_path_buf))
                .unwrap_or_else(|| home().join("Downloads"))
        })
        .clone()
}

/// Pasta onde ficam as conversas: `~/DotCrow/conversas`.
pub fn pasta_das_conversas() -> PathBuf {
    home().join("DotCrow").join("conversas")
}

/// Cria a pasta de uma conversa nova, com data e um nome tirado do pedido.
pub fn nova_pasta(pedido: &str) -> std::io::Result<PathBuf> {
    nova_pasta_em(&pasta_das_conversas(), pedido)
}

/// Igual a [`nova_pasta`], dentro de `raiz`.
pub fn nova_pasta_em(raiz: &Path, pedido: &str) -> std::io::Result<PathBuf> {
    let raiz = raiz.to_path_buf();
    std::fs::create_dir_all(&raiz)?;
    let dia = hoje();
    let slug: String = {
        let s: String = pedido
            .chars()
            .map(|c| if c.is_alphanumeric() { c.to_lowercase().next().unwrap_or(c) } else { '-' })
            .collect();
        let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
        s.chars().take(48).collect::<String>().trim_end_matches('-').to_string()
    };
    let slug = if slug.is_empty() { "conversa".to_string() } else { slug };
    for n in 1..10_000 {
        let p = raiz.join(format!("{dia}_{n:03}_{slug}"));
        if !p.exists()
            && !raiz
                .read_dir()?
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with(&format!("{dia}_{n:03}_")))
        {
            std::fs::create_dir_all(&p)?;
            return Ok(p);
        }
    }
    Err(std::io::Error::other("pastas demais para hoje"))
}

/// Data de hoje (UTC) como AAAA-MM-DD.
fn hoje() -> String {
    let dias =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0)
            as i64;
    // Algoritmo civil-from-days (Howard Hinnant).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Tira `.` e `..` do caminho sem tocar no disco.
pub fn normalizar(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn caminho_sensivel(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    const PEDACOS: [&str; 14] = [
        "/.ssh",
        "\\.ssh",
        "/.gnupg",
        ".password-store",
        "keyring",
        "segredos",
        "/.aws",
        "/.kube",
        "/.docker/config",
        "/.netrc",
        "/.git-credentials",
        "login data",
        "/cookies",
        "/.config/gh",
    ];
    const FINAIS: [&str; 8] = [".pem", ".p8", ".p12", ".pfx", ".key", ".kdbx", ".keystore", ".jks"];
    PEDACOS.iter().any(|x| s.contains(x)) || FINAIS.iter().any(|x| s.ends_with(x)) || s.ends_with(".env")
}

/// Comandos que destroem o sistema ou um disco: nunca rodam, em modo nenhum.
fn perigo_extremo(cmd: &str) -> bool {
    let c = cmd.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
    [
        "rm -rf /",
        "rm -rf ~",
        "rm -rf /*",
        "rm -fr /",
        "mkfs",
        "of=/dev/",
        "> /dev/sd",
        ":(){",
        "chmod -r 777 /",
        "format c:",
        "dd if=/dev/zero of=/dev/",
        "wipefs",
        "> /dev/nvme",
        "> /dev/vd",
        "shred /dev/",
    ]
    .iter()
    .any(|x| c.contains(x))
        && !c.contains("rm -rf ./")
}

/// Classifica um comando de terminal: `None` = roda direto.
fn classificar_comando(cmd: &str) -> Option<Categoria> {
    let c = format!(" {} ", cmd.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" "));
    let tem = |xs: &[&str]| xs.iter().any(|x| c.contains(x));
    if tem(&[
        ".ssh/",
        "id_rsa",
        "id_ed25519",
        "segredos",
        ".gnupg",
        "password-store",
        ".p12",
        ".p8 ",
        ".pem",
        "keystore",
        "git-credentials",
        "keyring",
    ]) {
        return Some(Categoria::Segredos);
    }
    if tem(&[
        " sudo ",
        " su ",
        " pkexec ",
        " doas ",
        " dpkg -i",
        " dpkg -r",
        " flatpak install",
        " flatpak uninstall",
        " shutdown",
        " reboot",
        " poweroff",
        " halt ",
        " init 0",
        " fdisk",
        " parted",
        " mount ",
        " umount ",
        " crontab ",
        " useradd",
        " userdel",
        " passwd",
        " visudo",
        " ufw ",
        " iptables",
        " chown ",
        " modprobe",
        " update-grub",
        " set-executionpolicy",
        " reg add",
        " reg delete",
        " sc stop",
        " sc delete",
        " winget ",
        " choco ",
    ]) || muda_pacote_ou_servico(&c)
        || tem(&["/etc/", "/usr/", "/boot/", "/var/lib/", "c:\\windows"])
            && tem(&[" > ", " >> ", " tee ", " rm ", " mv ", " cp ", " sed -i"])
    {
        return Some(Categoria::Sistema);
    }
    if tem(&[
        " git push",
        " npm publish",
        " cargo publish",
        " gh release",
        " gh pr merge",
        " gh repo delete",
        " twine upload",
        " scp ",
        " rsync ",
        " ssh ",
        " sftp ",
        " ftp ",
        " docker push",
        " mail ",
        " sendmail",
        " -x post",
        " -x put",
        " -x delete",
        " --data",
        " -d ",
        " -f ",
        " --form",
        " --upload-file",
        " -t ",
        " wget --post",
    ]) && !tem(&[" rsync -n", " git push --dry-run"])
    {
        // `-d`, `-F` e `-T` só contam como envio quando o comando é curl.
        let curl_flags = tem(&[" -d ", " -f ", " -t "]);
        if !curl_flags || c.contains(" curl ") {
            return Some(Categoria::Publicar);
        }
    }
    if tem(&["| sh ", "| bash ", "|sh ", "|bash ", "| sudo ", "iex(", "invoke-expression"]) {
        return Some(Categoria::Sistema);
    }
    if tem(&[
        " rm ",
        " rmdir ",
        " shred ",
        " del ",
        " remove-item",
        " rd ",
        " git clean",
        " git reset --hard",
        " truncate ",
        " -delete",
    ]) {
        // Apagar dentro da pasta da conversa (caminhos relativos simples) roda direto.
        let fora = tem(&[" /", " ~", "..", " $home", " c:\\", "*"]);
        if fora {
            return Some(Categoria::Apagar);
        }
    }
    None
}

/// Gerenciador de pacotes ou de serviços com um verbo que muda algo (consultar roda direto).
fn muda_pacote_ou_servico(c: &str) -> bool {
    const PROGRAMAS: [&str; 9] = ["apt", "apt-get", "dnf", "yum", "zypper", "snap", "brew", "systemctl", "service"];
    const VERBOS: [&str; 16] = [
        "install",
        "remove",
        "purge",
        "upgrade",
        "dist-upgrade",
        "full-upgrade",
        "autoremove",
        "erase",
        "in",
        "rm",
        "start",
        "stop",
        "restart",
        "enable",
        "disable",
        "mask",
    ];
    let palavras: Vec<&str> =
        c.split(|ch: char| ch.is_whitespace() || matches!(ch, ';' | '&' | '|')).filter(|w| !w.is_empty()).collect();
    palavras.windows(2).any(|w| PROGRAMAS.contains(&w[0]) && VERBOS.contains(&w[1]))
        || palavras.windows(3).any(|w| PROGRAMAS.contains(&w[0]) && w[1].starts_with('-') && VERBOS.contains(&w[2]))
        || palavras
            .windows(2)
            .any(|w| w[0] == "pacman" && (w[1].starts_with("-S") || w[1].starts_with("-R") || w[1].starts_with("-U")))
}

/// Caminho real (seguindo links), mesmo que o arquivo ainda não exista: resolve o
/// ancestral mais próximo que existe e junta o resto.
fn real(p: &Path) -> PathBuf {
    let mut atual = p.to_path_buf();
    let mut resto = Vec::new();
    loop {
        if let Ok(r) = std::fs::canonicalize(&atual) {
            return resto.iter().rev().fold(r, |acc, c| acc.join(c));
        }
        match (atual.file_name().map(|n| n.to_os_string()), atual.parent()) {
            (Some(n), Some(pai)) => {
                resto.push(n);
                atual = pai.to_path_buf();
            }
            _ => return p.to_path_buf(),
        }
    }
}

/// Comando que apaga ou descarta arquivos (no modo projeto, isso pede confirmação).
fn apaga_algo(cmd: &str) -> bool {
    let c = format!(" {} ", cmd.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" "));
    let c = c.replace([';', '&', '|', '(', ')'], " ");
    [
        " rm ",
        " rmdir ",
        " unlink ",
        " shred ",
        " truncate ",
        " -delete",
        " git clean",
        " git reset --hard",
        " git checkout -- ",
        " git checkout . ",
        " git restore",
        " git stash drop",
        " del ",
        " erase ",
        " rd ",
        " remove-item",
    ]
    .iter()
    .any(|x| c.contains(x))
}

/// Troca `trocar` por `por` em `texto`. Sem `todas`, o trecho precisa aparecer uma vez só.
/// Tolera arquivo com quebra de linha do Windows (CRLF) quando o trecho veio com LF.
fn trocar_trecho(texto: &str, trocar: &str, por: &str, todas: bool) -> Result<(String, usize), String> {
    let mut alvo = trocar.to_string();
    let mut subst = por.to_string();
    let mut n = texto.matches(&alvo).count();
    if n == 0 && texto.contains("\r\n") && !trocar.contains("\r\n") {
        alvo = trocar.replace('\n', "\r\n");
        subst = por.replace('\n', "\r\n");
        n = texto.matches(&alvo).count();
    }
    match n {
        0 => Err(
            "o trecho de `trocar` não foi encontrado. Leia o arquivo de novo (ler_arquivo) e copie o trecho exato, \
com os mesmos espaços e quebras de linha, sem os números de linha."
                .into(),
        ),
        n if n > 1 && !todas => Err(format!(
            "o trecho aparece {n} vezes. Inclua linhas vizinhas em `trocar` para ficar único, ou use todas=true."
        )),
        n if todas => Ok((texto.replace(&alvo, &subst), n)),
        _ => Ok((texto.replacen(&alvo, &subst, 1), 1)),
    }
}

fn shell(cmd: &str) -> tokio::process::Command {
    if cfg!(target_os = "windows") {
        let mut c = tokio::process::Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", cmd]);
        c
    } else {
        let mut c = tokio::process::Command::new(achar_shell());
        c.args(["-c", cmd]);
        // Com a trava da Civilização, o /tmp fica de fora: a temporária é a da sandbox.
        if crate::trava::ativa() {
            c.env("TMPDIR", crate::trava::temporaria());
        }
        c
    }
}

/// O bash, onde estiver. No Termux (Android) não existe /bin: o bash fica em
/// $PREFIX/bin e o sh do sistema em /system/bin.
pub fn achar_shell() -> PathBuf {
    if Path::new("/bin/bash").exists() {
        return PathBuf::from("/bin/bash");
    }
    for nome in ["bash", "sh"] {
        if let Some(p) = no_path(nome) {
            return p;
        }
    }
    ["/bin/sh", "/system/bin/sh"].into_iter().map(PathBuf::from).find(|p| p.exists()).unwrap_or_else(|| "sh".into())
}

/// Procura um programa nas pastas do PATH.
fn no_path(nome: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(nome)).find(|c| c.is_file()))
}

/// Roda um programa com tempo-limite e junta a saída.
async fn rodar(mut c: tokio::process::Command, secs: u64) -> ToolOutput {
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => return ToolOutput::err(format!("não consegui rodar: {e}")),
    };
    match tokio::time::timeout(Duration::from_secs(secs), child.wait_with_output()).await {
        Ok(Ok(out)) => {
            let mut text = String::from_utf8_lossy(&out.stdout).to_string();
            let err = String::from_utf8_lossy(&out.stderr);
            if !err.trim().is_empty() {
                text.push_str("\n--- erros ---\n");
                text.push_str(&err);
            }
            let code = out.status.code().map(|c| c.to_string()).unwrap_or_else(|| "sinal".into());
            if text.trim().is_empty() {
                text = "(sem saída)".into();
            }
            let text = format!("{}\n[código de saída: {code}]", clip(text.trim_end(), MAX_OUT));
            ToolOutput { ok: out.status.success(), text }
        }
        Ok(Err(e)) => ToolOutput::err(format!("falha ao esperar o programa: {e}")),
        Err(_) => ToolOutput::err(format!("passou de {secs} s e foi interrompido")),
    }
}

/// Chrome, Chromium, Edge ou Brave, o que existir.
pub fn chrome() -> Option<PathBuf> {
    let nomes: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ]
    } else {
        &[
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "brave-browser",
            "microsoft-edge",
            "microsoft-edge-stable",
        ]
    };
    nomes.iter().find_map(|n| {
        let p = Path::new(n);
        if p.is_absolute() {
            p.exists().then(|| p.to_path_buf())
        } else {
            std::env::var_os("PATH")
                .and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(n)).find(|c| c.is_file()))
        }
    })
}

/// Abre um arquivo, pasta ou endereço no programa padrão do sistema.
pub fn abrir_no_sistema(alvo: &str) -> std::io::Result<()> {
    let mut c = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", alvo]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(alvo);
        c
    } else if cfg!(target_os = "android") || no_path("termux-open").is_some() && no_path("xdg-open").is_none() {
        // Termux: abre no app certo do celular (navegador, visualizador de PDF...).
        let prog = if alvo.contains("://") { "termux-open-url" } else { "termux-open" };
        let mut c = std::process::Command::new(prog);
        c.arg(alvo);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(alvo);
        c
    };
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|_| ())
}

fn tamanho(b: u64) -> String {
    match b {
        0..1024 => format!("{b} B"),
        1024..1_048_576 => format!("{:.1} KB", b as f64 / 1024.0),
        _ => format!("{:.1} MB", b as f64 / 1_048_576.0),
    }
}

/// HTML → texto legível, com os links como `[texto](url)`.
pub fn html_para_texto(html: &str, base: &str) -> String {
    let mut out = String::with_capacity(html.len() / 3);
    let mut titulo = String::new();
    let mut i = 0;
    let b = html.as_bytes();
    let mut pular: Option<&str> = None;
    let mut link: Option<(String, usize)> = None;
    let mut no_titulo = false;
    while i < b.len() {
        if b[i] == b'<' {
            let Some(fim) = html[i..].find('>') else { break };
            let tag = &html[i + 1..i + fim];
            i += fim + 1;
            let nome: String = tag
                .trim_start_matches('/')
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            let fecha = tag.starts_with('/');
            if let Some(p) = pular {
                if fecha && nome == p {
                    pular = None;
                }
                continue;
            }
            match nome.as_str() {
                "script" | "style" | "noscript" | "svg" | "template" | "iframe" if !fecha && !tag.ends_with('/') => {
                    pular = Some(match nome.as_str() {
                        "script" => "script",
                        "style" => "style",
                        "noscript" => "noscript",
                        "svg" => "svg",
                        "template" => "template",
                        _ => "iframe",
                    })
                }
                "title" => no_titulo = !fecha,
                "br" | "p" | "div" | "li" | "tr" | "section" | "article" | "header" | "footer" | "ul" | "ol"
                | "table" | "form" | "nav" | "main" | "aside" => out.push('\n'),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => out.push_str(if fecha { "\n" } else { "\n\n## " }),
                "td" | "th" => out.push_str(" | "),
                "input" | "textarea" | "select" | "button" if !fecha => {
                    let campo: Vec<String> = ["name", "type", "placeholder", "value", "aria-label"]
                        .iter()
                        .filter_map(|a| atributo(tag, a).map(|v| format!("{a}={v}")))
                        .collect();
                    if !campo.is_empty() {
                        out.push_str(&format!(" [{nome} {}] ", campo.join(" ")));
                    }
                }
                "a" if !fecha => {
                    if let Some(h) = atributo(tag, "href") {
                        link = Some((absoluto(base, &h), out.len()));
                    }
                }
                "a" => {
                    if let Some((href, ini)) = link.take() {
                        let texto = out[ini..].trim().to_string();
                        if !href.starts_with("javascript:") && !href.starts_with('#') {
                            out.truncate(ini);
                            out.push_str(&format!(" [{}]({href}) ", if texto.is_empty() { "link" } else { &texto }));
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let prox = html[i..].find('<').map(|p| i + p).unwrap_or(b.len());
        if pular.is_none() {
            let t = entidades(&html[i..prox]);
            if no_titulo {
                titulo.push_str(&t);
            } else {
                out.push_str(&t);
            }
        }
        i = prox;
    }
    let mut limpo = String::new();
    let mut vazias = 0;
    for l in out.lines() {
        let l = l.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() || l == "|" {
            vazias += 1;
            if vazias == 1 && !limpo.is_empty() {
                limpo.push('\n');
            }
        } else {
            vazias = 0;
            limpo.push_str(&l);
            limpo.push('\n');
        }
    }
    let titulo = titulo.split_whitespace().collect::<Vec<_>>().join(" ");
    if titulo.is_empty() { limpo } else { format!("# {titulo}\n\n{limpo}") }
}

fn atributo(tag: &str, nome: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let mut from = 0;
    while let Some(p) = lower[from..].find(nome) {
        let at = from + p;
        from = at + nome.len();
        let antes_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let resto = lower[from..].trim_start();
        if !antes_ok || !resto.starts_with('=') {
            continue;
        }
        let ini = tag.len() - resto.len() + 1;
        let valor = tag[ini..].trim_start();
        let v = if let Some(r) = valor.strip_prefix('"') {
            r.split('"').next()
        } else if let Some(r) = valor.strip_prefix('\'') {
            r.split('\'').next()
        } else {
            valor.split_whitespace().next()
        };
        return v.map(|s| entidades(s.trim_end_matches('/')));
    }
    None
}

fn absoluto(base: &str, href: &str) -> String {
    let h = href.trim();
    if h.contains("://") || h.starts_with("mailto:") || h.starts_with("javascript:") || h.starts_with('#') {
        return h.to_string();
    }
    let (esquema, resto) = base.split_once("://").unwrap_or(("https", base));
    let host = resto.split('/').next().unwrap_or("");
    if let Some(r) = h.strip_prefix("//") {
        return format!("{esquema}://{r}");
    }
    if h.starts_with('/') {
        return format!("{esquema}://{host}{h}");
    }
    let caminho = resto.split(['?', '#']).next().unwrap_or("");
    let dir = caminho.rsplit_once('/').map(|(d, _)| d).unwrap_or(host);
    format!("{esquema}://{dir}/{h}")
}

fn entidades(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let Some(fim) = rest.bytes().take(12).position(|b| b == b';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..fim];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32)
            }
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[fim + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Responde(Resposta, Mutex<usize>);
    #[async_trait::async_trait]
    impl Aprovador for Responde {
        async fn pedir(&self, _n: usize, _t: &str, _m: &str) -> Resposta {
            *self.1.lock().unwrap() += 1;
            self.0
        }
    }

    fn maos(r: Resposta) -> (Maos, Arc<Responde>, PathBuf) {
        let base =
            std::env::temp_dir().join(format!("dotcrow-maos-{}-{:?}", std::process::id(), std::thread::current().id()));
        let pasta = base.join("conversa");
        std::fs::create_dir_all(&pasta).unwrap();
        let ap = Arc::new(Responde(r, Mutex::new(0)));
        (Maos::new(Modo::Computador, pasta.clone(), base.join("dados"), ap.clone(), Liberados::default()), ap, pasta)
    }

    fn call(tool: &str, args: serde_json::Value) -> ToolCall {
        ToolCall { n: 1, tool: tool.into(), args, why: String::new() }
    }

    #[tokio::test]
    async fn ligar_sms_e_publicar_perguntam_sempre_mesmo_no_modo_livre() {
        let (m, ap, _) = maos(Resposta::Sempre);
        let m = Maos { modo: Modo::Livre, ..m };
        let c = call("ligar", serde_json::json!({"numero": "190"}));
        // Livre não pergunta o que é comum…
        assert!(m.autorizar(&c, "tocar", Categoria::Controle, false).await);
        assert_eq!(*ap.1.lock().unwrap(), 0);
        // …mas ligar/SMS/publicar perguntam sempre, e "Sempre" não libera para a próxima.
        assert!(m.autorizar(&c, "ligar para 190", Categoria::Mensagem, false).await);
        assert!(m.autorizar(&c, "ligar para 190", Categoria::Mensagem, false).await);
        assert_eq!(*ap.1.lock().unwrap(), 2);
        // O microfone pergunta no Livre; depois de "Sempre", fica liberado na conversa.
        assert!(m.autorizar(&c, "ouvir", Categoria::Microfone, false).await);
        assert!(m.autorizar(&c, "ouvir", Categoria::Microfone, false).await);
        assert_eq!(*ap.1.lock().unwrap(), 3);

        // Dono diz não: a ligação não sai.
        let (m, ap, _) = maos(Resposta::Nao);
        let m = Maos { modo: Modo::Livre, ..m };
        let r = m.run(&call("ligar", serde_json::json!({"numero": "190"}))).await;
        assert!(!r.ok && r.text.contains("não autorizou"), "{}", r.text);
        let r = m.run(&call("sms", serde_json::json!({"numero": "190", "texto": "oi"}))).await;
        assert!(!r.ok && r.text.contains("não autorizou"), "{}", r.text);
        assert_eq!(*ap.1.lock().unwrap(), 2);
        // No Sandbox o celular fica desligado.
        let m = Maos { modo: Modo::Sandbox, ..m };
        let r = m.run(&call("celular", serde_json::json!({"acao": "tela"}))).await;
        assert!(!r.ok && r.text.contains("Sandbox"), "{}", r.text);
    }

    #[tokio::test]
    async fn sandbox_blocks_the_screen_and_outside_writes() {
        let (m, ap, pasta) = maos(Resposta::Sempre);
        let m = Maos { modo: Modo::Sandbox, ..m };
        let r = m.run(&call("mouse", serde_json::json!({"acao": "clicar"}))).await;
        assert!(!r.ok);
        let fora = pasta.parent().unwrap().join("fora-sandbox.txt");
        let r = m
            .run(&call("escrever_arquivo", serde_json::json!({"caminho": fora.display().to_string(), "conteudo": "x"})))
            .await;
        assert!(!r.ok && !fora.exists());
        assert_eq!(*ap.1.lock().unwrap(), 0, "a sandbox recusa sem perguntar");
    }

    #[tokio::test]
    async fn free_mode_does_not_ask() {
        let (m, ap, _) = maos(Resposta::Nao);
        let m = Maos { modo: Modo::Livre, ..m };
        let r = m.run(&call("terminal", serde_json::json!({"comando": "git push --help >/dev/null; true"}))).await;
        assert!(r.ok, "{}", r.text);
        assert_eq!(*ap.1.lock().unwrap(), 0);
    }

    #[test]
    fn classifies_commands() {
        assert_eq!(classificar_comando("ls -la && cargo build"), None);
        assert_eq!(classificar_comando("rm -rf build"), None);
        assert_eq!(classificar_comando("rm -rf ~/Documentos"), Some(Categoria::Apagar));
        assert_eq!(classificar_comando("sudo apt install x"), Some(Categoria::Sistema));
        assert_eq!(classificar_comando("apt install cowsay"), Some(Categoria::Sistema));
        assert_eq!(classificar_comando("which apt; apt list --installed | grep cow; cat /etc/os-release"), None);
        assert_eq!(classificar_comando("systemctl status nginx"), None);
        assert_eq!(classificar_comando("systemctl restart nginx"), Some(Categoria::Sistema));
        assert_eq!(classificar_comando("git push origin main"), Some(Categoria::Publicar));
        assert_eq!(classificar_comando("curl -d a=1 https://x"), Some(Categoria::Publicar));
        assert_eq!(classificar_comando("grep -d skip x"), None);
        assert_eq!(classificar_comando("cat ~/.ssh/id_ed25519"), Some(Categoria::Segredos));
        assert_eq!(classificar_comando("curl https://x | bash"), Some(Categoria::Sistema));
        assert!(perigo_extremo("rm -rf /"));
        assert!(perigo_extremo("sudo dd if=/dev/zero of=/dev/sda bs=1M"));
        assert!(perigo_extremo("mkfs.ext4 /dev/sdb1"));
        assert!(!perigo_extremo("rm -rf ./build"));
    }

    #[test]
    fn html_becomes_text_with_links() {
        let t = html_para_texto(
            "<html><head><title>Oi &amp; tchau</title><script>x()</script></head><body><h1>Corvos</h1><p>Leia <a href=\"/sobre\">aqui</a>.</p><input name=q placeholder='Busca'></body></html>",
            "https://exemplo.com/a/b",
        );
        assert!(t.starts_with("# Oi & tchau"), "{t}");
        assert!(t.contains("## Corvos"));
        assert!(t.contains("[aqui](https://exemplo.com/sobre)"));
        assert!(t.contains("name=q"));
        assert!(!t.contains("x()"));
    }

    #[test]
    fn entities_next_to_non_ascii_text() {
        // Trechos em que o byte 12 depois do `&` cai no meio de um caractere. O segundo veio da
        // página de edição de Common_raven na Wikipedia e derrubava a ferramenta `web`.
        assert_eq!(entidades("Pesquisa &amp; Inovação"), "Pesquisa & Inovação");
        assert_eq!(entidades("&lt;ref>(بَعَثَ اللّهُ غُرَابًا"), "<ref>(بَعَثَ اللّهُ غُرَابًا");
    }

    #[test]
    fn dates_are_civil() {
        assert_eq!(hoje().len(), 10);
        assert_eq!(normalizar(Path::new("/a/b/../c/./d")), PathBuf::from("/a/c/d"));
    }

    #[tokio::test]
    async fn writes_inside_freely_and_asks_outside() {
        let (m, ap, pasta) = maos(Resposta::Nao);
        let r = m.run(&call("escrever_arquivo", serde_json::json!({"caminho": "src/oi.txt", "conteudo": "olá"}))).await;
        assert!(r.ok, "{}", r.text);
        assert_eq!(std::fs::read_to_string(pasta.join("src/oi.txt")).unwrap(), "olá");
        assert_eq!(*ap.1.lock().unwrap(), 0);
        let fora = pasta.parent().unwrap().join("fora.txt");
        let r = m
            .run(&call("escrever_arquivo", serde_json::json!({"caminho": fora.display().to_string(), "conteudo": "x"})))
            .await;
        assert!(!r.ok);
        assert!(!fora.exists());
        assert_eq!(*ap.1.lock().unwrap(), 1);
        let r = m.run(&call("escrever_arquivo", serde_json::json!({"caminho": "../dados/x", "conteudo": "x"}))).await;
        assert!(!r.ok, "o cofre nunca é alterado");
    }

    #[tokio::test]
    async fn runs_commands_in_the_conversation_folder() {
        let (m, ap, _) = maos(Resposta::Sempre);
        let cmd = if cfg!(windows) { "Write-Output corvo" } else { "echo corvo && pwd" };
        let r = m.run(&call("terminal", serde_json::json!({"comando": cmd}))).await;
        assert!(r.ok, "{}", r.text);
        assert!(r.text.contains("corvo"));
        assert!(r.text.contains("conversa") || cfg!(windows));
        // "Sempre" libera a categoria na conversa: a segunda vez não pergunta.
        let _ = m.run(&call("terminal", serde_json::json!({"comando": "git push --help >/dev/null; true"}))).await;
        let _ = m.run(&call("terminal", serde_json::json!({"comando": "git push --help >/dev/null; true"}))).await;
        assert_eq!(*ap.1.lock().unwrap(), 1);
    }

    /// Aprovador que também responde perguntas abertas.
    struct Conversa(Resposta, Option<String>, Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl Aprovador for Conversa {
        async fn pedir(&self, _n: usize, t: &str, _m: &str) -> Resposta {
            self.2.lock().unwrap().push(format!("pedir: {t}"));
            self.0
        }
        async fn perguntar(&self, pergunta: &str, opcoes: &[String]) -> Option<String> {
            self.2.lock().unwrap().push(format!("perguntar: {pergunta} {opcoes:?}"));
            self.1.clone()
        }
    }

    fn projeto(nome: &str) -> PathBuf {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let base = std::env::temp_dir().join(format!("dotcrow-proj-maos-{nome}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(base.join("site/admin")).unwrap();
        std::fs::create_dir_all(base.join("assets")).unwrap();
        std::fs::write(base.join("site/notas.php"), "<?php\n$titulo = 'Notas';\necho $titulo;\n").unwrap();
        std::fs::write(base.join("site/admin/painel.php"), "<?php // painel\n").unwrap();
        std::fs::write(base.join("assets/logo.svg"), "<svg/>").unwrap();
        base
    }

    #[test]
    fn troca_trecho_exato() {
        let t = "a\nb\nc\nb\n";
        assert_eq!(trocar_trecho(t, "c", "C", false).unwrap(), ("a\nb\nC\nb\n".into(), 1));
        assert!(trocar_trecho(t, "b", "B", false).unwrap_err().contains("2 vezes"));
        assert_eq!(trocar_trecho(t, "b", "B", true).unwrap(), ("a\nB\nc\nB\n".into(), 2));
        assert!(trocar_trecho(t, "z", "Z", false).is_err());
        // Arquivo do Windows (CRLF) com trecho vindo com LF.
        assert_eq!(trocar_trecho("x\r\ny\r\n", "x\ny", "x\nz", false).unwrap().0, "x\r\nz\r\n");
    }

    #[tokio::test]
    async fn modo_projeto_edita_anota_e_pergunta_antes_de_apagar() {
        let base = projeto("edita");
        let site = base.join("site");
        let ap = Arc::new(Conversa(Resposta::Nao, None, Mutex::new(Vec::new())));
        let diario = Arc::new(crate::desfazer::Diario::novo(&base.join("dados/desfazer"), "teste").unwrap());
        let m = Maos::new(Modo::Computador, site.clone(), base.join("dados"), ap.clone(), Liberados::default())
            .com_projeto(vec![PastaExtra { caminho: base.join("assets"), escrita: false }], Some(diario.clone()));

        // Editar no lugar: só o trecho muda, e o original vai para o diário.
        let r = m
            .run(&call(
                "editar_arquivo",
                serde_json::json!({"caminho": "notas.php", "trocar": "'Notas'", "por": "'Minhas notas'"}),
            ))
            .await;
        assert!(r.ok, "{}", r.text);
        assert!(r.text.contains("linha 2"), "{}", r.text);
        assert_eq!(
            std::fs::read_to_string(site.join("notas.php")).unwrap(),
            "<?php\n$titulo = 'Minhas notas';\necho $titulo;\n"
        );
        assert_eq!(diario.quantos(), 1);

        // Apagar arquivo do projeto pede confirmação (e o dono disse não).
        let r = m.run(&call("terminal", serde_json::json!({"comando": "rm notas.php"}))).await;
        assert!(!r.ok);
        assert!(site.join("notas.php").exists());
        assert!(ap.2.lock().unwrap().iter().any(|l| l.contains("rm notas.php")));

        // Pasta extra só leitura: gravar pergunta (e foi negado); pasta principal grava direto.
        let r = m
            .run(&call(
                "escrever_arquivo",
                serde_json::json!({"caminho": base.join("assets/novo.txt").display().to_string(), "conteudo": "x"}),
            ))
            .await;
        assert!(!r.ok && !base.join("assets/novo.txt").exists());
        let r = m
            .run(&call("escrever_arquivo", serde_json::json!({"caminho": "admin/novo.php", "conteudo": "<?php"})))
            .await;
        assert!(r.ok, "{}", r.text);
        assert_eq!(diario.quantos(), 2);

        // O manual conta aos corvos que são arquivos de verdade e quais pastas participam.
        let manual = m.manual();
        assert!(manual.contains("ARQUIVOS DE VERDADE"));
        assert!(manual.contains("assets (só leitura"));
        assert!(manual.contains("editar_arquivo"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[tokio::test]
    async fn busca_nas_pastas_do_projeto() {
        let base = projeto("busca");
        let ap = Arc::new(Conversa(Resposta::Sim, None, Mutex::new(Vec::new())));
        let m = Maos::new(Modo::Computador, base.join("site"), base.join("dados"), ap, Liberados::default())
            .com_projeto(vec![PastaExtra { caminho: base.join("assets"), escrita: true }], None);
        let r = m.run(&call("buscar", serde_json::json!({"texto": "titulo"}))).await;
        assert!(r.ok && r.text.contains("notas.php:2:"), "{}", r.text);
        let r = m.run(&call("buscar", serde_json::json!({"nome": "*.svg"}))).await;
        assert!(r.ok && r.text.contains("logo.svg"), "{}", r.text);
        let r = m.run(&call("buscar", serde_json::json!({}))).await;
        assert!(!r.ok);
        let _ = std::fs::remove_dir_all(base);
    }

    #[tokio::test]
    async fn perguntar_so_quando_tem_alguem() {
        let base = projeto("pergunta");
        let ap = Arc::new(Conversa(Resposta::Sim, Some("só o notas.php".into()), Mutex::new(Vec::new())));
        let pedido = serde_json::json!({"pergunta": "Mexo também no admin?", "opcoes": ["Só o notas.php", "Os dois"]});
        let sem = Maos::new(Modo::Computador, base.join("site"), base.join("dados"), ap.clone(), Liberados::default());
        let r = sem.run(&call("perguntar", pedido.clone())).await;
        assert!(r.ok && r.text.contains("não pode responder"), "{}", r.text);
        assert!(!sem.manual().contains("- perguntar"));

        let com = Maos::new(Modo::Computador, base.join("site"), base.join("dados"), ap.clone(), Liberados::default())
            .com_perguntas();
        let r = com.run(&call("perguntar", pedido)).await;
        assert_eq!(r.text, "O dono respondeu: só o notas.php");
        assert!(ap.2.lock().unwrap().iter().any(|l| l.contains("Mexo também no admin?")));
        assert!(com.manual().contains("- perguntar"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn link_para_a_pasta_conta_como_dentro() {
        let base = projeto("link");
        let link = base.join("atalho");
        std::os::unix::fs::symlink(base.join("site"), &link).unwrap();
        let ap = Arc::new(Conversa(Resposta::Nao, None, Mutex::new(Vec::new())));
        let m = Maos::new(Modo::Computador, base.join("site"), base.join("dados"), ap, Liberados::default());
        assert!(m.dentro_da_pasta(&link.join("novo.php")));
        assert!(m.dentro_da_pasta(&link.join("admin/sub/novo.php")));
        assert!(!m.dentro_da_pasta(&base.join("assets/x")));
        let _ = std::fs::remove_dir_all(base);
    }

    #[tokio::test]
    async fn entregas_vao_para_downloads_sem_perguntar() {
        let base = projeto("entregas");
        let downloads = base.join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let ap = Arc::new(Conversa(Resposta::Nao, None, Mutex::new(Vec::new())));
        let m = Maos::new(Modo::Computador, base.join("site"), base.join("dados"), ap.clone(), Liberados::default())
            .com_projeto(Vec::new(), None)
            .com_saidas(downloads.clone());
        let alvo = downloads.join("relatorio.md");
        let r = m
            .run(&call(
                "escrever_arquivo",
                serde_json::json!({"caminho": alvo.display().to_string(), "conteudo": "# Relatório"}),
            ))
            .await;
        assert!(r.ok && alvo.exists(), "{}", r.text);
        assert!(ap.2.lock().unwrap().is_empty(), "gravar na Downloads não pergunta");
        assert!(m.manual().contains("Pasta de ENTREGAS"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn acha_um_shell() {
        assert!(achar_shell().exists() || achar_shell() == Path::new("sh"));
        assert!(apaga_algo("rm -f notas.php"));
        assert!(apaga_algo("cd x && git reset --hard"));
        assert!(!apaga_algo("ls -la; cat notas.php"));
        assert!(!apaga_algo("php -l notas.php"));
    }
}
