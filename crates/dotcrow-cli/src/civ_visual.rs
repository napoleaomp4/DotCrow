//! O que se vê da civilização no terminal e no painel:
//!
//! - o **corvo em pixel art** (8×8 pixels num quadrado colorido, uns 2 cm na tela) antes
//!   de cada fala, com cores tiradas do genoma (cada corvo tem o seu);
//! - as **marcas** (forma × cor + número, como `★12`) de cada fala, e a linha `↳` que diz
//!   a qual fala ela responde, para o dono, o oráculo e os jurados verem quem respondeu a
//!   quem;
//! - o **registro da conversa** (`conversa.jsonl`), retomado na próxima vez e lido pelo
//!   painel para desenhar as linhas entre as falas.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use dotcrow_agents::civilizacao::{Civilizacao, Rng, conversa};
use serde::{Deserialize, Serialize};

/// Uma cor RGB.
pub type Rgb = (u8, u8, u8);

/// Como pintar no terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cores {
    /// Sem cor (saída redirecionada, `NO_COLOR`): sem pixel art.
    Nenhuma,
    /// Paleta de 256 cores.
    C256,
    /// Cor verdadeira (24 bits).
    Verdadeira,
}

/// Descobre o que o terminal aceita.
pub fn cores(ligado: bool) -> Cores {
    if !ligado {
        return Cores::Nenhuma;
    }
    let tem = |k: &str| std::env::var_os(k).is_some();
    let ct = std::env::var("COLORTERM").unwrap_or_default().to_lowercase();
    if ct.contains("truecolor")
        || ct.contains("24bit")
        || tem("TERMUX_VERSION")
        || tem("WT_SESSION")
        || tem("VTE_VERSION")
    {
        return Cores::Verdadeira;
    }
    match std::env::var("TERM_PROGRAM").unwrap_or_default().as_str() {
        "iTerm.app" | "WezTerm" | "vscode" | "Hyper" | "ghostty" => Cores::Verdadeira,
        _ => Cores::C256,
    }
}

/// A cor mais perto na paleta de 256.
pub fn para_256((r, g, b): Rgb) -> u8 {
    let n = |v: u8| -> u8 {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            (v - 35) / 40
        }
    };
    let (cr, cg, cb) = (n(r), n(g), n(b));
    let cubo = 16 + 36 * cr + 6 * cg + cb;
    // Cinzas ficam melhor na rampa de cinza.
    if r.abs_diff(g) < 10 && g.abs_diff(b) < 10 {
        let media = (u16::from(r) + u16::from(g) + u16::from(b)) / 3;
        if media < 8 {
            return 16;
        }
        if media > 238 {
            return 231;
        }
        return 232 + ((media - 8) / 10).min(23) as u8;
    }
    cubo
}

fn frente(c: Rgb, cores: Cores) -> String {
    match cores {
        Cores::Verdadeira => format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2),
        Cores::C256 => format!("\x1b[38;5;{}m", para_256(c)),
        Cores::Nenhuma => String::new(),
    }
}

fn fundo(c: Rgb, cores: Cores) -> String {
    match cores {
        Cores::Verdadeira => format!("\x1b[48;2;{};{};{}m", c.0, c.1, c.2),
        Cores::C256 => format!("\x1b[48;5;{}m", para_256(c)),
        Cores::Nenhuma => String::new(),
    }
}

/// Texto colorido (sem cor, o texto puro).
pub fn pintar(c: Rgb, texto: &str, cores: Cores) -> String {
    if cores == Cores::Nenhuma { texto.to_string() } else { format!("{}\x1b[1m{texto}\x1b[0m", frente(c, cores)) }
}

/// Um desenho de 8×8 pixels num quadrado de fundo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    /// Os pixels (linha por linha); `None` é o fundo.
    pub pixels: [[Option<Rgb>; 8]; 8],
    /// A cor do quadrado.
    pub fundo: Rgb,
}

/// O molde do corvo, de lado, olhando para a direita: `B` corpo, `W` asa, `E` olho,
/// `K` bico, `L` pernas, `A` enfeite (coroa, chapéu ou flor).
const CORVO: [&str; 8] =
    ["...A.A..", "...BBB..", "..BBEBKK", "..BBBBB.", ".BWWWBB.", "BBWWWBB.", "B..BBB..", "...L.L.."];
/// O dono: uma pessoa de coroa.
const DONO: [&str; 8] =
    ["..A.A.A.", "..AAAAA.", "..SSSSS.", "..SESES.", "..SSSSS.", "...SSS..", ".CCCCCC.", ".CCCCCC."];

fn hsl((h, s, l): (f32, f32, f32)) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h % 360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let f = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (f(r), f(g), f(b))
}

