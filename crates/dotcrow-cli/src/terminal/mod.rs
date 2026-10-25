//! Modo terminal: o DotCrow dentro da pasta do projeto, como um assistente de linha
//! de comando.
//!
//! Rode `dotcrow` num terminal, dentro da pasta de um projeto, e os corvos trabalham
//! nos arquivos dali (o `notas.php`, por exemplo). Dá para:
//!
//! - pôr outras pastas no projeto (`/pasta + ~/assets`), só leitura ou leitura e escrita;
//! - ver arquivos do aparelho num navegador numerado (`/arquivos`), espiar um arquivo
//!   (`/ver notas.php`), buscar (`/buscar titulo`) e anexar ao pedido (`@notas.php`);
//! - deixar o Huginn conhecer o projeto (`/alinhar`) e perguntar antes de cada tarefa;
//! - desfazer a última tarefa (`/desfazer`) e rodar comandos (`!git status`).
//!
//! O ícone do DotCrow (sem terminal) continua abrindo o painel no navegador.

mod arquivos;
mod banner;
mod entrada;
mod projeto;
mod render;

use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use anyhow::{Result, bail};
use dotcrow_agents::{
    alinhamento::{self, Pergunta},
    orchestrator,
};
use dotcrow_providers::Router;
use entrada::{Entrada, Leitura};
use projeto::{Projeto, expandir};
pub use render::Render;
use zeroize::Zeroizing;

use crate::{
    Paint, busca, desfazer,
    maos::{self, Liberados, Maos, Modo},
    memoria,
    store::{self, Store},
};

/// Quanto da conversa vai para os corvos (o fim dela).
const MAX_HISTORICO: usize = 16_000;
/// Anexos: tamanho máximo de um arquivo inteiro e do total de um pedido.
const MAX_ANEXO: usize = 150_000;
const MAX_ANEXOS_TOTAL: usize = 400_000;

/// Opções da linha de comando.
#[derive(Debug, Default)]
pub struct Opcoes {
    /// Pasta do projeto (padrão: a pasta atual).
    pub pasta: Option<PathBuf>,
    /// Modo das mãos.
    pub modo: Option<Modo>,
    /// Usar só este provedor.
    pub provedor: Option<String>,
    /// Não perguntar antes das tarefas.
    pub direto: bool,
    /// Um pedido para já começar.
    pub pedido: Option<String>,
}

/// Abre o modo terminal.
pub async fn rodar(store: Store, op: Opcoes) -> Result<()> {
    let p = Paint::new();
    let atual = std::env::current_dir()?;
    let pasta = match &op.pasta {
        Some(c) => expandir(&c.to_string_lossy(), &atual),
        None => atual,
    };
    let pasta = maos::normalizar(&pasta);
    if !pasta.is_dir() {
        bail!("{} não é uma pasta", pasta.display());
    }
    // Fora de um projeto (na pasta pessoal, na raiz do sistema ou na pasta de dados), os
    // corvos não ganham tudo: trabalham na pasta Downloads, e o dono traz as pastas que
    // quiser com /pasta +.
    let home = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf());
    let generica = pasta.parent().is_none()
        || home.as_deref().is_some_and(|h| maos::normalizar(h) == pasta)
        || pasta.starts_with(store.dir());
    let downloads = maos::pasta_de_downloads();
    let raiz = if generica {
        std::fs::create_dir_all(&downloads)?;
        downloads.clone()
    } else {
        pasta
    };
    let mut projeto = Projeto::abrir(store.dir(), raiz)?;
    if op.direto {
        projeto.cfg.direto = true;
    }
    let modo = op.modo.or_else(|| projeto.cfg.modo.as_deref().and_then(Modo::parse)).unwrap_or_default();
    let raizes = Arc::new(RwLock::new(projeto.raizes()));
    let entrada = Entrada::nova(raizes.clone(), Some(store.dir().join("historico-terminal.txt")));
    let mut repl = Repl {
        store,
        projeto,
        entrada,
        raizes,
        modo,
        provedor: op.provedor,
        historico: String::new(),
        liberados: Liberados::default(),
        anexos: Vec::new(),
        p,
        generica,
        downloads,
    };
    repl.boas_vindas();
    if !repl.generica && repl.projeto.perfil().is_none() && !repl.projeto.cfg.sem_entrevista && repl.tem_provedor() {
        repl.oferecer_entrevista().await;
    }
    if let Some(pedido) = op.pedido.filter(|t| !t.trim().is_empty()) {
        repl.tarefa(&pedido).await;
    }
    repl.laco().await;
    println!("{}", p.dim("Até mais! 🐦‍⬛"));
    Ok(())
}

struct Repl {
    store: Store,
    projeto: Projeto,
    entrada: Arc<Entrada>,
    raizes: Arc<RwLock<Vec<PathBuf>>>,
    modo: Modo,
    provedor: Option<String>,
    historico: String,
    liberados: Liberados,
    anexos: Vec<PathBuf>,
    p: Paint,
    generica: bool,
    /// Onde vão as entregas que não são do projeto (a pasta Downloads).
    downloads: PathBuf,
}

impl Repl {
    // ------------------------------------------------------------------ início

    fn boas_vindas(&self) {
        let p = self.p;
        banner::mostrar(p);
        if self.generica {
            println!(
                "  {} {}",
                p.yellow("Pasta de trabalho:"),
                p.dim(&format!(
                    "{} (fora de um projeto, o DotCrow trabalha na Downloads)",
                    self.projeto.raiz.display()
                ))
            );
            println!(
                "  {}",
                p.dim(
                    "Para os corvos mexerem num projeto, entre na pasta dele (cd ~/meu-site) e rode dotcrow, ou traga \
pastas com /pasta + caminho."
                )
            );
        } else {
            println!("  {} {}", p.bold("Projeto:"), self.projeto.raiz.display());
            println!(
                "  {} {}",
                p.bold("Entregas:"),
                p.dim(&format!("{} (relatórios, PDFs, imagens e o que não é do projeto)", self.downloads.display()))
            );
        }
        for e in &self.projeto.cfg.pastas {
            let acesso = if e.escrita { "leitura e escrita" } else { "só leitura" };
            println!("    + {} {}", e.caminho.display(), p.dim(&format!("({acesso})")));
        }
        let provedor = self.resumo_provedor();
        let perguntas = if self.projeto.cfg.direto { "direto" } else { "sempre" };
        println!("  {}", p.dim(&format!("{provedor} · modo {} · perguntas: {perguntas}", self.modo.as_str())));
        if !self.tem_provedor() {
            println!(
                "\n  {} {}",
                p.yellow("Falta uma chave de API."),
                p.dim("Use /chave anthropic (ou openai, gemini, openrouter, deepseek, groq).")
            );
        }
        println!(
            "\n  {}\n",
            p.dim(
                "Escreva o que precisa. @arquivo anexa · /arquivos navega · /ajuda mostra tudo · Ctrl+C para a tarefa"
            )
        );
    }

