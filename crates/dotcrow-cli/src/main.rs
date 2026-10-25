//! `dotcrow`: a linha de comando do DotCrow.
//!
//! Cuida do cofre de chaves, dos provedores de LLM, roda tarefas com os corvos e
//! abre o painel visual (`dotcrow painel`), que também funciona no celular.

mod audio;
mod aviso;
mod busca;
mod cdp;
mod celular;
mod civ;
mod civ_economia;
mod civ_internet;
mod civ_julgamento;
mod civ_memoria;
mod civ_missao;
mod civ_visual;
mod conectores;
mod desfazer;
mod maos;
mod memoria;
mod pacote;
mod painel;
mod remoto;
mod store;
mod terminal;
mod trava;
mod treino;

use std::{io::IsTerminal, path::PathBuf, sync::Arc};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use dotcrow_agents::{
    AGENTS, TaskKind, default_flow,
    orchestrator::{self, Event},
    profile,
};
use dotcrow_providers::{
    CompletionRequest, LlmProvider, Message, ProviderError, Role, Router, catalog, mock::MockProvider,
};
use store::Store;
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(name = "dotcrow", version, about = "DotCrow: agentes open source que trabalham no seu computador")]
struct Cli {
    /// Pasta de dados (padrão: pasta de dados do usuário, ou a variável DOTCROW_HOME).
    #[arg(long, global = true, env = "DOTCROW_HOME")]
    home: Option<PathBuf>,

    /// Sem comando: num terminal, abre o modo terminal na pasta atual; pelo ícone, abre
    /// o painel visual no navegador.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Modo terminal: os corvos trabalham nos arquivos da pasta atual, com perguntas,
    /// navegador de arquivos, @anexos e desfazer. É o que `dotcrow` faz num terminal.
    #[command(visible_aliases = ["t", "chat"])]
    Terminal {
        /// Pasta do projeto (padrão: a pasta atual).
        #[arg(long, short = 'C')]
        pasta: Option<PathBuf>,
        /// computador (pergunta o que é sensível), livre (não pergunta) ou sandbox (caixa isolada).
        #[arg(long)]
        modo: Option<String>,
        /// Usa só este provedor.
        #[arg(long)]
        provedor: Option<String>,
        /// O Huginn não pergunta antes das tarefas.
        #[arg(long)]
        direto: bool,
        /// Um pedido para já começar.
        pedido: Vec<String>,
    },
    /// Abre o painel visual no navegador (também funciona no celular).
    #[command(visible_alias = "panel")]
    Painel {
        /// Porta do painel.
        #[arg(long, default_value_t = 7777)]
        porta: u16,
        /// Libera o acesso pelo celular e outros aparelhos da mesma rede Wi-Fi.
        #[arg(long)]
        rede: bool,
        /// Não abre o navegador sozinho.
        #[arg(long)]
        sem_navegador: bool,
        /// Gera um código de acesso novo (o antigo deixa de funcionar).
        #[arg(long)]
        novo_codigo: bool,
        /// Liga o acesso pelo celular pela internet (fora do Wi-Fi), por um relay.
        #[arg(long)]
        remoto: bool,
        /// Endereço do relay (padrão: wss://dotcrow.si/t). Só vale com --remoto.
        #[arg(long)]
        relay: Option<String>,
    },
    /// Gerencia as chaves de API (só dá para adicionar, listar e apagar).
    #[command(visible_alias = "keys")]
    Chaves {
        #[command(subcommand)]
        acao: ChavesAcao,
    },
    /// Testa as chaves: manda um pedido mínimo para cada provedor e mostra quem respondeu.
    #[command(visible_alias = "test")]
    Testar {
        /// Só este provedor.
        provedor: Option<String>,
    },
    /// Faz uma pergunta direta, sem passar pelos corvos.
    #[command(visible_alias = "ask")]
    Perguntar {
        /// A pergunta.
        #[arg(required = true)]
        texto: Vec<String>,
        /// Usa só este provedor.
        #[arg(long)]
        provedor: Option<String>,
    },
    /// Entrega uma tarefa aos corvos e mostra cada etapa.
    #[command(visible_alias = "task")]
    Tarefa {
        /// A tarefa, do jeito que você pediria a alguém.
        #[arg(required = true)]
        texto: Vec<String>,
        /// Tipo: organizar, operar ou escrever. Se faltar, o Huginn decide.
        #[arg(long)]
        tipo: Option<String>,
        /// Usa só este provedor.
        #[arg(long)]
        provedor: Option<String>,
        /// sandbox (caixa isolada), computador (pergunta o que é sensível) ou livre (não pergunta).
        #[arg(long, default_value = "computador")]
        modo: String,
    },
    /// Mostra e ajusta os provedores de LLM (ordem, modelo, endereço).
    #[command(visible_alias = "providers")]
    Provedores {
        #[command(subcommand)]
        acao: Option<ProvedoresAcao>,
    },
    /// Mostra os agentes e como cada um pensa.
    #[command(visible_alias = "agents")]
    Agentes,
    /// A Civilização dos Corvos (v2): os 10 corvos, cada um com genoma e modelo próprio
    /// (metade gpt-5-nano, metade claude-haiku-5-5). O bando fica salvo e é sempre o mesmo.
    #[command(visible_aliases = ["civ", "bando"])]
    Civilizacao {
        #[command(subcommand)]
        acao: Option<CivAcao>,
        /// Gênese com esta semente (troca o bando salvo).
        #[arg(long, global = true)]
        semente: Option<u64>,
        /// Desenho compacto (uma linha por corvo), bom no Termux estreito.
        #[arg(long)]
        compacto: bool,
        /// Modelos dos corvos, em rodízio: `provedor:modelo,provedor:modelo` (fica salvo).
        /// Padrão: `openai:gpt-5-nano,anthropic:claude-haiku-5-5` (metade em cada).
        #[arg(long, global = true)]
        modelos: Option<String>,
    },
    /// Abre o microfone: você fala, aperta Enter, e o que disse vira um pedido para os
    /// corvos. Ex.: dotcrow audio pra post no Twitter
    #[command(visible_aliases = ["áudio", "microfone", "mic"])]
    Audio {
        /// Para que é o áudio (ex.: "pra post no Twitter"); vai junto com a transcrição.
        contexto: Vec<String>,
        /// Só mostra a transcrição, sem mandar para os corvos.
        #[arg(long)]
        texto: bool,
        /// computador (pergunta o que é sensível), livre (não pergunta) ou sandbox.
        #[arg(long)]
        modo: Option<String>,
    },
    /// Conectores: mais de 200 serviços (X, GitHub, Google Drive, Notion, Shopify, Mercado
    /// Pago, Telegram, ViaCEP...) que os corvos usam pela API oficial, com a chave no cofre.
    /// Ler é livre; publicar, enviar e apagar pedem a sua confirmação.
    #[command(visible_aliases = ["connectors", "integracoes", "integrações"])]
    Conectores {
        #[command(subcommand)]
        acao: Option<ConectoresAcao>,
    },
    /// Mostra o que os corvos conseguem fazer no celular Android (ADB e Termux:API) e
    /// como ligar o que falta.
    #[command(visible_aliases = ["android", "telefone"])]
    Celular,
    /// Mostra o fluxo de uma tarefa entre os agentes.
    #[command(visible_alias = "flow")]
    Fluxo {
        /// Tipo de tarefa: organizar, operar ou escrever.
        #[arg(default_value = "operar")]
        tipo: String,
    },
    /// Demonstrações para quem está conhecendo o projeto.
    Demo {
        #[command(subcommand)]
        qual: DemoQual,
    },
}

