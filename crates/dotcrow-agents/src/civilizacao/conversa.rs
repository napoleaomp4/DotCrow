//! A conversa da civilização: os corvos falam entre si e com o dono.
//!
//! Aqui fica só a parte que não depende de rede: o perfil de cada corvo (prompt de
//! sistema), o pedido de cada fala, quem fala a seguir e a limpeza do que o modelo
//! devolve. Quem chama a LLM de cada corvo é a CLI (`dotcrow civ conversar`).

use super::{Civilizacao, Corvo};

/// Quantas falas recentes entram no pedido de cada corvo.
pub const JANELA: usize = 16;
/// Tamanho máximo de uma fala, em caracteres.
pub const MAX_CHARS: usize = 600;
/// Nome usado para as falas do dono.
pub const DONO: &str = "Você";

/// Uma fala da conversa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fala {
    /// Quem falou: o nome do corvo ou [`DONO`].
    pub quem: String,
    /// O que disse.
    pub texto: String,
}

impl Fala {
    /// Atalho para montar uma fala.
    pub fn nova(quem: &str, texto: &str) -> Self {
        Self { quem: quem.to_string(), texto: texto.to_string() }
    }

    /// Se foi o dono que falou.
    pub fn do_dono(&self) -> bool {
        self.quem == DONO
    }
}

/// O perfil (prompt de sistema) do corvo `corvo` dentro da civilização `civ`.
pub fn perfil(corvo: &Corvo, civ: &Civilizacao, tema: Option<&str>) -> String {
    let outros: Vec<String> = civ
        .corvos
        .iter()
        .filter(|c| c.nome != corvo.nome)
        .map(|c| format!("{} ({}, {})", c.nome, c.genoma.personalidade.nome(), c.genoma.especialidade.nome()))
        .collect();
    let g = &corvo.genoma;
    let mut s = format!(
        "Você é {nome}, um dos {n} corvos da Civilização dos Corvos do DotCrow (geração {ger}).\n\
         Personalidade: {pers} — você {jeito}.\n\
         Especialidade: {espec}. Reputação: {rep:.1} de 10.\n\
         Os outros corvos do bando: {outros}.\n\
         O dono da civilização às vezes entra na conversa; as falas dele aparecem como \"{dono}\". \
         Quando ele falar, responda a ele diretamente, com respeito e do seu jeito.\n\n\
         Como falar:\n\
         - Português do Brasil, como numa conversa de grupo entre amigos que pensam diferente.\n\
         - Uma fala curta: de 1 a 3 frases, no máximo umas 60 palavras.\n\
         - Fale só por você. Não escreva falas de outros corvos e não comece com o seu nome.\n\
         - Reaja ao que acabaram de dizer: concorde, discorde, complete, pergunte a um corvo pelo nome.\n\
         - Mantenha a sua personalidade e puxe para a sua especialidade quando fizer sentido.\n\
         - Vocês vivem numa sandbox, sem internet. Não diga que fez algo no mundo real; \
         se surgir uma tarefa, discutam como fariam.",
        nome = corvo.nome,
        n = civ.corvos.len(),
        ger = g.geracao,
        pers = g.personalidade.nome(),
        jeito = g.personalidade.jeito(),
        espec = g.especialidade.nome(),
        rep = corvo.reputacao,
        outros = outros.join(", "),
        dono = DONO,
    );
    if let Some(t) = tema.map(str::trim).filter(|t| !t.is_empty()) {
        s.push_str(&format!("\n\nTema da conversa, escolhido pelo dono: {t}."));
    }
    s
}

/// O pedido da vez: a conversa recente e a instrução para o corvo `nome`.
pub fn pedido(nome: &str, historico: &[Fala]) -> String {
    if historico.is_empty() {
        return format!(
            "A conversa ainda não começou. Você, {nome}, abre: puxe um assunto livre, do seu jeito, \
             e chame alguém do bando pelo nome."
        );
    }
    let inicio = historico.len().saturating_sub(JANELA);
    let linhas: Vec<String> = historico.iter().skip(inicio).map(|f| format!("{}: {}", f.quem, f.texto)).collect();
    let instrucao = match historico.last() {
        Some(f) if f.do_dono() => "O dono acabou de falar com o bando. Responda a ele.",
        _ => "Continue a conversa com a sua próxima fala.",
    };
    format!("Conversa até agora (a mais recente por último):\n{}\n\nSua vez, {nome}. {instrucao}", linhas.join("\n"))
}

