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

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
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
use crate::civ_economia::{self, Evento};
use crate::civ_memoria::{self, Memoria, agora};
use crate::civ_visual::{self, Linha};
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
    /// A economia: dotcoins, energia, comida, sementes, terrenos, contratos e mercado.
    #[serde(default)]
    pub economia: civ_economia::Economia,
    /// Quantas falas já entraram no registro da conversa (a marca de cada uma sai daqui).
    #[serde(default)]
    pub marcas: u64,
    /// Quantas falas cada corvo vivo já fez (para saber quando refletir).
    #[serde(default)]
    pub falas_de: BTreeMap<String, u64>,
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
        self.falas_de.remove(nome);
    }

    /// Os nomes dos corvos vivos.
    pub fn vivos(&self) -> Vec<String> {
        self.populacao.as_ref().map(|c| c.corvos.iter().map(|c| c.nome.clone()).collect()).unwrap_or_default()
    }

    /// O número da próxima fala do registro.
    pub fn proxima_marca(&mut self) -> u64 {
        self.marcas += 1;
        self.marcas
    }

    fn rag_de(&self, nome: &str) -> f32 {
        self.populacao
            .as_ref()
            .and_then(|c| c.corvos.iter().find(|c| c.nome == nome))
            .map(|c| c.rag_mb)
            .unwrap_or(RAG_INICIAL)
    }
}

/// Os acontecimentos do mundo (economia, nascimentos, mortes) entram no registro da
/// conversa como falas do Cartório e no livro-razão de quem participou.
pub(crate) fn anotar_mundo(pasta: &Path, e: &mut Estado, eventos: &[Evento]) -> Vec<Linha> {
    let mut out = Vec::new();
    for ev in eventos {
        let l = Linha {
            n: e.proxima_marca(),
            quem: conversa::MUNDO.to_string(),
            texto: ev.texto.clone(),
            quando: agora(),
            responde: None,
            modelo: String::new(),
        };
        let _ = civ_visual::anotar(pasta, &l);
        for nome in &ev.envolvidos {
            if !e.vivos().contains(nome) {
                continue;
            }
            let mut m = Memoria::abrir(pasta, nome);
            m.registrar("economia", &ev.texto);
            let _ = m.salvar(pasta, e.rag_de(nome));
        }
        out.push(l);
    }
    out
}