#[derive(Subcommand)]
enum CivAcao {
    /// Os corvos conversam entre si, cada um com o seu modelo. Escreva para falar com
    /// eles; `pare` (ou Ctrl+C) encerra.
    #[command(visible_aliases = ["conversa", "falar"])]
    Conversar {
        /// Segundos entre uma fala e outra.
        #[arg(long, default_value_t = 20)]
        intervalo: u64,
        /// Para sozinho depois de tantas falas (padrão: só quando você disser pare).
        #[arg(long)]
        max_falas: Option<u32>,
        /// Tema da conversa (padrão: livre, eles escolhem).
        #[arg(long)]
        tema: Option<String>,
    },
    /// Troca o modelo de um corvo. Ex.: dotcrow civ modelo Nyx anthropic:claude-haiku-5-5
    Modelo {
        /// Nome do corvo (sem acento também vale).
        corvo: String,
        /// provedor:modelo
        modelo: String,
    },
    /// Dá uma nota de 0 a 10 a um corvo. Ex.: dotcrow civ nota Nyx 10
    /// (muda a reputação, o ranking e quem sai para a internet).
    Nota {
        /// Nome do corvo (sem acento também vale).
        corvo: String,
        /// Nota de 0 a 10.
        nota: String,
    },
    /// Julgamento: 3 jurados sorteados e o oráculo (Opus 5.5) dão notas; 1º, 2º e 3º
    /// ganham memória e consultas à internet; os 5 últimos deixam de existir em 24 h.
    #[command(visible_alias = "julgamento")]
    Julgar,
    /// O DotCrow pede a sua nota (0 a 10) para cada corvo.
    Avaliar,
    /// Os corvos premiados usam as consultas à internet (passam pela Alfândega).
    /// Ex.: dotcrow civ pesquisar · dotcrow civ pesquisar Nyx · dotcrow civ pesquisar Nyx vulcões
    Pesquisar {
        /// Corvo e, se quiser, o assunto. Sem nada, cada corvo com consultas escolhe o seu.
        pedido: Vec<String>,
    },
    /// Mostra a memória de um corvo (falas, perguntas, aprendizados e o livro-razão).
    #[command(visible_alias = "alma")]
    Memoria {
        /// Nome do corvo.
        corvo: String,
    },
    /// O Livro dos Mortos: quem já deixou de existir e a lição que deixou.
    Mortos,
    /// O mapa dos terrenos (dono, plantio, colheita, quem está lá) e a economia: energia,
    /// dotcoins, comida e sementes de cada corvo, ofertas e o mercado.
    #[command(visible_aliases = ["mapa", "economia"])]
    Terrenos,
    /// Um corvo sai da conversa e FAZ algo, com mãos: terminal, arquivos, internet,
    /// navegador, mouse e teclado, a tela do celular, ligações e SMS (o irreversível pede
    /// a sua confirmação). Ex.: dotcrow civ missao Nyx poste no X o resumo do dia
    #[command(visible_aliases = ["missão", "fazer"])]
    Missao {
        /// Nome do corvo (ou "auto": o citado no pedido ou o de melhor reputação).
        corvo: String,
        /// O que fazer.
        #[arg(required = true)]
        pedido: Vec<String>,
        /// computador (pergunta o que é sensível), livre (não pergunta) ou sandbox.
        #[arg(long, default_value = "computador")]
        modo: String,
    },
    /// Nova Gênese: 10 corvos novos (o rodízio de modelos continua o mesmo).
    Nova,
    /// Pausa a civilização: sem conversa, julgamento nem missões, e o relógio das 24 h
    /// para. O DotCrow segue com os 6 corvos.
    #[command(visible_alias = "parar")]
    Pausar,
    /// Retoma a civilização pausada (os prazos somam o tempo parado).
    #[command(visible_aliases = ["continuar", "voltar"])]
    Retomar,
}