    fn resumo_provedor(&self) -> String {
        let status = self.store.provider_status();
        let ativo = match &self.provedor {
            Some(id) => status.iter().find(|s| s.id == *id),
            None => status.iter().filter(|s| s.position.is_some()).min_by_key(|s| s.position),
        };
        match ativo {
            Some(s) => format!("{} · {}", s.id, s.model),
            None => "sem provedor".into(),
        }
    }

    fn tem_provedor(&self) -> bool {
        self.store.router(self.provedor.as_deref()).map(|r| !r.order().is_empty()).unwrap_or(false)
    }

    /// O roteador, ou um aviso de como configurar.
    fn router(&self) -> Option<Router> {
        match self.store.router(self.provedor.as_deref()) {
            Ok(r) if !r.order().is_empty() => Some(r),
            Ok(_) => {
                println!(
                    "{}",
                    self.p.yellow(
                        "Nenhum provedor ativo. Adicione uma chave com /chave anthropic (ou openai, gemini...)."
                    )
                );
                None
            }
            Err(e) => {
                println!("{}", self.p.red(&format!("Provedor: {e}")));
                None
            }
        }
    }

    fn atualizar_raizes(&self) {
        if let Ok(mut r) = self.raizes.write() {
            *r = self.projeto.raizes();
        }
    }

    // ------------------------------------------------------------------ laço

    async fn laco(&mut self) {
        let mut ctrl_c_seguidos = 0;
        loop {
            let nome = self.projeto.nome();
            let anexos =
                if self.anexos.is_empty() { String::new() } else { format!(" [{} anexo(s)]", self.anexos.len()) };
            let simples = format!("{nome}{anexos} › ");
            let cor = format!("{}{} {} ", self.p.cyan(&self.p.bold(&nome)), self.p.yellow(&anexos), self.p.dim("›"));
            match self.entrada.ler(&simples, &cor, true).await {
                Leitura::Linha(l) => {
                    ctrl_c_seguidos = 0;
                    let l = l.trim();
                    if l.is_empty() {
                        continue;
                    }
                    if l.starts_with('/') {
                        if !self.comando(l).await {
                            return;
                        }
                    } else if let Some(cmd) = l.strip_prefix('!') {
                        self.shell(cmd.trim()).await;
                    } else if let Some(contexto) = crate::audio::gatilho(l) {
                        self.audio(&contexto).await;
                    } else {
                        self.tarefa(l).await;
                    }
                }
                Leitura::Interrompido => {
                    ctrl_c_seguidos += 1;
                    if ctrl_c_seguidos >= 2 {
                        return;
                    }
                    println!("{}", self.p.dim("(Ctrl+C de novo, Ctrl+D ou /sair para sair)"));
                }
                Leitura::Fim => return,
            }
        }
    }

    /// Roda um comando com barra. Devolve falso para sair.
    async fn comando(&mut self, linha: &str) -> bool {
        let p = self.p;
        let (cmd, arg) = match linha.split_once(char::is_whitespace) {
            Some((c, a)) => (c, a.trim()),
            None => (linha, ""),
        };
        match cmd {
            "/sair" | "/exit" | "/q" | "/quit" => return false,
            "/ajuda" | "/help" | "/?" => self.ajuda(),
            "/arquivos" | "/ls" | "/files" => {
                let inicio = if arg.is_empty() { self.projeto.raiz.clone() } else { expandir(arg, &self.projeto.raiz) };
                let saida = arquivos::navegar(&self.entrada, &self.projeto, inicio, p).await;
                self.anexos.extend(saida.anexar);
                if let Some(dir) = saida.adicionar {
                    self.adicionar_pasta(&dir).await;
                }
            }
            "/ver" | "/cat" => self.ver(arg),
            "/buscar" | "/procurar" | "/find" => self.buscar(arg).await,
            "/pastas" => self.mostrar_pastas(),
            "/pasta" => {
                let (acao, resto) = arg.split_once(char::is_whitespace).map(|(a, r)| (a, r.trim())).unwrap_or((arg, ""));
                match acao {
                    "+" | "adicionar" | "add" if !resto.is_empty() => {
                        let alvo = expandir(resto, &self.projeto.raiz);
                        self.adicionar_pasta(&alvo).await;
                    }
                    "-" | "remover" | "tirar" | "rm" if !resto.is_empty() => {
                        match self.projeto.remover_pasta(resto, &self.projeto.raiz.clone()) {
                            Some(c) => {
                                self.atualizar_raizes();
                                println!("{}", p.green(&format!("Tirei {} do projeto.", c.display())));
                            }
                            None => println!("{}", p.yellow("Não achei essa pasta. Veja os números em /pastas.")),
                        }
                    }
                    _ => {
                        self.mostrar_pastas();
                    }
                }
            }
            "/anexos" => {
                if arg == "limpar" {
                    self.anexos.clear();
                    println!("{}", p.dim("Anexos tirados."));
                } else if self.anexos.is_empty() {
                    println!("{}", p.dim("Nenhum anexo. Use @arquivo no pedido ou @N no /arquivos."));
                } else {
                    for a in &self.anexos {
                        println!("  📎 {}", a.display());
                    }
                    println!("{}", p.dim("Vão junto no próximo pedido. /anexos limpar tira."));
                }
            }
            "/alinhar" => self.alinhar().await,
            "/perfil" => match self.projeto.perfil() {
                Some(t) => println!("\n{}\n\n{}", render::indent(&t), p.dim("Refazer ou completar: /alinhar")),
                None => println!("{}", p.dim("Ainda não há perfil deste projeto. Faça com /alinhar.")),
            },
            "/perguntas" => {
                match arg {
                    "sempre" | "sim" | "on" => self.projeto.cfg.direto = false,
                    "direto" | "nao" | "não" | "off" | "nunca" => self.projeto.cfg.direto = true,
                    "" => self.projeto.cfg.direto = !self.projeto.cfg.direto,
                    _ => {
                        println!("{}", p.dim("Use /perguntas sempre ou /perguntas direto."));
                        return true;
                    }
                }
                let _ = self.projeto.salvar();
                if self.projeto.cfg.direto {
                    println!("{}", p.green("Direto: o Huginn não pergunta antes das tarefas (os corvos ainda podem perguntar no meio)."));
                } else {
                    println!("{}", p.green("Sempre: o Huginn pergunta antes das tarefas o que mudaria o resultado."));
                }
            }
            "/modo" => self.trocar_modo(arg),
            "/provedor" | "/provedores" => self.trocar_provedor(arg),
            "/chave" | "/chaves" => self.chave(arg).await,
            "/civ" | "/civilizacao" | "/bando" | "/corvos" => self.civilizacao(arg).await,
            "/audio" | "/áudio" | "/mic" | "/microfone" => self.audio(arg).await,
            "/conectores" | "/integracoes" => crate::conectores::listar(&self.store, arg, &p),
            "/celular" | "/android" => {
                if let Err(e) = crate::celular_status().await {
                    println!("{}", p.red(&format!("{e:#}")));
                }
            }
            "/testar" => {
                if let Err(e) = crate::testar(&self.store, (!arg.is_empty()).then_some(arg)).await {
                    println!("{}", p.red(&format!("{e:#}")));
                }
            }
            "/desfazer" | "/undo" => self.desfazer().await,
            "/nova" | "/limpar" | "/clear" => {
                self.historico.clear();
                self.anexos.clear();
                self.liberados = Liberados::default();
                println!("{}", p.green("Conversa nova. O perfil e as pastas do projeto continuam."));
            }
            "/painel" => println!(
                "{}",
                p.dim("O painel é o DotCrow no navegador: rode `dotcrow painel` em outra aba do terminal (no Termux, ele abre no navegador do celular).")
            ),
            outro => {
                let parecido = entrada::COMANDOS.iter().find(|c| c.starts_with(&outro[..outro.len().min(3)]));
                match parecido {
                    Some(c) => println!("{}", p.yellow(&format!("Não conheço {outro}. Quis dizer {c}? (/ajuda mostra todos)"))),
                    None => println!("{}", p.yellow(&format!("Não conheço {outro}. /ajuda mostra os comandos."))),
                }
            }
        }
        true
    }