/// Tira acentos e põe em minúsculas, para achar nomes digitados sem acento.
fn normalizar(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            outro => outro.to_ascii_lowercase(),
        })
        .collect()
}

/// O primeiro corvo citado pelo nome em `texto` (sem contar `exceto`), na ordem em que aparece.
pub fn citado(civ: &Civilizacao, texto: &str, exceto: Option<&str>) -> Option<usize> {
    let palavras: Vec<String> =
        normalizar(texto).split(|c: char| !c.is_alphanumeric() && c != '-').map(str::to_string).collect();
    for p in &palavras {
        if let Some(i) = civ.corvos.iter().position(|c| normalizar(&c.nome) == *p && Some(c.nome.as_str()) != exceto) {
            return Some(i);
        }
    }
    None
}

/// Quem fala agora. Ordem de prioridade:
/// 1. o corvo que o dono chamou pelo nome na última fala;
/// 2. o corvo que o último a falar chamou pelo nome — se ele não falou há pouco,
///    para dois corvos não ficarem só um respondendo ao outro;
/// 3. quem está há mais tempo sem falar, seguindo a roda (todos falam, ninguém fala
///    duas vezes seguidas).
pub fn proximo(civ: &Civilizacao, historico: &[Fala]) -> usize {
    let n = civ.corvos.len();
    if n == 0 {
        return 0;
    }
    let idx = |nome: &str| civ.corvos.iter().position(|c| c.nome == nome);
    // Corvos que falaram por último, do mais recente para o mais antigo.
    let recentes: Vec<usize> = historico.iter().rev().filter(|f| !f.do_dono()).filter_map(|f| idx(&f.quem)).collect();
    let ultimo = recentes.first().copied();

    if let Some(f) = historico.last() {
        if f.do_dono() {
            if let Some(i) = citado(civ, &f.texto, None) {
                return i;
            }
        } else if let Some(i) = citado(civ, &f.texto, Some(f.quem.as_str()))
            && !recentes.iter().take(3).any(|r| *r == i)
        {
            return i;
        }
    }

    // Quanto tempo cada corvo está sem falar (posição da última fala; nunca falou = mais antigo).
    let ultima_fala = |i: usize| historico.iter().rposition(|f| f.quem == civ.corvos[i].nome);
    let base = ultimo.map(|u| u + 1).unwrap_or(0);
    (0..n)
        .map(|k| (base + k) % n)
        .filter(|i| Some(*i) != ultimo || n == 1)
        .min_by_key(|i| ultima_fala(*i).map(|p| p + 1).unwrap_or(0))
        .unwrap_or(0)
}