fn clarear(c: Rgb, d: i16) -> Rgb {
    let f = |v: u8| (i16::from(v) + d).clamp(0, 255) as u8;
    (f(c.0), f(c.1), f(c.2))
}

fn montar(molde: &[&str; 8], cor: impl Fn(char) -> Option<Rgb>, fundo: Rgb) -> Sprite {
    let mut pixels = [[None; 8]; 8];
    for (y, linha) in molde.iter().enumerate() {
        for (x, ch) in linha.chars().take(8).enumerate() {
            pixels[y][x] = cor(ch);
        }
    }
    Sprite { pixels, fundo }
}

/// O corvo de cada genoma: corpo, asa, olho, bico, enfeite e quadrado sorteados pela
/// semente (sempre o mesmo para o mesmo corvo). Quem está no pódio usa coroa.
pub fn sprite_do_corvo(semente: u64, podio: bool) -> Sprite {
    let mut rng = Rng::new(semente ^ 0xC0_44_0C_A7);
    let corpos: [Rgb; 6] = [(26, 28, 40), (44, 30, 58), (46, 46, 50), (22, 52, 44), (72, 46, 30), (30, 40, 70)];
    // Um em 25 nasce albino.
    let corpo = if rng.ate(25) == 0 { (232, 232, 236) } else { corpos[rng.ate(corpos.len())] };
    let asa = clarear(corpo, if corpo.0 > 200 { -40 } else { 38 });
    let olhos: [Rgb; 5] = [(255, 255, 255), (255, 214, 0), (229, 57, 53), (0, 229, 255), (124, 252, 0)];
    let olho = olhos[rng.ate(olhos.len())];
    let bicos: [Rgb; 3] = [(130, 130, 140), (255, 193, 7), (255, 112, 67)];
    let bico = bicos[rng.ate(bicos.len())];
    let enfeite = if podio {
        Some((255, 202, 40))
    } else {
        match rng.ate(4) {
            0 => Some((229, 57, 53)),
            1 => Some((236, 64, 122)),
            _ => None,
        }
    };
    let quadrado = hsl((rng.ate(360) as f32, 0.45, 0.80));
    montar(
        &CORVO,
        |ch| match ch {
            'B' => Some(corpo),
            'W' => Some(asa),
            'E' => Some(olho),
            'K' => Some(bico),
            'L' => Some(bico),
            'A' => enfeite,
            _ => None,
        },
        quadrado,
    )
}

/// O desenho do dono.
pub fn sprite_do_dono() -> Sprite {
    montar(
        &DONO,
        |ch| match ch {
            'A' => Some((255, 202, 40)),
            'S' => Some((255, 205, 160)),
            'E' => Some((40, 40, 60)),
            'C' => Some((30, 136, 229)),
            _ => None,
        },
        (232, 240, 254),
    )
}

/// As 4 linhas do desenho no terminal (cada célula `▀` mostra 2 pixels: o de cima na
/// cor da letra, o de baixo no fundo). Sem cor, nada.
pub fn linhas(s: &Sprite, cores: Cores) -> Vec<String> {
    if cores == Cores::Nenhuma {
        return Vec::new();
    }
    (0..4)
        .map(|r| {
            let mut l = String::new();
            for x in 0..8 {
                let cima = s.pixels[2 * r][x].unwrap_or(s.fundo);
                let baixo = s.pixels[2 * r + 1][x].unwrap_or(s.fundo);
                l.push_str(&frente(cima, cores));
                l.push_str(&fundo(baixo, cores));
                l.push('▀');
            }
            l.push_str("\x1b[0m");
            l
        })
        .collect()
}

/// O desenho como SVG (para o painel), `lado` em pixels de tela.
pub fn svg(s: &Sprite, lado: u32) -> String {
    let p = lado / 8;
    let hex = |c: Rgb| format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{lado}\" height=\"{lado}\" viewBox=\"0 0 {lado} {lado}\" shape-rendering=\"crispEdges\"><rect width=\"{lado}\" height=\"{lado}\" rx=\"{}\" fill=\"{}\"/>",
        p,
        hex(s.fundo)
    );
    for (y, linha) in s.pixels.iter().enumerate() {
        for (x, px) in linha.iter().enumerate() {
            if let Some(c) = px {
                out.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{p}\" height=\"{p}\" fill=\"{}\"/>",
                    x as u32 * p,
                    y as u32 * p,
                    hex(*c)
                ));
            }
        }
    }
    out.push_str("</svg>");
    out
}