#[derive(Subcommand)]
enum ConectoresAcao {
    /// Lista o catálogo (ou busca por nome e categoria). Ex.: dotcrow conectores listar brasil
    #[command(visible_aliases = ["ls", "buscar"])]
    Listar {
        /// O que buscar (nome, id ou categoria).
        busca: Vec<String>,
    },
    /// Mostra o endereço da API, como vai a chave e onde pegar.
    Ver {
        /// Id do conector (ex.: github).
        id: String,
    },
    /// Liga um conector: guarda a chave no cofre (sem aparecer na tela) e a conta, se precisar.
    #[command(visible_alias = "add")]
    Adicionar {
        /// Id do conector (ex.: github, x, google, shopify).
        id: String,
        /// Domínio da sua conta, para serviços como Shopify, Jira e WordPress.
        #[arg(long)]
        conta: Option<String>,
        /// Rótulo opcional da chave.
        #[arg(long)]
        rotulo: Option<String>,
    },
    /// Faz uma leitura de teste no serviço.
    #[command(visible_alias = "test")]
    Testar {
        /// Id do conector.
        id: String,
    },
    /// Desliga um conector (apaga a chave e a conta).
    #[command(visible_alias = "rm")]
    Remover {
        /// Id do conector.
        id: String,
    },
}

#[derive(Subcommand)]
enum ChavesAcao {
    /// Adiciona uma chave. Ela é lida sem aparecer na tela e nunca mais é mostrada.
    #[command(visible_alias = "add")]
    Adicionar {
        /// Provedor: anthropic, openai, gemini, openrouter, deepseek, groq...
        provedor: String,
        /// Rótulo opcional, como "conta pessoal".
        #[arg(long)]
        rotulo: Option<String>,
    },
    /// Lista as chaves (provedor, rótulo, data e impressão digital). Nunca mostra a chave.
    #[command(visible_alias = "list")]
    Listar,
    /// Apaga uma chave pelo id mostrado em `listar`.
    #[command(visible_alias = "rm")]
    Apagar {
        /// Id da chave.
        id: String,
    },
}

#[derive(Subcommand)]
enum ProvedoresAcao {
    /// Define a ordem de preferência, separada por vírgulas. Ex.: anthropic,gemini,openai
    Ordem {
        /// Lista de provedores.
        lista: String,
    },
    /// Troca o modelo de um provedor. Use "padrao" para voltar ao modelo padrão.
    Modelo {
        /// Provedor.
        provedor: String,
        /// Nome do modelo, como claude-haiku-5-5.
        modelo: String,
    },
    /// Troca o endereço da API (ex.: Ollama em outra máquina). Use "padrao" para voltar.
    Url {
        /// Provedor.
        provedor: String,
        /// Endereço, como http://192.168.0.10:11434/v1
        url: String,
    },
}

#[derive(Subcommand)]
enum DemoQual {
    /// Simula a troca automática quando a API de um provedor acaba.
    #[command(visible_alias = "failover")]
    Troca,
}

/// Cores no terminal, só quando a saída é um terminal de verdade.
#[derive(Clone, Copy)]
pub(crate) struct Paint {
    pub(crate) on: bool,
}

impl Paint {
    fn new() -> Self {
        Self { on: std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() }
    }
    fn wrap(&self, code: &str, s: &str) -> String {
        if self.on { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
    }
    fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }
    fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }
    fn green(&self, s: &str) -> String {
        self.wrap("32", s)
    }
    fn red(&self, s: &str) -> String {
        self.wrap("31", s)
    }
    fn yellow(&self, s: &str) -> String {
        self.wrap("33", s)
    }
    fn cyan(&self, s: &str) -> String {
        self.wrap("36", s)
    }
    /// Texto numa cor da paleta de 256 (degrada para texto puro sem cor).
    fn c256(&self, code: u8, s: &str) -> String {
        self.wrap(&format!("38;5;{code}"), s)
    }
    /// Igual a `c256`, mas em negrito.
    fn c256b(&self, code: u8, s: &str) -> String {
        self.wrap(&format!("1;38;5;{code}"), s)
    }
}

fn read_secret() -> Result<Zeroizing<String>> {
    if std::io::stdin().is_terminal() {
        let s = rpassword::prompt_password("Cole a chave (ela não vai aparecer): ")?;
        Ok(Zeroizing::new(s))
    } else {
        let mut s = String::new();
        std::io::stdin().read_line(&mut s)?;
        Ok(Zeroizing::new(s))
    }
}

