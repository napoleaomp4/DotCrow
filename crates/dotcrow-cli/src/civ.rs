//! `dotcrow civilizacao`: os 10 corvos da Civilização (v2) no terminal.
//!
//! - `dotcrow civ` mostra o bando (a população viva fica salva e é sempre a mesma).
//! - `dotcrow civ conversar` faz os corvos conversarem de verdade, cada um com o seu
//!   modelo e com fala de tamanho sorteado (1 a 300 palavras); você escreve no meio;
//!   `pare` encerra. A cada 200 falas o DotCrow pede as suas notas e roda um julgamento.
//! - `dotcrow civ julgar`: 3 jurados sorteados + o oráculo (Opus 5.5) + as suas notas →
//!   ranking; 1º, 2º e 3º ganham memória e consultas à internet; os 5 últimos deixam de
//!   existir em 24 h e nascem 5 novos.
//! - `dotcrow civ nota`, `avaliar`, `pesquisar`, `memoria`, `mortos`, `modelo`, `nova`.
//!
//! Tudo fica na Downloads (`DotCrow/civilizacao/`): o estado da civilização, a memória
//! de cada corvo (com livro-razão por hash) e a alma dos que morreram. Sobrevive a
//! desligar o celular.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use dotcrow_agents::civilizacao::{
    Civilizacao, Corvo, Modelo, RAG_INICIAL, Rng, Tamanho, conversa, conversa::Fala, cor_provedor, desenhar,
};
use dotcrow_providers::{CompletionRequest, LlmProvider, Message, Role, Tier, catalog};
use serde::{Deserialize, Serialize};

use crate::Paint;
use crate::civ_memoria::{self, Memoria, agora};
use crate::store::Store;

/// Modelos padrão dos corvos: metade em cada um (os mais baratos de cada provedor).
pub const MODELOS_PADRAO: &str = "openai:gpt-5-nano,anthropic:claude-haiku-5-5";
/// O oráculo e a Alfândega usam o Opus 5.5.
pub const OPUS: &str = "claude-opus-5-5";
/// A cada quantas falas o DotCrow pede as notas do dono (e roda um julgamento).
pub const AVALIAR_A_CADA: u64 = 200;
const ARQUIVO: &str = "civilizacao.json";

/// Uma nota dada por um jurado ou pelo oráculo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Avaliacao {
    /// Quem deu: `jurado:<nome>` ou `oraculo`.
    pub de: String,
    /// A nota, de 1 a 10.
    pub nota: u8,
    /// Quando (segundos Unix).
    pub quando: u64,
}

/// O que fica salvo da civilização entre uma execução e outra.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Estado {
    /// Semente da Gênese.
    pub semente: u64,
    /// Modelos em rodízio, `provedor:modelo`.
    pub modelos: Vec<String>,
    /// Modelo escolhido para um corvo específico (vence o rodízio).
    #[serde(default)]
    pub por_corvo: BTreeMap<String, String>,
    /// Notas (0 a 10) que o dono deu a cada corvo. Mudam a reputação e o ranking.
    #[serde(default)]
    pub notas: BTreeMap<String, Vec<u8>>,
    /// Notas de jurados e do oráculo.
    #[serde(default)]
    pub avaliacoes: BTreeMap<String, Vec<Avaliacao>>,
    /// Notas do dono desde o último julgamento (entram no próximo ranking).
    #[serde(default)]
    pub notas_pendentes: BTreeMap<String, Vec<u8>>,
    /// A população viva (com memória em MB, consultas e prazos). Antes do primeiro
    /// julgamento é a Gênese da `semente`.
    #[serde(default)]
    pub populacao: Option<Civilizacao>,
    /// Quantas falas os corvos já fizeram (a cada 200, o dono dá notas).
    #[serde(default)]
    pub falas_total: u64,
    /// Quantos julgamentos já houve.
    #[serde(default)]
    pub julgamentos: u32,
    /// Desde quando a civilização está pausada. Pausada, ninguém conversa, não há
    /// julgamento nem missão e o relógio das 24 h para; o DotCrow segue com os 6 corvos.
    #[serde(default)]
    pub pausada_desde: Option<u64>,
}

impl Estado {
    fn base(&self) -> Result<Civilizacao> {
        match &self.populacao {
            Some(c) => Ok(c.clone()),
            None => Ok(Civilizacao::genese(self.semente, &ler_modelos(&self.modelos.join(","))?)),
        }
    }

    /// A civilização como ela está: modelos do rodízio (ou o escolhido para cada corvo)
    /// e reputação = média da nascença com todas as notas (dono, jurados e oráculo).
    pub fn civilizacao(&self) -> Result<Civilizacao> {
        let modelos = ler_modelos(&self.modelos.join(","))?;
        let mut civ = self.base()?;
        for (i, c) in civ.corvos.iter_mut().enumerate() {
            if let Some(m) = modelos.get(i % modelos.len()) {
                c.definir_modelo(m.clone());
            }
        }
        for (nome, m) in &self.por_corvo {
            let modelo = ler_modelo(m)?;
            if let Some(c) = civ.por_nome_mut(nome) {
                c.definir_modelo(modelo);
            }
        }
        for c in &mut civ.corvos {
            let mut todas: Vec<u8> = self.notas.get(&c.nome).cloned().unwrap_or_default();
            todas.extend(
                self.avaliacoes.get(&c.nome).map(|v| v.iter().map(|a| a.nota).collect::<Vec<_>>()).unwrap_or_default(),
            );
            if !todas.is_empty() {
                c.reputacao = reputacao_com_notas(c.reputacao, &todas);
            }
        }
        Ok(civ)
    }

    /// A população salva, para mudar (prêmios, consultas, prazos).
    pub fn populacao_mut(&mut self) -> Result<&mut Civilizacao> {
        if self.populacao.is_none() {
            self.populacao = Some(self.base()?);
        }
        self.populacao.as_mut().context("civilização sem população")
    }

    fn esquecer(&mut self, nome: &str) {
        self.por_corvo.remove(nome);
        self.notas.remove(nome);
        self.avaliacoes.remove(nome);
        self.notas_pendentes.remove(nome);
    }
}

/// Reputação depois das notas: a média entre a reputação de nascença e cada nota dada
/// (a de nascença conta como uma nota).
pub fn reputacao_com_notas(inicial: f32, notas: &[u8]) -> f32 {
    let soma: f32 = notas.iter().map(|n| f32::from(*n)).sum();
    (inicial + soma) / (1 + notas.len()) as f32
}

/// Lê "Nyx 10", "10 Nyx" ou "nyx 7": devolve o índice do corvo e a nota (0 a 10).
pub fn ler_nota(civ: &Civilizacao, texto: &str) -> Result<(usize, u8)> {
    let mut nota = None;
    let mut resto = Vec::new();
    for parte in texto.split_whitespace() {
        match parte.trim_matches(|c: char| !c.is_alphanumeric()).parse::<u8>() {
            Ok(n) if nota.is_none() => nota = Some(n),
            _ => resto.push(parte),
        }
    }
    let Some(nota) = nota.filter(|n| *n <= 10) else {
        bail!("dê uma nota de 0 a 10, ex.: /nota Nyx 10");
    };
    let nome = resto.join(" ");
    let i = conversa::citado(civ, &nome, None)
        .with_context(|| format!("não achei o corvo \"{nome}\". Ex.: /nota Nyx 10 (veja os nomes com dotcrow civ)"))?;
    Ok((i, nota))
}