/// As formas das marcas.
pub const FORMAS: [&str; 10] = ["★", "■", "●", "▲", "◆", "♥", "♣", "♠", "✚", "⬢"];
/// As cores das marcas.
pub const CORES_MARCA: [Rgb; 8] = [
    (239, 83, 80),
    (255, 167, 38),
    (255, 238, 88),
    (102, 187, 106),
    (41, 182, 246),
    (171, 71, 188),
    (236, 64, 122),
    (38, 198, 218),
];

/// A forma da fala `n`.
pub fn forma(n: u64) -> &'static str {
    FORMAS[(n % FORMAS.len() as u64) as usize]
}

/// A cor da fala `n` (forma e cor mudam juntas: 40 combinações seguidas sem repetir).
pub fn cor_da_marca(n: u64) -> Rgb {
    CORES_MARCA[(n % CORES_MARCA.len() as u64) as usize]
}

/// A marca da fala `n`, como `★12`, colorida.
pub fn marca(n: u64, cores: Cores) -> String {
    pintar(cor_da_marca(n), &format!("{}{n}", forma(n)), cores)
}

/// Uma fala no registro da conversa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Linha {
    /// Número da fala (a marca sai dele).
    pub n: u64,
    /// Quem falou (corvo, [`conversa::DONO`] ou [`conversa::MUNDO`]).
    pub quem: String,
    /// O que disse.
    pub texto: String,
    /// Quando (segundos Unix).
    pub quando: u64,
    /// A fala a que responde.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responde: Option<u64>,
    /// O modelo que falou.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub modelo: String,
}

/// O arquivo da conversa (dentro da pasta da civilização).
pub fn arquivo_conversa(pasta: &Path) -> PathBuf {
    pasta.join("conversa.jsonl")
}

/// Acrescenta uma fala ao registro. Passando de 4 MB, guarda só as 3000 últimas.
pub fn anotar(pasta: &Path, l: &Linha) -> anyhow::Result<()> {
    let arq = arquivo_conversa(pasta);
    if std::fs::metadata(&arq).is_ok_and(|m| m.len() > 4 * 1024 * 1024) {
        let todas = ler(pasta);
        let resto: Vec<String> =
            todas.iter().skip(todas.len().saturating_sub(3000)).filter_map(|l| serde_json::to_string(l).ok()).collect();
        std::fs::write(&arq, format!("{}\n", resto.join("\n")))?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&arq)?;
    writeln!(f, "{}", serde_json::to_string(l)?)?;
    Ok(())
}

