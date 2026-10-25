//! `dotcrow civilizacao`: os 10 corvos da Civilização (v2) no terminal.
//!
//! - `dotcrow civ` mostra o bando (sempre o mesmo, salvo em `civilizacao.json`).
//! - `dotcrow civ conversar` faz os corvos conversarem de verdade, cada um com o seu
//!   modelo; você escreve no meio e eles respondem; `pare` encerra.
//! - `dotcrow civ modelo <corvo> <provedor:modelo>` troca o modelo de um corvo.
//! - `dotcrow civ nova` faz uma Gênese nova.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use dotcrow_agents::civilizacao::{
    Civilizacao, Corvo, Modelo, Tamanho, conversa, conversa::Fala, cor_provedor, desenhar,
};
use dotcrow_providers::{CompletionRequest, LlmProvider, Message, Role, Tier, catalog};
use serde::{Deserialize, Serialize};

use crate::Paint;
use crate::store::Store;

/// Modelos padrão dos corvos: metade em cada um (os mais baratos de cada provedor).
pub const MODELOS_PADRAO: &str = "openai:gpt-5-nano,anthropic:claude-haiku-5-5";
const ARQUIVO: &str = "civilizacao.json";

/// O que fica salvo para o bando ser sempre o mesmo entre uma execução e outra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estado {
    /// Semente da Gênese.
    pub semente: u64,
    /// Modelos em rodízio, `provedor:modelo`.
    pub modelos: Vec<String>,
    /// Modelo escolhido para um corvo específico (vence o rodízio).
    #[serde(default)]
    pub por_corvo: BTreeMap<String, String>,
}

impl Estado {
    /// Monta a civilização a partir do estado salvo.
    pub fn civilizacao(&self) -> Result<Civilizacao> {
        let modelos = ler_modelos(&self.modelos.join(","))?;
        let mut civ = Civilizacao::genese(self.semente, &modelos);
        for (nome, m) in &self.por_corvo {
            let modelo = ler_modelo(m)?;
            if let Some(c) = civ.por_nome_mut(nome) {
                c.definir_modelo(modelo);
            }
        }
        Ok(civ)
    }
}

fn relogio() -> u64 {
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

fn caminho(dir: &Path) -> std::path::PathBuf {
    dir.join(ARQUIVO)
}

fn salvar(dir: &Path, e: &Estado) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("criando {}", dir.display()))?;
    std::fs::write(caminho(dir), serde_json::to_vec_pretty(e)?).context("salvando a civilização")?;
    Ok(())
}