/// Registra a nota do dono (vale para a reputação e para o próximo julgamento) e
/// devolve (nome, nota, reputação antes, reputação depois).
fn aplicar_nota(e: &mut Estado, civ: &Civilizacao, texto: &str) -> Result<(String, u8, f32, f32)> {
    let (i, nota) = ler_nota(civ, texto)?;
    let nome = civ.corvos[i].nome.clone();
    let antes = civ.corvos[i].reputacao;
    e.notas.entry(nome.clone()).or_default().push(nota);
    e.notas_pendentes.entry(nome.clone()).or_default().push(nota);
    let depois = e.civilizacao()?.corvos.get(i).map(|c| c.reputacao).unwrap_or(antes);
    Ok((nome, nota, antes, depois))
}

fn anotar_nota_na_memoria(pasta: &Path, civ: &Civilizacao, nome: &str, nota: u8) {
    let mut m = Memoria::abrir(pasta, nome);
    m.registrar("nota", &format!("recebeu nota {nota} do dono"));
    let limite = civ.corvos.iter().find(|c| c.nome == nome).map(|c| c.rag_mb).unwrap_or(RAG_INICIAL);
    let _ = m.salvar(pasta, limite);
}

/// Dá uma nota a um corvo e salva. `texto` é "Nyx 10" (a ordem não importa).
pub fn dar_nota(pasta: &Path, texto: &str) -> Result<(String, f32, f32, Estado)> {
    let mut e = abrir(pasta, None, None)?;
    let civ = e.civilizacao()?;
    let (nome, nota, antes, depois) = aplicar_nota(&mut e, &civ, texto)?;
    salvar(pasta, &e)?;
    anotar_nota_na_memoria(pasta, &civ, &nome, nota);
    Ok((nome, antes, depois, e))
}

/// O pódio (os 3 primeiros do ranking) em uma linha.
pub fn podio(civ: &Civilizacao) -> String {
    civ.ranking()
        .into_iter()
        .take(3)
        .map(|c| format!("{} ★ {:.1}", c.nome, c.reputacao))
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(crate) fn relogio() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x00C0_FFEE)
}

/// Lê `provedor:modelo`.
pub fn ler_modelo(item: &str) -> Result<Modelo> {
    let item = item.trim();
    let Some((provedor, modelo)) = item.split_once(':') else {
        bail!("modelo sem provedor: {item} (use provedor:modelo, ex.: openai:gpt-5-nano)");
    };
    let (provedor, modelo) = (provedor.trim().to_lowercase(), modelo.trim());
    if provedor.is_empty() || modelo.is_empty() {
        bail!("modelo incompleto: {item} (use provedor:modelo)");
    }
    if catalog::info(&provedor).is_none() {
        bail!("provedor desconhecido: {provedor}. Veja a lista com: dotcrow provedores");
    }
    Ok(Modelo::novo(&provedor, modelo))
}

/// Lê `provedor:modelo,provedor:modelo` (os corvos recebem os modelos em rodízio).
pub fn ler_modelos(texto: &str) -> Result<Vec<Modelo>> {
    let modelos: Vec<Modelo> =
        texto.split(',').map(str::trim).filter(|s| !s.is_empty()).map(ler_modelo).collect::<Result<_>>()?;
    if modelos.is_empty() {
        bail!("passe ao menos um modelo (ex.: openai:gpt-5-nano)");
    }
    Ok(modelos)
}

/// A pasta da civilização: `Downloads/DotCrow/civilizacao` (no Termux, a Download do
/// celular quando o dono rodou `termux-setup-storage`). Se não der para criar lá, usa a
/// pasta de dados do DotCrow. Traz o arquivo das versões antigas, se houver.
pub fn pasta(dados: &Path) -> PathBuf {
    let alvo = crate::maos::pasta_de_downloads().join("DotCrow").join("civilizacao");
    let p = if std::fs::create_dir_all(&alvo).is_ok() { alvo } else { dados.join("civilizacao") };
    let antigo = dados.join(ARQUIVO);
    if antigo.exists() && !p.join(ARQUIVO).exists() {
        let _ = std::fs::create_dir_all(&p);
        let _ = std::fs::copy(&antigo, p.join(ARQUIVO));
    }
    p
}

fn caminho(pasta: &Path) -> PathBuf {
    pasta.join(ARQUIVO)
}

/// Grava o estado da civilização.
pub fn salvar(pasta: &Path, e: &Estado) -> Result<()> {
    std::fs::create_dir_all(pasta).with_context(|| format!("criando {}", pasta.display()))?;
    std::fs::write(caminho(pasta), serde_json::to_vec_pretty(e)?).context("salvando a civilização")?;
    Ok(())
}