fn when(ts: u64) -> String {
    // Data simples em UTC sem dependências extras: AAAA-MM-DD.
    let days = (ts / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

// Algoritmo de Howard Hinnant (domínio público) para converter dias em data civil.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn parse_kind(tipo: &str) -> Result<TaskKind> {
    TaskKind::parse(tipo)
        .ok_or_else(|| anyhow::anyhow!("tipo desconhecido: {tipo} (use organizar, operar ou escrever)"))
}

fn ensure_router(router: &Router) -> Result<()> {
    if router.order().is_empty() {
        bail!(
            "nenhum provedor ativo. Adicione uma chave, por exemplo:\n  dotcrow chaves adicionar anthropic\nou abra o painel: dotcrow painel"
        );
    }
    Ok(())
}

async fn demo_troca() -> Result<()> {
    println!("Simulação: Claude (anthropic) está sem créditos, GPT (openai) está no limite, Gemini responde.\n");
    let providers: Vec<Arc<dyn LlmProvider>> = vec![
        MockProvider::failing("anthropic", ProviderError::QuotaExhausted),
        MockProvider::failing("openai", ProviderError::RateLimited { retry_after: None }),
        MockProvider::ok("gemini"),
    ];
    let router = Router::new(providers);
    let req = CompletionRequest {
        messages: vec![Message { role: Role::User, content: "organize minha pasta Downloads".into() }],
        max_tokens: 256,
        tier: dotcrow_providers::Tier::Standard,
    };
    for rodada in 1..=2 {
        let out = router.complete(&req).await?;
        println!("Pedido {rodada}:");
        for a in &out.attempts {
            match &a.error {
                Some(e) => println!("  ✗ {:<10} {e} → trocando", a.provider),
                None => println!("  ↷ {:<10} de castigo, pulado", a.provider),
            }
        }
        println!("  ✓ {:<10} {}\n", out.provider, out.response.text);
    }
    Ok(())
}

pub(crate) async fn testar(store: &Store, only: Option<&str>) -> Result<()> {
    let p = Paint::new();
    let targets = store.test_targets(only);
    if targets.is_empty() {
        println!("Nenhuma chave para testar. Adicione uma com: dotcrow chaves adicionar anthropic");
        return Ok(());
    }
    println!("Testando {} provedor(es)...\n", targets.len());
    let mut ok = 0;
    for id in &targets {
        let r = store.test_provider(id).await;
        if r.ok {
            ok += 1;
            println!("  {} {:<11} {}  {}", p.green("✓"), id, r.model, p.dim(&format!("{} ms", r.ms)));
        } else {
            println!("  {} {:<11} {}", p.red("✗"), id, r.error.unwrap_or_default());
            if let Some(h) = r.hint {
                println!("    {}", p.dim(&h));
            }
        }
    }
    println!("\n{ok} de {} funcionando.", targets.len());
    Ok(())
}

/// `dotcrow celular`: o que dá para fazer no celular e como ligar o resto.
pub(crate) async fn celular_status() -> Result<()> {
    let p = Paint::new();
    println!("\n  {}\n", p.bold("📱 Mãos no celular"));
    for (nome, ok, detalhe) in celular::diagnostico().await {
        let marca = if ok { p.green("✓") } else { p.yellow("·") };
        println!("  {marca} {nome}\n      {}", p.dim(&detalhe));
    }
    println!(
        "\n  {}\n{}",
        p.bold("Para os corvos tocarem, rolarem, digitarem e apertarem botões (ADB, uma vez só):"),
        p.dim(
            "    1. pkg install android-tools termux-api   (e o app Termux:API, do F-Droid)\n\
    2. Configurações → Sobre o telefone → toque 7 vezes em \"Número da versão\" (liga as Opções do desenvolvedor)\n\
    3. Opções do desenvolvedor → Depuração sem fio → ligue → \"Parear o dispositivo com um código\"\n\
    4. Com a tela de pareamento aberta (tela dividida ajuda): adb pair IP:PORTA CÓDIGO\n\
    5. Depois, com o IP:PORTA da tela da Depuração sem fio: adb connect IP:PORTA\n\
    6. Confira com: dotcrow celular   (com mais de um aparelho: export ANDROID_SERIAL=IP:PORTA)\n\
    No PC: celular no cabo USB, Depuração USB ligada e o platform-tools (adb) instalado."
        )
    );
    println!(
        "\n  {}\n",
        p.dim(
            "Ligar, SMS e tocar em publicar/enviar/pagar/apagar sempre pedem a sua confirmação, mesmo no modo livre."
        )
    );
    Ok(())
}

/// Pergunta ao dono no próprio terminal.
struct TerminalAprovador;

#[async_trait::async_trait]
impl maos::Aprovador for TerminalAprovador {
    async fn pedir(&self, n: usize, titulo: &str, motivo: &str) -> maos::Resposta {
        let pergunta = format!(
            "\n   ⚠ Ação {n}: {titulo}\n     Precisa da sua confirmação: {motivo}.\n     Permitir? [s]im / [a] sempre nesta tarefa / [n]ão: "
        );
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            if !std::io::stdin().is_terminal() {
                return maos::Resposta::Nao;
            }
            print!("{pergunta}");
            let _ = std::io::stdout().flush();
            let mut linha = String::new();
            let _ = std::io::stdin().read_line(&mut linha);
            maos::Resposta::parse(&linha).unwrap_or(maos::Resposta::Nao)
        })
        .await
        .unwrap_or(maos::Resposta::Nao)
    }
}

