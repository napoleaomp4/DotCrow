//! Rumo: o que mantém um corvo no caminho numa tarefa longa.
//!
//! Uma tarefa grande (codificar um projeto inteiro, organizar centenas de arquivos,
//! operar vários sites) não anda em linha reta: o corvo planeja, executa, testa,
//! erra, corrige e replaneja. Este módulo guarda o que o ciclo de ações em
//! [`crate::orchestrator`] precisa para isso:
//!
//! - **Plano vivo** ([`Plano`]): a lista de itens que o próprio corvo monta com a
//!   ferramenta `plano` e atualiza com `marcar`. Ela volta para ele nas revisões de
//!   rumo e sobrevive à compactação da conversa, então o objetivo nunca se perde.
//! - **Bússola** ([`Bussola`]): percebe quando o corvo anda em círculo (a mesma
//!   ação várias vezes, erros seguidos) e quando quer concluir sem ter conferido o
//!   que mudou, e devolve o puxão de orelha certo.
//! - **Compactação** ([`trecho_para_compactar`]): quando a conversa fica grande
//!   demais, o miolo vira um diário curto, em vez de simplesmente cortar texto.

use serde::Serialize;
use serde_json::Value;

/// Situação de um item do plano.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Estado {
    /// Ainda não começou.
    Pendente,
    /// O corvo está nele agora.
    Fazendo,
    /// Pronto e conferido.
    Feito,
    /// Deixado de lado de propósito (com motivo na nota).
    Pulado,
}

impl Estado {
    fn ler(s: &str) -> Option<Self> {
        let s = s.trim().to_lowercase();
        Some(match s.as_str() {
            "pendente" | "a fazer" | "todo" | "pending" => Self::Pendente,
            "fazendo" | "em andamento" | "andamento" | "doing" | "in_progress" => Self::Fazendo,
            "feito" | "pronto" | "ok" | "concluido" | "concluído" | "done" => Self::Feito,
            "pulado" | "pular" | "ignorado" | "skip" | "skipped" => Self::Pulado,
            _ => return None,
        })
    }

    fn marca(self) -> &'static str {
        match self {
            Self::Pendente => "☐",
            Self::Fazendo => "▶",
            Self::Feito => "✓",
            Self::Pulado => "↷",
        }
    }

    fn nome(self) -> &'static str {
        match self {
            Self::Pendente => "pendente",
            Self::Fazendo => "fazendo",
            Self::Feito => "feito",
            Self::Pulado => "pulado",
        }
    }

    fn aberto(self) -> bool {
        matches!(self, Self::Pendente | Self::Fazendo)
    }
}

/// Um item do plano, como o painel e o terminal mostram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    /// O que fazer.
    pub texto: String,
    /// Situação.
    pub estado: Estado,
    /// Observação do corvo (ex.: por que pulou, o que faltou).
    pub nota: String,
}

/// O plano vivo de uma tarefa.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plano {
    itens: Vec<Item>,
}

/// Limite de itens: um plano maior que isso já é sinal de que precisa de etapas.
const MAX_ITENS: usize = 40;

impl Plano {
    /// Os itens, na ordem.
    pub fn itens(&self) -> &[Item] {
        &self.itens
    }

    /// Ainda não há plano.
    pub fn vazio(&self) -> bool {
        self.itens.is_empty()
    }

    /// Itens ainda abertos (pendentes ou em andamento).
    pub fn abertos(&self) -> Vec<(usize, &Item)> {
        self.itens.iter().enumerate().filter(|(_, i)| i.estado.aberto()).map(|(n, i)| (n + 1, i)).collect()
    }