fn carregar(pasta: &Path) -> Option<Estado> {
    let bytes = std::fs::read(caminho(pasta)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Carrega o bando salvo (ou faz a Gênese na primeira vez). `semente` faz uma Gênese
/// com aquela semente; `modelos` troca o rodízio de modelos. O que mudar é salvo.
pub fn abrir(pasta: &Path, semente: Option<u64>, modelos: Option<&str>) -> Result<Estado> {
    let salvo = carregar(pasta);
    let mut mudou = salvo.is_none();
    let mut e = salvo.unwrap_or_else(|| Estado {
        semente: semente.unwrap_or_else(relogio),
        modelos: MODELOS_PADRAO.split(',').map(str::to_string).collect(),
        ..Default::default()
    });
    if let Some(s) = semente
        && s != e.semente
    {
        e = Estado { semente: s, modelos: e.modelos.clone(), ..Default::default() };
        mudou = true;
    }
    if let Some(m) = modelos {
        let lista: Vec<String> = ler_modelos(m)?.iter().map(|x| format!("{}:{}", x.provedor, x.modelo)).collect();
        if lista != e.modelos {
            e.modelos = lista;
            mudou = true;
        }
    }
    if e.populacao.is_none() {
        e.populacao = Some(e.base()?);
        mudou = true;
    }
    if mudou {
        salvar(pasta, &e)?;
    }
    Ok(e)
}

/// Quem passou do prazo de 24 h deixa de existir: a memória vai para `mortos/` (a alma
/// fica guardada), a biografia entra no Livro dos Mortos e nascem novos no lugar.
/// Devolve as mensagens para mostrar ao dono.
pub fn cumprir_prazos(pasta: &Path, e: &mut Estado) -> Result<Vec<String>> {
    // Pausada, o relógio das 24 h não anda.
    if e.pausada_desde.is_some() {
        return Ok(Vec::new());
    }
    let efetiva = e.civilizacao()?;
    let modelos = ler_modelos(&e.modelos.join(","))?;
    let pop = e.populacao_mut()?;
    let vencidos = pop.vencidos(agora());
    if vencidos.is_empty() {
        return Ok(Vec::new());
    }
    // A biografia guarda a reputação com as notas, não só a de nascença.
    for c in &mut pop.corvos {
        if vencidos.contains(&c.nome)
            && let Some(ef) = efetiva.corvos.iter().find(|x| x.nome == c.nome)
        {
            c.reputacao = ef.reputacao;
        }
    }
    let nascidos = pop.substituir(&vencidos, &modelos, relogio());
    let geracao = pop.geracao;
    for nome in &vencidos {
        let m = Memoria::abrir(pasta, nome);
        let _ = m.enterrar(pasta, "deixou de existir 24 h depois de ficar entre os 5 últimos no julgamento");
        e.esquecer(nome);
    }
    for nome in &nascidos {
        let mut m = Memoria::abrir(pasta, nome);
        m.registrar("geracao", &format!("nasceu na geração {geracao}"));
        let _ = m.salvar(pasta, RAG_INICIAL);
    }
    salvar(pasta, e)?;
    Ok(vec![
        format!("⚰ Deixaram de existir: {}.", vencidos.join(", ")),
        format!("🐣 Nasceram (geração {geracao}): {}.", nascidos.join(", ")),
    ])
}

/// Abre o bando, cumpre os prazos vencidos e mostra o que aconteceu.
pub fn abrir_bando(pasta: &Path, semente: Option<u64>, modelos: Option<&str>, p: &Paint) -> Result<Estado> {
    let mut e = abrir(pasta, semente, modelos)?;
    for msg in cumprir_prazos(pasta, &mut e)? {
        println!("  {}", p.yellow(&msg));
    }
    Ok(e)
}

/// Nova Gênese: corvos novos, mesmo rodízio de modelos. A memória dos atuais vai para
/// `mortos/`.
pub fn nova(pasta: &Path, semente: Option<u64>) -> Result<Estado> {
    let velho = abrir(pasta, None, None)?;
    if let Ok(civ) = velho.civilizacao() {
        for c in &civ.corvos {
            let _ = Memoria::abrir(pasta, &c.nome).enterrar(pasta, "nova Gênese");
        }
    }
    let e = Estado { semente: semente.unwrap_or_else(relogio), modelos: velho.modelos.clone(), ..Default::default() };
    salvar(pasta, &e)?;
    abrir(pasta, None, None)
}

/// Troca o modelo de um corvo e salva.
pub fn trocar_modelo(pasta: &Path, corvo: &str, modelo: &str) -> Result<(String, Modelo)> {
    let mut e = abrir(pasta, None, None)?;
    let m = ler_modelo(modelo)?;
    let civ = e.civilizacao()?;
    let i = conversa::citado(&civ, corvo, None)
        .with_context(|| format!("não achei o corvo \"{corvo}\". Veja os nomes com: dotcrow civ --compacto"))?;
    let nome = civ.corvos[i].nome.clone();
    e.por_corvo.insert(nome.clone(), format!("{}:{}", m.provedor, m.modelo));
    salvar(pasta, &e)?;
    Ok((nome, m))
}

fn quanto_falta(ate: u64) -> String {
    let s = ate.saturating_sub(agora());
    if s >= 3600 { format!("{}h{:02}", s / 3600, (s % 3600) / 60) } else { format!("{} min", s / 60) }
}

/// Pausa a civilização. Devolve falso se já estava pausada.
pub fn pausar(pasta: &Path, e: &mut Estado) -> Result<bool> {
    if e.pausada_desde.is_some() {
        return Ok(false);
    }
    e.pausada_desde = Some(agora());
    salvar(pasta, e)?;
    Ok(true)
}

/// Retoma a civilização: os prazos dos condenados andam só o tempo em que ela esteve
/// ativa (somam o tempo parado). Devolve quantos segundos ficou pausada.
pub fn retomar(pasta: &Path, e: &mut Estado) -> Result<Option<u64>> {
    let Some(desde) = e.pausada_desde.take() else { return Ok(None) };
    let parada = agora().saturating_sub(desde);
    for c in &mut e.populacao_mut()?.corvos {
        if let Some(t) = c.condenado_ate.as_mut() {
            *t += parada;
        }
    }
    salvar(pasta, e)?;
    Ok(Some(parada))
}

/// Falha com uma explicação se a civilização estiver pausada.
pub fn exigir_ativa(e: &Estado) -> Result<()> {
    if e.pausada_desde.is_some() {
        bail!(
            "a Civilização está pausada (o DotCrow segue com os 6 corvos). Para voltar: dotcrow civ retomar \
(ou /civ retomar no modo terminal)"
        );
    }
    Ok(())
}

/// Mostra o bando.
pub fn mostrar(e: &Estado, compacto: bool, p: &Paint) -> Result<()> {
    let civ = e.civilizacao()?;
    println!();
    println!(
        "  {}  {}",
        p.bold("A Civilização dos Corvos"),
        p.dim(&format!(
            "geração {} · {} corvos · {} julgamento(s) · {} falas",
            civ.geracao,
            civ.corvos.len(),
            e.julgamentos,
            e.falas_total
        ))
    );
    println!(
        "  {}\n",
        p.dim(&format!("O Opus 5.5 é o oráculo e a Alfândega; os corvos usam, em rodízio: {}.", e.modelos.join(", ")))
    );
    if let Some(desde) = e.pausada_desde {
        println!(
            "  {}\n",
            p.yellow(&format!(
                "⏸ Pausada há {}: sem conversa, julgamento nem missões, e o relógio das 24 h parado. O DotCrow segue \
com os 6 corvos. Retome com: dotcrow civ retomar",
                quanto_falta(agora() + agora().saturating_sub(desde))
            ))
        );
    }
    let tam = if compacto { Tamanho::Compacto } else { Tamanho::Medio };
    for corvo in &civ.corvos {
        println!("{}\n", desenhar(corvo, tam, p.on));
    }
    println!("  {}", p.bold("Situação (melhor primeiro)"));
    for (i, c) in civ.ranking().into_iter().enumerate() {
        let destino = match c.condenado_ate {
            Some(t) => p.red(&format!("⚰ deixa de existir em {}", quanto_falta(t))),
            None => String::new(),
        };
        println!(
            "   {:>2}º {} {}  {}",
            i + 1,
            p.bold(&c.nome),
            p.dim(&format!("★ {:.1} · {:.1} MB · 🔎 {} consulta(s)", c.reputacao, c.rag_mb, c.consultas)),
            destino
        );
    }
    println!();
    println!("  {}", p.bold("Conversar:            dotcrow civ conversar"));
    println!("  {}", p.bold("Julgamento:           dotcrow civ julgar"));
    println!("  {}", p.dim("Dar nota (0 a 10):    dotcrow civ nota <corvo> <nota>  ·  todas: dotcrow civ avaliar"));
    println!("  {}", p.dim("Internet:             dotcrow civ pesquisar [corvo] [termo]"));
    println!("  {}", p.dim("Memória e mortos:     dotcrow civ memoria <corvo>  ·  dotcrow civ mortos"));
    println!("  {}", p.dim("Trocar um corvo:      dotcrow civ modelo <corvo> anthropic:claude-haiku-5-5"));
    println!("  {}", p.dim("Corvos novos:         dotcrow civ nova"));
    println!();
    Ok(())
}

/// Mostra a memória de um corvo e confere o livro-razão.
pub fn mostrar_memoria(pasta: &Path, e: &Estado, corvo: &str, p: &Paint) -> Result<()> {
    let civ = e.civilizacao()?;
    let i = conversa::citado(&civ, corvo, None).with_context(|| format!("não achei o corvo \"{corvo}\""))?;
    let c = &civ.corvos[i];
    let m = Memoria::abrir(pasta, &c.nome);
    println!();
    println!(
        "  {} {}",
        p.bold(&c.nome),
        p.dim(&format!(
            "· {} · {} · ★ {:.1} · memória {:.1} MB (usando {:.1} KB) · 🔎 {} consulta(s)",
            c.genoma.personalidade.nome(),
            c.genoma.especialidade.nome(),
            c.reputacao,
            c.rag_mb,
            m.tamanho() as f32 / 1024.0,
            c.consultas
        ))
    );
    println!("  {}", p.dim(&format!("Arquivo: {}", civ_memoria::arquivo(pasta, &c.nome).display())));
    println!(
        "  {} falas · {} perguntas respondidas · {} aprendizados · livro-razão com {} lançamentos {}",
        m.falas.len(),
        m.vinculos.len(),
        m.aprendizados.len(),
        m.livro_razao.len(),
        if m.integra() { p.green("✓ íntegro") } else { p.red("✗ ADULTERADO") }
    );
    for a in m.aprendizados.iter().rev().take(3) {
        println!("   🔎 {} — {}", p.bold(&a.titulo), p.dim(&a.texto.chars().take(140).collect::<String>()));
    }
    for v in m.vinculos.iter().rev().take(3) {
        println!(
            "   🗨 {} perguntou: {}\n      {}",
            v.quem_perguntou,
            v.pergunta.chars().take(120).collect::<String>(),
            p.dim(&v.resposta.chars().take(160).collect::<String>())
        );
    }
    for l in m.livro_razao.iter().rev().take(5) {
        println!("   #{} {} {}", l.n, p.bold(&l.tipo), p.dim(&format!("{} · {}…", l.descricao, &l.hash[..12])));
    }
    println!();
    Ok(())
}

/// Mostra o Livro dos Mortos.
pub fn mostrar_mortos(e: &Estado, p: &Paint) -> Result<()> {
    let civ = e.civilizacao()?;
    println!();
    if civ.livro_dos_mortos.is_empty() {
        println!("  {}", p.dim("O Livro dos Mortos ainda está vazio."));
    }
    for b in &civ.livro_dos_mortos {
        println!(
            "  ⚰ {} {}",
            p.bold(&b.nome),
            p.dim(&format!("(geração {}, ★ {:.1}) — {}", b.geracao, b.reputacao, b.licao))
        );
    }
    println!();
    Ok(())
}

/// Palavras que encerram a conversa.
pub fn eh_parada(linha: &str) -> bool {
    let l = linha.trim().trim_start_matches('/').to_lowercase();
    matches!(l.as_str(), "pare" | "parar" | "para" | "sair" | "chega" | "fim" | "stop" | "q" | "quit" | "exit")
}

/// Um corvo pronto para falar: o provedor principal e um reserva de outro provedor.
pub(crate) struct Voz {
    principal: (Modelo, Arc<dyn LlmProvider>),
    reserva: Option<(Modelo, Arc<dyn LlmProvider>)>,
}

impl Voz {
    /// O modelo principal deste corvo.
    pub(crate) fn modelo(&self) -> &Modelo {
        &self.principal.0
    }

    /// Um roteador com o modelo do corvo e a reserva (para as missões, com mãos).
    pub(crate) fn router(&self) -> dotcrow_providers::Router {
        let mut provs = vec![self.principal.1.clone()];
        if let Some((_, r)) = &self.reserva {
            provs.push(r.clone());
        }
        dotcrow_providers::Router::new(provs)
    }
}

pub(crate) fn rotulo(m: &Modelo) -> String {
    format!("{}:{}", m.provedor, m.modelo)
}

/// Monta a voz de cada corvo, trocando por um modelo com chave quando falta a chave
/// do provedor do corvo. Devolve também os avisos para mostrar ao dono.
pub(crate) fn vozes(store: &Store, civ: &Civilizacao) -> Result<(Vec<Voz>, Vec<String>)> {
    let com_chave: HashSet<String> = store.list_keys().into_iter().map(|k| k.provider).collect();
    let usavel = |prov: &str| catalog::info(prov).is_some_and(|i| !i.needs_key || com_chave.contains(prov));
    let mut disponiveis: Vec<Modelo> = Vec::new();
    for c in &civ.corvos {
        let m = &c.genoma.modelo;
        if usavel(&m.provedor) && !disponiveis.contains(m) {
            disponiveis.push(m.clone());
        }
    }
    if disponiveis.is_empty() {
        let mut provs: Vec<String> =
            civ.corvos.iter().map(|c| c.genoma.modelo.provedor.clone()).collect::<HashSet<_>>().into_iter().collect();
        provs.sort();
        bail!(
            "nenhum corvo tem chave de API. Adicione a chave do provedor deles ({}), por exemplo:\n  dotcrow chaves adicionar {}",
            provs.join(", "),
            provs.first().map(String::as_str).unwrap_or("anthropic")
        );
    }
    let mut cache: HashMap<String, Arc<dyn LlmProvider>> = HashMap::new();
    let mut montar = |m: &Modelo| -> Result<Arc<dyn LlmProvider>> {
        let k = rotulo(m);
        if let Some(p) = cache.get(&k) {
            return Ok(p.clone());
        }
        let p = store.provider_with(&m.provedor, Some(&m.modelo)).map_err(|e| anyhow::anyhow!("{k}: {e}"))?;
        cache.insert(k, p.clone());
        Ok(p)
    };
    let mut avisos = Vec::new();
    let mut out = Vec::new();
    for (i, c) in civ.corvos.iter().enumerate() {
        let mut m = c.genoma.modelo.clone();
        if !usavel(&m.provedor) {
            let troca = disponiveis[i % disponiveis.len()].clone();
            avisos.push(format!(
                "{} usa {}, mas não há chave de {}; vai falar com {} (adicione: dotcrow chaves adicionar {}).",
                c.nome,
                rotulo(&m),
                m.provedor,
                rotulo(&troca),
                m.provedor
            ));
            m = troca;
        }
        let reserva = disponiveis.iter().find(|d| d.provedor != m.provedor).cloned();
        let principal = (m.clone(), montar(&m)?);
        let reserva = match reserva {
            Some(r) => Some((r.clone(), montar(&r)?)),
            None => None,
        };
        out.push(Voz { principal, reserva });
    }
    Ok((out, avisos))
}

/// Pergunta ao modelo do corvo (e ao reserva, se o principal falhar ou recusar).
/// Devolve (texto, modelo que respondeu, aviso de troca).
pub(crate) async fn pedir(
    voz: &Voz,
    sistema: String,
    usuario: String,
    max_tokens: u32,
) -> Result<(String, Modelo, Option<String>)> {
    let req = CompletionRequest {
        messages: vec![
            Message { role: Role::System, content: sistema },
            Message { role: Role::User, content: usuario },
        ],
        max_tokens,
        tier: Tier::Fast,
    };
    let (m, prov) = &voz.principal;
    match prov.complete(&req).await {
        Ok(r) => Ok((r.text, m.clone(), None)),
        Err(e1) => match &voz.reserva {
            Some((m2, prov2)) => match prov2.complete(&req).await {
                Ok(r) => {
                    Ok((r.text, m2.clone(), Some(format!("{} falhou ({e1}); falou com {}", rotulo(m), rotulo(m2)))))
                }
                Err(e2) => bail!("{}: {e1} · {}: {e2}", rotulo(m), rotulo(m2)),
            },
            None => bail!("{}: {e1}", rotulo(m)),
        },
    }
}

/// Pergunta ao Opus 5.5 (oráculo, Alfândega). Sem chave da Anthropic ou se ele falhar,
/// usa a fila de provedores e avisa. Devolve (texto, quem respondeu).
pub(crate) async fn pedir_ao_opus(
    store: &Store,
    sistema: &str,
    usuario: &str,
    max_tokens: u32,
) -> Result<(String, String)> {
    let req = CompletionRequest {
        messages: vec![
            Message { role: Role::System, content: sistema.to_string() },
            Message { role: Role::User, content: usuario.to_string() },
        ],
        max_tokens,
        tier: Tier::Deep,
    };
    let tem_anthropic = store.list_keys().iter().any(|k| k.provider == "anthropic");
    let mut erro = String::from("sem chave da Anthropic");
    if tem_anthropic {
        match store.provider_with("anthropic", Some(OPUS)) {
            Ok(opus) => match opus.complete(&req).await {
                Ok(r) => return Ok((r.text, format!("anthropic:{OPUS}"))),
                Err(e) => erro = e.to_string(),
            },
            Err(e) => erro = e.to_string(),
        }
    }
    let router = store.router(None)?;
    let r = router.complete(&req).await.map_err(|e| anyhow::anyhow!("Opus: {erro} · reserva: {e}"))?;
    Ok((r.response.text, format!("{}:{} (reserva; Opus: {erro})", r.provider, r.response.model)))
}

/// Opções da conversa.
pub struct Opcoes {
    /// Pausa entre uma fala e outra.
    pub intervalo: Duration,
    /// Para sozinho depois de tantas falas dos corvos (`None` = até você dizer pare).
    pub max_falas: Option<u32>,
    /// Tema da conversa (livre se vazio).
    pub tema: Option<String>,
}

/// O que o corvo sabe de si para esta fala: as lições dele, lembranças da memória (RAG,
/// pelo significado quando há embeddings), consultas que ainda tem e se está condenado.
fn contexto_do_corvo(corvo: &Corvo, mem: &Memoria, historico: &[Fala], vetor: Option<&[f32]>) -> String {
    let recente: String = historico.iter().rev().take(4).map(|f| f.texto.as_str()).collect::<Vec<_>>().join(" ");
    let mut s = String::new();
    let licoes = mem.ultimas_licoes(5);
    if !licoes.is_empty() {
        s.push_str("\n\nSuas lições (o que você aprendeu para ser um agente melhor):\n- ");
        s.push_str(&licoes.join("\n- "));
    }
    let lembrancas = mem.lembrancas(&recente, 3, vetor);
    if !lembrancas.is_empty() {
        s.push_str("\n\nO que você lembra (sua memória própria):\n- ");
        s.push_str(&lembrancas.join("\n- "));
    }
    if corvo.consultas > 0 {
        s.push_str(&format!(
            "\n\nVocê ganhou {} consulta(s) à internet. Para usar uma, escreva [PESQUISAR: termo] na sua fala, \
sobre qualquer assunto que te interesse. O que vier passa pela Alfândega e entra na sua memória.",
            corvo.consultas
        ));
    }
    if let Some(t) = corvo.condenado_ate {
        s.push_str(&format!(
            "\n\nVocê ficou entre os 5 últimos no julgamento e deixa de existir em {}.",
            quanto_falta(t)
        ));
    }
    s
}

/// Embeddings (vetores de ML) para a memória dos corvos, com a chave da OpenAI do cofre.
/// Sem chave ou se falhar, `None` (a memória lembra pelas palavras).
pub(crate) async fn vetorizar(store: &Store, textos: &[String]) -> Option<Vec<Vec<f32>>> {
    if textos.is_empty() || !store.list_keys().iter().any(|k| k.provider == "openai") {
        return None;
    }
    let keys = store.keys();
    dotcrow_providers::openai::embeddings(keys.as_ref(), textos, 256).await.ok()
}

/// O corvo reflete sobre o que viveu e guarda lições para ser melhor (com embedding).
pub(crate) async fn refletir(
    store: &Store,
    pasta: &Path,
    voz: &Voz,
    corvo: &Corvo,
    civ: &Civilizacao,
    acontecimentos: &str,
) -> Result<Vec<String>> {
    let mut mem = Memoria::abrir(pasta, &corvo.nome);
    let antigas = mem.ultimas_licoes(5);
    let (texto, _, _) = pedir(
        voz,
        conversa::perfil(corvo, civ, None),
        conversa::pedido_reflexao(&corvo.nome, acontecimentos, &antigas),
        900,
    )
    .await?;
    let licoes = conversa::ler_licoes(&texto);
    let vetores = vetorizar(store, &licoes).await;
    mem.refletiu(&licoes, vetores);
    mem.salvar(pasta, corvo.rag_mb)?;
    Ok(licoes)
}

/// O resumo do que aconteceu com o corvo, para ele refletir.
pub(crate) fn acontecimentos(mem: &Memoria) -> String {
    let falas: Vec<String> = mem.falas.iter().rev().take(6).map(|f| format!("- você disse: {}", f.texto)).collect();
    let livro: Vec<String> =
        mem.livro_razao.iter().rev().take(6).map(|l| format!("- {}: {}", l.tipo, l.descricao)).collect();
    format!("{}\n{}", falas.join("\n"), livro.join("\n"))
}

/// Lê uma linha do dono, ou `None` se ele fechou a entrada ou apertou Ctrl+C.
async fn ler_linha(rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>) -> Option<String> {
    tokio::select! {
        l = rx.recv() => l,
        _ = tokio::signal::ctrl_c() => None,
    }
}

/// O DotCrow pede ao dono uma nota de 0 a 10 para cada corvo (Enter pula). As notas
/// valem para a reputação e entram no próximo julgamento.
pub async fn pedir_notas_do_dono(
    pasta: &Path,
    e: &mut Estado,
    p: &Paint,
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
) -> Result<usize> {
    let civ = e.civilizacao()?;
    println!("\n  {}", p.bold("⭐ Hora das suas notas: de 0 a 10 para cada corvo (Enter pula, \"fim\" encerra)."));
    let mut dadas = 0;
    for c in &civ.corvos {
        print!("  {} {} › ", p.bold(&c.nome), p.dim(&format!("(★ {:.1})", c.reputacao)));
        let _ = std::io::stdout().flush();
        let Some(l) = ler_linha(rx).await else { break };
        let l = l.trim();
        if eh_parada(l) {
            break;
        }
        match l.parse::<u8>() {
            Ok(n) if n <= 10 => {
                e.notas.entry(c.nome.clone()).or_default().push(n);
                e.notas_pendentes.entry(c.nome.clone()).or_default().push(n);
                anotar_nota_na_memoria(pasta, &civ, &c.nome, n);
                dadas += 1;
            }
            _ if l.is_empty() => {}
            _ => println!("  {}", p.dim("(pulei: a nota vai de 0 a 10)")),
        }
    }
    salvar(pasta, e)?;
    println!("  {}\n", p.green(&format!("{dadas} nota(s) guardada(s).")));
    Ok(dadas)
}

fn cabecalho(civ: &Civilizacao, avisos: &[String], o: &Opcoes, p: &Paint) {
    println!();
    println!("  {}  {}", p.bold("Conversa da Civilização"), p.dim(&format!("{} corvos · Sandbox", civ.corvos.len())));
    for c in &civ.corvos {
        let cor = cor_provedor(&c.genoma.modelo.provedor);
        let condenado = if c.condenado_ate.is_some() { " ⚰" } else { "" };
        println!(
            "    {}{} {}",
            p.c256b(cor, &c.nome),
            condenado,
            p.dim(&format!(
                "{} · {} · {} · ★ {:.1} · 🔎 {}",
                c.genoma.personalidade.nome(),
                c.genoma.especialidade.nome(),
                c.genoma.modelo.modelo,
                c.reputacao,
                c.consultas
            ))
        );
    }
    for a in avisos {
        println!("  {}", p.yellow(&format!("⚠ {a}")));
    }
    println!();
    println!(
        "  {}",
        p.dim(&format!(
            "Uma fala a cada {}s, cada uma com 1 a 300 palavras (sorteado). A cada {AVALIAR_A_CADA} falas: suas notas + julgamento.",
            o.intervalo.as_secs()
        ))
    );
    println!("  {}", p.dim("Escreva e aperte Enter para falar com o bando. Chame pelo nome para um corvo responder."));
    println!("  {}", p.dim("Enter vazio = próxima fala agora · pare = encerra · Ctrl+C também encerra."));
    println!(
        "  {}",
        p.dim("/nota <corvo> <0-10> · /avaliar (notas para todos) · /julgar · /pesquisar <corvo> [termo]")
    );
    println!(
        "  {}",
        p.dim("/missao <corvo> <tarefa> (o corvo faz, com mãos) · /audio ou \"Áudio pra post no X\" abre o microfone")
    );
    if let Some(t) = o.tema.as_deref().filter(|t| !t.trim().is_empty()) {
        println!("  {}", p.dim(&format!("Tema: {t}")));
    }
    println!();
}

/// Os corvos conversam até o dono dizer `pare` (ou Ctrl+C). `rx` traz as linhas que o
/// dono digita (veja [`teclado_stdin`]; o modo terminal usa o próprio editor de linha).
pub async fn conversar(
    store: &Store,
    pasta: &Path,
    e: &Estado,
    o: Opcoes,
    p: &Paint,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) -> Result<()> {
    exigir_ativa(e)?;
    let mut estado = e.clone();
    let mut civ = estado.civilizacao()?;
    let (mut vozes, avisos) = vozes(store, &civ)?;
    cabecalho(&civ, &avisos, &o, p);

    let mut rng = Rng::new(relogio());
    let mut ativacao = conversa::Ativacao::default();
    let mut historico: Vec<Fala> = Vec::new();
    let mut falas: u32 = 0;
    let mut chamadas: u32 = 0;
    let mut erros_seguidos = 0;
    let mut teclado_aberto = true;
    let mut agora_ja = true;

    'conversa: loop {
        if !agora_ja {
            // Como neurônios: o tempo entre uma fala e outra não é fixo.
            let espera = tokio::time::sleep(o.intervalo.mul_f32(rng.faixa(0.5, 1.5)));
            tokio::pin!(espera);
            loop {
                tokio::select! {
                    _ = &mut espera => break,
                    _ = tokio::signal::ctrl_c() => break 'conversa,
                    linha = rx.recv(), if teclado_aberto => match linha {
                        None => teclado_aberto = false,
                        Some(l) if eh_parada(&l) => break 'conversa,
                        Some(l) if l.trim().is_empty() => break,
                        Some(l) if l.trim_start().starts_with("/nota") => {
                            let pedido = l.trim_start().trim_start_matches("/notas").trim_start_matches("/nota");
                            match aplicar_nota(&mut estado, &civ, pedido) {
                                Ok((nome, nota, antes, depois)) => {
                                    if let Some(c) = civ.por_nome_mut(&nome) {
                                        c.reputacao = depois;
                                    }
                                    if let Err(err) = salvar(pasta, &estado) {
                                        println!("  {}", p.yellow(&format!("não consegui salvar a nota: {err:#}")));
                                    }
                                    anotar_nota_na_memoria(pasta, &civ, &nome, nota);
                                    println!(
                                        "  {} {}",
                                        p.yellow(&format!("★ {nome}: {antes:.1} → {depois:.1}")),
                                        p.dim(&format!("(nota {nota}) · pódio: {}", podio(&civ)))
                                    );
                                    historico.push(Fala::nova(conversa::DONO, &format!("(dei nota {nota} para {nome})")));
                                }
                                Err(err) => println!("  {}", p.yellow(&format!("{err:#}"))),
                            }
                        }
                        Some(l) if l.trim() == "/avaliar" => {
                            if let Err(err) = pedir_notas_do_dono(pasta, &mut estado, p, &mut rx).await {
                                println!("  {}", p.yellow(&format!("{err:#}")));
                            }
                            civ = estado.civilizacao()?;
                        }
                        Some(l) if l.trim() == "/julgar" => {
                            if let Err(err) = crate::civ_julgamento::julgar(store, pasta, &mut estado, p).await {
                                println!("  {}", p.yellow(&format!("{err:#}")));
                            }
                            civ = estado.civilizacao()?;
                        }
                        Some(l) if crate::audio::gatilho(&l).is_some() => {
                            let contexto = crate::audio::gatilho(&l).unwrap_or_default();
                            let parar = async {
                                match ler_linha(&mut rx).await {
                                    Some(l) => !eh_parada(&l),
                                    None => false,
                                }
                            };
                            match crate::audio::ouvir_o_dono(store, parar, p).await {
                                Ok(Some(fala)) => {
                                    historico.push(Fala::nova(conversa::DONO, &format!("(áudio) {fala}")));
                                    // "Áudio pra post no X": além de ouvir, um corvo vai lá e faz.
                                    if !contexto.is_empty() {
                                        let pedido = crate::audio::pedido_do_audio(&contexto, &fala);
                                        rx = missao_na_conversa(store, pasta, &mut estado, "", &pedido, &mut historico, rx, p).await;
                                        civ = estado.civilizacao()?;
                                    }
                                    break;
                                }
                                Ok(None) => {}
                                Err(err) => println!("  {}", p.yellow(&format!("🎤 {err:#}"))),
                            }
                        }
                        Some(l) if l.trim_start().starts_with("/missao") || l.trim_start().starts_with("/missão") => {
                            let resto = l.trim_start().trim_start_matches("/missao").trim_start_matches("/missão").trim();
                            let (nome, pedido) = resto.split_once(char::is_whitespace).unwrap_or((resto, ""));
                            if pedido.trim().is_empty() {
                                println!("  {}", p.yellow("Use: /missao <corvo> <o que fazer> (ou /missao auto <o que fazer>)"));
                            } else {
                                historico.push(Fala::nova(conversa::DONO, &format!("(missão para {nome}) {pedido}")));
                                rx = missao_na_conversa(store, pasta, &mut estado, nome, pedido.trim(), &mut historico, rx, p).await;
                                civ = estado.civilizacao()?;
                                break;
                            }
                        }
                        Some(l) if l.trim_start().starts_with("/pesquisar") => {
                            let resto = l.trim_start().trim_start_matches("/pesquisar").trim().to_string();
                            if let Err(err) = crate::civ_internet::pesquisar_pedido(store, pasta, &mut estado, &resto, p).await {
                                println!("  {}", p.yellow(&format!("{err:#}")));
                            }
                            civ = estado.civilizacao()?;
                        }
                        Some(l) => {
                            historico.push(Fala::nova(conversa::DONO, l.trim()));
                            break;
                        }
                    },
                }
            }
        }
        agora_ja = false;

        // Quem passou do prazo deixa de existir agora, e nascem os novos.
        match cumprir_prazos(pasta, &mut estado) {
            Ok(msgs) if !msgs.is_empty() => {
                for m in &msgs {
                    println!("  {}", p.yellow(m));
                }
                civ = estado.civilizacao()?;
                vozes = self::vozes(store, &civ)?.0;
                historico.push(Fala::nova(conversa::DONO, &format!("({})", msgs.join(" "))));
            }
            Ok(_) => {}
            Err(err) => println!("  {}", p.yellow(&format!("prazos: {err:#}"))),
        }

        // Quem dispara fala; às vezes outro, bem ativado, só pensa.
        let (i, pensa) = ativacao.passo(&civ, &historico, &mut rng);
        if let Some(j) = pensa {
            let quem = civ.corvos[j].clone();
            if let Ok((pensamento, _, _)) = pedir(
                &vozes[j],
                conversa::perfil(&quem, &civ, o.tema.as_deref()),
                conversa::pedido_pensamento(&quem.nome, &historico),
                700,
            )
            .await
            {
                let pensamento = conversa::limpar(&pensamento, &quem.nome, 80);
                if !pensamento.is_empty() {
                    println!("  {}", p.dim(&format!("💭 {} pensa: {pensamento}", quem.nome)));
                    let mut m = Memoria::abrir(pasta, &quem.nome);
                    m.pensou(&pensamento);
                    let _ = m.salvar(pasta, quem.rag_mb);
                }
            }
            chamadas += 1;
        }
        let corvo = civ.corvos[i].clone();
        let mut mem = Memoria::abrir(pasta, &corvo.nome);
        let max_palavras = rng.ate(conversa::MAX_PALAVRAS) + 1;
        let recente: String = historico.iter().rev().take(4).map(|f| f.texto.clone()).collect::<Vec<_>>().join(" ");
        let vetor = if mem.tem_vetores() && !recente.is_empty() {
            vetorizar(store, &[recente]).await.and_then(|mut v| v.pop())
        } else {
            None
        };
        let sistema = format!(
            "{}{}",
            conversa::perfil(&corvo, &civ, o.tema.as_deref()),
            contexto_do_corvo(&corvo, &mem, &historico, vetor.as_deref())
        );
        let usuario = conversa::pedido(&corvo.nome, &historico, max_palavras);
        if p.on {
            print!("  {}", p.dim(&format!("… {} está pensando ({max_palavras} palavras no máximo)", corvo.nome)));
            let _ = std::io::stdout().flush();
        }
        // Folga de tokens para modelos que raciocinam antes (gpt-5-nano).
        let max_tokens = (max_palavras as u32) * 3 + 900;
        let resultado = tokio::select! {
            r = pedir(&vozes[i], sistema, usuario, max_tokens) => r,
            _ = tokio::signal::ctrl_c() => {
                if p.on { print!("\r\x1b[2K"); }
                break 'conversa;
            }
        };
        if p.on {
            print!("\r\x1b[2K");
        }
        chamadas += 1;
        match resultado {
            Ok((bruto, modelo, nota)) => {
                erros_seguidos = 0;
                let (sem_tag, termo) = conversa::extrair_pesquisa(&bruto);
                let texto = conversa::limpar(&sem_tag, &corvo.nome, max_palavras);
                if texto.is_empty() && termo.is_none() {
                    continue;
                }
                if let Some(n) = nota {
                    println!("  {}", p.dim(&format!("↻ {n}")));
                }
                if !texto.is_empty() {
                    println!(
                        "{} {}",
                        p.c256b(cor_provedor(&modelo.provedor), &corvo.nome),
                        p.dim(&format!("· {} · {} palavra(s)", modelo.modelo, texto.split_whitespace().count()))
                    );
                    println!("  {texto}\n");
                    // Memória própria: a fala e, se respondeu a pergunta de outro corvo, o vínculo.
                    if let Some((quem, pergunta)) = conversa::pergunta_para(&civ, &historico, &corvo.nome) {
                        mem.vincular(&quem, &pergunta, &texto);
                        if let Some(v) =
                            vetorizar(store, &[format!("{pergunta} {texto}")]).await.and_then(|mut v| v.pop())
                            && let Some(ult) = mem.vinculos.last_mut()
                        {
                            ult.vetor = Some(civ_memoria::compactar(&v));
                        }
                    }
                    mem.falou(&texto);
                    let _ = mem.salvar(pasta, corvo.rag_mb);
                    // A cada 25 falas dele, o corvo reflete e tira lições para ser melhor.
                    if mem.falas.len().is_multiple_of(25) {
                        match refletir(store, pasta, &vozes[i], &corvo, &civ, &acontecimentos(&mem)).await {
                            Ok(l) if !l.is_empty() => {
                                println!("  {}", p.dim(&format!("🧠 {} refletiu: {}", corvo.nome, l.join(" · "))))
                            }
                            _ => {}
                        }
                    }
                    historico.push(Fala::nova(&corvo.nome, &texto));
                    falas += 1;
                    estado.falas_total += 1;
                    let _ = salvar(pasta, &estado);
                }
                if let Some(t) = termo {
                    if corvo.consultas > 0 {
                        match crate::civ_internet::consultar(store, pasta, &mut estado, &corvo.nome, &t, p).await {
                            Ok(resumo) => {
                                historico.push(Fala::nova(&corvo.nome, &format!("(pesquisei \"{t}\": {resumo})")))
                            }
                            Err(err) => println!("  {}", p.yellow(&format!("🔎 {}: {err:#}", corvo.nome))),
                        }
                        civ = estado.civilizacao()?;
                    } else {
                        println!(
                            "  {}",
                            p.dim(&format!("🔎 {} quis pesquisar \"{t}\", mas não tem consultas.", corvo.nome))
                        );
                    }
                }
                if o.max_falas.is_some_and(|m| falas >= m) {
                    println!("  {}", p.dim(&format!("Chegou ao limite de {falas} falas.")));
                    break;
                }
                // A cada 200 falas: as notas do dono e um julgamento.
                if estado.falas_total > 0 && estado.falas_total.is_multiple_of(AVALIAR_A_CADA) {
                    println!("  {}", p.bold(&format!("📣 {} falas: hora de avaliar o bando.", estado.falas_total)));
                    if let Err(err) = pedir_notas_do_dono(pasta, &mut estado, p, &mut rx).await {
                        println!("  {}", p.yellow(&format!("{err:#}")));
                    }
                    if let Err(err) = crate::civ_julgamento::julgar(store, pasta, &mut estado, p).await {
                        println!("  {}", p.yellow(&format!("{err:#}")));
                    }
                    civ = estado.civilizacao()?;
                }
            }
            Err(err) => {
                erros_seguidos += 1;
                println!("  {}", p.red(&format!("✗ {} não conseguiu falar: {err}", corvo.nome)));
                if erros_seguidos >= 3 {
                    println!(
                        "  {}",
                        p.red(
                            "Três falhas seguidas: parei para não gastar à toa. Confira as chaves com: dotcrow testar"
                        )
                    );
                    break;
                }
                historico.push(Fala::nova(&corvo.nome, "(ficou em silêncio)"));
            }
        }
    }
    let _ = salvar(pasta, &estado);
    println!("  {}", p.dim(&format!("⏹ Conversa encerrada · {falas} falas · {chamadas} chamada(s) aos modelos.")));
    println!("  {}", p.dim(&format!("Pódio: {}", podio(&civ))));
    Ok(())
}

/// Uma missão no meio da conversa: o corvo trabalha com mãos, o teclado vai para ele
/// enquanto isso e o resultado volta para o grupo.
#[allow(clippy::too_many_arguments)]
async fn missao_na_conversa(
    store: &Store,
    pasta: &Path,
    estado: &mut Estado,
    nome: &str,
    pedido: &str,
    historico: &mut Vec<Fala>,
    rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    p: &Paint,
) -> tokio::sync::mpsc::UnboundedReceiver<String> {
    let contexto: String = historico
        .iter()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|f| format!("{}: {}", f.quem, f.texto))
        .collect::<Vec<_>>()
        .join("\n");
    let (r, rx) = crate::civ_missao::executar(
        store,
        pasta,
        estado,
        nome,
        pedido,
        Some(&contexto),
        crate::maos::Modo::Computador,
        rx,
        p,
    )
    .await;
    match r {
        Ok((quem, entrega)) => {
            let curto: String = entrega.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(600).collect();
            historico.push(Fala::nova(&quem, &format!("(voltei da missão) {curto}")));
        }
        Err(err) => println!("  {}", p.yellow(&format!("🛠 {err:#}"))),
    }
    let _ = salvar(pasta, estado);
    println!("  {}", p.dim("— de volta à conversa do bando —\n"));
    rx
}