fn carregar(dir: &Path) -> Option<Estado> {
    let bytes = std::fs::read(caminho(dir)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Carrega o bando salvo (ou faz a Gênese na primeira vez). `semente` faz uma Gênese
/// com aquela semente; `modelos` troca o rodízio de modelos. O que mudar é salvo.
pub fn abrir(dir: &Path, semente: Option<u64>, modelos: Option<&str>) -> Result<Estado> {
    let salvo = carregar(dir);
    let mut e = salvo.clone().unwrap_or_else(|| Estado {
        semente: semente.unwrap_or_else(relogio),
        modelos: MODELOS_PADRAO.split(',').map(str::to_string).collect(),
        por_corvo: BTreeMap::new(),
    });
    if let Some(s) = semente
        && s != e.semente
    {
        e.semente = s;
        e.por_corvo.clear();
    }
    if let Some(m) = modelos {
        let lista = ler_modelos(m)?;
        e.modelos = lista.iter().map(|x| format!("{}:{}", x.provedor, x.modelo)).collect();
    }
    if salvo.as_ref() != Some(&e) {
        salvar(dir, &e)?;
    }
    Ok(e)
}

/// Nova Gênese: corvos novos, mesmo rodízio de modelos.
pub fn nova(dir: &Path, semente: Option<u64>) -> Result<Estado> {
    let mut e = abrir(dir, None, None)?;
    e.semente = semente.unwrap_or_else(relogio);
    e.por_corvo.clear();
    salvar(dir, &e)?;
    Ok(e)
}

/// Troca o modelo de um corvo e salva.
pub fn trocar_modelo(dir: &Path, corvo: &str, modelo: &str) -> Result<(String, Modelo)> {
    let mut e = abrir(dir, None, None)?;
    let m = ler_modelo(modelo)?;
    let civ = e.civilizacao()?;
    let i = conversa::citado(&civ, corvo, None)
        .with_context(|| format!("não achei o corvo \"{corvo}\". Veja os nomes com: dotcrow civ --compacto"))?;
    let nome = civ.corvos[i].nome.clone();
    e.por_corvo.insert(nome.clone(), format!("{}:{}", m.provedor, m.modelo));
    salvar(dir, &e)?;
    Ok((nome, m))
}

/// Mostra o bando.
pub fn mostrar(e: &Estado, compacto: bool, p: &Paint) -> Result<()> {
    let civ = e.civilizacao()?;
    println!();
    println!(
        "  {}  {}",
        p.bold("A Civilização dos Corvos"),
        p.dim(&format!("geração {} · {} corvos · Sandbox", civ.geracao, civ.corvos.len()))
    );
    println!(
        "  {}\n",
        p.dim(&format!("Opus 5.5 fica nos pontos de controle; os corvos usam, em rodízio: {}.", e.modelos.join(", ")))
    );
    let tam = if compacto { Tamanho::Compacto } else { Tamanho::Medio };
    for corvo in &civ.corvos {
        println!("{}\n", desenhar(corvo, tam, p.on));
    }
    println!("  {}", p.bold("Conexões externas de hoje"));
    println!("  {}", p.dim("só os 3 melhores do ranking saem para o mundo — 3, 2 e 1 conexão:"));
    for (nome, n) in civ.conexoes_do_dia().into_iter().take(3) {
        println!("    {} {}", p.bold(&nome), p.dim(&format!("· {n} conexão(ões)")));
    }
    println!();
    println!("  {}", p.bold("Conversar com eles:   dotcrow civ conversar"));
    println!("  {}", p.dim("Trocar um corvo:      dotcrow civ modelo <corvo> anthropic:claude-haiku-5-5"));
    println!("  {}", p.dim(&format!("Trocar o rodízio:     dotcrow civ --modelos {MODELOS_PADRAO}")));
    println!("  {}", p.dim("Corvos novos:         dotcrow civ nova"));
    println!();
    Ok(())
}

/// Palavras que encerram a conversa.
pub fn eh_parada(linha: &str) -> bool {
    let l = linha.trim().trim_start_matches('/').to_lowercase();
    matches!(l.as_str(), "pare" | "parar" | "para" | "sair" | "chega" | "fim" | "stop" | "q" | "quit" | "exit")
}

/// Um corvo pronto para falar: o provedor principal e um reserva de outro provedor.
struct Voz {
    principal: (Modelo, Arc<dyn LlmProvider>),
    reserva: Option<(Modelo, Arc<dyn LlmProvider>)>,
}

fn rotulo(m: &Modelo) -> String {
    format!("{}:{}", m.provedor, m.modelo)
}

/// Monta a voz de cada corvo, trocando por um modelo com chave quando falta a chave
/// do provedor do corvo. Devolve também os avisos para mostrar ao dono.
fn vozes(store: &Store, civ: &Civilizacao) -> Result<(Vec<Voz>, Vec<String>)> {
    let com_chave: HashSet<String> = store.list_keys().into_iter().map(|k| k.provider).collect();
    let usavel = |prov: &str| catalog::info(prov).is_some_and(|i| !i.needs_key || com_chave.contains(prov));
    // Modelos disponíveis (com chave), na ordem em que aparecem no bando.
    let mut disponiveis: Vec<Modelo> = Vec::new();
    for c in &civ.corvos {
        let m = &c.genoma.modelo;
        if usavel(&m.provedor) && !disponiveis.contains(m) {
            disponiveis.push(m.clone());
        }
    }
    if disponiveis.is_empty() {
        let provs: Vec<String> = civ.corvos.iter().map(|c| c.genoma.modelo.provedor.clone()).collect();
        let mut provs: Vec<String> = provs.into_iter().collect::<HashSet<_>>().into_iter().collect();
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

/// Opções da conversa.
pub struct Opcoes {
    /// Pausa entre uma fala e outra.
    pub intervalo: Duration,
    /// Para sozinho depois de tantas falas dos corvos (`None` = até você dizer pare).
    pub max_falas: Option<u32>,
    /// Tema da conversa (livre se vazio).
    pub tema: Option<String>,
}

async fn falar(
    voz: &Voz,
    corvo: &Corvo,
    civ: &Civilizacao,
    historico: &[Fala],
    tema: Option<&str>,
) -> Result<(String, Modelo, Option<String>)> {
    let req = CompletionRequest {
        messages: vec![
            Message { role: Role::System, content: conversa::perfil(corvo, civ, tema) },
            Message { role: Role::User, content: conversa::pedido(&corvo.nome, historico) },
        ],
        // Folga para modelos que raciocinam antes (gpt-5-nano); a fala é cortada depois.
        max_tokens: 1200,
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

/// Os corvos conversam até o dono dizer `pare` (ou Ctrl+C). `rx` traz as linhas que o
/// dono digita (veja [`teclado_stdin`]; o modo terminal usa o próprio editor de linha).
pub async fn conversar(
    store: &Store,
    e: &Estado,
    o: Opcoes,
    p: &Paint,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) -> Result<()> {
    let civ = e.civilizacao()?;
    let (vozes, avisos) = vozes(store, &civ)?;

    println!();
    println!("  {}  {}", p.bold("Conversa da Civilização"), p.dim(&format!("{} corvos · Sandbox", civ.corvos.len())));
    for c in &civ.corvos {
        let cor = cor_provedor(&c.genoma.modelo.provedor);
        println!(
            "    {} {}",
            p.c256b(cor, &c.nome),
            p.dim(&format!(
                "{} · {} · {}",
                c.genoma.personalidade.nome(),
                c.genoma.especialidade.nome(),
                c.genoma.modelo.modelo
            ))
        );
    }
    for a in &avisos {
        println!("  {}", p.yellow(&format!("⚠ {a}")));
    }
    let por_hora = 3600 / o.intervalo.as_secs().max(1);
    println!();
    println!(
        "  {}",
        p.dim(&format!("Uma fala a cada {}s (≈ {por_hora} chamadas curtas por hora).", o.intervalo.as_secs()))
    );
    println!("  {}", p.dim("Escreva e aperte Enter para falar com o bando. Chame pelo nome para um corvo responder."));
    println!("  {}", p.dim("Enter vazio = próxima fala agora · pare = encerra · Ctrl+C também encerra."));
    if let Some(t) = o.tema.as_deref().filter(|t| !t.trim().is_empty()) {
        println!("  {}", p.dim(&format!("Tema: {t}")));
    }
    println!();

    let mut historico: Vec<Fala> = Vec::new();
    let mut falas: u32 = 0;
    let mut chamadas: u32 = 0;
    let mut erros_seguidos = 0;
    let mut teclado_aberto = true;
    let mut agora = true;

    'conversa: loop {
        if !agora {
            let espera = tokio::time::sleep(o.intervalo);
            tokio::pin!(espera);
            loop {
                tokio::select! {
                    _ = &mut espera => break,
                    _ = tokio::signal::ctrl_c() => break 'conversa,
                    linha = rx.recv(), if teclado_aberto => match linha {
                        None => teclado_aberto = false,
                        Some(l) if eh_parada(&l) => break 'conversa,
                        Some(l) if l.trim().is_empty() => break,
                        Some(l) => {
                            historico.push(Fala::nova(conversa::DONO, l.trim()));
                            break;
                        }
                    },
                }
            }
        }
        agora = false;

        let i = conversa::proximo(&civ, &historico);
        let corvo = &civ.corvos[i];
        let voz = &vozes[i];
        if p.on {
            print!("  {}", p.dim(&format!("… {} está pensando", corvo.nome)));
            let _ = std::io::stdout().flush();
        }
        let resultado = tokio::select! {
            r = falar(voz, corvo, &civ, &historico, o.tema.as_deref()) => r,
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
            Ok((texto, modelo, nota)) => {
                erros_seguidos = 0;
                let texto = conversa::limpar(&texto, &corvo.nome);
                if texto.is_empty() {
                    continue;
                }
                if let Some(n) = nota {
                    println!("  {}", p.dim(&format!("↻ {n}")));
                }
                println!(
                    "{} {}",
                    p.c256b(cor_provedor(&modelo.provedor), &corvo.nome),
                    p.dim(&format!("· {}", modelo.modelo))
                );
                println!("  {texto}\n");
                historico.push(Fala::nova(&corvo.nome, &texto));
                falas += 1;
                if o.max_falas.is_some_and(|m| falas >= m) {
                    println!("  {}", p.dim(&format!("Chegou ao limite de {falas} falas.")));
                    break;
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
                // Passa a vez para o próximo da roda.
                historico.push(Fala::nova(&corvo.nome, "(ficou em silêncio)"));
            }
        }
    }
    println!("  {}", p.dim(&format!("⏹ Conversa encerrada · {falas} falas · {chamadas} chamada(s) aos modelos.")));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasta() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dotcrow-civ-{}-{}", std::process::id(), relogio()));
        std::fs::create_dir_all(&d).unwrap_or_default();
        d
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
        let d = pasta();
        let a = abrir(&d, None, None).unwrap_or_else(|e| panic!("{e}"));
        let b = abrir(&d, None, None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a, b);
        let na: Vec<String> =
            a.civilizacao().unwrap_or_else(|e| panic!("{e}")).corvos.into_iter().map(|c| c.nome).collect();
        let nb: Vec<String> =
            b.civilizacao().unwrap_or_else(|e| panic!("{e}")).corvos.into_iter().map(|c| c.nome).collect();
        assert_eq!(na, nb);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn troca_o_modelo_de_um_corvo() {
        let d = pasta();
        let e = abrir(&d, Some(7), None).unwrap_or_else(|e| panic!("{e}"));
        let nome = e.civilizacao().unwrap_or_else(|e| panic!("{e}")).corvos[0].nome.clone();
        let (achado, _) =
            trocar_modelo(&d, &nome.to_lowercase(), "gemini:gemini-3.8-flash").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(achado, nome);
        let civ = abrir(&d, None, None).and_then(|e| e.civilizacao()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(civ.corvos[0].genoma.modelo.modelo, "gemini-3.8-flash");
        // Os outros continuam no rodízio padrão.
        assert!(civ.corvos[1..].iter().all(|c| c.genoma.modelo.modelo != "gemini-3.8-flash"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nova_genese_muda_os_corvos() {
        let d = pasta();
        let a = abrir(&d, Some(1), None).unwrap_or_else(|e| panic!("{e}"));
        let b = nova(&d, Some(2)).unwrap_or_else(|e| panic!("{e}"));
        assert_ne!(a.semente, b.semente);
        assert_eq!(abrir(&d, None, None).unwrap_or_else(|e| panic!("{e}")).semente, 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn palavras_de_parada() {
        assert!(eh_parada("pare"));
        assert!(eh_parada(" /sair "));
        assert!(eh_parada("PARE"));
        assert!(!eh_parada("pare de falar de futebol e falem de música"));
    }
}