    /// Ferramenta `plano {itens: [...]}`: monta ou refaz o plano. Itens que já existiam
    /// (mesmo texto) mantêm a situação, para replanejar não apagar o que já foi feito.
    pub fn definir(&mut self, args: &Value) -> Result<String, String> {
        let lista: Vec<String> = match args.get("itens").or_else(|| args.get("passos")) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(o) => o.get("texto").and_then(Value::as_str).map(str::to_string),
                    _ => None,
                })
                .collect(),
            Some(Value::String(s)) => s.lines().map(str::to_string).collect(),
            _ => return Err("plano precisa de args.itens: uma lista de passos curtos".into()),
        };
        let lista: Vec<String> = lista
            .into_iter()
            .map(|s| {
                s.trim().trim_start_matches(|c: char| c.is_ascii_digit() || ".-)*☐ ".contains(c)).trim().to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();
        if lista.is_empty() {
            return Err("o plano veio vazio: mande de 2 a 15 passos curtos".into());
        }
        if lista.len() > MAX_ITENS {
            return Err(format!("plano grande demais ({} itens): agrupe em até {MAX_ITENS} etapas", lista.len()));
        }
        let antigo = std::mem::take(&mut self.itens);
        self.itens = lista
            .into_iter()
            .map(|texto| match antigo.iter().find(|i| i.texto.eq_ignore_ascii_case(&texto)) {
                Some(i) => i.clone(),
                None => Item { texto, estado: Estado::Pendente, nota: String::new() },
            })
            .collect();
        Ok(format!("Plano registrado ({} itens).\n{}", self.itens.len(), self.render()))
    }

    /// Ferramenta `marcar {item, estado, nota?}`: atualiza um item (o número começa em 1).
    pub fn marcar(&mut self, args: &Value) -> Result<String, String> {
        if self.itens.is_empty() {
            return Err("ainda não há plano: monte com a ferramenta plano {itens: [...]} primeiro".into());
        }
        let n = args
            .get("item")
            .or_else(|| args.get("n"))
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
            .ok_or("marcar precisa de args.item (o número do item no plano)")? as usize;
        let Some(item) = n.checked_sub(1).and_then(|i| self.itens.get_mut(i)) else {
            return Err(format!("o plano tem {} itens; o item {n} não existe", self.itens.len()));
        };
        let estado = args
            .get("estado")
            .and_then(Value::as_str)
            .and_then(Estado::ler)
            .ok_or("estado precisa ser pendente, fazendo, feito ou pulado")?;
        let nota = args.get("nota").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if estado == Estado::Pulado && nota.is_empty() {
            return Err("para pular um item, diga o motivo em args.nota".into());
        }
        item.estado = estado;
        if !nota.is_empty() {
            item.nota = nota;
        }
        Ok(format!("Item {n} → {}.\n{}", estado.nome(), self.render()))
    }

    /// O plano em texto, com as marcas (☐ ▶ ✓ ↷).
    pub fn render(&self) -> String {
        self.itens
            .iter()
            .enumerate()
            .map(|(i, it)| {
                let nota = if it.nota.is_empty() { String::new() } else { format!(" — {}", it.nota) };
                format!("{} {}. {}{nota}", it.estado.marca(), i + 1, it.texto)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Ferramentas que mudam arquivos (depois delas, é bom conferir antes de concluir).
const MUDAM_ARQUIVO: [&str; 4] = ["escrever_arquivo", "editar_arquivo", "escrever", "editar"];
/// Ferramentas que conferem: rodar algo, ler o que foi escrito, olhar a página.
const CONFEREM: [&str; 6] = ["terminal", "comando", "shell", "ler_arquivo", "ler", "navegador"];

/// O que a bússola manda dizer ao corvo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Puxao {
    /// Está andando em círculo: mude de estratégia.
    Circulo(String),
    /// Quer concluir com itens do plano abertos.
    PlanoAberto(String),
    /// Quer concluir sem ter conferido o que mudou.
    SemConferir,
}

/// Vigia o andamento de uma tarefa longa.
#[derive(Debug, Default)]
pub struct Bussola {
    assinaturas: Vec<String>,
    erros_seguidos: usize,
    mudou_sem_conferir: bool,
    ultimo_alerta: usize,
    recusas_de_plano: usize,
    pediu_conferencia: bool,
}

impl Bussola {
    /// Registra o resultado de uma ação e diz se o corvo está andando em círculo.
    pub fn registrar(&mut self, n: usize, ferramenta: &str, args: &Value, ok: bool) -> Option<Puxao> {
        let assinatura = format!("{ferramenta} {}", args);
        self.assinaturas.push(assinatura.clone());
        if self.assinaturas.len() > 8 {
            self.assinaturas.remove(0);
        }
        self.erros_seguidos = if ok { 0 } else { self.erros_seguidos + 1 };
        if ok && MUDAM_ARQUIVO.contains(&ferramenta) {
            self.mudou_sem_conferir = true;
        } else if CONFEREM.contains(&ferramenta) {
            self.mudou_sem_conferir = false;
        }
        // Um alerta por vez: depois de puxar a orelha, dá 4 ações para o corvo reagir.
        if n < self.ultimo_alerta + 4 && self.ultimo_alerta > 0 {
            return None;
        }
        let repetidas = self.assinaturas.iter().filter(|a| **a == assinatura).count();
        let motivo = if repetidas >= 3 {
            Some(format!(
                "você pediu a mesma ação ({ferramenta}) {repetidas} vezes nas últimas {} — repetir não vai dar outro \
resultado",
                self.assinaturas.len()
            ))
        } else if self.erros_seguidos >= 4 {
            Some(format!("as últimas {} ações deram erro", self.erros_seguidos))
        } else {
            None
        };
        motivo.map(|m| {
            self.ultimo_alerta = n;
            Puxao::Circulo(m)
        })
    }

    /// O corvo quer concluir: pode? Devolve o que falta (cada trava vale poucas vezes,
    /// para nunca prender o corvo para sempre).
    pub fn antes_de_concluir(&mut self, plano: &Plano) -> Option<Puxao> {
        let abertos = plano.abertos();
        if !abertos.is_empty() && self.recusas_de_plano < 2 {
            self.recusas_de_plano += 1;
            let lista = abertos.iter().map(|(n, i)| format!("{n}. {}", i.texto)).collect::<Vec<_>>().join("; ");
            return Some(Puxao::PlanoAberto(lista));
        }
        if self.mudou_sem_conferir && !self.pediu_conferencia {
            self.pediu_conferencia = true;
            return Some(Puxao::SemConferir);
        }
        None
    }
}

impl Puxao {
    /// O texto que volta para o corvo.
    pub fn mensagem(&self) -> String {
        match self {
            Self::Circulo(m) => format!(
                "⚠ ALERTA DE RUMO: {m}. Pare e pense antes da próxima ação: o que exatamente está dando errado? Leia a \
mensagem de erro com atenção, confira suas suposições (o arquivo existe? o caminho está certo? a dependência está \
instalada?) e tente um caminho DIFERENTE. Se o plano não serve mais, refaça-o com a ferramenta plano."
            ),
            Self::PlanoAberto(l) => format!(
                "Ainda não dá para concluir: o plano tem itens abertos ({l}). Termine-os, ou marque com \
marcar {{item, estado: \"pulado\", nota}} explicando por que não serão feitos. Depois conclua."
            ),
            Self::SemConferir => "Antes de concluir, confira o que você mudou: rode o programa ou os testes, compile, \
ou leia de novo os arquivos alterados. Só conclua depois de ver que funciona (ou diga claramente na entrega o que não \
deu para testar e por quê)."
                .to_string(),
        }
    }

    /// Resumo curto para o painel.
    pub fn resumo(&self) -> String {
        match self {
            Self::Circulo(m) => format!("andando em círculo: {m}"),
            Self::PlanoAberto(l) => format!("quis concluir com itens abertos: {l}"),
            Self::SemConferir => "quis concluir sem conferir o que mudou".into(),
        }
    }
}

/// Mensagem da revisão de rumo que entra a cada tanto de ações.
pub fn revisao(n: usize, max: usize, plano: &Plano) -> String {
    let estado = if plano.vazio() {
        "Você ainda não montou um plano. Se a tarefa tem mais de 3 passos, monte agora com a ferramenta plano.".into()
    } else {
        format!("Plano atual:\n{}", plano.render())
    };
    format!(
        "🧭 REVISÃO DE RUMO (ação {n} de no máximo {max}). Pare um instante: o que já foi feito leva ao objetivo do \
dono? {estado}\nAtualize o plano se algo mudou (marcar itens feitos, acrescentar o que descobriu) e siga com a \
próxima ação. Se a tarefa está grande demais para o que resta, priorize o essencial e deixe o resto registrado no plano."
    )
}

/// Caracteres acima dos quais a conversa do ciclo de ações é compactada.
pub const LIMITE_CONVERSA: usize = 90_000;
/// Mensagens recentes que nunca entram na compactação.
const RECENTES: usize = 8;

/// O trecho do meio da conversa que pode virar diário: `(início, fim)` (fim exclusivo).
/// Fica de fora o começo (sistema + pedido, índices 0 e 1) e as últimas mensagens. O fim
/// cai antes de uma resposta do corvo, para a conversa continuar bem formada.
pub fn trecho_para_compactar(tamanhos: &[(bool, usize)]) -> Option<(usize, usize)> {
    let total: usize = tamanhos.iter().map(|(_, t)| t).sum();
    if total <= LIMITE_CONVERSA || tamanhos.len() <= 2 + RECENTES + 2 {
        return None;
    }
    let mut fim = tamanhos.len() - RECENTES;
    // `true` = mensagem do corvo (assistant). O trecho termina logo antes de uma.
    while fim > 3 && !tamanhos[fim].0 {
        fim -= 1;
    }
    (fim > 3).then_some((2, fim))
}

/// Pedido para a LLM resumir o trecho em diário.
pub const PEDIDO_DE_DIARIO: &str = "Resuma o trabalho abaixo num DIÁRIO curto (no máximo 40 linhas) para o próprio \
corvo continuar a tarefa sem se perder. Inclua: o que já foi feito e confirmado; arquivos criados ou alterados (com \
caminho); comandos importantes e o resultado; erros encontrados e como foram resolvidos (ou não); decisões tomadas; \
o que ainda falta. Não invente nada que não esteja no texto.";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plano_define_marca_e_mantem_o_feito_ao_replanejar() {
        let mut p = Plano::default();
        p.definir(&json!({"itens": ["1. criar pasta", "escrever código", "testar"]})).unwrap();
        assert_eq!(p.itens()[0].texto, "criar pasta");
        p.marcar(&json!({"item": 1, "estado": "feito"})).unwrap();
        assert!(p.marcar(&json!({"item": 2, "estado": "pulado"})).is_err(), "pular exige motivo");
        p.definir(&json!({"itens": ["criar pasta", "escrever código", "documentar", "testar"]})).unwrap();
        assert_eq!(p.itens()[0].estado, Estado::Feito);
        assert_eq!(p.abertos().len(), 3);
        assert!(p.render().starts_with("✓ 1. criar pasta"));
        assert!(p.marcar(&json!({"item": 9, "estado": "feito"})).is_err());
    }

    #[test]
    fn bussola_percebe_o_circulo_e_da_um_tempo() {
        let mut b = Bussola::default();
        let a = json!({"comando": "cargo build"});
        assert!(b.registrar(1, "terminal", &a, false).is_none());
        assert!(b.registrar(2, "terminal", &a, false).is_none());
        assert!(matches!(b.registrar(3, "terminal", &a, false), Some(Puxao::Circulo(_))));
        assert!(b.registrar(4, "terminal", &a, false).is_none(), "um alerta por vez");
    }

    #[test]
    fn bussola_trava_concluir_com_plano_aberto_e_sem_conferir() {
        let mut b = Bussola::default();
        let mut p = Plano::default();
        p.definir(&json!({"itens": ["a", "b"]})).unwrap();
        b.registrar(1, "editar_arquivo", &json!({"caminho": "x"}), true);
        assert!(matches!(b.antes_de_concluir(&p), Some(Puxao::PlanoAberto(_))));
        p.marcar(&json!({"item": 1, "estado": "feito"})).unwrap();
        p.marcar(&json!({"item": 2, "estado": "feito"})).unwrap();
        assert_eq!(b.antes_de_concluir(&p), Some(Puxao::SemConferir));
        assert_eq!(b.antes_de_concluir(&p), None, "a trava de conferir vale uma vez");
        // Depois de 2 recusas por plano aberto, deixa passar (nunca prende para sempre).
        let mut b2 = Bussola::default();
        let mut p2 = Plano::default();
        p2.definir(&json!({"itens": ["a"]})).unwrap();
        assert!(b2.antes_de_concluir(&p2).is_some());
        assert!(b2.antes_de_concluir(&p2).is_some());
        assert!(b2.antes_de_concluir(&p2).is_none());
    }

    #[test]
    fn rodar_algo_depois_de_editar_conta_como_conferir() {
        let mut b = Bussola::default();
        b.registrar(1, "escrever_arquivo", &json!({}), true);
        b.registrar(2, "terminal", &json!({"comando": "python app.py"}), true);
        assert_eq!(b.antes_de_concluir(&Plano::default()), None);
    }

    #[test]
    fn compactacao_pega_o_miolo_e_termina_antes_do_corvo() {
        let mut t = vec![(false, 1000), (false, 1000)];
        for i in 0..30 {
            t.push((i % 2 == 0, 5_000));
        }
        let (ini, fim) = trecho_para_compactar(&t).unwrap();
        assert_eq!(ini, 2);
        assert!(t[fim].0, "o trecho termina logo antes de uma resposta do corvo");
        assert!(fim <= t.len() - 8);
        assert!(trecho_para_compactar(&[(false, 10), (false, 10)]).is_none());
    }
}
