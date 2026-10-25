//! O julgamento da civilização (`dotcrow civ julgar`, `/julgar` e a cada 200 falas).
//!
//! 1. Cada corvo precisa de 2 respostas a perguntas feitas por outro corvo. Elas vêm da
//!    memória dele (os vínculos das conversas); se faltar, outro corvo sorteado faz a
//!    pergunta na hora (entrevista) e a resposta tem tamanho sorteado (1 a 300 palavras).
//! 2. 3 jurados sorteados dão nota de 1 a 10 aos outros 9, olhando 2 respostas de cada.
//! 3. O oráculo (Opus 5.5) recebe 1 pergunta e resposta de cada corvo e dá nota de 1 a 10.
//! 4. Entram também as notas do dono desde o último julgamento.
//! 5. Ranking pela média: 1º +3 MB e 6 consultas, 2º +2 MB e 4, 3º +1 MB e 2; 4º e 5º
//!    continuam; do 6º ao 10º deixam de existir em 24 h.
//!
//! Tudo vai para o livro-razão de cada corvo.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, bail};
use dotcrow_agents::civilizacao::{
    JURADOS, PRAZO_CONDENADO, Rng, conversa,
    julgamento::{self, Amostra, Par},
    premio,
};

use crate::Paint;
use crate::civ::{self, Avaliacao, Estado, pedir, pedir_ao_opus, relogio, salvar};
use crate::civ_memoria::{Memoria, agora};
use crate::store::Store;