/// Todas as falas do registro.
pub fn ler(pasta: &Path) -> Vec<Linha> {
    std::fs::read_to_string(arquivo_conversa(pasta))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// As `n` últimas falas.
pub fn ultimas(pasta: &Path, n: usize) -> Vec<Linha> {
    let todas = ler(pasta);
    let ini = todas.len().saturating_sub(n);
    todas.into_iter().skip(ini).collect()
}

/// A qual fala esta responde: a do dono que chamou o corvo; a pergunta feita a ele; a
/// última fala de um corvo que ele cita; a última do dono, se foi o dono que falou por
/// último; senão, a fala anterior.
pub fn responde_a(civ: &Civilizacao, recentes: &[Linha], quem: &str, texto: &str, chamado: Option<u64>) -> Option<u64> {
    if chamado.is_some() {
        return chamado;
    }
    let falas: Vec<&Linha> = recentes.iter().filter(|l| l.quem != conversa::MUNDO && l.quem != quem).collect();
    let ultima = falas.last()?;
    // Uma pergunta para ele.
    if ultima.quem != conversa::DONO
        && ultima.texto.contains('?')
        && conversa::citado(civ, &ultima.texto, Some(&ultima.quem)).is_some_and(|i| civ.corvos[i].nome == quem)
    {
        return Some(ultima.n);
    }
    // Ele cita alguém que falou há pouco.
    if let Some(i) = conversa::citado(civ, texto, Some(quem)) {
        let nome = &civ.corvos[i].nome;
        if let Some(l) = falas.iter().rev().take(8).find(|l| &l.quem == nome) {
            return Some(l.n);
        }
    }
    let n = conversa::normalizar(texto);
    if ["voce", "dono", "senhor", "chefe"].iter().any(|w| n.split_whitespace().any(|x| x == *w))
        && let Some(l) = falas.iter().rev().take(8).find(|l| l.quem == conversa::DONO)
    {
        return Some(l.n);
    }
    Some(ultima.n)
}

/// Largura do terminal (80 se não der para saber).
pub fn largura() -> usize {
    terminal_size::terminal_size().map(|(w, _)| w.0 as usize).unwrap_or(80).clamp(30, 160)
}

/// Quebra o texto em linhas de até `largura` caracteres.
pub fn quebrar(texto: &str, largura: usize) -> Vec<String> {
    let mut out = Vec::new();
    for par in texto.split('\n') {
        let mut linha = String::new();
        for w in par.split_whitespace() {
            let cabe = linha.chars().count() + 1 + w.chars().count() <= largura;
            if !linha.is_empty() && !cabe {
                out.push(std::mem::take(&mut linha));
            }
            if !linha.is_empty() {
                linha.push(' ');
            }
            linha.push_str(w);
        }
        out.push(linha);
    }
    out
}

/// Monta a fala para o terminal: o desenho à esquerda e, ao lado, o cabeçalho (nome,
/// marca, a quem responde) e o texto.
pub fn bloco(sprite: Option<&Sprite>, cabecalho: &str, texto: &str, cores: Cores) -> String {
    let desenho = sprite.map(|s| linhas(s, cores)).unwrap_or_default();
    let recuo = if desenho.is_empty() { 2 } else { 11 };
    let mut corpo = vec![cabecalho.to_string()];
    corpo.extend(quebrar(texto, largura().saturating_sub(recuo + 1).max(20)));
    let n = corpo.len().max(desenho.len());
    let mut out = String::new();
    for i in 0..n {
        let esq = match desenho.get(i) {
            Some(d) => format!("  {d} "),
            None => " ".repeat(recuo),
        };
        out.push_str(&esq);
        out.push_str(corpo.get(i).map(String::as_str).unwrap_or(""));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_corvo_tem_o_seu_desenho_e_e_sempre_o_mesmo() {
        let a = sprite_do_corvo(1, false);
        assert_eq!(a, sprite_do_corvo(1, false));
        let diferentes = (2..12).filter(|s| sprite_do_corvo(*s, false) != a).count();
        assert!(diferentes >= 8);
        // O pódio usa coroa dourada.
        assert_eq!(sprite_do_corvo(1, true).pixels[0][3], Some((255, 202, 40)));
        let l = linhas(&a, Cores::Verdadeira);
        assert_eq!(l.len(), 4, "8×8 pixels em 4 linhas de terminal");
        assert_eq!(l[0].matches('▀').count(), 8);
        assert!(linhas(&a, Cores::Nenhuma).is_empty());
        assert!(svg(&a, 64).starts_with("<svg"));
    }

    #[test]
    fn marcas_mudam_de_forma_e_cor() {
        let mut vistas = std::collections::HashSet::new();
        for n in 0..40 {
            vistas.insert((forma(n), cor_da_marca(n)));
        }
        assert_eq!(vistas.len(), 40);
        assert_eq!(marca(12, Cores::Nenhuma), "●12");
    }

    #[test]
    fn paleta_256() {
        assert_eq!(para_256((0, 0, 0)), 16);
        assert_eq!(para_256((255, 255, 255)), 231);
        assert_eq!(para_256((255, 0, 0)), 196);
    }

    #[test]
    fn registro_e_respostas() {
        let d = std::env::temp_dir().join(format!("dotcrow-visual-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        let civ = Civilizacao::genese(7, &[dotcrow_agents::civilizacao::Modelo::novo("openai", "gpt-5-nano")]);
        let (a, b) = (civ.corvos[0].nome.clone(), civ.corvos[1].nome.clone());
        let l = |n, quem: &str, texto: &str| Linha {
            n,
            quem: quem.into(),
            texto: texto.into(),
            quando: n,
            responde: None,
            modelo: String::new(),
        };
        for x in [l(1, conversa::DONO, "oi bando"), l(2, &a, &format!("{b}, o que acha?")), l(3, conversa::MUNDO, "🌾")]
        {
            anotar(&d, &x).unwrap_or_else(|e| panic!("{e}"));
        }
        let r = ultimas(&d, 10);
        assert_eq!(r.len(), 3);
        assert_eq!(responde_a(&civ, &r, &b, "acho bom", None), Some(2), "responde à pergunta feita a ele");
        assert_eq!(responde_a(&civ, &r, &b, "acho bom", Some(1)), Some(1), "chamado pelo dono");
        let outro = civ.corvos[2].nome.clone();
        assert_eq!(responde_a(&civ, &r, &outro, "concordo com você, dono", None), Some(1));
        assert_eq!(responde_a(&civ, &r, &outro, &format!("discordo de {a}"), None), Some(2));
        let _ = std::fs::remove_dir_all(&d);
        let q = quebrar("um dois três quatro cinco", 9);
        assert_eq!(q, vec!["um dois", "três", "quatro", "cinco"]);
        assert!(bloco(Some(&sprite_do_dono()), "Você", "oi", Cores::C256).lines().count() >= 4);
    }
}
