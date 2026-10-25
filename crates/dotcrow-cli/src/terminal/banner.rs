//! A abertura do modo terminal: um corvo, o nome do projeto, a versão e o site —
//! com uma animação curta para a coisa parecer viva.
//!
//! Pensado para caber em qualquer terminal, do desktop largo ao Termux de celular:
//!
//! - **cheio** (≥ 64 colunas): corvo grande + tipografia `DotCrow` larga;
//! - **médio** (46–63): corvo grande + tipografia compacta;
//! - **compacto** (< 46, Termux retrato): um corvo pequeno + nome, versão e site.
//!
//! E degrada sozinho: sem TTY, com `NO_COLOR` ou com `DOTCROW_SEM_ANIMACAO=1`, sai
//! de uma vez, sem cor e sem animação.

use std::{io::Write, thread::sleep, time::Duration};

use terminal_size::{Width, terminal_size};

use crate::Paint;

/// O corvo grande, empoleirado, de perfil. ASCII puro para não quebrar em fonte
/// nenhuma (o Termux troca de fonte à vontade).
const CORVO: &[&str] = &[
    r"          __",
    r"       .-'  `-.__",
    r"      /  .-.     `-._",
    r"     |  (   )  o     `>",
    r"      \  `-'   \__   /",
    r"       \    .--' / .'",
    r"        `--'    `-'",
    r"         /| |\",
    r"        ' | | `",
];

/// O corvo pequeno, para tela de celular.
const CORVO_MINI: &[&str] = &[r"     __", r"   .-' o `-.", r"   \   `-. >", r"    `-----'"];

/// "DotCrow" em tipografia larga (figlet slant). ASCII puro.
const PALAVRA_LARGA: &[&str] = &[
    r"    ____        __  ______",
    r"   / __ \____  / /_/ ____/________ _      __",
    r"  / / / / __ \/ __/ /   / ___/ __ \ | /| / /",
    r" / /_/ / /_/ / /_/ /___/ /  / /_/ / |/ |/ /",
    r"/_____/\____/\__/\____/_/   \____/|__/|__/",
];

/// "DotCrow" em tipografia compacta (figlet small), para terminal estreito.
const PALAVRA_COMPACTA: &[&str] = &[
    r" ___      _    ___",
    r"|   \ ___| |_ / __|_ _ _____ __ __",
    r"| |) / _ \  _| (__| '_/ _ \ V  V /",
    r"|___/\___/\__|\___|_| \___/\_/\_/",
];

/// Cores da marca (paleta de 256): corvo roxo, tipografia âmbar, olho dourado,
/// link ciano, nome branco.
const ROXO: u8 = 141;
const AMBAR: u8 = 214;
const OLHO: u8 = 226;
const LINK: u8 = 44;
const BRANCO: u8 = 255;

/// Mostra o banner de abertura. Chamada uma vez, quando o modo terminal começa.
pub fn mostrar(p: Paint) {
    let anima = p.on && std::env::var_os("DOTCROW_SEM_ANIMACAO").is_none();
    let larg = largura();
    println!();

    if larg >= 46 {
        corvo(p, anima, CORVO, 3);
        let palavra = if larg >= 64 { PALAVRA_LARGA } else { PALAVRA_COMPACTA };
        for linha in palavra {
            println!("  {}", p.c256(AMBAR, linha));
            if anima {
                passo(16);
            }
        }
        assinatura(p, larg >= 64);
    } else {
        corvo(p, anima, CORVO_MINI, 2);
        compacto(p);
    }

    if anima {
        acordar(p, larg);
    }
    println!();
}

/// Desenha um corvo (grande ou mini), uma linha por vez quando animando.
fn corvo(p: Paint, anima: bool, arte: &[&str], recuo: usize) {
    let sp = " ".repeat(recuo);
    for linha in arte {
        let colorido = if linha.contains(" o ") { com_olho(p, linha) } else { p.c256(ROXO, linha) };
        println!("{sp}{colorido}");
        if anima {
            passo(14);
        }
    }
}

/// Colore a linha do olho: o corpo em roxo e o `o` em dourado.
fn com_olho(p: Paint, linha: &str) -> String {
    match linha.split_once(" o ") {
        Some((antes, depois)) => {
            format!("{}{}{}", p.c256(ROXO, antes), p.c256b(OLHO, " o "), p.c256(ROXO, depois))
        }
        None => p.c256(ROXO, linha),
    }
}

/// Nome, versão e site embaixo da tipografia.
fn assinatura(p: Paint, largo: bool) {
    let v = env!("CARGO_PKG_VERSION");
    println!();
    if largo {
        println!(
            "   {}{}{}{}",
            p.c256b(BRANCO, "DotCrow"),
            p.dim(&format!("  v{v}")),
            p.dim("  ·  modo terminal  ·  "),
            p.c256(LINK, "dotcrow.si"),
        );
        println!("   {}", p.dim("um bando de corvos trabalha no seu computador 🐦‍⬛"));
    } else {
        println!("   {}{}", p.c256b(BRANCO, "DotCrow"), p.dim(&format!("  v{v}  ·  modo terminal")));
        println!("   {}", p.c256(LINK, "dotcrow.si"));
    }
}

/// Assinatura de uma linha para tela estreita (Termux retrato).
fn compacto(p: Paint) {
    let v = env!("CARGO_PKG_VERSION");
    println!();
    println!("  {}  {}", p.c256b(BRANCO, "DotCrow"), p.dim(&format!("v{v}")));
    println!("  {}", p.c256(LINK, "dotcrow.si"));
}

/// Uma pausa curta para a animação.
fn passo(ms: u64) {
    sleep(Duration::from_millis(ms));
}

/// "Acordando os corvos…" — uma linha que pulsa e depois some, só no TTY.
fn acordar(p: Paint, larg: usize) {
    let base = if larg < 30 { "acordando" } else { "acordando os corvos" };
    for pontos in 0..4 {
        let reticencias = ".".repeat(pontos);
        print!("\r   {}{}   ", p.dim(base), p.dim(&reticencias));
        let _ = std::io::stdout().flush();
        passo(80);
    }
    // Apaga a linha da animação.
    print!("\r\x1b[2K");
    let _ = std::io::stdout().flush();
}

/// Largura do terminal: a real (ioctl via `terminal_size`), depois `COLUMNS`,
/// e 72 como último padrão. No Termux a largura real é o que importa.
fn largura() -> usize {
    if let Some((Width(w), _)) = terminal_size()
        && w > 0
    {
        return w as usize;
    }
    std::env::var("COLUMNS").ok().and_then(|c| c.trim().parse().ok()).filter(|&w| w > 0).unwrap_or(72)
}
