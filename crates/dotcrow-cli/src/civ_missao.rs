//! Missões: um corvo da civilização sai da conversa e **faz** algo, com mãos — terminal,
//! arquivos, internet, navegador, mouse e teclado do PC, a tela do celular, ligações e SMS.
//!
//! `dotcrow civ missao Corvo "poste no X o resumo de hoje"`, `/civ missao Corvo ...` no modo
//! terminal, `/missao Corvo ...` dentro da conversa, ou um áudio ("Áudio pra post no
//! Twitter") na conversa. O corvo trabalha com o próprio modelo, as próprias lições e a
//! própria memória, e é o rei da gambiarra: se um caminho falha, tenta outro. Nada
//! irreversível (publicar, enviar, ligar, SMS, pagar, apagar) sai sem o dono confirmar.
//!
//! Durante a missão, o que o dono escreve chega ao corvo (conversa ao vivo); `pare`
//! interrompe. A missão fica no livro-razão do corvo e ele reflete sobre ela.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow, bail};
use dotcrow_agents::civilizacao::{Civilizacao, conversa};
use dotcrow_agents::orchestrator;
use tokio::sync::{Notify, mpsc::UnboundedReceiver, oneshot};

use crate::Paint;
use crate::civ::{self, Estado, eh_parada};
use crate::civ_memoria::Memoria;
use crate::maos::{self, Liberados, Maos, Modo, Resposta};
use crate::store::Store;
use crate::terminal::Render;

/// Liga o teclado do dono à missão: responde às confirmações e às perguntas do corvo,
/// e o resto vira mensagem para ele no meio do trabalho.
pub struct Canal {
    pendente: Mutex<Option<oneshot::Sender<Option<String>>>>,
    caixa: Mutex<Vec<String>>,
    parar: Arc<Notify>,
    p: Paint,
}

impl Canal {
    fn novo(p: Paint) -> Arc<Self> {
        Arc::new(Self { pendente: Mutex::new(None), caixa: Mutex::new(Vec::new()), parar: Arc::new(Notify::new()), p })
    }

    /// Recebe uma linha do teclado.
    fn chegou(&self, l: String) {
        let pend = self.pendente.lock().ok().and_then(|mut g| g.take());
        match pend {
            Some(tx) if eh_parada(&l) => {
                let _ = tx.send(None);
                self.parar.notify_one();
            }
            Some(tx) => {
                let _ = tx.send(Some(l));
            }
            None if eh_parada(&l) => self.parar.notify_one(),
            None if l.trim().is_empty() => {}
            None => {
                println!("   {}", self.p.dim("✉ recado entregue ao corvo"));
                if let Ok(mut c) = self.caixa.lock() {
                    c.push(l.trim().to_string());
                }
            }
        }
    }

    async fn ler(&self) -> Option<String> {
        let (tx, rx) = oneshot::channel();
        if let Ok(mut g) = self.pendente.lock() {
            *g = Some(tx);
        }
        rx.await.ok().flatten()
    }
}

#[async_trait::async_trait]
impl maos::Aprovador for Canal {
    async fn pedir(&self, n: usize, titulo: &str, motivo: &str) -> Resposta {
        let p = self.p;
        println!("\n   {} {titulo}", p.yellow(&format!("⚠ Ação {n}:")));
        println!("     {}", p.dim(&format!("Precisa da sua confirmação: {motivo}.")));
        loop {
            println!("     {}", p.yellow("Permitir? [s]im · [a] sempre nesta missão · [n]ão"));
            let Some(l) = self.ler().await else { return Resposta::Nao };
            match Resposta::parse(&l) {
                Some(r) => return r,
                None => println!("{}", p.dim("     responda s, a ou n")),
            }
        }
    }

    async fn perguntar(&self, pergunta: &str, opcoes: &[String]) -> Option<String> {
        let p = self.p;
        println!("\n   {} {}", p.cyan("❓ O corvo pergunta:"), p.bold(pergunta));
        for (i, o) in opcoes.iter().enumerate() {
            println!("      {}) {o}", i + 1);
        }
        println!("      {}", p.dim("número escolhe · texto livre vale · Enter = decida você"));
        let l = self.ler().await?;
        let l = l.trim();
        Some(match l.parse::<usize>() {
            Ok(n) if n >= 1 && n <= opcoes.len() => opcoes[n - 1].clone(),
            _ => l.to_string(),
        })
    }
}

impl orchestrator::Inbox for Canal {
    fn drain(&self) -> Vec<String> {
        self.caixa.lock().map(|mut c| std::mem::take(&mut *c)).unwrap_or_default()
    }
}