    /// A conversa dos 10 corvos da civilização, aqui dentro do modo terminal.
    async fn civilizacao(&mut self, arg: &str) {
        let p = self.p;
        let pasta = crate::civ::pasta(self.store.dir());
        let mut e = match crate::civ::abrir_bando(&pasta, None, None, &p) {
            Ok(e) => e,
            Err(err) => {
                println!("{}", p.red(&format!("{err:#}")));
                return;
            }
        };
        let arg = arg.trim();
        if matches!(arg, "ver" | "mostrar" | "lista") {
            if let Err(err) = crate::civ::mostrar(&e, true, &p) {
                println!("{}", p.red(&format!("{err:#}")));
            }
            return;
        }
        if matches!(arg, "julgar" | "julgamento") {
            if self.civ_trancada(&["julgar".to_string()]).await {
                return;
            }
            if let Err(err) = crate::civ_julgamento::julgar(&self.store, &pasta, &mut e, &p).await {
                println!("{}", p.red(&format!("{err:#}")));
            }
            return;
        }
        if matches!(arg, "mortos") {
            let _ = crate::civ::mostrar_mortos(&e, &p);
            return;
        }
        if matches!(arg, "terrenos" | "mapa" | "economia") {
            crate::civ::mostrar_terrenos(&e, &p);
            return;
        }
        if matches!(arg, "pausar" | "parar") {
            match crate::civ::pausar(&pasta, &mut e) {
                Ok(true) => println!("{}", p.yellow("⏸ Civilização pausada. Aqui seguem os 6 corvos e o Oráculo.")),
                Ok(false) => println!("{}", p.dim("Ela já estava pausada. /civ retomar volta.")),
                Err(err) => println!("{}", p.red(&format!("{err:#}"))),
            }
            return;
        }
        if matches!(arg, "retomar" | "continuar" | "voltar") {
            match crate::civ::retomar(&pasta, &mut e) {
                Ok(Some(s)) => println!("{}", p.green(&format!("▶ Civilização de volta (pausada {} min).", s / 60))),
                Ok(None) => println!("{}", p.dim("Ela não estava pausada.")),
                Err(err) => println!("{}", p.red(&format!("{err:#}"))),
            }
            return;
        }
        if let Some(resto) = ["missao ", "missão ", "fazer "].iter().find_map(|pre| arg.strip_prefix(pre)) {
            let (nome, pedido) = resto.trim().split_once(char::is_whitespace).unwrap_or((resto.trim(), ""));
            if pedido.trim().is_empty() {
                println!("{}", p.yellow("Use: /civ missao <corvo> <o que fazer> (ou /civ missao auto <o que fazer>)"));
                return;
            }
            let args = ["missao", "--modo", self.modo.as_str(), nome, pedido.trim()].map(str::to_string);
            if self.civ_trancada(&args).await {
                println!("{}", p.dim("O que o corvo entregou fica em Downloads/DotCrow/entregas."));
                return;
            }
            let (rx, leitor) = self.leitor_de_linhas();
            let (r, rx) =
                crate::civ_missao::executar(&self.store, &pasta, &mut e, nome, pedido.trim(), None, self.modo, rx, &p)
                    .await;
            drop(rx);
            match r {
                Ok((quem, entrega)) => {
                    println!("\n{}\n{}\n", p.bold(&format!("Entrega de {quem}:")), render::indent(entrega.trim()));
                    let bloco = format!("## Missão de {quem}\n{pedido}\n\n## Entrega\n{entrega}\n\n");
                    self.historico.push_str(&bloco);
                }
                Err(err) => println!("{}", p.red(&format!("{err:#}"))),
            }
            let _ = crate::civ::salvar(&pasta, &e);
            if !leitor.is_finished() {
                println!("{}", p.dim("Aperte Enter para voltar ao DotCrow."));
            }
            let _ = leitor.await;
            return;
        }
        if let Some(corvo) = arg.strip_prefix("memoria ").or_else(|| arg.strip_prefix("memória ")) {
            if let Err(err) = crate::civ::mostrar_memoria(&pasta, &e, corvo.trim(), &p) {
                println!("{}", p.red(&format!("{err:#}")));
            }
            return;
        }
        let tema = arg.strip_prefix("conversar").unwrap_or(arg).trim();
        let tema = (!tema.is_empty()).then(|| tema.to_string());
        let mut args = vec!["conversar".to_string()];
        if let Some(t) = &tema {
            args.extend(["--tema".to_string(), t.clone()]);
        }
        if self.civ_trancada(&args).await {
            return;
        }
        let (rx, leitor) = self.leitor_de_linhas();
        let o = crate::civ::Opcoes { intervalo: std::time::Duration::from_secs(20), max_falas: None, tema };
        if let Err(err) = crate::civ::conversar(&self.store, &pasta, &e, o, &p, rx).await {
            println!("{}", p.red(&format!("{err:#}")));
        }
        if !leitor.is_finished() {
            println!("{}", p.dim("Aperte Enter para voltar ao DotCrow."));
        }
        let _ = leitor.await;
    }

