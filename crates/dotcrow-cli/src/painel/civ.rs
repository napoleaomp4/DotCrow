//! A Civilização no painel (`/civ`): as duas janelas — o mapa dos terrenos e as mentes
//! (os embeddings de cada corvo num círculo que gira) — e a conversa com o corvo em
//! pixel art de 2 cm antes de cada fala e as linhas ligando cada resposta à fala
//! respondida. Só lê: quem muda a civilização é a conversa no terminal.

use std::path::Path;

use axum::{Json, extract::State};
use dotcrow_agents::civilizacao::Rng;
use serde_json::{Value, json};

use super::{ApiError, Shared, bad};
use crate::civ_memoria::{Memoria, artefatos, cosseno, expandir, vetor_local};
use crate::civ_visual;

/// A página.
pub const CIV_HTML: &str = include_str!("civ.html");

fn hex((r, g, b): civ_visual::Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn curto(s: &str, n: usize) -> String {
    let t: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > n { format!("{}…", t.chars().take(n).collect::<String>()) } else { t }
}

/// A mente de um corvo: cada lembrança vira um ponto (pelo embedding, ou pelo vetor
/// local quando não há chave da OpenAI), projetado num círculo; pontos parecidos ficam
/// ligados. As cores dizem o que é: personalidade (falas), ideias (pensamentos),
/// habilidades (lições), conhecimentos (pesquisas), relações (perguntas respondidas) e
/// criações (artefatos).
pub fn mente(pasta: &Path, nome: &str) -> Value {
    let m = Memoria::abrir(pasta, nome);
    let mut itens: Vec<(&str, String, Option<String>)> = Vec::new();
    let ultimos = |n: usize, len: usize| len.saturating_sub(n);
    for f in m.falas.iter().skip(ultimos(40, m.falas.len())) {
        itens.push(("personalidade", f.texto.clone(), f.vetor.clone()));
    }
    for f in m.pensamentos.iter().skip(ultimos(25, m.pensamentos.len())) {
        itens.push(("ideias", f.texto.clone(), f.vetor.clone()));
    }
    for f in m.licoes.iter().skip(ultimos(30, m.licoes.len())) {
        itens.push(("habilidades", f.texto.clone(), f.vetor.clone()));
    }
    for a in m.aprendizados.iter().skip(ultimos(30, m.aprendizados.len())) {
        itens.push(("conhecimentos", format!("{}: {}", a.titulo, a.texto), a.vetor.clone()));
    }
    for v in m.vinculos.iter().skip(ultimos(30, m.vinculos.len())) {
        itens.push((
            "relacoes",
            format!("{} perguntou: {} — {}", v.quem_perguntou, v.pergunta, v.resposta),
            v.vetor.clone(),
        ));
    }
    for a in artefatos(pasta, nome) {
        itens.push(("criacoes", a, None));
    }
    let vetores: Vec<Vec<f32>> = itens
        .iter()
        .map(|(_, t, v)| {
            let e = v.as_deref().map(expandir).unwrap_or_default();
            if e.len() == crate::civ_memoria::DIM_LOCAL { e } else { vetor_local(t) }
        })
        .collect();
    // Projeção fixa (a mesma para todos os corvos): dá para comparar as mentes.
    let mut rng = Rng::new(0x0C0_44E);
    let dim = crate::civ_memoria::DIM_LOCAL;
    let r1: Vec<f32> = (0..dim).map(|_| rng.faixa(-1.0, 1.0)).collect();
    let r2: Vec<f32> = (0..dim).map(|_| rng.faixa(-1.0, 1.0)).collect();
    let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    let mut xy: Vec<(f32, f32)> = vetores.iter().map(|v| (dot(v, &r1), dot(v, &r2))).collect();
    let maior = xy.iter().map(|(x, y)| (x * x + y * y).sqrt()).fold(0.0_f32, f32::max).max(1e-6);
    for p in &mut xy {
        p.0 = p.0 / maior * 0.92;
        p.1 = p.1 / maior * 0.92;
    }
    let pontos: Vec<Value> =
        itens.iter().zip(&xy).map(|((c, t, _), (x, y))| json!({"x": x, "y": y, "c": c, "t": curto(t, 90)})).collect();
    let mut ligacoes: Vec<(usize, usize, f32)> = Vec::new();
    for i in 0..vetores.len() {
        let melhor = (0..vetores.len())
            .filter(|j| *j != i)
            .map(|j| (j, cosseno(&vetores[i], &vetores[j])))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((j, c)) = melhor
            && c > 0.15
            && !ligacoes.iter().any(|(a, b, _)| (*a == j && *b == i) || (*a == i && *b == j))
        {
            ligacoes.push((i, j, c));
        }
    }
    json!({
        "pontos": pontos,
        "ligacoes": ligacoes.iter().map(|(a, b, c)| json!([a, b, c])).collect::<Vec<_>>(),
        "ficha": m.ficha.resumo,
        "perfil": m.ficha.perfil,
        "bytes": m.tamanho(),
        "integra": m.integra(),
    })
}

/// Tudo o que o painel mostra da civilização.
pub fn visao(pasta: &Path) -> Option<Value> {
    let e = crate::civ::ler(pasta)?;
    let civ = e.civilizacao().ok()?;
    let eco = &e.economia;
    let agora = crate::civ_memoria::agora();
    let corvos: Vec<Value> = civ
        .corvos
        .iter()
        .map(|c| {
            let b = eco.bolsas.get(&c.nome);
            let podio = eco.podio.contains(&c.nome);
            let sprite = civ_visual::sprite_do_corvo(c.genoma.semente, podio);
            json!({
                "nome": c.nome,
                "personalidade": c.genoma.personalidade.nome(),
                "especialidade": c.genoma.especialidade.nome(),
                "modelo": c.genoma.modelo.modelo,
                "provedor": c.genoma.modelo.provedor,
                "reputacao": c.reputacao,
                "rag_mb": c.rag_mb,
                "consultas": c.consultas,
                "condenado_ate": c.condenado_ate,
                "podio": podio,
                "cor": hex(sprite.fundo),
                "sprite": civ_visual::svg(&sprite, 64),
                "energia": b.map(|b| b.energia),
                "dotcoins": b.map(|b| b.dotcoins),
                "comida": b.map(|b| b.comida),
                "sementes": b.map(|b| b.sementes),
                "dormindo": b.and_then(|b| b.dormindo_ate).is_some_and(|t| t > agora),
                "local": b.and_then(|b| b.local.clone()),
                "fazendo": b.map(|b| b.fazendo.clone()).unwrap_or_default(),
                "artefatos": artefatos(pasta, &c.nome),
                "mente": mente(pasta, &c.nome),
            })
        })
        .collect();
    let conversa: Vec<Value> = civ_visual::ultimas(pasta, 80)
        .into_iter()
        .map(|l| {
            json!({
                "n": l.n,
                "quem": l.quem,
                "texto": l.texto,
                "quando": l.quando,
                "responde": l.responde,
                "modelo": l.modelo,
                "forma": civ_visual::forma(l.n),
                "cor": hex(civ_visual::cor_da_marca(l.n)),
            })
        })
        .collect();
    Some(json!({
        "agora": agora,
        "pausada": e.pausada_desde.is_some(),
        "geracao": civ.geracao,
        "falas": e.falas_total,
        "julgamentos": e.julgamentos,
        "nascimentos": eco.nascimentos,
        "corvos": corvos,
        "terrenos": eco.terrenos,
        "largura": crate::civ_economia::LARGURA_MAPA,
        "ofertas": eco.ofertas.iter().map(|o| json!({
            "id": o.id, "de": o.de, "para": o.para, "item": o.item.nome(o.qtd), "preco": o.preco
        })).collect::<Vec<_>>(),
        "contratos": eco.contratos,
        "mercado": eco.mercado_aberto(agora),
        "podio": eco.podio,
        "diario": eco.diario.iter().rev().take(30).collect::<Vec<_>>(),
        "conversa": conversa,
        "dono_sprite": civ_visual::svg(&civ_visual::sprite_do_dono(), 64),
        "mortos": civ.livro_dos_mortos.iter().rev().take(10).map(|b| json!({
            "nome": b.nome, "geracao": b.geracao, "licao": b.licao
        })).collect::<Vec<_>>(),
    }))
}

/// `GET /api/civ`.
pub async fn estado(State(st): State<Shared>) -> Result<Json<Value>, ApiError> {
    let pasta = crate::civ::pasta(st.store.dir());
    let v = tokio::task::spawn_blocking(move || visao(&pasta)).await.map_err(bad)?;
    v.map(Json).ok_or_else(|| bad("a civilização ainda não existe: rode dotcrow civ uma vez"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visao_da_civilizacao() {
        let d = std::env::temp_dir().join(format!("dotcrow-painel-civ-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        assert!(visao(&d).is_none(), "sem civilização ainda");
        let e = crate::civ::abrir(&d, Some(3), None).unwrap_or_else(|e| panic!("{e}"));
        let nome = e.vivos()[0].clone();
        let mut m = Memoria::abrir(&d, &nome);
        m.falou("os corvos guardam o que aprendem");
        m.falou("guardar o que aprendem é o que nos faz corvos");
        m.pensou("quero plantar no meu terreno");
        m.salvar(&d, 1.0).unwrap_or_else(|e| panic!("{e}"));
        let v = visao(&d).unwrap_or_else(|| panic!("devia ter visão"));
        assert_eq!(v["corvos"].as_array().map(Vec::len), Some(10));
        assert_eq!(v["terrenos"].as_array().map(Vec::len), Some(5), "a Gênese dá 5 terrenos");
        let c = v["corvos"]
            .as_array()
            .and_then(|a| a.iter().find(|c| c["nome"] == nome.as_str()))
            .cloned()
            .unwrap_or_default();
        assert!(c["sprite"].as_str().is_some_and(|s| s.starts_with("<svg")));
        assert!(c["mente"]["pontos"].as_array().is_some_and(|p| p.len() >= 3));
        assert!(c["energia"].as_f64().is_some_and(|x| (60.0..=100.0).contains(&x)));
        assert!(v["conversa"].as_array().is_some_and(|c| !c.is_empty()), "os nascimentos entram no registro");
        let _ = std::fs::remove_dir_all(&d);
    }
}