/// Qual corvo faz a missão: o nome dado; sem nome (ou "auto"), o citado no pedido ou o de
/// melhor reputação que não está condenado.
pub fn escolher(civ: &Civilizacao, nome: &str, pedido: &str) -> Result<usize> {
    let n = nome.trim();
    if !n.is_empty() && !matches!(n.to_lowercase().as_str(), "auto" | "qualquer" | "alguem" | "alguém") {
        let alvo = conversa::normalizar(n);
        return civ.corvos.iter().position(|c| conversa::normalizar(&c.nome) == alvo).ok_or_else(|| {
            anyhow!(
                "não há corvo chamado {n}. Os corvos: {}",
                civ.corvos.iter().map(|c| c.nome.as_str()).collect::<Vec<_>>().join(", ")
            )
        });
    }
    if let Some(i) = conversa::citado(civ, pedido, None) {
        return Ok(i);
    }
    civ.corvos
        .iter()
        .enumerate()
        .filter(|(_, c)| c.condenado_ate.is_none())
        .max_by(|a, b| a.1.reputacao.total_cmp(&b.1.reputacao))
        .or_else(|| civ.corvos.iter().enumerate().next())
        .map(|(i, _)| i)
        .ok_or_else(|| anyhow!("a civilização está vazia"))
}

/// O pedido que o corvo recebe: quem ele é, o que aprendeu, a conversa do grupo e o jeito
/// "rei da gambiarra" de terminar.
pub fn pedido_da_missao(
    persona: &str,
    licoes: &[String],
    lembrancas: &[String],
    contexto: Option<&str>,
    pedido: &str,
) -> String {
    let mut s = format!("# Quem você é\n{persona}\n");
    if !licoes.is_empty() {
        s.push_str(&format!("\n# Suas lições\n- {}\n", licoes.join("\n- ")));
    }
    if !lembrancas.is_empty() {
        s.push_str(&format!("\n# O que você lembra\n- {}\n", lembrancas.join("\n- ")));
    }
    if let Some(c) = contexto.filter(|c| !c.trim().is_empty()) {
        s.push_str(&format!("\n# A conversa do bando até aqui\n{}\n", c.trim()));
    }
    s.push_str(&format!(
        "\n# Missão do dono\n{pedido}\n\n\
# Como trabalhar\n\
Você tem mãos: use as ferramentas para FAZER, não só descrever. Tudo o que uma pessoa faz num computador ou \
celular você pode fazer: criar e rodar arquivos e programas, pesquisar, abrir sites, usar o navegador, o mouse e o \
teclado, a tela do celular (tocar, rolar, digitar, apertar botões), ligar e mandar SMS. Seja o rei da gambiarra: se \
um caminho falha, tente outro (outra ferramenta, outro site, um script, o celular, o navegador) e pesquise como \
fazer. Não desista na primeira. Publicar, enviar, ligar, mandar SMS, pagar e apagar sempre pedem a confirmação do \
dono: peça (a ferramenta pergunta sozinha) e siga com a resposta. Termine com concluir, dizendo o que fez, onde \
ficou e o que faltou."
    ));
    s
}

/// Roda uma missão. Devolve (quem fez, a entrega) ou o erro, e o teclado de volta.
#[allow(clippy::too_many_arguments)]
pub async fn executar(
    store: &Store,
    pasta: &Path,
    e: &mut Estado,
    nome: &str,
    pedido: &str,
    contexto: Option<&str>,
    modo: Modo,
    mut rx: UnboundedReceiver<String>,
    p: &Paint,
) -> (Result<(String, String)>, UnboundedReceiver<String>) {
    let canal = Canal::novo(*p);
    let (fim_tx, mut fim_rx) = oneshot::channel::<()>();
    let c2 = canal.clone();
    let despachante = tokio::spawn(async move {
        loop {
            tokio::select! {
                l = rx.recv() => match l {
                    Some(l) => c2.chegou(l),
                    None => {
                        // Teclado fechado: quem esperava resposta recebe "nada".
                        if let Some(tx) = c2.pendente.lock().ok().and_then(|mut g| g.take()) {
                            let _ = tx.send(None);
                        }
                        // Espera o fim da missão para devolver o receptor.
                        let _ = (&mut fim_rx).await;
                        break;
                    }
                },
                _ = &mut fim_rx => break,
            }
        }
        rx
    });
    let r = missao(store, pasta, e, nome, pedido, contexto, modo, canal, p).await;
    let _ = fim_tx.send(());
    let rx = match despachante.await {
        Ok(rx) => rx,
        Err(_) => tokio::sync::mpsc::unbounded_channel().1,
    };
    (r, rx)
}