    /// A Civilização roda num processo à parte, trancado na pasta DotCrow (a trava do
    /// Linux não tem volta, e o modo terminal precisa continuar enxergando o projeto).
    /// Devolve falso se não deu para abrir o processo (aí roda aqui mesmo, sem a trava).
    async fn civ_trancada(&self, args: &[String]) -> bool {
        let Ok(exe) = std::env::current_exe() else { return false };
        let mut c = tokio::process::Command::new(exe);
        c.arg("--home").arg(self.store.dir()).arg("civ").args(args);
        c.stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());
        let Ok(mut filho) = c.spawn() else { return false };
        // O Ctrl+C chega aos dois: o filho encerra a conversa; este aqui só espera.
        loop {
            tokio::select! {
                _ = filho.wait() => break,
                _ = tokio::signal::ctrl_c() => {}
            }
        }
        println!();
        true
    }

    /// As linhas digitadas vêm pelo próprio editor do modo terminal; Ctrl+C vira "pare".
    fn leitor_de_linhas(&self) -> (tokio::sync::mpsc::UnboundedReceiver<String>, tokio::task::JoinHandle<()>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let entrada = self.entrada.clone();
        let leitor = tokio::spawn(async move {
            loop {
                match entrada.ler("", "", false).await {
                    Leitura::Linha(l) => {
                        let parar = crate::civ::eh_parada(&l);
                        if tx.send(l).is_err() || parar {
                            break;
                        }
                    }
                    Leitura::Interrompido => {
                        let _ = tx.send("pare".into());
                        break;
                    }
                    Leitura::Fim => break,
                }
            }
        });
        (rx, leitor)
    }

    /// Abre o microfone; o que o dono falou vira a tarefa (sem as perguntas antes: os
    /// corvos se viram, e ainda podem perguntar no meio).
    async fn audio(&mut self, contexto: &str) {
        let p = self.p;
        let entrada = self.entrada.clone();
        let parar = async move { matches!(entrada.ler("", "", false).await, Leitura::Linha(_)) };
        match crate::audio::ouvir_o_dono(&self.store, parar, &p).await {
            Ok(Some(fala)) => {
                let pedido = crate::audio::pedido_do_audio(contexto, &fala);
                self.tarefa_com(&pedido, false).await;
            }
            Ok(None) => {}
            Err(e) => println!("{}", p.red(&format!("🎤 {e:#}"))),
        }
    }

    fn ajuda(&self) {
        let p = self.p;
        let linhas: [(&str, &str); 31] = [
            ("", "Pedidos"),
            ("texto livre", "uma tarefa para os corvos (eles trabalham nos arquivos do projeto)"),
            ("/audio [pra quê]", "abre o microfone: você fala, Enter termina e os corvos fazem o que você pediu"),
            ("Áudio pra post no X", "o mesmo, escrito do seu jeito (áudio pra..., audio para...)"),
            ("@notas.php", "anexa um arquivo ou pasta ao pedido (Tab completa o caminho)"),
            ("!comando", "roda um comando aqui mesmo (ex.: !git status, !php -l notas.php)"),
            ("", "Arquivos e pastas"),
            ("/arquivos [pasta]", "navega pelas pastas do aparelho: abrir, ver, anexar, pôr no projeto"),
            ("/ver arquivo [linha]", "mostra um arquivo com números de linha"),
            ("/buscar termo", "acha arquivos pelo nome e pelo conteúdo, nas pastas do projeto"),
            ("/pastas", "as pastas que participam do projeto"),
            ("/pasta + caminho", "põe uma pasta no projeto (pergunta se é leitura ou escrita)"),
            ("/pasta - N", "tira uma pasta do projeto"),
            ("", "Alinhamento"),
            ("/alinhar", "o Huginn olha o projeto e faz perguntas; vira o perfil do projeto"),
            ("/perfil", "mostra o perfil do projeto"),
            ("/perguntas sempre|direto", "o Huginn pergunta (ou não) antes de cada tarefa"),
            ("", "Civilização dos Corvos"),
            (
                "/civ [tema]",
                "os 10 corvos conversam (cada um com o seu modelo); escreva para falar com eles, pare encerra",
            ),
            ("/civ ver|julgar|mortos", "o bando · um julgamento (jurados + oráculo) · o Livro dos Mortos"),
            ("/civ pausar|retomar", "pausa a civilização (o relógio das 24 h para) e o DotCrow fica com os 6 corvos"),
            ("/civ missao <corvo> <tarefa>", "um corvo faz a tarefa com mãos (PC, celular, internet), do jeito dele"),
            ("", "Celular"),
            ("/celular", "o que os corvos podem fazer no Android (tocar, rolar, digitar, ligar, SMS) e como ligar"),
            (
                "/conectores [busca]",
                "os 257 serviços com API (X, GitHub, Drive...): dotcrow conectores adicionar <id> liga",
            ),
            ("", "Controle"),
            ("/desfazer", "volta os arquivos que a última tarefa mudou"),
            ("/modo computador|livre|sandbox", "quanto os corvos podem fazer sem perguntar"),
            ("/provedor [nome|auto]", "qual IA usar · /chave nome adiciona chave · /testar testa"),
            ("/nova", "começa uma conversa nova (o projeto continua)"),
            ("/sair", "sai (ou Ctrl+D)"),
        ];
        println!();
        for (cmd, desc) in linhas {
            if cmd.is_empty() {
                println!("  {}", p.bold(desc));
            } else {
                println!("    {:<32} {}", p.cyan(cmd), p.dim(desc));
            }
        }
        println!(
            "\n  {}\n",
            p.dim("Ctrl+C para a tarefa em andamento. Setas ↑↓ trazem pedidos anteriores. No Termux, rode termux-setup-storage \
para ver os arquivos do celular em ~/storage/shared.")
        );
    }

    // ------------------------------------------------------------------ arquivos

    fn ver(&self, arg: &str) {
        let p = self.p;
        if arg.is_empty() {
            println!("{}", p.dim("Use /ver arquivo [linha]. Ex.: /ver notas.php 120"));
            return;
        }
        let (caminho, linha) = match arg.rsplit_once(char::is_whitespace) {
            Some((c, n)) if n.parse::<usize>().is_ok() => (c.trim(), n.parse().unwrap_or(1)),
            _ => (arg, 1),
        };
        let alvo = self.achar(caminho);
        print!("{}", arquivos::previa(&alvo, linha, 60, p));
    }

    /// Resolve um caminho digitado: relativo à pasta principal ou, se não existir lá,
    /// a uma das pastas extras.
    fn achar(&self, caminho: &str) -> PathBuf {
        let caminho = caminho.trim_start_matches('@');
        let alvo = expandir(caminho, &self.projeto.raiz);
        if alvo.exists() {
            return alvo;
        }
        self.projeto.cfg.pastas.iter().map(|e| e.caminho.join(caminho)).find(|a| a.exists()).unwrap_or(alvo)
    }

    async fn buscar(&mut self, termo: &str) {
        let p = self.p;
        if termo.is_empty() {
            println!("{}", p.dim("Use /buscar termo. Ex.: /buscar titulo  ou  /buscar *.php"));
            return;
        }
        let raizes = self.projeto.raizes();
        // A busca roda fora do laço principal, para pastas grandes não travarem a tela.
        let projeto_raiz = self.projeto.raiz.clone();
        let cfg = self.projeto.cfg.clone();
        let dados = self.projeto.dados.clone();
        let termo2 = termo.to_string();
        let (achados, texto) = tokio::task::spawn_blocking(move || {
            let tmp = Projeto { raiz: projeto_raiz, dados, cfg };
            arquivos::buscar(&tmp, &termo2)
        })
        .await
        .unwrap_or_default();
        if achados.is_empty() {
            println!(
                "{}",
                p.dim(&format!(
                    "Nada com \"{termo}\" nas pastas do projeto (.git, node_modules e vendor ficam de fora)."
                ))
            );
            return;
        }
        print!("{texto}");
        loop {
            let pergunta = "N vê · @N anexa · Enter volta › ";
            let linha = match self.entrada.ler(pergunta, &p.dim(pergunta), false).await {
                Leitura::Linha(l) => l.trim().to_string(),
                _ => return,
            };
            if linha.is_empty() {
                return;
            }
            let (anexar, num) = match linha.strip_prefix('@') {
                Some(n) => (true, n),
                None => (false, linha.as_str()),
            };
            match num.parse::<usize>().ok().filter(|n| *n >= 1 && *n <= achados.len()) {
                Some(n) if anexar => {
                    let a = achados[n - 1].clone();
                    println!("{}", p.green(&format!("📎 {} vai junto no próximo pedido", busca::exibir(&a, &raizes))));
                    if !self.anexos.contains(&a) {
                        self.anexos.push(a);
                    }
                }
                Some(n) => print!("{}", arquivos::previa(&achados[n - 1], 1, 40, p)),
                None => println!("{}", p.yellow("número fora da lista")),
            }
        }
    }

    fn mostrar_pastas(&self) {
        let p = self.p;
        println!("\n  {}", p.bold("Pastas do projeto"));
        println!("   1. {} {}", self.projeto.raiz.display(), p.dim("(principal · leitura e escrita)"));
        for (i, e) in self.projeto.cfg.pastas.iter().enumerate() {
            let acesso = if e.escrita { p.green("leitura e escrita") } else { p.yellow("só leitura") };
            let some = if e.caminho.is_dir() { String::new() } else { p.red(" (não encontrada)") };
            println!("   {}. {} ({acesso}){some}", i + 2, e.caminho.display());
            if let Some(n) = &e.nota {
                println!("      {}", p.dim(n));
            }
        }
        println!("\n  {}\n", p.dim("/pasta + caminho põe outra · /pasta - N tira · /arquivos escolhe navegando"));
    }

    async fn adicionar_pasta(&mut self, dir: &std::path::Path) {
        let p = self.p;
        let cofre = self.store.dir().to_path_buf();
        let dir = match self.projeto.validar_pasta(dir, &cofre) {
            Ok(d) => d,
            Err(e) => {
                println!("{}", p.yellow(&format!("Não dá para pôr essa pasta: {e}.")));
                return;
            }
        };
        let dir = dir.as_path();
        println!("\n  Como os corvos podem usar {}?", p.bold(&dir.display().to_string()));
        println!("     1) Ler e alterar (sem perguntar)");
        println!("     2) Só ler (alterar pede confirmação)");
        let escrita = loop {
            let q = "  [1/2, Enter = só ler] › ";
            match self.entrada.ler(q, &p.cyan(q), false).await {
                Leitura::Linha(l) => match l.trim() {
                    "1" | "a" | "alterar" | "escrita" => break true,
                    "2" | "" | "l" | "ler" | "leitura" => break false,
                    _ => println!("{}", p.dim("responda 1 ou 2")),
                },
                _ => {
                    println!("{}", p.dim("Cancelado."));
                    return;
                }
            }
        };
        let q = "  Para que serve essa pasta no projeto? (Enter pula) › ";
        let nota = match self.entrada.ler(q, &p.cyan(q), false).await {
            Leitura::Linha(l) => Some(l),
            _ => None,
        };
        match self.projeto.adicionar_pasta(dir, escrita, nota, &cofre) {
            Ok(c) => {
                self.atualizar_raizes();
                let acesso = if escrita { "leitura e escrita" } else { "só leitura" };
                println!("{}", p.green(&format!("✔ {} faz parte do projeto ({acesso}).", c.display())));
                if self.projeto.perfil().is_some() {
                    println!("{}", p.dim("  Quer que o Huginn conheça a pasta nova? /alinhar atualiza o perfil."));
                }
            }
            Err(e) => println!("{}", p.yellow(&format!("Não pus: {e}."))),
        }
    }

    // ------------------------------------------------------------------ alinhamento

    async fn oferecer_entrevista(&mut self) {
        let p = self.p;
        println!(
            "  {} Posso fazer umas perguntas rápidas para os corvos entenderem o projeto?",
            p.cyan("🧭 Primeira vez nesta pasta.")
        );
        let q = "  [S/n] › ";
        match self.entrada.ler(q, &p.cyan(q), false).await {
            Leitura::Linha(l) if matches!(l.trim().to_lowercase().as_str(), "" | "s" | "sim" | "y" | "yes") => {
                self.alinhar().await;
            }
            Leitura::Linha(_) => {
                self.projeto.cfg.sem_entrevista = true;
                let _ = self.projeto.salvar();
                println!("{}", p.dim("  Sem problema. Quando quiser: /alinhar\n"));
            }
            _ => {}
        }
    }

    /// Entrevista do projeto: o Huginn olha a pasta, pergunta e a Pena escreve o perfil.
    async fn alinhar(&mut self) {
        let p = self.p;
        let Some(router) = self.router() else { return };
        println!("{}", p.dim("  🧭 O Huginn está olhando a pasta..."));
        let retrato = self.projeto.retrato();
        let r = tokio::select! {
            r = alinhamento::perguntas_do_projeto(&router, &retrato) => r,
            _ = tokio::signal::ctrl_c() => { println!("{}", p.dim("  ⏹ Cancelado.")); return; }
        };
        let perguntas = match r {
            Ok(ps) if !ps.is_empty() => ps,
            Ok(_) => perguntas_padrao(),
            Err(e) => {
                println!(
                    "{}",
                    p.yellow(&format!("  Não consegui falar com a IA ({e}). Vou fazer as perguntas básicas."))
                );
                perguntas_padrao()
            }
        };
        let Some(respostas) = self.fazer_perguntas("O Huginn quer conhecer o projeto", &perguntas).await else {
            println!("{}", p.dim("  ⏹ Alinhamento cancelado."));
            return;
        };
        if respostas.iter().all(|r| r.resposta.trim().is_empty()) {
            println!("{}", p.dim("  Nada respondido; o perfil fica como estava."));
            return;
        }
        println!("{}", p.dim("  ✍ A Pena está escrevendo o perfil do projeto..."));
        let antigo = self.projeto.perfil();
        let r = tokio::select! {
            r = alinhamento::perfil_do_projeto(&router, &retrato, &respostas, antigo.as_deref()) => r.ok(),
            _ = tokio::signal::ctrl_c() => None,
        };
        let texto = match r {
            Some(t) if !t.trim().is_empty() => t,
            _ => format!("## Respostas do dono\n{}", alinhamento::formatar_respostas(&respostas)),
        };
        match self.projeto.salvar_perfil(&texto) {
            Ok(()) => {
                println!("\n{}\n", render::indent(&texto));
                println!(
                    "{}\n",
                    p.green(
                        "  ✔ Perfil guardado: vai junto em toda tarefa nesta pasta. Ver: /perfil · refazer: /alinhar"
                    )
                );
            }
            Err(e) => println!("{}", p.red(&format!("  Não consegui guardar o perfil: {e}"))),
        }
    }

    /// Faz as perguntas uma a uma. `None` = o dono cancelou (Ctrl+C).
    async fn fazer_perguntas(&self, titulo: &str, perguntas: &[Pergunta]) -> Option<Vec<alinhamento::Resposta>> {
        let p = self.p;
        println!(
            "\n  {} {}",
            p.cyan(&format!("🧭 {titulo}:")),
            p.dim("número escolhe · texto livre vale · Enter = decida você · /pular segue sem responder")
        );
        let mut respostas = Vec::new();
        for (i, q) in perguntas.iter().enumerate() {
            println!("\n  {}. {}", i + 1, p.bold(&q.texto));
            for (j, o) in q.opcoes.iter().enumerate() {
                println!("     {}) {o}", j + 1);
            }
            let prompt = "  › ";
            match self.entrada.ler(prompt, &p.cyan(prompt), false).await {
                Leitura::Linha(l) => {
                    let l = l.trim();
                    if l == "/pular" {
                        break;
                    }
                    let resposta = match l.parse::<usize>() {
                        Ok(n) if n >= 1 && n <= q.opcoes.len() => q.opcoes[n - 1].clone(),
                        _ => l.to_string(),
                    };
                    respostas.push(alinhamento::Resposta { pergunta: q.texto.clone(), resposta });
                }
                Leitura::Interrompido | Leitura::Fim => return None,
            }
        }
        println!();
        Some(respostas)
    }

    // ------------------------------------------------------------------ tarefas

    async fn tarefa(&mut self, pedido: &str) {
        self.tarefa_com(pedido, true).await;
    }

    /// Uma tarefa; `alinhar` = o Huginn pode perguntar antes de começar.
    async fn tarefa_com(&mut self, pedido: &str, alinhar: bool) {
        let p = self.p;
        let Some(router) = self.router() else { return };

        // 1. Anexos (@arquivo no texto e os marcados no /arquivos ou /buscar).
        let (mut pedido_final, anexados) = self.expandir_anexos(pedido);
        for a in &anexados {
            println!("{}", p.dim(&format!("📎 {a}")));
        }

        // 1.5. O Oráculo (Opus 5.5) lê o pedido e decide quantos corvos ele precisa.
        // "Oi" ele mesmo responde; tarefa pequena vai para um corvo só.
        let oraculo = self.store.oraculo();
        let contexto_oraculo = match self.historico_para_corvos() {
            h if h.trim().is_empty() => format!("# Pedido do dono\n{pedido_final}"),
            h => format!("# Conversa até aqui\n{h}\n\n# Pedido do dono agora\n{pedido_final}"),
        };
        if p.on {
            print!("{}", p.dim("🔮 O Oráculo está lendo o pedido…"));
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        let triagem = tokio::select! {
            t = orchestrator::triar(oraculo.as_ref().unwrap_or(&router), &router, &contexto_oraculo) => Some(t),
            _ = tokio::signal::ctrl_c() => None,
        };
        if p.on {
            print!("\r\x1b[2K");
        }
        let Some(triagem) = triagem else {
            println!("{}", p.dim("⏹ Cancelado."));
            return;
        };
        if triagem.nivel == orchestrator::Nivel::Conversa {
            let quem = if triagem.model.is_empty() { String::new() } else { format!(" · {}", triagem.model) };
            println!("{} {}\n{}\n", p.bold("🔮 Oráculo"), p.dim(&quem), render::indent(&triagem.resposta));
            let bloco = format!("## Pedido\n{pedido}\n\n## Entrega\n{}\n\n", triagem.resposta);
            self.historico.push_str(&bloco);
            return;
        }
        let alinhar = alinhar && triagem.nivel == orchestrator::Nivel::Completa;

        // 2. O Huginn pergunta antes, se algo mudaria o resultado.
        if alinhar && !self.projeto.cfg.direto {
            println!("{}", p.dim("🧭 O Huginn está vendo se precisa perguntar algo..."));
            let contexto = self.contexto();
            let r = tokio::select! {
                r = alinhamento::perguntas_antes(&router, &contexto, &pedido_final) => Some(r),
                _ = tokio::signal::ctrl_c() => None,
            };
            match r {
                None => {
                    println!("{}", p.dim("⏹ Cancelado."));
                    return;
                }
                Some(Ok(ps)) if !ps.is_empty() => {
                    let Some(rs) = self.fazer_perguntas("O Huginn quer alinhar antes de começar", &ps).await else {
                        println!("{}", p.dim("⏹ Cancelado."));
                        return;
                    };
                    if !rs.is_empty() {
                        pedido_final.push_str(&format!(
                            "\n\n# Respostas do dono às perguntas do Huginn\n{}",
                            alinhamento::formatar_respostas(&rs)
                        ));
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => println!("{}", p.dim(&format!("(não deu para alinhar antes: {e}; seguindo)"))),
            }
        }

        // 3. Mãos no projeto, com diário para desfazer e perguntas no meio da tarefa.
        let diario = desfazer::Diario::novo(&self.projeto.pasta_desfazer(), pedido).ok().map(Arc::new);
        let parar = Arc::new(tokio::sync::Notify::new());
        let aprovador = Arc::new(Aprovador { entrada: self.entrada.clone(), p, parar: parar.clone() });
        let maos = Maos::new(
            self.modo,
            self.projeto.raiz.clone(),
            self.store.dir().to_path_buf(),
            aprovador,
            self.liberados.clone(),
        )
        .com_perguntas()
        .com_projeto(self.projeto.extras(), diario.clone())
        .com_saidas(self.downloads.clone())
        .com_conectores(crate::conectores::Conexoes::do_store(&self.store));
        let memoria = self.store.master_key().ok().and_then(|k| memoria::Memoria::abrir(self.store.dir(), k).ok());
        let historico = self.historico_para_corvos();
        let input = orchestrator::TaskInput {
            request: &pedido_final,
            kind: None,
            hands: Some(&maos),
            history: Some(&historico),
            memory: memoria.as_ref().map(|m| m as &dyn dotcrow_agents::Memory),
            inbox: None,
            oraculo: orchestrator::Oraculo::Decidido(&triagem),
        };
        println!();
        let mut render = Render::new(p);
        let resultado = tokio::select! {
            r = orchestrator::run(&router, input, |e| render.evento(e)) => Some(r),
            _ = tokio::signal::ctrl_c() => None,
            _ = parar.notified() => None,
        };
        let entrega = match &resultado {
            None => {
                println!("\n{}", p.yellow("⏹ Tarefa interrompida."));
                "(o dono interrompeu a tarefa)".to_string()
            }
            Some(Ok(o)) => format!("{}\n\n{}", o.result.trim(), o.summary.trim()),
            Some(Err(e)) => format!("(a tarefa parou: {e})"),
        };

        // 4. O que mudou nos arquivos.
        if let Some(d) = &diario {
            let itens = d.itens();
            if !itens.is_empty() {
                let raizes = self.projeto.raizes();
                println!("\n{}", p.bold(&format!("✎ {} arquivo(s) mexido(s):", itens.len())));
                for i in itens.iter().take(30) {
                    let marca = if i.copia.is_none() && !i.sem_copia { "novo" } else { "alterado" };
                    println!("   {} {}", p.dim(&format!("{marca:<9}")), busca::exibir(&i.original, &raizes));
                }
                if itens.len() > 30 {
                    println!("   {}", p.dim(&format!("… e mais {}", itens.len() - 30)));
                }
                println!("   {}", p.dim("/desfazer volta tudo como estava antes desta tarefa"));
            }
            d.descartar_se_vazio();
        }
        println!();

        // 5. A conversa continua: o próximo pedido sabe o que aconteceu.
        let bloco = format!("## Pedido\n{pedido}\n\n## Entrega\n{entrega}\n\n");
        self.projeto.anotar_conversa(&bloco);
        self.historico.push_str(&bloco);
        let total = self.historico.chars().count();
        if total > MAX_HISTORICO {
            self.historico = self.historico.chars().skip(total - MAX_HISTORICO).collect();
        }
    }

    /// Acrescenta ao pedido os arquivos citados com @ e os anexos marcados.
    fn expandir_anexos(&mut self, pedido: &str) -> (String, Vec<String>) {
        let mut caminhos: Vec<PathBuf> = std::mem::take(&mut self.anexos);
        for palavra in pedido.split_whitespace() {
            let Some(c) = palavra.strip_prefix('@') else { continue };
            let c = c.trim_end_matches([',', '.', ';', ':', ')', '!', '?', '"', '\'']);
            if c.is_empty() {
                continue;
            }
            let alvo = self.achar(c);
            if alvo.exists() {
                caminhos.push(alvo);
            }
        }
        let mut vistos = std::collections::HashSet::new();
        caminhos.retain(|c| vistos.insert(c.clone()));

        let raizes = self.projeto.raizes();
        let mut texto = pedido.to_string();
        let mut mostrados = Vec::new();
        let mut total = 0usize;
        for c in caminhos {
            let nome = busca::exibir(&c, &raizes);
            if c.is_dir() {
                let lista = arquivos::listar(&c, false)
                    .map(|itens| {
                        itens
                            .iter()
                            .take(200)
                            .map(|i| if i.pasta { format!("{}/", i.nome) } else { i.nome.clone() })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                texto.push_str(&format!("\n\n# Pasta anexada: {}\n{lista}", c.display()));
                mostrados.push(format!("{nome}/ (lista da pasta)"));
                continue;
            }
            let Ok(bytes) = std::fs::read(&c) else { continue };
            let tam = arquivos::tamanho(bytes.len() as u64);
            if busca::parece_binario(&bytes) {
                texto.push_str(&format!(
                    "\n\n# Arquivo anexado (binário, conteúdo não incluído): {} ({tam})",
                    c.display()
                ));
                mostrados.push(format!("{nome} ({tam}, binário: vai só o caminho)"));
                continue;
            }
            let conteudo = String::from_utf8_lossy(&bytes);
            let ext = c.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
            let (corpo, cortado) = if conteudo.len() > MAX_ANEXO || total + conteudo.len() > MAX_ANEXOS_TOTAL {
                (conteudo.lines().take(300).collect::<Vec<_>>().join("\n"), true)
            } else {
                (conteudo.to_string(), false)
            };
            total += corpo.len();
            let aviso = if cortado { " — só o começo; leia o resto com ler_arquivo" } else { "" };
            texto.push_str(&format!("\n\n# Arquivo anexado: {}{aviso}\n```{ext}\n{corpo}\n```", c.display()));
            mostrados.push(format!("{nome} ({tam}){}", if cortado { ", só o começo" } else { "" }));
        }
        (texto, mostrados)
    }

    /// O que o Huginn precisa saber para decidir se pergunta.
    fn contexto(&self) -> String {
        let mut t = String::new();
        if let Some(perfil) = self.projeto.perfil() {
            t.push_str(&format!("# Perfil do projeto (o que o dono contou)\n{perfil}\n\n"));
        }
        t.push_str(&format!("# Pastas do projeto\n{}\n", self.projeto.descrever_pastas()));
        if let Ok(itens) = arquivos::listar(&self.projeto.raiz, false) {
            let nomes: Vec<String> =
                itens.iter().take(80).map(|i| if i.pasta { format!("{}/", i.nome) } else { i.nome.clone() }).collect();
            t.push_str(&format!("\n# Arquivos na pasta principal\n{}\n", nomes.join("  ")));
        }
        if !self.historico.is_empty() {
            let total = self.historico.chars().count();
            let fim: String = self.historico.chars().skip(total.saturating_sub(6000)).collect();
            t.push_str(&format!("\n# Conversa até aqui\n{fim}"));
        }
        t
    }

    /// O que vai junto para os corvos como "conversa até aqui".
    fn historico_para_corvos(&self) -> String {
        let mut t = String::new();
        if let Some(perfil) = self.projeto.perfil() {
            t.push_str(&format!("## Perfil do projeto (o que o dono contou; siga isto)\n{perfil}\n\n"));
        }
        t.push_str(&format!("## Pastas do projeto\n{}\n\n", self.projeto.descrever_pastas()));
        t.push_str(&self.historico);
        t
    }

    // ------------------------------------------------------------------ outros

    async fn shell(&mut self, cmd: &str) {
        if cmd.is_empty() {
            println!("{}", self.p.dim("Use !comando. Ex.: !git status"));
            return;
        }
        let mut c = if cfg!(target_os = "windows") {
            let mut c = tokio::process::Command::new("powershell");
            c.args(["-NoProfile", "-Command", cmd]);
            c
        } else {
            let mut c = tokio::process::Command::new(maos::achar_shell());
            c.args(["-c", cmd]);
            c
        };
        c.current_dir(&self.projeto.raiz);
        match c.status().await {
            Ok(s) if !s.success() => {
                println!("{}", self.p.dim(&format!("[saiu com código {}]", s.code().unwrap_or(-1))));
            }
            Ok(_) => {}
            Err(e) => println!("{}", self.p.red(&format!("não consegui rodar: {e}"))),
        }
        self.historico.push_str(&format!("## O dono rodou no terminal\n`{cmd}`\n\n"));
    }

    fn trocar_modo(&mut self, arg: &str) {
        let p = self.p;
        if arg.is_empty() {
            println!(
                "  Modo atual: {}. {}",
                p.bold(self.modo.as_str()),
                p.dim("computador = pergunta o que é sensível · livre = não pergunta · sandbox = caixa isolada")
            );
            return;
        }
        match Modo::parse(arg) {
            Some(Modo::Sandbox) if cfg!(target_os = "android") => {
                println!("{}", p.yellow("O modo sandbox não existe no Android (Termux)."));
            }
            Some(m) => {
                self.modo = m;
                self.projeto.cfg.modo = Some(m.as_str().to_string());
                let _ = self.projeto.salvar();
                println!("{}", p.green(&format!("Modo {}.", m.as_str())));
            }
            None => println!("{}", p.yellow("Use /modo computador, /modo livre ou /modo sandbox.")),
        }
    }

    fn trocar_provedor(&mut self, arg: &str) {
        let p = self.p;
        if !arg.is_empty() {
            if matches!(arg, "auto" | "todos" | "padrao" | "padrão") {
                self.provedor = None;
            } else {
                let id = store::canonical_provider(arg);
                if dotcrow_providers::catalog::info(&id).is_none() {
                    println!("{}", p.yellow(&format!("Não conheço o provedor {id}.")));
                    return;
                }
                self.provedor = Some(id);
            }
        }
        println!();
        for s in self.store.provider_status() {
            let escolhido = match &self.provedor {
                Some(id) => *id == s.id,
                None => s.position == Some(1),
            };
            let marca = if escolhido { p.green("●") } else { " ".into() };
            let estado = if s.has_key {
                p.green("chave ok")
            } else if !s.needs_key {
                p.dim("local")
            } else {
                p.dim("sem chave")
            };
            println!("  {marca} {:<11} {:<24} {estado}  {}", s.id, s.name, p.dim(&s.model));
        }
        let modo = match &self.provedor {
            Some(id) => format!("Usando só {id} nesta sessão. /provedor auto volta à troca automática."),
            None => "Troca automática: se um acabar a cota, o próximo assume.".into(),
        };
        println!("\n  {}\n", p.dim(&modo));
    }

    async fn chave(&mut self, arg: &str) {
        let p = self.p;
        if arg.is_empty() {
            println!(
                "{}",
                p.dim("Use /chave provedor. Ex.: /chave anthropic (ou openai, gemini, openrouter, deepseek, groq)")
            );
            return;
        }
        let provedor = arg.to_string();
        let lido = tokio::task::spawn_blocking(|| {
            rpassword::prompt_password("  Cole a chave (ela não vai aparecer): ").map(Zeroizing::new)
        })
        .await;
        let segredo = match lido {
            Ok(Ok(s)) if !s.trim().is_empty() => s,
            _ => {
                println!("{}", p.dim("  Cancelado."));
                return;
            }
        };
        let tinha = self.tem_provedor();
        match self.store.add_key(&provedor, segredo, Some("terminal".into())) {
            Ok(info) => {
                println!(
                    "{}",
                    p.green(&format!(
                        "  ✔ Chave guardada ({}, {}). Ela não pode ser exibida.",
                        info.provider, info.fingerprint
                    ))
                );
                if dotcrow_providers::catalog::info(&info.provider).is_none() {
                    println!(
                        "{}",
                        p.yellow(&format!("  \"{}\" não é um provedor que o DotCrow conhece.", info.provider))
                    );
                } else {
                    let _ = crate::testar(&self.store, Some(&info.provider)).await;
                }
                if !tinha && !self.generica && self.projeto.perfil().is_none() && self.tem_provedor() {
                    self.oferecer_entrevista().await;
                }
            }
            Err(e) => println!("{}", p.red(&format!("  Não guardei: {e:#}"))),
        }
    }

    async fn desfazer(&mut self) {
        let p = self.p;
        let Some((dir, idx)) = desfazer::ultimo(&self.projeto.pasta_desfazer()) else {
            println!(
                "{}",
                p.dim("Nada para desfazer (as tarefas deste projeto não mudaram arquivos, ou já foram desfeitas).")
            );
            return;
        };
        let raizes = self.projeto.raizes();
        println!("\n  Última tarefa: {}", p.bold(&idx.pedido));
        for i in &idx.itens {
            let acao = match (&i.copia, i.sem_copia) {
                (Some(_), _) => "volta ao original",
                (None, true) => "sem cópia (grande)",
                (None, false) => "apaga (era novo)",
            };
            println!("   {} {}", p.dim(&format!("{acao:<19}")), busca::exibir(&i.original, &raizes));
        }
        println!(
            "  {}",
            p.dim("Só volta o que passou pelas ferramentas de arquivo; mudanças feitas por comandos no terminal não.")
        );
        let q = "  Desfazer? [s/N] › ";
        match self.entrada.ler(q, &p.cyan(q), false).await {
            Leitura::Linha(l) if matches!(l.trim().to_lowercase().as_str(), "s" | "sim" | "y" | "yes") => {
                match desfazer::desfazer(&dir) {
                    Ok(linhas) => {
                        for l in linhas {
                            println!("   {l}");
                        }
                        println!("{}", p.green("  ✔ Desfeito."));
                        self.historico
                            .push_str("## O dono desfez a última tarefa (os arquivos voltaram ao que eram)\n\n");
                    }
                    Err(e) => println!("{}", p.red(&format!("  Não consegui: {e}"))),
                }
            }
            _ => println!("{}", p.dim("  Nada foi mudado.")),
        }
    }
}

/// Perguntas de sempre, quando a IA não respondeu.
fn perguntas_padrao() -> Vec<Pergunta> {
    let q = |t: &str, o: &[&str]| Pergunta { texto: t.into(), opcoes: o.iter().map(|s| (*s).to_string()).collect() };
    vec![
        q("Em uma frase: o que é este projeto e para quem?", &[]),
        q("O que você quer conseguir agora com ele?", &[]),
        q(
            "Tem algo que os corvos NÃO podem mexer?",
            &["Pode mexer em tudo da pasta", "Não mexer no banco de dados", "Não mexer em arquivos de configuração"],
        ),
        q("Como você confere se ficou certo?", &["Abro no navegador", "Rodo os testes", "Subo no servidor e testo lá"]),
    ]
}

/// Pergunta ao dono no próprio terminal, pela mesma entrada do modo terminal: Ctrl+C
/// numa confirmação para a tarefa inteira.
struct Aprovador {
    entrada: Arc<Entrada>,
    p: Paint,
    parar: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl maos::Aprovador for Aprovador {
    async fn pedir(&self, n: usize, titulo: &str, motivo: &str) -> maos::Resposta {
        let p = self.p;
        println!("\n   {} {titulo}", p.yellow(&format!("⚠ Ação {n}:")));
        println!("     {}", p.dim(&format!("Precisa da sua confirmação: {motivo}.")));
        loop {
            let q = "     Permitir? [s]im · [a] sempre nesta conversa · [n]ão › ";
            match self.entrada.ler(q, &p.yellow(q), false).await {
                Leitura::Linha(l) => match maos::Resposta::parse(&l) {
                    Some(r) => return r,
                    None => println!("{}", p.dim("     responda s, a ou n")),
                },
                Leitura::Interrompido => {
                    self.parar.notify_one();
                    return maos::Resposta::Nao;
                }
                Leitura::Fim => return maos::Resposta::Nao,
            }
        }
    }

    async fn perguntar(&self, pergunta: &str, opcoes: &[String]) -> Option<String> {
        let p = self.p;
        println!("\n   {} {}", p.cyan("❓ Um corvo pergunta:"), p.bold(pergunta));
        for (i, o) in opcoes.iter().enumerate() {
            println!("      {}) {o}", i + 1);
        }
        println!("      {}", p.dim("número escolhe · texto livre vale · Enter = decida você"));
        let q = "   › ";
        match self.entrada.ler(q, &p.cyan(q), false).await {
            Leitura::Linha(l) => {
                let l = l.trim();
                Some(match l.parse::<usize>() {
                    Ok(n) if n >= 1 && n <= opcoes.len() => opcoes[n - 1].clone(),
                    _ => l.to_string(),
                })
            }
            Leitura::Interrompido => {
                self.parar.notify_one();
                None
            }
            Leitura::Fim => None,
        }
    }
}