async fn tarefa(store: &Store, texto: &str, tipo: Option<&str>, provedor: Option<&str>, modo: &str) -> Result<()> {
    let p = Paint::new();
    let kind = tipo.map(parse_kind).transpose()?;
    let modo = maos::Modo::parse(modo).ok_or_else(|| anyhow::anyhow!("modo: sandbox, computador ou livre"))?;
    let router = store.router(provedor)?;
    ensure_router(&router)?;
    let pasta = maos::nova_pasta(texto)?;
    println!("{} {}", p.bold("Tarefa:"), texto);
    println!("{}\n", p.dim(&format!("Pasta da conversa: {} · modo {}", pasta.display(), modo.as_str())));
    let hands =
        maos::Maos::new(modo, pasta, store.dir().to_path_buf(), Arc::new(TerminalAprovador), Default::default())
            .com_conectores(conectores::Conexoes::do_store(store));
    let mut render = terminal::Render::new(p);
    let print = |e: Event| render.evento(e);
    let memoria = store.master_key().ok().and_then(|k| memoria::Memoria::abrir(store.dir(), k).ok());
    let oraculo = store.oraculo();
    let input = orchestrator::TaskInput {
        request: texto,
        kind,
        hands: Some(&hands),
        history: None,
        memory: memoria.as_ref().map(|m| m as &dyn dotcrow_agents::Memory),
        inbox: None,
        oraculo: orchestrator::Oraculo::Router(oraculo.as_ref().unwrap_or(&router)),
    };
    orchestrator::run(&router, input, print).await?;
    Ok(())
}

fn provedores(store: &Store, acao: Option<&ProvedoresAcao>) -> Result<()> {
    let p = Paint::new();
    let reset = |v: &str| matches!(v.trim().to_lowercase().as_str(), "padrao" | "padrão" | "default" | "");
    match acao {
        Some(ProvedoresAcao::Ordem { lista }) => {
            let ids: Vec<String> = lista.split(',').map(store::canonical_provider).filter(|s| !s.is_empty()).collect();
            if let Some(bad) = ids.iter().find(|id| catalog::info(id).is_none()) {
                bail!("provedor desconhecido: {bad}");
            }
            store.update_config(|c| c.order = ids)?;
            println!("Ordem salva.\n");
        }
        Some(ProvedoresAcao::Modelo { provedor, modelo }) => {
            let id = store::canonical_provider(provedor);
            if catalog::info(&id).is_none() {
                bail!("provedor desconhecido: {id}");
            }
            store.update_config(|c| {
                if reset(modelo) {
                    c.models.remove(&id);
                } else {
                    c.models.insert(id.clone(), modelo.trim().to_string());
                }
            })?;
            println!("Modelo de {id} salvo. Teste com: dotcrow testar {id}\n");
        }
        Some(ProvedoresAcao::Url { provedor, url }) => {
            let id = store::canonical_provider(provedor);
            if catalog::info(&id).is_none() {
                bail!("provedor desconhecido: {id}");
            }
            if !reset(url) && !(url.starts_with("http://") || url.starts_with("https://")) {
                bail!("o endereço precisa começar com http:// ou https://");
            }
            store.update_config(|c| {
                if reset(url) {
                    c.urls.remove(&id);
                } else {
                    c.urls.insert(id.clone(), url.trim().to_string());
                }
            })?;
            println!("Endereço de {id} salvo.\n");
        }
        None => {}
    }
    for s in store.provider_status() {
        let pos = s.position.map(|n| format!("{n}º")).unwrap_or_else(|| "  ".into());
        let state = if s.has_key {
            p.green("chave ok")
        } else if !s.needs_key {
            p.dim("sem chave (local)")
        } else {
            p.dim("sem chave")
        };
        let model = if s.default_model { s.model.clone() } else { format!("{} (escolhido)", s.model) };
        println!("{pos:>3}  {:<11} {:<26} {:<18} {}", s.id, s.name, state, p.dim(&model));
    }
    println!(
        "\n{}",
        p.dim("A coluna da esquerda é a ordem em que o DotCrow tenta. Ajuste com: dotcrow provedores ordem a,b,c")
    );
    for w in store.warnings() {
        println!("{}", p.yellow(&format!("! {w}")));
    }
    Ok(())
}

/// O `dotcrow` sem comando foi digitado num terminal (e não aberto pelo ícone)?
///
/// No Linux, no macOS e no Termux, o ícone roda sem terminal, então basta olhar se a
/// entrada e a saída são um terminal. No Windows, clicar no `dotcrow.exe` ou no atalho
/// também abre uma janela de console; aí só conta como terminal quando veio de um
/// (cmd, PowerShell, Git Bash ou o terminal do VS Code), e o ícone segue abrindo o painel.
/// `DOTCROW_TERMINAL=1` força o modo terminal e `DOTCROW_TERMINAL=0` força o painel.
fn aberto_no_terminal() -> bool {
    match std::env::var("DOTCROW_TERMINAL").as_deref() {
        Ok("1") => return true,
        Ok("0") => return false,
        _ => {}
    }
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return false;
    }
    if cfg!(target_os = "windows") {
        let tem = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
        // O PowerShell põe a pasta de módulos do usuário no PSModulePath dos filhos;
        // aberto pelo Explorer, o PSModulePath é só o do sistema.
        let do_powershell = match (std::env::var("PSModulePath"), std::env::var("USERPROFILE")) {
            (Ok(m), Ok(u)) if !u.is_empty() => m.to_lowercase().contains(&u.to_lowercase()),
            _ => false,
        };
        return tem("PROMPT") || tem("MSYSTEM") || tem("TERM_PROGRAM") || do_powershell;
    }
    true
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    // O ícone roda `dotcrow` sem comando e sem terminal: aí um erro precisa aparecer na tela.
    let pelo_icone = cli.command.is_none() && !std::io::stderr().is_terminal();
    // A Civilização roda trancada na pasta DotCrow. A trava vem antes de qualquer thread
    // (o runtime do tokio nasce depois), para valer para todas.
    trancar_a_civilizacao(&cli);
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Erro: não consegui iniciar o DotCrow: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match rt.block_on(run(cli)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Erro: {e:?}");
            if pelo_icone {
                aviso::erro_na_tela(&format!("{e:#}"));
            }
            std::process::ExitCode::FAILURE
        }
    }
}

