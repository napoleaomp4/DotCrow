//! Mostra os eventos de uma tarefa no terminal, com o texto dos corvos chegando ao vivo.

use std::io::Write;

use dotcrow_agents::orchestrator::Event;

use crate::Paint;

/// Desenha os eventos de uma tarefa. Guarda qual etapa está transmitindo texto agora,
/// para não repetir no fim o que já apareceu ao vivo.
pub struct Render {
    p: Paint,
    aberto: Option<usize>,
    /// O Oráculo só conversou: não há "tarefa concluída" para anunciar.
    conversa: bool,
}

impl Render {
    /// Novo desenhista.
    pub fn new(p: Paint) -> Self {
        Self { p, aberto: None, conversa: false }
    }

    /// Fecha a linha de uma etapa que estava transmitindo.
    fn fechar(&mut self) {
        if self.aberto.take().is_some() {
            println!();
        }
    }

    /// Mostra um evento.
    pub fn evento(&mut self, e: Event) {
        let p = self.p;
        match e {
            Event::StageStarted { step, name, what, .. } => {
                self.fechar();
                println!("{} {}", p.bold(&format!("▸ {}. {name}", rotulo(step))), p.dim(&format!("· {what}…")));
            }
            Event::StageDelta { step, chunk, .. } => {
                if self.aberto != Some(step) {
                    self.fechar();
                    self.aberto = Some(step);
                    print!("   ");
                }
                print!("{}", chunk.replace('\n', "\n   "));
                let _ = std::io::stdout().flush();
            }
            Event::StageFinished { step, text, provider, model, failovers, .. } => {
                if self.aberto == Some(step) {
                    self.aberto = None;
                    println!();
                } else {
                    self.fechar();
                    println!("{}", indent(&text));
                }
                for f in failovers {
                    println!("   {}", p.yellow(&format!("↻ {f} → troquei de provedor")));
                }
                println!("   {}\n", p.dim(&format!("via {provider} · {model}")));
            }
            Event::StageSkipped { step, name, reason, .. } => {
                self.fechar();
                println!("{}\n", p.dim(&format!("↷ {}. {name} · pulado: {reason}", rotulo(step))));
            }
            Event::KindChosen { kind } => {
                self.fechar();
                println!("{}\n", p.dim(&format!("Tipo da tarefa: {}", kind.label_pt())));
            }
            Event::Reworking { round, reason } => {
                self.fechar();
                println!("{}\n", p.yellow(&format!("↩ Ajuste {round}: o Vigia pediu para refazer — {reason}")));
            }
            Event::Blocked { .. } => {
                self.fechar();
                println!("{}\n", p.red("⛔ O Vigia bloqueou o plano. Nada será feito."));
            }
            Event::Finished { .. } => {
                self.fechar();
                if !self.conversa {
                    println!("{}", p.green("✔ Tarefa concluída."));
                }
            }
            Event::Oraculo { nivel, corvos, conselho, resposta, model, .. } => {
                self.fechar();
                let quem = if model.is_empty() { String::new() } else { format!("· {model}") };
                match nivel.as_str() {
                    "conversa" => {
                        self.conversa = true;
                        println!("{} {}", p.bold("🔮 Oráculo"), p.dim(&quem));
                        if let Some(r) = resposta {
                            println!("{}\n", indent(&r));
                        }
                    }
                    "simples" => {
                        println!(
                            "{} {}",
                            p.bold(&format!("🔮 Oráculo: tarefa simples, só {} trabalha", corvos.join(", "))),
                            p.dim(&quem)
                        );
                        if !conselho.is_empty() {
                            println!("   {}\n", p.dim(&conselho));
                        }
                    }
                    _ => {
                        println!("{} {}", p.bold("🔮 Oráculo: o bando inteiro trabalha"), p.dim(&quem));
                        if !conselho.is_empty() {
                            println!("{}\n", p.dim(&indent(&conselho)));
                        }
                    }
                }
            }
            Event::Failed { error } => {
                self.fechar();
                println!("{}", p.red(&format!("✗ A tarefa parou: {error}")));
            }
            Event::ActionStarted { n, title, why, .. } => {
                self.fechar();
                println!("   {} {}", p.bold(&format!("⚙ {n}.")), title);
                if !why.is_empty() {
                    println!("      {}", p.dim(&why));
                }
            }
            Event::ActionFinished { ok, output, .. } => {
                let first: String = output.lines().take(6).collect::<Vec<_>>().join("\n      ");
                let line = format!("      {first}");
                println!("{}", if ok { p.dim(&line) } else { p.yellow(&line) });
            }
            Event::TeamPlanned { lanes } => {
                self.fechar();
                println!("{}", p.bold(&format!("👥 Equipe: {} corvos em paralelo", lanes.len())));
                for (i, l) in lanes.iter().enumerate() {
                    println!("   {}. {}", i + 1, l.title);
                }
                println!();
            }
            Event::LaneEvent { lane, event } => {
                let resumo = match event.as_ref() {
                    Event::StageStarted { name, what, .. } => format!("{name} · {what}…"),
                    Event::ActionStarted { title, .. } => format!("⚙ {title}"),
                    Event::Finished { .. } => "concluído".into(),
                    Event::Failed { error } => format!("falhou: {error}"),
                    _ => String::new(),
                };
                if !resumo.is_empty() {
                    self.fechar();
                    println!("   {} {resumo}", p.dim(&format!("[{lane}]")));
                }
            }
            // A confirmação aparece pelo próprio aprovador do terminal; a conversa ao vivo é do painel.
            Event::Plano { itens, .. } => {
                self.fechar();
                let feitos = itens.iter().filter(|i| i.estado == dotcrow_agents::rumo::Estado::Feito).count();
                println!("{}", p.bold(&format!("   🗺 Plano · {feitos} de {} feito(s)", itens.len())));
                for (n, i) in itens.iter().enumerate() {
                    let (m, linha) = match i.estado {
                        dotcrow_agents::rumo::Estado::Feito => ("✓", p.green(&i.texto)),
                        dotcrow_agents::rumo::Estado::Fazendo => ("▶", p.yellow(&i.texto)),
                        dotcrow_agents::rumo::Estado::Pulado => ("↷", p.dim(&i.texto)),
                        dotcrow_agents::rumo::Estado::Pendente => ("☐", i.texto.clone()),
                    };
                    let nota = if i.nota.is_empty() { String::new() } else { p.dim(&format!(" — {}", i.nota)) };
                    println!("     {m} {}. {linha}{nota}", n + 1);
                }
                println!();
            }
            Event::Rumo { motivo, .. } => {
                self.fechar();
                println!("{}\n", p.yellow(&format!("   🧭 Rumo corrigido: {motivo}")));
            }
            Event::Compactado { mensagens, .. } => {
                self.fechar();
                println!(
                    "{}\n",
                    p.dim(&format!("   🗒 Resumi {mensagens} mensagens antigas num diário para não perder o fio."))
                );
            }
            Event::OwnerMessage { .. }
            | Event::ApprovalNeeded { .. }
            | Event::ApprovalResolved { .. }
            | Event::Started { .. } => {}
        }
    }
}

/// Número da etapa para mostrar; as rodadas de ajuste (41, 51...) viram 4.1, 5.1.
fn rotulo(step: usize) -> String {
    if step >= 40 { format!("{}.{}", step / 10, step % 10) } else { step.to_string() }
}

/// Recuo de três espaços em cada linha.
pub fn indent(text: &str) -> String {
    text.lines().map(|l| format!("   {l}")).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotulos_das_rodadas() {
        assert_eq!(rotulo(4), "4");
        assert_eq!(rotulo(41), "4.1");
        assert_eq!(rotulo(52), "5.2");
    }
}