/// Garante a bolsa de cada vivo (quem nasce ganha dotcoins e energia; a cada 2
/// nascimentos surge um terreno) e anota o que aconteceu.
fn garantir_economia(pasta: &Path, e: &mut Estado) -> Vec<Linha> {
    let vivos = e.vivos();
    let ev = e.economia.garantir(&vivos, &mut Rng::new(relogio()));
    anotar_mundo(pasta, e, &ev)
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

/// A pasta que a Civilização enxerga: `Downloads/DotCrow` (a civilização, as missões, as
/// entregas, os áudios e a pasta temporária ficam aqui dentro).
pub fn raiz_da_sandbox() -> PathBuf {
    crate::maos::pasta_de_downloads().join("DotCrow")
}

/// A pasta da civilização: `Downloads/DotCrow/civilizacao` (no Termux, a Download do
/// celular quando o dono rodou `termux-setup-storage`). Se não der para criar lá, usa a
/// pasta de dados do DotCrow. Traz o arquivo das versões antigas, se houver.
pub fn pasta(dados: &Path) -> PathBuf {
    let alvo = raiz_da_sandbox().join("civilizacao");
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
    // Grava num temporário e troca: quem lê ao mesmo tempo (o painel) nunca vê meio arquivo.
    let tmp = pasta.join(format!("{ARQUIVO}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec_pretty(e)?).context("salvando a civilização")?;
    std::fs::rename(&tmp, caminho(pasta)).context("salvando a civilização")?;
    Ok(())
}

fn carregar(pasta: &Path) -> Option<Estado> {
    let bytes = std::fs::read(caminho(pasta)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Só lê o estado salvo, sem mudar nada (para o painel, enquanto a conversa roda em
/// outro processo).
pub fn ler(pasta: &Path) -> Option<Estado> {
    carregar(pasta)
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
    if !garantir_economia(pasta, &mut e).is_empty() {
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
    let mut msgs = vec![
        format!("⚰ Deixaram de existir: {}.", vencidos.join(", ")),
        format!("🐣 Nasceram (geração {geracao}): {}.", nascidos.join(", ")),
    ];
    msgs.extend(garantir_economia(pasta, e).into_iter().map(|l| l.texto));
    salvar(pasta, e)?;
    Ok(msgs)
}

/// O relógio da economia (só com a civilização ativa): a energia cai, quem chegou a 50%
/// dorme e come, e quem não tinha comida morre de fome e dá lugar a um corvo novo.
/// Devolve os acontecimentos (já anotados) e se a população mudou.
pub fn passo_da_economia(pasta: &Path, e: &mut Estado) -> Result<(Vec<Linha>, bool)> {
    if e.pausada_desde.is_some() {
        return Ok((Vec::new(), false));
    }
    let t = agora();
    let mut ev = e.economia.tick(t);
    let (sono, famintos) = e.economia.sono(t);
    ev.extend(sono);
    let mut linhas = anotar_mundo(pasta, e, &ev);
    if famintos.is_empty() {
        return Ok((linhas, false));
    }
    let efetiva = e.civilizacao()?;
    let modelos = ler_modelos(&e.modelos.join(","))?;
    let pop = e.populacao_mut()?;
    for c in &mut pop.corvos {
        if let Some(ef) = efetiva.corvos.iter().find(|x| x.nome == c.nome) {
            c.reputacao = ef.reputacao;
        }
    }
    let nascidos = pop.substituir_por(&famintos, &modelos, relogio(), |m| {
        format!(
            "corvo {} de {} morreu de fome: dormiu com 50% de energia e não tinha comida (reputação {:.1})",
            m.genoma.personalidade.nome(),
            m.genoma.especialidade.nome(),
            m.reputacao
        )
    });
    let geracao = pop.geracao;
    let mut ev = Vec::new();
    for nome in &famintos {
        let _ = Memoria::abrir(pasta, nome).enterrar(pasta, "morreu de fome: dormiu com 50% de energia sem comida");
        e.esquecer(nome);
        ev.extend(e.economia.morrer(nome));
    }
    for nome in &nascidos {
        let mut m = Memoria::abrir(pasta, nome);
        m.registrar("geracao", &format!("nasceu na geração {geracao}, no lugar de quem morreu de fome"));
        let _ = m.salvar(pasta, RAG_INICIAL);
    }
    ev.push(Evento {
        texto: format!("🐣 Nasceram (geração {geracao}): {}.", nascidos.join(", ")),
        envolvidos: Vec::new(),
    });
    linhas.extend(anotar_mundo(pasta, e, &ev));
    linhas.extend(garantir_economia(pasta, e));
    salvar(pasta, e)?;
    Ok((linhas, true))
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
    // O tempo parado não gasta energia.
    e.economia.ultimo_tick = 0;
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
        let bolsa = e
            .economia
            .bolsas
            .get(&c.nome)
            .map(|b| format!(" · ⚡{:.0}% · 💰{} · 🍞{}", b.energia, b.dotcoins, b.comida))
            .unwrap_or_default();
        println!(
            "   {:>2}º {} {}  {}",
            i + 1,
            p.bold(&c.nome),
            p.dim(&format!("★ {:.1} · {:.1} MB · 🔎 {}{bolsa}", c.reputacao, c.rag_mb, c.consultas)),
            destino
        );
    }
    println!();
    println!("  {}", p.bold("Conversar:            dotcrow civ conversar"));
    println!("  {}", p.bold("Terrenos e economia:  dotcrow civ terrenos"));
    println!("  {}", p.bold("Julgamento:           dotcrow civ julgar"));
    println!("  {}", p.dim("Dar nota (0 a 10):    dotcrow civ nota <corvo> <nota>  ·  todas: dotcrow civ avaliar"));
    println!("  {}", p.dim("Internet:             dotcrow civ pesquisar [corvo] [termo]"));
    println!("  {}", p.dim("Memória e mortos:     dotcrow civ memoria <corvo>  ·  dotcrow civ mortos"));
    println!("  {}", p.dim("Trocar um corvo:      dotcrow civ modelo <corvo> anthropic:claude-haiku-5-5"));
    println!("  {}", p.dim("Corvos novos:         dotcrow civ nova"));
    println!();
    Ok(())
}

/// Mostra a memória de um corvo (a pasta dele, a ficha, os artefatos, o que tem) e
/// confere o livro-razão.
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
    println!(
        "  {}",
        p.dim(&format!(
            "Pasta: {} (memoria.json · rag.json · livro-razao.json · ficha.json · artefatos/)",
            civ_memoria::pasta_do_corvo(pasta, &c.nome).display()
        ))
    );
    if let Some(b) = e.economia.bolsas.get(&c.nome) {
        let terrenos: Vec<&str> = e
            .economia
            .terrenos
            .iter()
            .filter(|t| t.dono.as_deref() == Some(c.nome.as_str()))
            .map(|t| t.id.as_str())
            .collect();
        println!(
            "  ⚡ {:.0}% · 💰 {} dotcoins · 🍞 {} comida(s) · 🌰 {} semente(s) · 🗺 {}",
            b.energia,
            b.dotcoins,
            b.comida,
            b.sementes,
            if terrenos.is_empty() { "sem terreno".to_string() } else { terrenos.join(", ") }
        );
    }
    if !m.ficha.perfil.is_empty() || !m.ficha.resumo.is_empty() {
        println!("  {} {}", p.bold("Ficha:"), p.dim(format!("{} {}", m.ficha.perfil, m.ficha.resumo).trim()));
    }
    println!(
        "  {} falas · {} perguntas respondidas · {} aprendizados · {} lições · livro-razão com {} lançamentos {}",
        m.falas.len(),
        m.vinculos.len(),
        m.aprendizados.len(),
        m.licoes.len(),
        m.livro_razao.len(),
        if m.integra() { p.green("✓ íntegro") } else { p.red("✗ ADULTERADO") }
    );
    let artefatos = civ_memoria::artefatos(pasta, &c.nome);
    if !artefatos.is_empty() {
        println!("  {} {}", p.bold("Artefatos:"), artefatos.join(", "));
    }
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

fn celula(texto: &str, largura: usize) -> String {
    let t: String = texto.chars().take(largura).collect();
    let falta = largura.saturating_sub(t.chars().count());
    format!("{t}{}", " ".repeat(falta))
}

/// O mapa dos terrenos (dono, plantio, colheita, quem está lá) e a economia de cada corvo.
pub fn mostrar_terrenos(e: &Estado, p: &Paint) {
    let eco = &e.economia;
    let t = agora();
    println!();
    println!(
        "  {}  {}",
        p.bold("Os terrenos da Civilização"),
        p.dim(&format!("{} m² · {} nascimentos (a cada 2, surge 1 m²)", eco.terrenos.len(), eco.nascimentos))
    );
    if eco.terrenos.is_empty() {
        println!("  {}", p.dim("Ainda não há terrenos."));
    }
    const L: usize = 14;
    let colunas = civ_economia::LARGURA_MAPA as usize;
    for linha in eco.terrenos.chunks(colunas) {
        let borda = |a: &str, b: &str, c: &str| format!("  {a}{}{c}", vec!["─".repeat(L); linha.len()].join(b));
        println!("{}", borda("┌", "┬", "┐"));
        let mut l1 = Vec::new();
        let mut l2 = Vec::new();
        let mut l3 = Vec::new();
        for terreno in linha {
            let dono = terreno.dono.clone().unwrap_or_else(|| "à venda".into());
            l1.push(p.bold(&celula(&format!("{} {dono}", terreno.id), L)));
            let estado = match &terreno.plantio {
                None => p.dim(&celula(". vazio", L)),
                Some(pl) if pl.pronto <= t => p.yellow(&celula("# pronto!", L)),
                Some(pl) => p.green(&celula(&format!("* cresce {}s", pl.pronto - t), L)),
            };
            l2.push(estado);
            let la: Vec<&str> = eco
                .bolsas
                .iter()
                .filter(|(_, b)| b.local.as_deref() == Some(terreno.id.as_str()))
                .map(|(n, _)| n.as_str())
                .collect();
            l3.push(p.cyan(&celula(&if la.is_empty() { String::new() } else { format!("@ {}", la.join(",")) }, L)));
        }
        println!("  │{}│", l1.join("│"));
        println!("  │{}│", l2.join("│"));
        println!("  │{}│", l3.join("│"));
        println!("{}", borda("└", "┴", "┘"));
    }
    println!("  {}", p.dim(". vazio · * plantado (30 s até colher) · # pronto para colher · @ quem está lá"));
    println!();
    println!("  {}", p.bold("Bolsas"));
    for (nome, b) in &eco.bolsas {
        let dorme = if b.dormindo_ate.is_some_and(|x| x > t) { " 😴" } else { "" };
        let fazendo = if b.fazendo.is_empty() { String::new() } else { format!(" · {}", b.fazendo) };
        println!(
            "   {} {}{dorme}",
            p.bold(&celula(nome, 10)),
            p.dim(&format!(
                "⚡{:>3.0}% · 💰{:>4} · 🍞{:>2} · 🌰{:>2}{fazendo}",
                b.energia, b.dotcoins, b.comida, b.sementes
            ))
        );
    }
    if let Some(m) = eco.mercado_aberto(t) {
        println!("  {}", p.yellow(&format!("🔔 Mercado aberto por {} (mais {} s)", m.aberto_por, m.ate - t)));
    }
    if !eco.podio.is_empty() {
        println!("  {}", p.dim(&format!("Pódio (abre o mercado): {}", eco.podio.join(", "))));
    }
    if !eco.ofertas.is_empty() {
        println!("  {}", p.bold("Ofertas"));
        for o in eco.ofertas.iter().rev().take(8) {
            println!(
                "   {} {} vende {} por {} {}",
                o.id,
                o.de,
                o.item.nome(o.qtd),
                o.preco,
                o.para.as_deref().map(|x| format!("(para {x})")).unwrap_or_default()
            );
        }
    }
    if !eco.diario.is_empty() {
        println!("  {}", p.bold("Últimos acontecimentos"));
        for d in eco.diario.iter().rev().take(8) {
            println!("   {}", p.dim(d));
        }
    }
    println!();
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

/// A ficha fixa do corvo (quem ele é pelo genoma), escrita na primeira vez.
pub(crate) fn garantir_ficha(mem: &mut Memoria, corvo: &Corvo) {
    let g = &corvo.genoma;
    let nascido = mem.livro_razao.first().map(|l| civ_memoria::data(l.quando)).unwrap_or_default();
    let perfil = format!(
        "{}: {} ({}), especialista em {}, geração {}, nascido em {nascido}.",
        corvo.nome,
        g.personalidade.nome(),
        g.personalidade.jeito(),
        g.especialidade.nome(),
        g.geracao
    );
    if mem.ficha.perfil != perfil {
        mem.ficha.perfil = perfil;
    }
}

/// O que o corvo sabe de si para esta fala: a ficha, as lições, lembranças da memória
/// (RAG, pelo significado), os artefatos, o que tem (economia), as consultas e se está
/// condenado.
fn contexto_do_corvo(
    corvo: &Corvo,
    mem: &Memoria,
    historico: &[Fala],
    vetor: Option<&[f32]>,
    e: &Estado,
    pasta: &Path,
) -> String {
    let recente: String = historico.iter().rev().take(4).map(|f| f.texto.as_str()).collect::<Vec<_>>().join(" ");
    let mut s = String::new();
    if !mem.ficha.resumo.is_empty() {
        s.push_str(&format!("\n\nSua ficha (quem você é e o que já viveu, escrita por você):\n{}", mem.ficha.resumo));
    }
    let licoes = mem.ultimas_licoes(5);
    if !licoes.is_empty() {
        s.push_str("\n\nSuas lições (o que você aprendeu para ser um agente melhor):\n- ");
        s.push_str(&licoes.join("\n- "));
    }
    let lembrancas = mem.lembrancas(&recente, 6, vetor);
    if !lembrancas.is_empty() {
        s.push_str("\n\nO que você lembra (sua memória própria, de conversas anteriores também):\n- ");
        s.push_str(&lembrancas.join("\n- "));
    }
    let artefatos = civ_memoria::artefatos(pasta, &corvo.nome);
    if !artefatos.is_empty() {
        s.push_str(&format!(
            "\n\nSeus artefatos (na sua pasta; dá para vender ou dar): {}.",
            artefatos.iter().take(10).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    if corvo.consultas > 0 {
        s.push_str(&format!(
            "\n\nVocê ganhou {} consulta(s) à internet. Para usar uma, escreva [PESQUISAR: sua pergunta] na sua fala. \
A mesma pergunta é procurada em vários buscadores e conta como UMA consulta, e só se trouxer algo novo. O que vier \
passa pela Alfândega e entra na sua memória.",
            corvo.consultas
        ));
    }
    s.push_str(&e.economia.situacao(&corvo.nome, agora(), corvo.consultas));
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

/// O corvo reflete sobre o que viveu: guarda lições para ser melhor (com embedding) e
/// reescreve a ficha, que vai com ele para as próximas conversas.
pub(crate) async fn refletir(
    store: &Store,
    pasta: &Path,
    voz: &Voz,
    corvo: &Corvo,
    civ: &Civilizacao,
    acontecimentos: &str,
) -> Result<Vec<String>> {
    let antigas = Memoria::abrir(pasta, &corvo.nome).ultimas_licoes(5);
    let (texto, _, _) = pedir(
        voz,
        conversa::perfil(corvo, civ, None),
        conversa::pedido_reflexao(&corvo.nome, acontecimentos, &antigas),
        1400,
    )
    .await?;
    let licoes = conversa::ler_licoes(&texto);
    let vetores = vetorizar(store, &licoes).await;
    // Abre de novo depois da espera: o que mudou nesse meio-tempo não se perde.
    let mut mem = Memoria::abrir(pasta, &corvo.nome);
    mem.refletiu(&licoes, vetores);
    if let Some(f) = conversa::ler_ficha(&texto) {
        mem.atualizar_ficha(&f);
        mem.registrar("ficha", "reescreveu a própria ficha");
    }
    garantir_ficha(&mut mem, corvo);
    mem.salvar(pasta, corvo.rag_mb)?;
    Ok(licoes)
}

/// O resumo do que aconteceu com o corvo, para ele refletir.
pub(crate) fn acontecimentos(mem: &Memoria) -> String {
    let falas: Vec<String> = mem.falas.iter().rev().take(8).map(|f| format!("- você disse: {}", f.texto)).collect();
    let livro: Vec<String> =
        mem.livro_razao.iter().rev().take(10).map(|l| format!("- {}: {}", l.tipo, l.descricao)).collect();
    let ficha =
        if mem.ficha.resumo.is_empty() { String::new() } else { format!("\nSua ficha atual: {}", mem.ficha.resumo) };
    format!("{}\n{}{ficha}", falas.join("\n"), livro.join("\n"))
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
    println!(
        "  {}",
        p.dim("Escreva e aperte Enter para falar com o bando. Chame pelo nome (\"Nyx, ...\") e é ele quem responde.")
    );
    println!(
        "  {}",
        p.dim("Cada fala tem uma marca (★12); ↳ mostra a qual fala ela responde. /terrenos mostra o mapa e as bolsas.")
    );
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

/// Lê `[CRIAR: tipo | título]` (também `[CRIAR: tipo: título]`) da fala: o corvo cria um
/// artefato (livro, música, poema, arte...) na pasta dele. Devolve a fala sem a tag e
/// (tipo, título).
pub fn extrair_criacao(texto: &str) -> (String, Option<(String, String)>) {
    for (ini, _) in texto.match_indices('[') {
        let resto = &texto[ini..];
        let Some(fim) = resto.find(']') else { continue };
        let Some((chave, args)) = resto[1..fim].split_once(':') else { continue };
        if !matches!(conversa::normalizar(chave.trim()).as_str(), "criar" | "crio" | "escrever" | "compor") {
            continue;
        }
        let (tipo, titulo) = match args.split_once(['|', ':']) {
            Some((t, ti)) => (t.trim().to_string(), ti.trim().to_string()),
            None => {
                let a = args.trim();
                match a.split_once(char::is_whitespace) {
                    Some((t, ti)) => (t.to_string(), ti.trim().to_string()),
                    None => ("texto".to_string(), a.to_string()),
                }
            }
        };
        let titulo = titulo.trim_matches(|c| c == '"' || c == '“' || c == '”' || c == '\'').to_string();
        let limpo = format!("{}{}", &texto[..ini], &resto[fim + 1..]).split_whitespace().collect::<Vec<_>>().join(" ");
        if titulo.is_empty() {
            return (limpo, None);
        }
        return (limpo, Some((tipo.to_lowercase(), titulo)));
    }
    (texto.to_string(), None)
}

fn curto(s: &str, n: usize) -> String {
    let t: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > n { format!("{}…", t.chars().take(n).collect::<String>()) } else { t }
}

/// Um corvo cria um artefato com uma chamada à parte (a obra não conta no limite de
/// palavras da fala). Um por corvo a cada 5 minutos.
async fn criar_artefato_do_corvo(
    pasta: &Path,
    voz: &Voz,
    corvo: &Corvo,
    civ: &Civilizacao,
    tipo: &str,
    titulo: &str,
) -> Result<Evento> {
    let t = agora();
    if let Some(l) = Memoria::abrir(pasta, &corvo.nome).livro_razao.iter().rev().find(|l| l.tipo == "artefato")
        && t < l.quando + 300
    {
        bail!("criou um artefato há pouco; o próximo em {} s", l.quando + 300 - t);
    }
    let n = conversa::normalizar(tipo);
    let instrucao = if ["arte", "desenho", "pintura", "imagem", "ilustracao", "svg", "quadro"].contains(&n.as_str()) {
        "Desenhe como um SVG de 256×256: só formas (rect, circle, ellipse, path, polygon, line, text), sem script \
e sem imagens de fora. Responda só com o <svg>…</svg>."
    } else if ["musica", "cancao", "samba", "rap", "hino", "letra"].contains(&n.as_str()) {
        "Escreva a letra com a cifra dos acordes por cima dos versos e, se quiser, a melodia em notação ABC num bloco \
```abc. Até 400 palavras."
    } else {
        "Escreva em Markdown, com começo, meio e fim. Até 600 palavras."
    };
    let (obra, _, _) = pedir(
        voz,
        conversa::perfil(corvo, civ, None),
        format!(
            "Você, {}, decidiu criar um(a) {tipo} chamado \"{titulo}\". {instrucao} Só a obra, sem comentários antes ou \
depois.",
            corvo.nome
        ),
        3200,
    )
    .await?;
    if obra.trim().is_empty() {
        bail!("a obra veio vazia");
    }
    let mut mem = Memoria::abrir(pasta, &corvo.nome);
    let (caminho, hash) = mem.criar_artefato(pasta, tipo, titulo, &obra)?;
    mem.salvar(pasta, corvo.rag_mb)?;
    let arquivo = caminho.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Ok(Evento {
        texto: format!(
            "📜 {} criou o {tipo} “{titulo}” ({arquivo} · sha256 {}…). Dá para vender ou dar: [VENDER: artefato {arquivo} por 20]",
            corvo.nome,
            &hash[..12]
        ),
        envolvidos: Vec::new(),
    })
}

/// Um aviso do mundo na tela.
fn mostrar_mundo(p: &Paint, l: &Linha) {
    println!("  {} {}", p.dim(&format!("📜 {}", conversa::MUNDO)), p.dim(&l.texto));
}

/// A fala de um corvo na tela: o desenho dele, a marca, a quem responde e o texto.
#[allow(clippy::too_many_arguments)]
fn mostrar_fala(
    e: &Estado,
    corvo: &Corvo,
    modelo: &Modelo,
    l: &Linha,
    recentes: &[Linha],
    cores: civ_visual::Cores,
    p: &Paint,
) {
    let sprite = civ_visual::sprite_do_corvo(corvo.genoma.semente, e.economia.podio.contains(&corvo.nome));
    let resp = l
        .responde
        .and_then(|r| recentes.iter().find(|x| x.n == r))
        .map(|r| format!(" ↳ {} {}", civ_visual::marca(r.n, cores), p.dim(&r.quem)))
        .unwrap_or_default();
    let energia = e.economia.bolsas.get(&corvo.nome).map(|b| format!(" · ⚡{:.0}%", b.energia)).unwrap_or_default();
    let cabeca = format!(
        "{} {}{resp} {}",
        p.c256b(cor_provedor(&modelo.provedor), &corvo.nome),
        civ_visual::marca(l.n, cores),
        p.dim(&format!("· {} · {} palavra(s){energia}", modelo.modelo, l.texto.split_whitespace().count()))
    );
    println!("{}", civ_visual::bloco(Some(&sprite), &cabeca, &l.texto, cores));
}

/// Os corvos conversam até o dono dizer `pare` (ou Ctrl+C). `rx` traz as linhas que o
/// dono digita (veja [`teclado_stdin`]; o modo terminal usa o próprio editor de linha).
/// A conversa fica registrada em `conversa.jsonl` e é retomada na próxima vez.
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
    let cores = civ_visual::cores(p.on);
    cabecalho(&civ, &avisos, &o, p);
    // O tempo com a civilização fechada não gasta energia.
    estado.economia.ultimo_tick = 0;

    let mut rng = Rng::new(relogio());
    let mut ativacao = conversa::Ativacao::default();
    // Retoma a conversa de onde parou (os corvos lembram do que estavam falando).
    let mut recentes: Vec<Linha> = civ_visual::ultimas(pasta, 40);
    let mut historico: Vec<Fala> = recentes.iter().map(|l| Fala::nova(&l.quem, &l.texto)).collect();
    if let Some(ult) = recentes.iter().rev().find(|l| l.quem != conversa::MUNDO) {
        println!(
            "  {}",
            p.dim(&format!("↺ Retomando a conversa de onde parou (última fala em {}):", civ_memoria::data(ult.quando)))
        );
        for l in
            recentes.iter().rev().filter(|l| l.quem != conversa::MUNDO).take(3).collect::<Vec<_>>().into_iter().rev()
        {
            println!(
                "    {}",
                p.dim(&format!(
                    "{} {}: {}",
                    civ_visual::marca(l.n, civ_visual::Cores::Nenhuma),
                    l.quem,
                    curto(&l.texto, 110)
                ))
            );
        }
        println!();
    }
    // Quem o dono chamou pelo nome, na ordem, com a fala dele a que vão responder.
    let mut fila: VecDeque<(String, u64)> = VecDeque::new();
    let mut falas: u32 = 0;
    let mut chamadas: u32 = 0;
    let mut erros_seguidos = 0;
    let mut teclado_aberto = true;
    let mut agora_ja = true;

    'conversa: loop {
        if !agora_ja {
            // Como neurônios: o tempo entre uma fala e outra não é fixo. Com o mercado
            // aberto, o bando fala mais rápido (é só 1 minuto).
            let base = if estado.economia.mercado_aberto(agora()).is_some() {
                o.intervalo.min(Duration::from_secs(6))
            } else {
                o.intervalo
            };
            let espera = tokio::time::sleep(base.mul_f32(rng.faixa(0.5, 1.5)));
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
                        Some(l) if matches!(l.trim(), "/terrenos" | "/mapa" | "/economia") => mostrar_terrenos(&estado, p),
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
                            vozes = self::vozes(store, &civ)?.0;
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
                            let texto = l.trim().to_string();
                            let n = estado.proxima_marca();
                            let linha = Linha {
                                n,
                                quem: conversa::DONO.to_string(),
                                texto: texto.clone(),
                                quando: agora(),
                                responde: None,
                                modelo: String::new(),
                            };
                            let _ = civ_visual::anotar(pasta, &linha);
                            let chamados: Vec<String> =
                                conversa::chamados(&civ, &texto).into_iter().map(|i| civ.corvos[i].nome.clone()).collect();
                            let para = if chamados.is_empty() { "o bando".to_string() } else { chamados.join(", ") };
                            println!("  {} {}", civ_visual::marca(n, cores), p.dim(&format!("sua fala, para {para}")));
                            for nome in chamados {
                                if estado.economia.dormindo(&nome, agora()) {
                                    println!("  {}", p.dim(&format!("😴 {nome} está dormindo; responde assim que acordar.")));
                                }
                                fila.push_back((nome, n));
                            }
                            historico.push(Fala::nova(conversa::DONO, &texto));
                            recentes.push(linha);
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
                historico.push(Fala::nova(conversa::MUNDO, &msgs.join(" ")));
            }
            Ok(_) => {}
            Err(err) => println!("  {}", p.yellow(&format!("prazos: {err:#}"))),
        }
        // A economia anda: energia, sono, fome, colheitas prontas, mercado.
        match passo_da_economia(pasta, &mut estado) {
            Ok((linhas, mudou)) => {
                for l in &linhas {
                    mostrar_mundo(p, l);
                    historico.push(Fala::nova(conversa::MUNDO, &l.texto));
                }
                recentes.extend(linhas);
                if mudou {
                    civ = estado.civilizacao()?;
                    vozes = self::vozes(store, &civ)?.0;
                }
            }
            Err(err) => println!("  {}", p.yellow(&format!("economia: {err:#}"))),
        }

        // Quem fala: quem o dono chamou pelo nome (na ordem); senão, quem dispara (como
        // neurônios). Quem dorme não fala.
        let t = agora();
        let (mut i, pensa) = ativacao.passo(&civ, &historico, &mut rng);
        let mut chamado: Option<u64> = None;
        fila.retain(|(nome, _)| civ.corvos.iter().any(|c| &c.nome == nome));
        if let Some(pos) = fila.iter().position(|(nome, _)| !estado.economia.dormindo(nome, t))
            && let Some((nome, n)) = fila.remove(pos)
            && let Some(j) = civ.corvos.iter().position(|c| c.nome == nome)
        {
            i = j;
            chamado = Some(n);
        }
        if chamado.is_none() && estado.economia.dormindo(&civ.corvos[i].nome, t) {
            let nivel = |j: usize| ativacao.nivel.get(j).copied().unwrap_or(0.0);
            let acordado = (0..civ.corvos.len())
                .filter(|j| !estado.economia.dormindo(&civ.corvos[*j].nome, t))
                .max_by(|a, b| nivel(*a).partial_cmp(&nivel(*b)).unwrap_or(std::cmp::Ordering::Equal));
            match acordado {
                Some(j) => i = j,
                None => {
                    println!("  {}", p.dim("😴 O bando inteiro está dormindo…"));
                    continue;
                }
            }
        }
        if let Some(j) = pensa.filter(|j| *j != i && !estado.economia.dormindo(&civ.corvos[*j].nome, t)) {
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
                if !pensamento.is_empty() && !conversa::eh_silencio(&pensamento) {
                    println!("  {}", p.dim(&format!("💭 {} pensa: {pensamento}", quem.nome)));
                    let mut m = Memoria::abrir(pasta, &quem.nome);
                    m.pensou(&pensamento);
                    let _ = m.salvar(pasta, quem.rag_mb);
                    estado.economia.gastar(&quem.nome, civ_economia::ENERGIA_POR_PENSAMENTO);
                }
            }
            chamadas += 1;
        }
        let corvo = civ.corvos[i].clone();
        let mut mem = Memoria::abrir(pasta, &corvo.nome);
        garantir_ficha(&mut mem, &corvo);
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
            contexto_do_corvo(&corvo, &mem, &historico, vetor.as_deref(), &estado, pasta)
        );
        let usuario = conversa::pedido(&corvo.nome, &historico, max_palavras, chamado.is_some());
        if p.on {
            print!("  {}", p.dim(&format!("… {} está pensando ({max_palavras} palavras no máximo)", corvo.nome)));
            let _ = std::io::stdout().flush();
        }
        // Folga de tokens para modelos que raciocinam antes (gpt-5-nano) e para as ações.
        let max_tokens = (max_palavras as u32) * 3 + 1100;
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
                // As ações entre colchetes saem antes de contar as palavras.
                let (sem_pesquisa, termo) = conversa::extrair_pesquisa(&bruto);
                let (sem_criar, criar) = extrair_criacao(&sem_pesquisa);
                let (sem_acoes, acoes) = civ_economia::extrair_acoes(&sem_criar);
                let silencio = conversa::eh_silencio(&sem_acoes);
                let texto = if silencio {
                    String::new()
                } else {
                    conversa::limpar(&sem_acoes.replace(conversa::SILENCIO, " "), &corvo.nome, max_palavras)
                };
                if let Some(n) = nota {
                    println!("  {}", p.dim(&format!("↻ {n}")));
                }
                estado.economia.gastar(&corvo.nome, civ_economia::ENERGIA_POR_FALA);
                if texto.is_empty() {
                    if acoes.is_empty() && termo.is_none() && criar.is_none() {
                        println!("  {}", p.dim(&format!("🤫 {} preferiu ficar em silêncio.", corvo.nome)));
                    }
                    if let Some(a) = ativacao.nivel.get_mut(i) {
                        *a = 0.0;
                    }
                } else {
                    let responde = civ_visual::responde_a(&civ, &recentes, &corvo.nome, &texto, chamado);
                    let linha = Linha {
                        n: estado.proxima_marca(),
                        quem: corvo.nome.clone(),
                        texto: texto.clone(),
                        quando: agora(),
                        responde,
                        modelo: rotulo(&modelo),
                    };
                    let _ = civ_visual::anotar(pasta, &linha);
                    mostrar_fala(&estado, &corvo, &modelo, &linha, &recentes, cores, p);
                    // Respondeu a uma pergunta (do dono ou de outro corvo): vira um vínculo,
                    // que os jurados e o oráculo avaliam.
                    if let Some(r) = responde.and_then(|r| recentes.iter().find(|x| x.n == r))
                        && r.texto.contains('?')
                        && r.quem != conversa::MUNDO
                    {
                        let pergunta = format!("{} {}", civ_visual::forma(r.n), r.texto);
                        mem.vincular(&r.quem, &pergunta, &texto);
                        if let Some(v) =
                            vetorizar(store, &[format!("{} {texto}", r.texto)]).await.and_then(|mut v| v.pop())
                            && let Some(ult) = mem.vinculos.last_mut()
                        {
                            ult.vetor = Some(civ_memoria::compactar(&v));
                        }
                    }
                    mem.falou(&texto);
                    let _ = mem.salvar(pasta, corvo.rag_mb);
                    historico.push(Fala::nova(&corvo.nome, &texto));
                    recentes.push(linha);
                    if recentes.len() > 60 {
                        recentes.drain(..recentes.len() - 60);
                    }
                    falas += 1;
                    estado.falas_total += 1;
                    let k = {
                        let k = estado.falas_de.entry(corvo.nome.clone()).or_default();
                        *k += 1;
                        *k
                    };
                    // Na 5ª fala e depois a cada 20, o corvo reflete: tira lições e
                    // reescreve a ficha (é assim que ele lembra quem é).
                    if k == 5 || k.is_multiple_of(20) {
                        match refletir(
                            store,
                            pasta,
                            &vozes[i],
                            &corvo,
                            &civ,
                            &acontecimentos(&Memoria::abrir(pasta, &corvo.nome)),
                        )
                        .await
                        {
                            Ok(l) if !l.is_empty() => {
                                println!("  {}", p.dim(&format!("🧠 {} refletiu: {}", corvo.nome, l.join(" · "))))
                            }
                            _ => {}
                        }
                        chamadas += 1;
                    }
                }
                // As ações da economia e a criação de artefatos.
                let vivos = estado.vivos();
                let mut eventos = Vec::new();
                for a in &acoes {
                    match estado.economia.executar(&corvo.nome, a, agora(), &vivos, pasta) {
                        Ok(ev) => eventos.push(ev),
                        Err(msg) => eventos
                            .push(Evento { texto: format!("✗ {}: {msg}.", corvo.nome), envolvidos: Vec::new() }),
                    }
                }
                if let Some((tipo, titulo)) = criar {
                    estado.economia.gastar(&corvo.nome, 3.0);
                    match criar_artefato_do_corvo(pasta, &vozes[i], &corvo, &civ, &tipo, &titulo).await {
                        Ok(ev) => eventos.push(ev),
                        Err(err) => eventos.push(Evento {
                            texto: format!("✗ {} não conseguiu criar “{titulo}”: {err:#}.", corvo.nome),
                            envolvidos: Vec::new(),
                        }),
                    }
                    chamadas += 1;
                }
                let linhas = anotar_mundo(pasta, &mut estado, &eventos);
                for l in &linhas {
                    mostrar_mundo(p, l);
                    historico.push(Fala::nova(conversa::MUNDO, &l.texto));
                }
                recentes.extend(linhas);
                let _ = salvar(pasta, &estado);
                if let Some(t) = termo {
                    if corvo.consultas > 0 {
                        match crate::civ_internet::consultar(store, pasta, &mut estado, &corvo.nome, &t, p).await {
                            Ok(Some(resumo)) => historico
                                .push(Fala::nova(&corvo.nome, &format!("(pesquisei \"{t}\" e aprendi: {resumo})"))),
                            Ok(None) => historico.push(Fala::nova(
                                conversa::MUNDO,
                                &format!(
                                    "🔎 A pesquisa de {} (\"{t}\") não trouxe nada novo; a consulta não foi gasta.",
                                    corvo.nome
                                ),
                            )),
                            Err(err) => println!("  {}", p.yellow(&format!("🔎 {}: {err:#}", corvo.nome))),
                        }
                        civ = estado.civilizacao()?;
                        chamadas += 1;
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
                if !texto.is_empty() && estado.falas_total > 0 && estado.falas_total.is_multiple_of(AVALIAR_A_CADA) {
                    println!("  {}", p.bold(&format!("📣 {} falas: hora de avaliar o bando.", estado.falas_total)));
                    if let Err(err) = pedir_notas_do_dono(pasta, &mut estado, p, &mut rx).await {
                        println!("  {}", p.yellow(&format!("{err:#}")));
                    }
                    if let Err(err) = crate::civ_julgamento::julgar(store, pasta, &mut estado, p).await {
                        println!("  {}", p.yellow(&format!("{err:#}")));
                    }
                    civ = estado.civilizacao()?;
                    vozes = self::vozes(store, &civ)?.0;
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
        assert!(msgs[0].starts_with("⚰") && msgs[1].starts_with("🐣"));
        // A economia acompanha: os mortos perdem a bolsa, os novos nascem com a deles e
        // a cada 2 nascimentos surge um terreno (5 da Gênese + 2 agora, com 15 nascidos).
        assert!(msgs.iter().any(|m| m.contains("dotcoins")));
        let depois = nomes(&e);
        assert!(depois.iter().all(|n| e.economia.bolsas.contains_key(n)));
        assert!(condenados.iter().all(|n| !e.economia.bolsas.contains_key(n)));
        assert_eq!(e.economia.nascimentos, 15);
        assert_eq!(e.economia.terrenos.len(), 7);
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
    fn fome_mata_e_nasce_outro() {
        let d = pasta_teste();
        let mut e = abrir(&d, Some(9), None).unwrap_or_else(|e| panic!("{e}"));
        let vivos = nomes(&e);
        let (faminto, alimentado) = (vivos[0].clone(), vivos[1].clone());
        for (n, b) in e.economia.bolsas.iter_mut() {
            b.energia = if *n == faminto || *n == alimentado { 50.0 } else { 90.0 };
            b.comida = if *n == alimentado { 5 } else { 0 };
        }
        let (linhas, mudou) = passo_da_economia(&d, &mut e).unwrap_or_else(|e| panic!("{e}"));
        assert!(mudou);
        let agora_vivos = nomes(&e);
        assert!(!agora_vivos.contains(&faminto), "morreu de fome");
        assert!(agora_vivos.contains(&alimentado) && e.economia.dormindo(&alimentado, agora() + 1));
        assert_eq!(agora_vivos.len(), 10, "nasce outro no lugar");
        assert!(linhas.iter().any(|l| l.texto.contains("morreu de fome")));
        let civ = e.civilizacao().unwrap_or_else(|e| panic!("{e}"));
        assert!(civ.livro_dos_mortos.iter().any(|b| b.nome == faminto && b.licao.contains("fome")));
        assert!(d.join("mortos").read_dir().is_ok_and(|mut r| r.next().is_some()), "a alma vai para mortos/");
        // O registro da conversa tem os avisos do Cartório (para retomar e para o painel).
        assert!(civ_visual::ler(&d).iter().any(|l| l.quem == conversa::MUNDO && l.texto.contains("fome")));
        // Pausada, o relógio não anda.
        e.pausada_desde = Some(agora());
        assert!(passo_da_economia(&d, &mut e).unwrap_or_else(|e| panic!("{e}")).0.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn le_o_pedido_de_criar_artefato() {
        assert_eq!(
            extrair_criacao("Vou escrever. [CRIAR: livro | O Voo da Noite] Aguardem."),
            ("Vou escrever. Aguardem.".to_string(), Some(("livro".to_string(), "O Voo da Noite".to_string())))
        );
        assert_eq!(extrair_criacao("[criar: música: \"Lua\"]").1, Some(("música".to_string(), "Lua".to_string())));
        assert_eq!(
            extrair_criacao("[CRIAR: poema Chuva fina]").1,
            Some(("poema".to_string(), "Chuva fina".to_string()))
        );
        assert_eq!(extrair_criacao("sem nada [PLANTAR: T1]").1, None);
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
