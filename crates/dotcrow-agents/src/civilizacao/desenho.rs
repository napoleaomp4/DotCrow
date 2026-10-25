//! Desenho de um corvo no terminal, derivado do genoma.
//!
//! O desenho não é decoração: a cor vem do provedor do modelo, o símbolo no peito
//! vem da especialidade, as estrelas vêm da reputação e o rótulo mostra o estado.
//! Três tamanhos, para caber do desktop ao Termux estreito.

use super::Corvo;

/// Tamanho do desenho.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tamanho {
    /// Desenho grande, para desktop.
    Completo,
    /// Desenho médio.
    Medio,
    /// Uma linha só, para Termux estreito.
    Compacto,
}

/// Cor (paleta de 256) de cada provedor.
pub fn cor_provedor(provedor: &str) -> u8 {
    match provedor {
        "anthropic" => 214,
        "openai" => 40,
        "gemini" => 39,
        "openrouter" => 170,
        "deepseek" => 75,
        "groq" => 208,
        "ollama" => 245,
        _ => 245,
    }
}

fn pinta(on: bool, code: u8, s: &str) -> String {
    if on { format!("\x1b[38;5;{code}m{s}\x1b[0m") } else { s.to_string() }
}

fn forte(on: bool, code: u8, s: &str) -> String {
    if on { format!("\x1b[1;38;5;{code}m{s}\x1b[0m") } else { s.to_string() }
}

fn fraco(on: bool, s: &str) -> String {
    if on { format!("\x1b[2m{s}\x1b[0m") } else { s.to_string() }
}

/// Desenha o corvo. `cor` liga as cores ANSI (desligue sem TTY ou com NO_COLOR).
pub fn desenhar(corvo: &Corvo, tamanho: Tamanho, cor: bool) -> String {
    let cp = cor_provedor(&corvo.genoma.modelo.provedor);
    let simbolo = corvo.genoma.especialidade.simbolo();
    let nome = forte(cor, 255, &corvo.nome);
    let rep = pinta(cor, 226, &format!("★ {:.1}", corvo.reputacao));
    let estado = fraco(cor, &format!("[{}]", corvo.estado.rotulo()));
    let modelo = &corvo.genoma.modelo.modelo;
    let espec = corvo.genoma.especialidade.nome();
    let ger = corvo.genoma.geracao;

    match tamanho {
        Tamanho::Compacto => format!(
            "{nome}  {rep} {estado}  {} {}  {} {espec}",
            fraco(cor, "·"),
            pinta(cor, cp, modelo),
            fraco(cor, "·"),
        ),
        Tamanho::Medio => [
            format!("  {nome}  {rep}   {estado}"),
            format!("       {}", pinta(cor, cp, ".-.")),
            format!("      {}      {}", pinta(cor, cp, "(o o)"), fraco(cor, &format!("⟨{espec}⟩"))),
            format!(
                "     {}     {}",
                pinta(cor, cp, &format!("<  {simbolo}  >")),
                fraco(cor, &format!("modelo: {} · ger. {ger}", pinta(cor, cp, modelo))),
            ),
            format!("      {}", pinta(cor, cp, "'-'")),
        ]
        .join("\n"),
        Tamanho::Completo => [
            format!("   {nome}   {rep}   {estado}"),
            format!("        {}", pinta(cor, cp, "__")),
            format!("      {}", pinta(cor, cp, ".-'  '-.")),
            format!("     {}    {}", pinta(cor, cp, "( o  o )"), fraco(cor, &format!("⟨{espec}⟩")),),
            format!(
                "      {}    {}",
                pinta(cor, cp, &format!(">  {simbolo}  <")),
                fraco(cor, &format!("modelo: {}", pinta(cor, cp, modelo))),
            ),
            format!("       {}    {}", pinta(cor, cp, "'--'"), fraco(cor, &format!("geração {ger}"))),
            format!(
                "      {}    {}",
                pinta(cor, cp, "/|  |\\"),
                fraco(cor, &format!("RAG {:.1} MB · {} dias", corvo.rag_mb, corvo.idade_dias)),
            ),
        ]
        .join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Civilizacao, Modelo};
    use super::*;

    fn um_corvo() -> super::super::Corvo {
        let c = Civilizacao::genese(1, &[Modelo::novo("openai", "gpt-5-nano")]);
        c.corvos.into_iter().next().unwrap_or_else(|| panic!("a Gênese deve criar corvos"))
    }

    #[test]
    fn compacto_mostra_nome_e_modelo() {
        let corvo = um_corvo();
        let s = desenhar(&corvo, Tamanho::Compacto, false);
        assert!(s.contains(&corvo.nome));
        assert!(s.contains("gpt-5-nano"));
        assert!(!s.contains('\n'));
    }

    #[test]
    fn medio_mostra_o_simbolo_da_especialidade() {
        let corvo = um_corvo();
        let s = desenhar(&corvo, Tamanho::Medio, false);
        assert!(s.contains(corvo.genoma.especialidade.simbolo()));
        assert!(s.lines().count() >= 5);
    }

    #[test]
    fn sem_cor_nao_tem_escape_ansi() {
        let corvo = um_corvo();
        let s = desenhar(&corvo, Tamanho::Completo, false);
        assert!(!s.contains('\u{1b}'));
    }
}