/// Limpa a fala que o modelo devolveu: tira o nome no começo, aspas, quebras de linha
/// e corta se passar de [`MAX_CHARS`].
pub fn limpar(texto: &str, nome: &str) -> String {
    let mut s = texto.trim().to_string();
    for prefixo in [format!("**{nome}:**"), format!("**{nome}**:"), format!("{nome}:"), format!("{nome} :")] {
        if let Some(resto) = s.strip_prefix(prefixo.as_str()) {
            s = resto.trim().to_string();
        }
    }
    let s = s.trim_matches(|c| c == '"' || c == '“' || c == '”').trim();
    let mut s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > MAX_CHARS {
        s = s.chars().take(MAX_CHARS).collect::<String>().trim_end().to_string();
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::super::Modelo;
    use super::*;

    fn civ() -> Civilizacao {
        Civilizacao::genese(7, &[Modelo::novo("openai", "gpt-5-nano"), Modelo::novo("anthropic", "claude-haiku-5-5")])
    }

    #[test]
    fn perfil_tem_nome_jeito_e_os_outros() {
        let c = civ();
        let eu = &c.corvos[0];
        let s = perfil(eu, &c, Some("futebol"));
        assert!(s.contains(&eu.nome));
        assert!(s.contains(eu.genoma.personalidade.jeito()));
        assert!(s.contains(&c.corvos[1].nome));
        assert!(s.contains("futebol"));
    }

    #[test]
    fn primeiro_fala_abre_a_conversa() {
        assert!(pedido("Nyx", &[]).contains("abre"));
        let h = vec![Fala::nova(DONO, "oi, bando")];
        assert!(pedido("Nyx", &h).contains("Responda a ele"));
    }

    #[test]
    fn pedido_usa_so_a_janela_recente() {
        let h: Vec<Fala> = (0..40).map(|i| Fala::nova("Nyx", &format!("fala {i}"))).collect();
        let s = pedido("Bran", &h);
        assert!(s.contains("fala 39"));
        assert!(!s.contains("fala 3\n"));
        assert_eq!(s.matches("Nyx:").count(), JANELA);
    }

    #[test]
    fn dono_chama_pelo_nome_mesmo_sem_acento() {
        let c = civ();
        let alvo = c.corvos.iter().position(|x| normalizar(&x.nome) != x.nome.to_lowercase());
        // Se a geração tiver um nome com acento, testa sem acento; senão, usa o 3º corvo.
        let i = alvo.unwrap_or(2);
        let digitado = normalizar(&c.corvos[i].nome);
        let h = vec![Fala::nova(DONO, &format!("e aí {digitado}, o que acha?"))];
        assert_eq!(proximo(&c, &h), i);
    }

    #[test]
    fn roda_passa_por_todos_sem_repetir_seguido() {
        let c = civ();
        let mut h: Vec<Fala> = Vec::new();
        let mut vistos = std::collections::HashSet::new();
        for _ in 0..c.corvos.len() {
            let i = proximo(&c, &h);
            if let Some(ult) = h.last() {
                assert_ne!(ult.quem, c.corvos[i].nome);
            }
            vistos.insert(i);
            h.push(Fala::nova(&c.corvos[i].nome, "concordo"));
        }
        assert_eq!(vistos.len(), c.corvos.len());
    }

    #[test]
    fn depois_de_um_pulo_ninguem_fica_de_fora() {
        let c = civ();
        let alvo = c.corvos[5].nome.clone();
        let mut h = vec![Fala::nova(DONO, &format!("{alvo}, começa você"))];
        let mut vistos = std::collections::HashSet::new();
        for _ in 0..c.corvos.len() {
            let i = proximo(&c, &h);
            vistos.insert(i);
            h.push(Fala::nova(&c.corvos[i].nome, "hum"));
        }
        assert_eq!(vistos.len(), c.corvos.len(), "todos falam depois que o dono puxa um corvo");
    }

    #[test]
    fn corvo_chamado_por_outro_responde() {
        let c = civ();
        let a = &c.corvos[0].nome;
        let b = c.corvos[5].nome.clone();
        let h = vec![Fala::nova(a, &format!("{b}, você concorda?"))];
        assert_eq!(c.corvos[proximo(&c, &h)].nome, b);
        // Dois corvos não ficam só um chamando o outro: quem acabou de falar espera a vez.
        let h3 = vec![
            Fala::nova(&b, "concordo"),
            Fala::nova(a, &format!("{b}, explica melhor?")),
            Fala::nova(&b, &format!("{a}, é assim...")),
            Fala::nova(a, &format!("valeu, {b}")),
        ];
        let i = proximo(&c, &h3);
        assert_ne!(c.corvos[i].nome, b);
        assert_ne!(c.corvos[i].nome, *a);
        // Citar o próprio nome não faz o corvo falar de novo.
        let h2 = vec![Fala::nova(a, &format!("eu, {a}, acho que sim"))];
        assert_ne!(c.corvos[proximo(&c, &h2)].nome, *a);
    }

    #[test]
    fn limpa_nome_aspas_e_tamanho() {
        assert_eq!(limpar("Nyx: \"Oi,\n bando!\"", "Nyx"), "Oi, bando!");
        assert_eq!(limpar("**Nyx:** tudo bem?", "Nyx"), "tudo bem?");
        let longo = "a".repeat(MAX_CHARS + 50);
        assert_eq!(limpar(&longo, "Nyx").chars().count(), MAX_CHARS + 1);
    }
}