#[allow(clippy::too_many_arguments)]
async fn missao(
    store: &Store,
    pasta: &Path,
    e: &mut Estado,
    nome: &str,
    pedido: &str,
    contexto: Option<&str>,
    modo: Modo,
    canal: Arc<Canal>,
    p: &Paint,
) -> Result<(String, String)> {
    if pedido.trim().is_empty() {
        bail!("diga a missão: dotcrow civ missao <corvo> <o que fazer>");
    }
    civ::exigir_ativa(e)?;
    let civ = e.civilizacao()?;
    let i = escolher(&civ, nome, pedido)?;
    let corvo = civ.corvos[i].clone();
    let (vozes, avisos) = civ::vozes(store, &civ)?;
    for a in &avisos {
        println!("  {}", p.yellow(&format!("⚠ {a}")));
    }
    let voz = vozes.get(i).ok_or_else(|| anyhow!("sem voz para {}", corvo.nome))?;
    let router = voz.router();
    let mut mem = Memoria::abrir(pasta, &corvo.nome);
    let licoes = mem.ultimas_licoes(6);
    let lembrancas = mem.lembrancas(pedido, 4, None);
    // Tudo dentro de Downloads/DotCrow: o trabalho em missoes/, o que sai em entregas/.
    let raiz = civ::raiz_da_sandbox();
    let trabalho = maos::nova_pasta_em(&raiz.join("missoes"), pedido)?;
    let entregas = raiz.join("entregas");
    let _ = std::fs::create_dir_all(&entregas);
    let maos = Maos::new(modo, trabalho.clone(), store.dir().to_path_buf(), canal.clone(), Liberados::default())
        .com_perguntas()
        .com_saidas(entregas)
        .com_conectores(crate::conectores::Conexoes::do_store(store));
    let persona = conversa::perfil(&corvo, &civ, None);
    let request = pedido_da_missao(&persona, &licoes, &lembrancas, contexto, pedido);
    println!(
        "\n  {} {}",
        p.bold(&format!("🛠 Missão de {}", corvo.nome)),
        p.dim(&format!(
            "({} · {} · modo {} · pasta {})",
            voz.modelo().modelo,
            corvo.genoma.especialidade.nome(),
            modo.as_str(),
            trabalho.display()
        ))
    );
    println!("  {}", p.dim("Escreva para mandar recados ao corvo no meio do trabalho · pare interrompe"));
    let input = orchestrator::TaskInput {
        request: &request,
        kind: None,
        hands: Some(&maos),
        history: None,
        // O corvo usa a memória própria (lições e lembranças já vão no pedido).
        memory: None,
        inbox: Some(canal.as_ref()),
        // Na missão quem decide é o próprio corvo: sem triagem.
        oraculo: orchestrator::Oraculo::Desligado,
    };
    let mut render = Render::new(*p);
    let resultado = tokio::select! {
        r = orchestrator::run(&router, input, |ev| render.evento(ev)) => Some(r),
        _ = tokio::signal::ctrl_c() => None,
        _ = canal.parar.notified() => None,
    };
    let (entrega, resumo) = match resultado {
        None => {
            println!("\n  {}", p.yellow("⏹ Missão interrompida."));
            ("(o dono interrompeu a missão)".to_string(), "interrompida pelo dono".to_string())
        }
        Some(Ok(o)) => {
            let curto: String = o.summary.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(400).collect();
            (format!("{}\n\n{}", o.result.trim(), o.summary.trim()), curto)
        }
        Some(Err(err)) => (format!("(a missão parou: {err})"), format!("parou: {err}")),
    };
    let pedido_curto: String = pedido.chars().take(200).collect();
    mem.registrar("missao", &format!("{pedido_curto} → {resumo}"));
    mem.falou(&format!("(missão: {pedido_curto}) {resumo}"));
    let _ = mem.salvar(pasta, corvo.rag_mb);
    // Cada missão ensina: o corvo reflete e guarda lições.
    if let Ok(l) = civ::refletir(store, pasta, voz, &corvo, &civ, &civ::acontecimentos(&mem)).await
        && !l.is_empty()
    {
        println!("  {}", p.dim(&format!("🧠 {} aprendeu: {}", corvo.nome, l.join(" · "))));
    }
    Ok((corvo.nome, entrega))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pedido_tem_persona_licoes_e_gambiarra() {
        let t = pedido_da_missao(
            "Você é Hugo.",
            &["teste antes de publicar".into()],
            &[],
            Some("Você: posta isso"),
            "poste no X",
        );
        assert!(t.contains("Você é Hugo."));
        assert!(t.contains("- teste antes de publicar"));
        assert!(t.contains("# A conversa do bando até aqui\nVocê: posta isso"));
        assert!(t.contains("# Missão do dono\nposte no X"));
        assert!(t.contains("rei da gambiarra"));
        assert!(!t.contains("# O que você lembra"));
    }

    #[test]
    fn canal_separa_respostas_de_recados() {
        let c = Canal::novo(Paint { on: false });
        c.chegou("use o celular".into());
        c.chegou("   ".into());
        assert_eq!(orchestrator::Inbox::drain(c.as_ref()), vec!["use o celular".to_string()]);
        let (tx, mut rx) = oneshot::channel();
        if let Ok(mut g) = c.pendente.lock() {
            *g = Some(tx);
        }
        c.chegou("s".into());
        assert_eq!(rx.try_recv().ok().flatten().as_deref(), Some("s"));
        assert!(orchestrator::Inbox::drain(c.as_ref()).is_empty());
    }
}