fn curto(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n { s } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

/// Roda um julgamento completo e salva o resultado.
pub async fn julgar(store: &Store, pasta: &Path, e: &mut Estado, p: &Paint) -> Result<()> {
    civ::exigir_ativa(e)?;
    for msg in civ::cumprir_prazos(pasta, e)? {
        println!("  {}", p.yellow(&msg));
    }
    let civ = e.civilizacao()?;
    if let Some(c) = civ.corvos.iter().find(|c| c.condenado_ate.is_some()) {
        let quando = c.condenado_ate.unwrap_or(0).saturating_sub(agora());
        bail!(
            "ainda há corvos condenados do último julgamento (o primeiro parte em {}h{:02}). O próximo julgamento \
fica para depois que eles deixarem de existir.",
            quando / 3600,
            (quando % 3600) / 60
        );
    }
    let (vozes, avisos) = civ::vozes(store, &civ)?;
    for a in &avisos {
        println!("  {}", p.yellow(&format!("⚠ {a}")));
    }
    let mut rng = Rng::new(relogio());
    let n = e.julgamentos + 1;
    println!("\n  {}", p.bold(&format!("⚖ Julgamento {n} da Civilização")));

    // 1. Duas respostas de cada corvo a perguntas de outro corvo.
    println!("  {}", p.dim("Juntando 2 respostas de cada corvo a perguntas de outros corvos…"));
    let mut amostras: Vec<Amostra> = Vec::new();
    for (i, alvo) in civ.corvos.iter().enumerate() {
        let mut mem = Memoria::abrir(pasta, &alvo.nome);
        while mem.vinculos.len() < 2 {
            let outros: Vec<usize> = (0..civ.corvos.len()).filter(|j| *j != i).collect();
            let j = outros[rng.ate(outros.len())];
            let quem = &civ.corvos[j];
            let (pergunta, _, _) =
                pedir(&vozes[j], conversa::perfil(quem, &civ, None), julgamento::pedido_pergunta(quem, alvo), 700)
                    .await?;
            let pergunta = conversa::limpar(&pergunta, &quem.nome, 80);
            let max_palavras = rng.ate(conversa::MAX_PALAVRAS) + 1;
            let (resposta, _, _) = pedir(
                &vozes[i],
                conversa::perfil(alvo, &civ, None),
                julgamento::pedido_resposta(&quem.nome, &pergunta, max_palavras),
                (max_palavras as u32) * 3 + 900,
            )
            .await?;
            let resposta = conversa::limpar(&resposta, &alvo.nome, max_palavras);
            println!("   🗨 {} → {}: {}", quem.nome, alvo.nome, p.dim(&curto(&pergunta, 110)));
            println!("      {}", p.dim(&curto(&resposta, 140)));
            mem.vincular(&quem.nome, &pergunta, &resposta);
            if let Some(v) = civ::vetorizar(store, &[format!("{pergunta} {resposta}")]).await.and_then(|mut v| v.pop())
                && let Some(ult) = mem.vinculos.last_mut()
            {
                ult.vetor = Some(crate::civ_memoria::compactar(&v));
            }
        }
        let pares: Vec<Par> = julgamento::sortear(&mem.vinculos, 2, &mut rng)
            .into_iter()
            .map(|v| Par { quem_perguntou: v.quem_perguntou, pergunta: v.pergunta, resposta: v.resposta })
            .collect();
        let _ = mem.salvar(pasta, alvo.rag_mb);
        amostras.push(Amostra { corvo: alvo.nome.clone(), pares });
    }

    // 2. Três jurados, cada um julga os outros 9.
    let jurados = julgamento::sortear_jurados(&civ, JURADOS, &mut rng);
    let nomes_jurados: Vec<&str> = jurados.iter().map(|j| civ.corvos[*j].nome.as_str()).collect();
    println!("  {}", p.bold(&format!("Jurados sorteados: {}", nomes_jurados.join(", "))));
    let mut notas_rodada: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut detalhe: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut avaliacoes: Vec<(String, Avaliacao)> = Vec::new();
    for &j in &jurados {
        let jurado = &civ.corvos[j];
        let outras: Vec<Amostra> = amostras.iter().filter(|a| a.corvo != jurado.nome).cloned().collect();
        let permitidos: Vec<String> = outras.iter().map(|a| a.corvo.clone()).collect();
        let (sistema, usuario) = julgamento::pedido_jurado(jurado, &outras);
        match pedir(&vozes[j], sistema, usuario, 1500).await {
            Ok((texto, modelo, _)) => {
                let notas = julgamento::ler_notas(&civ, &texto, &permitidos);
                println!(
                    "   {} {} {}",
                    p.bold(&jurado.nome),
                    p.dim(&format!("({})", modelo.modelo)),
                    p.dim(&notas.iter().map(|(n, v)| format!("{n} {v}")).collect::<Vec<_>>().join(" · "))
                );
                for (nome, nota) in notas {
                    notas_rodada.entry(nome.clone()).or_default().push(nota);
                    detalhe.entry(nome.clone()).or_default().push(format!("{} {nota}", jurado.nome));
                    avaliacoes.push((nome, Avaliacao { de: format!("jurado:{}", jurado.nome), nota, quando: agora() }));
                }
            }
            Err(err) => println!("   {}", p.yellow(&format!("{} não conseguiu julgar: {err}", jurado.nome))),
        }
    }

    // 3. O oráculo (Opus 5.5): 1 pergunta e resposta de cada corvo.
    let do_oraculo: Vec<Amostra> = amostras
        .iter()
        .map(|a| Amostra { corvo: a.corvo.clone(), pares: julgamento::sortear(&a.pares, 1, &mut rng) })
        .collect();
    let todos: Vec<String> = civ.corvos.iter().map(|c| c.nome.clone()).collect();
    let (sistema, usuario) = julgamento::pedido_oraculo(&do_oraculo);
    match pedir_ao_opus(store, &sistema, &usuario, 2000).await {
        Ok((texto, quem)) => {
            let notas = julgamento::ler_notas(&civ, &texto, &todos);
            println!(
                "   {} {} {}",
                p.bold("Oráculo"),
                p.dim(&format!("({quem})")),
                p.dim(&notas.iter().map(|(n, v)| format!("{n} {v}")).collect::<Vec<_>>().join(" · "))
            );
            for (nome, nota) in notas {
                notas_rodada.entry(nome.clone()).or_default().push(nota);
                detalhe.entry(nome.clone()).or_default().push(format!("oráculo {nota}"));
                avaliacoes.push((nome, Avaliacao { de: "oraculo".into(), nota, quando: agora() }));
            }
        }
        Err(err) => println!("   {}", p.yellow(&format!("O oráculo não respondeu: {err:#}"))),
    }

    // 4. As notas do dono desde o último julgamento.
    for (nome, ns) in &e.notas_pendentes {
        for n in ns {
            notas_rodada.entry(nome.clone()).or_default().push(*n);
            detalhe.entry(nome.clone()).or_default().push(format!("dono {n}"));
        }
    }

    // 5. Ranking, prêmios e condenações.
    let ranking = julgamento::ranking(&civ, &notas_rodada);
    for (nome, a) in avaliacoes {
        e.avaliacoes.entry(nome).or_default().push(a);
    }
    let prazo = agora() + PRAZO_CONDENADO;
    let mut linhas = Vec::new();
    {
        let pop = e.populacao_mut()?;
        for (lugar, (nome, media)) in ranking.iter().enumerate() {
            let Some(c) = pop.por_nome_mut(nome) else { continue };
            let mut mem = Memoria::abrir(pasta, nome);
            let notas_txt = detalhe.get(nome).map(|v| v.join(", ")).unwrap_or_default();
            mem.registrar(
                "julgamento",
                &format!("julgamento {n}: {}º lugar, média {media:.1} ({notas_txt})", lugar + 1),
            );
            let (mb, consultas) = premio(lugar);
            let destino = if mb > 0.0 {
                c.rag_mb += mb;
                c.consultas += consultas;
                mem.registrar("premio", &format!("+{mb:.0} MB de memória e +{consultas} consultas à internet"));
                format!("🏆 +{mb:.0} MB · +{consultas} consultas")
            } else if lugar >= 5 {
                c.condenado_ate = Some(prazo);
                mem.registrar("condenado", "ficou entre os 5 últimos: deixa de existir em 24 h");
                "⚰ deixa de existir em 24 h".to_string()
            } else {
                "continua".to_string()
            };
            let _ = mem.salvar(pasta, c.rag_mb);
            linhas.push((lugar, nome.clone(), *media, notas_txt, destino));
        }
    }
    e.notas_pendentes.clear();
    e.julgamentos = n;
    salvar(pasta, e)?;

    // Cada acontecimento ensina: todos refletem sobre o resultado e guardam lições.
    println!("  {}", p.dim("🧠 Os corvos refletem sobre o julgamento…"));
    let depois = e.civilizacao()?;
    for (i, c) in depois.corvos.iter().enumerate() {
        let Some(voz) = vozes.get(i) else { continue };
        let mem = Memoria::abrir(pasta, &c.nome);
        if let Ok(l) = civ::refletir(store, pasta, voz, c, &depois, &civ::acontecimentos(&mem)).await
            && !l.is_empty()
        {
            println!("   {} {}", p.bold(&c.nome), p.dim(&l.join(" · ")));
        }
    }

    println!("\n  {}", p.bold(&format!("Resultado do julgamento {n}")));
    for (lugar, nome, media, notas, destino) in linhas {
        let destino = if lugar < 3 {
            p.green(&destino)
        } else if lugar >= 5 {
            p.red(&destino)
        } else {
            p.dim(&destino)
        };
        println!("   {:>2}º {} {}  {}", lugar + 1, p.bold(&nome), p.dim(&format!("{media:.1} ({notas})")), destino);
    }
    println!(
        "  {}\n",
        p.dim("Os prêmios de consulta são usados pelos próprios corvos na conversa ([PESQUISAR: termo]) ou com: dotcrow civ pesquisar")
    );
    Ok(())
}