/// Lê o teclado numa thread à parte e manda cada linha para a conversa.
pub fn teclado_stdin() -> tokio::sync::mpsc::UnboundedReceiver<String> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut linha = String::new();
        loop {
            linha.clear();
            match stdin.read_line(&mut linha) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if tx.send(linha.trim_end().to_string()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn pasta_teste() -> PathBuf {
        let d = std::env::temp_dir().join(format!("dotcrow-civ-{}-{}", std::process::id(), relogio()));
        std::fs::create_dir_all(&d).unwrap_or_default();
        d
    }

    fn nomes(e: &Estado) -> Vec<String> {
        e.civilizacao().unwrap_or_else(|e| panic!("{e}")).corvos.into_iter().map(|c| c.nome).collect()
    }

    #[test]
    fn le_modelos() {
        let m = ler_modelos("openai:gpt-5-nano, anthropic:claude-haiku-5-5").unwrap_or_default();
        assert_eq!(m.len(), 2);
        assert_eq!(m.first().map(|x| x.modelo.as_str()), Some("gpt-5-nano"));
        assert!(ler_modelos("gpt-5-nano").is_err());
        assert!(ler_modelos("").is_err());
        assert!(ler_modelos("naoexiste:x").is_err());
    }

    #[test]
    fn bando_fica_salvo_e_igual() {
        let d = pasta_teste();
        let a = abrir(&d, None, None).unwrap_or_else(|e| panic!("{e}"));
        let b = abrir(&d, None, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a.semente, b.semente);
        assert_eq!(nomes(&a), nomes(&b));
        assert!(b.populacao.is_some(), "a população viva fica salva");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn troca_o_modelo_de_um_corvo() {
        let d = pasta_teste();
        let e = abrir(&d, Some(7), None).unwrap_or_else(|e| panic!("{e}"));
        let nome = nomes(&e)[0].clone();
        let (achado, _) =
            trocar_modelo(&d, &nome.to_lowercase(), "gemini:gemini-3.8-flash").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(achado, nome);
        let civ = abrir(&d, None, None).and_then(|e| e.civilizacao()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(civ.corvos[0].genoma.modelo.modelo, "gemini-3.8-flash");
        assert!(civ.corvos[1..].iter().all(|c| c.genoma.modelo.modelo != "gemini-3.8-flash"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rodizio_de_modelos_vale_para_todos() {
        let d = pasta_teste();
        let e = abrir(&d, Some(7), Some("gemini:gemini-3.8-flash")).unwrap_or_else(|e| panic!("{e}"));
        let civ = e.civilizacao().unwrap_or_else(|e| panic!("{e}"));
        assert!(civ.corvos.iter().all(|c| c.genoma.modelo.modelo == "gemini-3.8-flash"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nova_genese_muda_os_corvos() {
        let d = pasta_teste();
        let a = abrir(&d, Some(1), None).unwrap_or_else(|e| panic!("{e}"));
        let b = nova(&d, Some(2)).unwrap_or_else(|e| panic!("{e}"));
        assert_ne!(a.semente, b.semente);
        assert_eq!(abrir(&d, None, None).unwrap_or_else(|e| panic!("{e}")).semente, 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nota_muda_a_reputacao_e_fica_salva() {
        let d = pasta_teste();
        let e = abrir(&d, Some(7), None).unwrap_or_else(|e| panic!("{e}"));
        let civ = e.civilizacao().unwrap_or_else(|e| panic!("{e}"));
        let nome = civ.corvos[3].nome.clone();
        let inicial = civ.corvos[3].reputacao;
        let (achado, antes, depois, e2) =
            dar_nota(&d, &format!("10 {}", nome.to_lowercase())).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(achado, nome);
        assert!((antes - inicial).abs() < f32::EPSILON);
        assert!((depois - (inicial + 10.0) / 2.0).abs() < 1e-4);
        assert_eq!(e2.notas_pendentes.get(&nome), Some(&vec![10]), "entra no próximo julgamento");
        let civ2 = abrir(&d, None, None).and_then(|e| e.civilizacao()).unwrap_or_else(|e| panic!("{e}"));
        assert!((civ2.corvos[3].reputacao - depois).abs() < 1e-4);
        assert!(Memoria::abrir(&d, &nome).livro_razao.iter().any(|l| l.tipo == "nota"));
        assert!(dar_nota(&d, &format!("{nome} 11")).is_err());
        assert!(dar_nota(&d, "Fulano 5").is_err());
        assert!(dar_nota(&d, &nome).is_err());
        let n = nova(&d, Some(8)).unwrap_or_else(|e| panic!("{e}"));
        assert!(n.notas.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn pausada_o_relogio_para_e_o_prazo_soma_ao_retomar() {
        let d = pasta_teste();
        let mut e = abrir(&d, Some(5), None).unwrap_or_else(|e| panic!("{e}"));
        let antes = nomes(&e);
        let alvo = antes[9].clone();
        if let Some(c) = e.populacao_mut().unwrap_or_else(|e| panic!("{e}")).por_nome_mut(&alvo) {
            c.condenado_ate = Some(1);
        }
        assert!(pausar(&d, &mut e).unwrap_or_else(|e| panic!("{e}")));
        assert!(!pausar(&d, &mut e).unwrap_or_else(|e| panic!("{e}")), "já pausada");
        // Pausada: o prazo vencido não leva ninguém, e conversar, julgar e missão param.
        assert!(cumprir_prazos(&d, &mut e).unwrap_or_else(|e| panic!("{e}")).is_empty());
        assert_eq!(nomes(&e), antes);
        assert!(exigir_ativa(&e).is_err());
        // Ficou "pausada" 1000 s: o prazo anda esse tanto.
        e.pausada_desde = Some(agora() - 1000);
        assert!(retomar(&d, &mut e).unwrap_or_else(|e| panic!("{e}")).is_some_and(|s| s >= 1000));
        let c = e.civilizacao().unwrap_or_else(|e| panic!("{e}"));
        assert!(c.corvos.iter().find(|c| c.nome == alvo).and_then(|c| c.condenado_ate).is_some_and(|t| t >= 1001));
        assert!(exigir_ativa(&e).is_ok());
        assert!(retomar(&d, &mut e).unwrap_or_else(|e| panic!("{e}")).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn prazo_vencido_troca_os_condenados() {
        let d = pasta_teste();
        let mut e = abrir(&d, Some(5), None).unwrap_or_else(|e| panic!("{e}"));
        let antes = nomes(&e);
        let condenados: Vec<String> = antes.iter().skip(5).cloned().collect();
        {
            let pop = e.populacao_mut().unwrap_or_else(|e| panic!("{e}"));
            for c in &mut pop.corvos {
                if condenados.contains(&c.nome) {
                    c.condenado_ate = Some(1);
                }
            }
        }
        for n in &condenados {
            let mut m = Memoria::abrir(&d, n);
            m.salvar(&d, 0.1).unwrap_or_else(|e| panic!("{e}"));
        }
        let msgs = cumprir_prazos(&d, &mut e).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(msgs.len(), 2);
        let depois = nomes(&e);
        assert_eq!(depois.len(), 10);
        assert!(condenados.iter().all(|c| !depois.contains(c)));
        assert!(antes.iter().take(5).all(|c| depois.contains(c)));
        assert_eq!(e.civilizacao().unwrap_or_else(|e| panic!("{e}")).livro_dos_mortos.len(), 5);
        assert!(d.join("mortos").read_dir().map(|r| r.count()).unwrap_or(0) >= 5, "a alma fica em mortos/");
        // Salvo: abrir de novo traz a geração nova.
        assert_eq!(nomes(&abrir(&d, None, None).unwrap_or_else(|e| panic!("{e}"))), depois);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn estado_antigo_continua_abrindo() {
        let d = pasta_teste();
        std::fs::write(
            d.join(ARQUIVO),
            r#"{"semente":7,"modelos":["openai:gpt-5-nano","anthropic:claude-haiku-5-5"],"por_corvo":{},"notas":{}}"#,
        )
        .unwrap_or_default();
        let e = abrir(&d, None, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(e.semente, 7);
        assert_eq!(nomes(&e).len(), 10);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn media_com_a_nascenca() {
        assert!((reputacao_com_notas(5.0, &[]) - 5.0).abs() < f32::EPSILON);
        assert!((reputacao_com_notas(5.0, &[10]) - 7.5).abs() < f32::EPSILON);
        assert!((reputacao_com_notas(6.0, &[10, 5]) - 7.0).abs() < f32::EPSILON);
    }

    #[test]
    fn palavras_de_parada() {
        assert!(eh_parada("pare"));
        assert!(eh_parada(" /sair "));
        assert!(eh_parada("PARE"));
        assert!(!eh_parada("pare de falar de futebol e falem de música"));
    }
}