/// No modo Civilização (conversar, julgar, pesquisar e missão), o processo só enxerga
/// `Downloads/DotCrow` e a pasta de dados do DotCrow (Linux, Landlock). Veja [`trava`].
fn trancar_a_civilizacao(cli: &Cli) {
    let Some(Command::Civilizacao {
        acao: Some(CivAcao::Conversar { .. } | CivAcao::Julgar | CivAcao::Pesquisar { .. } | CivAcao::Missao { .. }),
        ..
    }) = &cli.command
    else {
        return;
    };
    let Ok(dados) = cli.home.clone().map(Ok).unwrap_or_else(store::default_data_dir) else { return };
    let p = Paint::new();
    let pasta = civ::pasta(&dados);
    let raiz = civ::raiz_da_sandbox();
    let livres = vec![raiz.clone(), dados.clone(), pasta];
    match trava::aplicar(&livres, &raiz.join("tmp")) {
        trava::Situacao::Completa | trava::Situacao::Parcial => eprintln!(
            "{}",
            p.dim(&format!(
                "🔒 Sandbox trancada (Landlock): só {} e {} (o sistema só para leitura).",
                raiz.display(),
                dados.display()
            ))
        ),
        trava::Situacao::Indisponivel(motivo) => {
            eprintln!("{}", p.yellow(&format!("⚠ Sem a trava de pastas: {motivo}. A civilização roda sem ela.")))
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let dir = match &cli.home {
        Some(h) => h.clone(),
        None => store::default_data_dir()?,
    };
    let command = match cli.command {
        Some(c) => c,
        // Digitou `dotcrow` num terminal: modo terminal. Pelo ícone: painel, como sempre.
        None if aberto_no_terminal() => {
            Command::Terminal { pasta: None, modo: None, provedor: None, direto: false, pedido: Vec::new() }
        }
        None => Command::Painel {
            porta: 7777,
            rede: false,
            sem_navegador: false,
            novo_codigo: false,
            remoto: false,
            relay: None,
        },
    };
    match &command {
        Command::Agentes => {
            for a in &AGENTS {
                println!("{} · {}", a.name, a.role);
                println!("  pensa:    {}", a.thinks);
                println!("  organiza: {}\n", a.organizes);
            }
            return Ok(());
        }
        Command::Fluxo { tipo } => {
            for (i, s) in default_flow(parse_kind(tipo)?).iter().enumerate() {
                println!("{}. {:<7} {}", i + 1, profile(s.agent).name, s.what);
            }
            return Ok(());
        }
        Command::Demo { qual: DemoQual::Troca } => return demo_troca().await,
        Command::Celular => return celular_status().await,
        Command::Civilizacao { acao, semente, compacto, modelos } => {
            let p = Paint::new();
            let pasta = civ::pasta(&dir);
            match acao {
                None => {
                    let e = civ::abrir_bando(&pasta, *semente, modelos.as_deref(), &p)?;
                    civ::mostrar(&e, *compacto, &p)?;
                    println!("{}", p.dim(&format!("  Pasta da civilização: {}\n", pasta.display())));
                    return Ok(());
                }
                Some(CivAcao::Nova) => {
                    if let Some(m) = modelos.as_deref() {
                        civ::abrir(&pasta, None, Some(m))?;
                    }
                    let e = civ::nova(&pasta, *semente)?;
                    println!(
                        "{}",
                        p.green("Nova Gênese: 10 corvos novos (a memória dos anteriores foi para mortos/).")
                    );
                    civ::mostrar(&e, true, &p)?;
                    return Ok(());
                }
                Some(CivAcao::Modelo { corvo, modelo }) => {
                    let (nome, m) = civ::trocar_modelo(&pasta, corvo, modelo)?;
                    println!("{} agora fala com {}:{}.", p.bold(&nome), m.provedor, m.modelo);
                    return Ok(());
                }
                Some(CivAcao::Nota { corvo, nota }) => {
                    let (nome, antes, depois, e) = civ::dar_nota(&pasta, &format!("{corvo} {nota}"))?;
                    println!("{} {}", p.bold(&nome), p.yellow(&format!("★ {antes:.1} → ★ {depois:.1}")));
                    println!("{}", p.dim(&format!("Pódio: {}", civ::podio(&e.civilizacao()?))));
                    return Ok(());
                }
                Some(CivAcao::Memoria { corvo }) => {
                    let e = civ::abrir_bando(&pasta, None, None, &p)?;
                    civ::mostrar_memoria(&pasta, &e, corvo, &p)?;
                    return Ok(());
                }
                Some(CivAcao::Mortos) => {
                    let e = civ::abrir_bando(&pasta, None, None, &p)?;
                    civ::mostrar_mortos(&e, &p)?;
                    return Ok(());
                }
                Some(CivAcao::Terrenos) => {
                    let e = civ::abrir_bando(&pasta, None, None, &p)?;
                    civ::mostrar_terrenos(&e, &p);
                    return Ok(());
                }
                Some(CivAcao::Pausar) => {
                    let mut e = civ::abrir_bando(&pasta, None, None, &p)?;
                    if civ::pausar(&pasta, &mut e)? {
                        println!("{}", p.yellow("⏸ Civilização pausada. O DotCrow segue com os 6 corvos e o Oráculo."));
                        println!("{}", p.dim("Os prazos de 24 h param. Para voltar: dotcrow civ retomar"));
                    } else {
                        println!("{}", p.dim("A civilização já estava pausada. Para voltar: dotcrow civ retomar"));
                    }
                    return Ok(());
                }
                Some(CivAcao::Retomar) => {
                    let mut e = civ::abrir_bando(&pasta, None, None, &p)?;
                    match civ::retomar(&pasta, &mut e)? {
                        Some(s) => println!(
                            "{}",
                            p.green(&format!(
                                "▶ Civilização de volta (ficou pausada {} min; os prazos somaram esse tempo).",
                                s / 60
                            ))
                        ),
                        None => println!("{}", p.dim("A civilização não estava pausada.")),
                    }
                    return Ok(());
                }
                Some(CivAcao::Avaliar) => {
                    let mut e = civ::abrir_bando(&pasta, None, None, &p)?;
                    let mut rx = civ::teclado_stdin();
                    civ::pedir_notas_do_dono(&pasta, &mut e, &p, &mut rx).await?;
                    return Ok(());
                }
                Some(
                    CivAcao::Conversar { .. } | CivAcao::Julgar | CivAcao::Pesquisar { .. } | CivAcao::Missao { .. },
                ) => {}
            }
        }
        _ => {}
    }

    let store = Store::open(dir)?;
    match &command {
        Command::Chaves { acao } => match acao {
            ChavesAcao::Adicionar { provedor, rotulo } => {
                let secret = read_secret()?;
                let info = store.add_key(provedor, secret, rotulo.clone())?;
                if info.provider != provedor.trim().to_lowercase() {
                    println!("Guardei como \"{}\", que é o nome que o DotCrow usa.", info.provider);
                }
                println!("Chave guardada. id {}  impressão digital {}", info.id, info.fingerprint);
                println!("Ela não pode ser exibida. Para trocar, apague e adicione outra.");
                if catalog::info(&info.provider).is_none() {
                    println!(
                        "Atenção: \"{}\" não é um provedor que o DotCrow conhece; a chave não será usada.",
                        info.provider
                    );
                } else {
                    println!("Teste agora com: dotcrow testar {}", info.provider);
                }
            }
            ChavesAcao::Listar => {
                let keys = store.list_keys();
                if keys.is_empty() {
                    println!("Nenhuma chave ainda. Use: dotcrow chaves adicionar anthropic");
                }
                for k in keys {
                    println!(
                        "{}  {:<11} {:<20} {}  adicionada em {}",
                        k.id,
                        k.provider,
                        k.label.as_deref().unwrap_or("-"),
                        k.fingerprint,
                        when(k.created_at)
                    );
                }
            }
            ChavesAcao::Apagar { id } => {
                let info = store.delete_key(id)?;
                println!("Chave {} ({}) apagada.", info.id, info.provider);
            }
        },
        Command::Testar { provedor } => testar(&store, provedor.as_deref()).await?,
        Command::Perguntar { texto, provedor } => {
            let router = store.router(provedor.as_deref())?;
            ensure_router(&router)?;
            let out = orchestrator::quick_answer(&router, &texto.join(" ")).await?;
            let p = Paint::new();
            println!("{}\n", out.response.text.trim());
            for a in &out.attempts {
                if let Some(e) = &a.error {
                    println!("{}", p.yellow(&format!("↻ {}: {e} → troquei de provedor", a.provider)));
                }
            }
            println!("{}", p.dim(&format!("via {} · {}", out.provider, out.response.model)));
        }
        Command::Tarefa { texto, tipo, provedor, modo } => {
            tarefa(&store, &texto.join(" "), tipo.as_deref(), provedor.as_deref(), modo).await?
        }
        Command::Provedores { acao } => provedores(&store, acao.as_ref())?,
        Command::Conectores { acao } => {
            let p = Paint::new();
            match acao {
                None => conectores::listar(&store, "", &p),
                Some(ConectoresAcao::Listar { busca }) => conectores::listar(&store, &busca.join(" "), &p),
                Some(ConectoresAcao::Ver { id }) => conectores::ver(&store, id, &p)?,
                Some(ConectoresAcao::Testar { id }) => conectores::testar(&store, id, &p).await?,
                Some(ConectoresAcao::Remover { id }) => {
                    let c = conectores::achar(id).ok_or_else(|| anyhow::anyhow!("não conheço o conector \"{id}\""))?;
                    let n = conectores::remover(&store, c)?;
                    println!("{} desligado ({n} chave(s) apagada(s)).", c.nome);
                    if !c.chave.is_empty() {
                        println!("Ele usava a chave de {}, que vale para outros conectores também.", c.chave);
                    }
                }
                Some(ConectoresAcao::Adicionar { id, conta, rotulo }) => {
                    let c = conectores::achar(id)
                        .ok_or_else(|| anyhow::anyhow!("não conheço o conector \"{id}\" (veja: dotcrow conectores)"))?;
                    if !c.dica.is_empty() {
                        println!("{} {}", p.bold(c.nome), p.dim(&format!("· {}", c.dica)));
                    }
                    let segredo = if c.precisa_chave() {
                        println!("{}", p.dim(&format!("Formato: {}.", c.formato())));
                        Some(read_secret()?)
                    } else {
                        None
                    };
                    conectores::adicionar(&store, c, conta.as_deref(), segredo, rotulo.clone())?;
                    println!("{}", p.green(&format!("{} ligado. A chave fica no cofre e nunca é mostrada.", c.nome)));
                    if c.teste.is_some() {
                        println!("Teste agora com: dotcrow conectores testar {}", c.id);
                    }
                }
            }
        }
        Command::Terminal { pasta, modo, provedor, direto, pedido } => {
            let modo = modo
                .as_deref()
                .map(|m| maos::Modo::parse(m).ok_or_else(|| anyhow::anyhow!("modo: sandbox, computador ou livre")))
                .transpose()?;
            let opcoes = terminal::Opcoes {
                pasta: pasta.clone(),
                modo,
                provedor: provedor.clone(),
                direto: *direto,
                pedido: (!pedido.is_empty()).then(|| pedido.join(" ")),
            };
            return terminal::rodar(store, opcoes).await;
        }
        Command::Painel { porta, rede, sem_navegador, novo_codigo, remoto, relay } => {
            painel::serve(
                Arc::new(store),
                painel::Options {
                    port: *porta,
                    lan: *rede,
                    open_browser: !sem_navegador,
                    new_code: *novo_codigo,
                    remoto: *remoto,
                    relay: relay.clone(),
                },
            )
            .await?
        }
        Command::Civilizacao {
            acao: Some(CivAcao::Conversar { intervalo, max_falas, tema }),
            semente,
            modelos,
            ..
        } => {
            let p = Paint::new();
            let pasta = civ::pasta(store.dir());
            let e = civ::abrir_bando(&pasta, *semente, modelos.as_deref(), &p)?;
            let o = civ::Opcoes {
                intervalo: std::time::Duration::from_secs((*intervalo).max(1)),
                max_falas: *max_falas,
                tema: tema.clone(),
            };
            civ::conversar(&store, &pasta, &e, o, &p, civ::teclado_stdin()).await?;
        }
        Command::Civilizacao { acao: Some(CivAcao::Julgar), .. } => {
            let p = Paint::new();
            let pasta = civ::pasta(store.dir());
            let mut e = civ::abrir_bando(&pasta, None, None, &p)?;
            civ_julgamento::julgar(&store, &pasta, &mut e, &p).await?;
        }
        Command::Civilizacao { acao: Some(CivAcao::Missao { corvo, pedido, modo }), .. } => {
            let p = Paint::new();
            let modo = maos::Modo::parse(modo).ok_or_else(|| anyhow::anyhow!("modo: sandbox, computador ou livre"))?;
            let pasta = civ::pasta(store.dir());
            let mut e = civ::abrir_bando(&pasta, None, None, &p)?;
            let (r, _) = civ_missao::executar(
                &store,
                &pasta,
                &mut e,
                corvo,
                &pedido.join(" "),
                None,
                modo,
                civ::teclado_stdin(),
                &p,
            )
            .await;
            let (quem, entrega) = r?;
            println!("\n{} {}\n", p.bold(&format!("Entrega de {quem}:")), entrega.trim());
            civ::salvar(&pasta, &e)?;
        }
        Command::Audio { contexto, texto, modo } => {
            let p = Paint::new();
            let parar = async {
                tokio::select! {
                    l = tokio::task::spawn_blocking(|| {
                        let mut s = String::new();
                        std::io::stdin().read_line(&mut s).is_ok()
                    }) => l.unwrap_or(true),
                    _ = tokio::signal::ctrl_c() => false,
                }
            };
            let Some(fala) = audio::ouvir_o_dono(&store, parar, &p).await? else { return Ok(()) };
            if *texto {
                return Ok(());
            }
            let modo = modo
                .as_deref()
                .map(|m| maos::Modo::parse(m).ok_or_else(|| anyhow::anyhow!("modo: sandbox, computador ou livre")))
                .transpose()?;
            let opcoes = terminal::Opcoes {
                pasta: None,
                modo,
                provedor: None,
                direto: false,
                pedido: Some(audio::pedido_do_audio(&contexto.join(" "), &fala)),
            };
            return terminal::rodar(store, opcoes).await;
        }
        Command::Civilizacao { acao: Some(CivAcao::Pesquisar { pedido }), .. } => {
            let p = Paint::new();
            let pasta = civ::pasta(store.dir());
            let mut e = civ::abrir_bando(&pasta, None, None, &p)?;
            civ_internet::pesquisar_pedido(&store, &pasta, &mut e, &pedido.join(" "), &p).await?;
        }
        Command::Agentes
        | Command::Fluxo { .. }
        | Command::Demo { .. }
        | Command::Celular
        | Command::Civilizacao { .. } => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(when(0), "1970-01-01");
        assert_eq!(when(1_792_368_000), "2026-10-19");
        assert_eq!(when(951_782_400), "2000-02-29");
    }
}
