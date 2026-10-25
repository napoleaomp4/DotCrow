//! A economia da civilização: dotcoins, energia, comida, sementes, terrenos, contratos,
//! ofertas entre corvos, o Mercado Central e o mercado aberto pelo pódio.
//!
//! Regras:
//! - Cada corvo nasce com **10 a 1000 dotcoins** (a moeda da sandbox) e **60% a 100% de
//!   energia**, sorteados.
//! - A cada **2 nascimentos** surge **1 m² de terreno** (`T1`, `T2`...), sorteado para um
//!   corvo vivo. Na Gênese nascem 10, então já surgem 5 terrenos.
//! - Para plantar é preciso terreno e semente; a comida fica pronta em **30 segundos** e
//!   cada colheita dá 3 comidas. Quem colhe é o dono ou alguém contratado por ele.
//! - A energia cai **1% por minuto** (só com a civilização rodando) e **1% por fala**
//!   (0,5% por pensamento). Com **50%**, o corvo dorme sozinho e come o que tem (cada
//!   comida dá +10%) até 100%. Se não tiver comida e continuar com 50% ou menos, morre, e
//!   nasce outro com outras características.
//! - Tudo pode ser usado, vendido, dado ou trocado a qualquer momento: dotcoins, comida,
//!   sementes, terrenos e artefatos. Os corvos podem contratar uns aos outros.
//! - O **Mercado Central** (do sistema, não é um corvo) vende semente e comida a preço fixo,
//!   compra a colheita e vende os terrenos sem dono: é por ele que a moeda entra e sai.
//! - Os 3 primeiros do último julgamento podem abrir o **mercado** por 1 minuto: todos são
//!   chamados a oferecer, vender e comprar.
//!
//! Os corvos agem escrevendo ações entre colchetes na fala (`[PLANTAR: T3]`,
//! `[VENDER: comida 2 para Nyx por 12]`...). O DotCrow executa, avisa o bando e lança no
//! livro-razão de quem participou. Nenhuma ação gasta chamada extra de IA.

use std::collections::BTreeMap;
use std::path::Path;

use dotcrow_agents::civilizacao::{Rng, conversa};
use serde::{Deserialize, Serialize};

/// Energia que cai por minuto com a civilização rodando.
pub const ENERGIA_POR_MINUTO: f32 = 1.0;
/// Energia que cada fala gasta.
pub const ENERGIA_POR_FALA: f32 = 1.0;
/// Energia que cada pensamento gasta.
pub const ENERGIA_POR_PENSAMENTO: f32 = 0.5;
/// Com esta energia (ou menos), o corvo dorme e come.
pub const DORME_EM: f32 = 50.0;
/// Quanto cada comida devolve de energia.
pub const ENERGIA_POR_COMIDA: f32 = 10.0;
/// Tempo de um sono (o corvo não fala dormindo).
pub const SONO_SEGUNDOS: u64 = 30;
/// Preço da semente no Mercado Central.
pub const PRECO_SEMENTE: u32 = 2;
/// Preço da comida no Mercado Central.
pub const PRECO_COMIDA: u32 = 5;
/// Quanto o Mercado Central paga por comida.
pub const COMPRA_COMIDA: u32 = 3;
/// Quanto o Mercado Central paga por semente.
pub const COMPRA_SEMENTE: u32 = 1;
/// Preço de um terreno sem dono no Mercado Central.
pub const PRECO_TERRENO: u32 = 50;
/// Do plantio à colheita.
pub const CRESCER_SEGUNDOS: u64 = 30;
/// Comidas por colheita.
pub const COLHEITA: u32 = 3;
/// Quanto dura o mercado aberto pelo pódio.
pub const MERCADO_SEGUNDOS: u64 = 60;
/// Intervalo mínimo entre dois mercados abertos pelo mesmo corvo.
pub const MERCADO_PAUSA: u64 = 600;
/// Quanto dura um contrato aceito.
pub const CONTRATO_SEGUNDOS: u64 = 600;
/// Largura do mapa de terrenos.
pub const LARGURA_MAPA: u32 = 6;

/// O que um corvo tem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bolsa {
    /// Dotcoins.
    pub dotcoins: u32,
    /// Energia, de 0 a 100.
    pub energia: f32,
    /// Comidas guardadas.
    pub comida: u32,
    /// Sementes.
    pub sementes: u32,
    /// Dormindo até (segundos Unix).
    #[serde(default)]
    pub dormindo_ate: Option<u64>,
    /// Terreno onde está agora.
    #[serde(default)]
    pub local: Option<String>,
    /// O que está fazendo (para o mapa).
    #[serde(default)]
    pub fazendo: String,
}

/// Um plantio num terreno.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plantio {
    /// Quem plantou.
    pub plantado_por: String,
    /// Quando plantou.
    pub inicio: u64,
    /// Quando fica pronto para colher.
    pub pronto: u64,
    /// Se o bando já foi avisado que está pronto.
    #[serde(default)]
    pub avisado: bool,
}

/// 1 m² de terreno virtual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terreno {
    /// Id (`T1`, `T2`...).
    pub id: String,
    /// Coluna no mapa.
    pub x: u32,
    /// Linha no mapa.
    pub y: u32,
    /// Dono (`None` = do Mercado Central, à venda).
    pub dono: Option<String>,
    /// O que está plantado.
    #[serde(default)]
    pub plantio: Option<Plantio>,
}

/// Uma coisa que se troca.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "id", rename_all = "snake_case")]
pub enum Item {
    /// Dotcoins.
    Dotcoins,
    /// Comida.
    Comida,
    /// Sementes.
    Sementes,
    /// Um terreno.
    Terreno(String),
    /// Um artefato (nome do arquivo).
    Artefato(String),
}

impl Item {
    /// Como o item aparece num aviso (`3 comida(s)`, `o terreno T2`).
    pub fn nome(&self, qtd: u32) -> String {
        match self {
            Item::Dotcoins => format!("{qtd} dotcoins"),
            Item::Comida => format!("{qtd} comida(s)"),
            Item::Sementes => format!("{qtd} semente(s)"),
            Item::Terreno(t) => format!("o terreno {t}"),
            Item::Artefato(a) => format!("o artefato {a}"),
        }
    }
}

/// Uma oferta de venda de um corvo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Oferta {
    /// Id (`O1`...).
    pub id: String,
    /// Quem vende.
    pub de: String,
    /// Para quem (`None` = para qualquer um).
    pub para: Option<String>,
    /// O que vende.
    pub item: Item,
    /// Quantos.
    pub qtd: u32,
    /// Por quantos dotcoins.
    pub preco: u32,
    /// Quando ofereceu.
    pub criada: u64,
}

/// Um contrato de trabalho.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contrato {
    /// Id (`C1`...).
    pub id: String,
    /// Quem contrata.
    pub patrao: String,
    /// Quem é contratado.
    pub empregado: String,
    /// Pagamento em dotcoins.
    pub pagamento: u32,
    /// Para quê.
    pub servico: String,
    /// Aceito até (`None` = ainda é proposta).
    pub ate: Option<u64>,
}

/// O mercado aberto pelo pódio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mercado {
    /// Quem abriu.
    pub aberto_por: String,
    /// Até quando.
    pub ate: u64,
}

/// Um acontecimento da economia: o texto e quem participou (vai para o livro-razão deles).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evento {
    /// O que aconteceu.
    pub texto: String,
    /// Quem participou.
    pub envolvidos: Vec<String>,
}

impl Evento {
    fn novo(texto: impl Into<String>, envolvidos: &[&str]) -> Self {
        Self { texto: texto.into(), envolvidos: envolvidos.iter().map(|s| s.to_string()).collect() }
    }
}

/// Toda a economia (fica salva junto com a civilização).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Economia {
    /// O que cada corvo vivo tem.
    #[serde(default)]
    pub bolsas: BTreeMap<String, Bolsa>,
    /// Os terrenos.
    #[serde(default)]
    pub terrenos: Vec<Terreno>,
    /// Ofertas abertas.
    #[serde(default)]
    pub ofertas: Vec<Oferta>,
    /// Contratos (propostas e aceitos).
    #[serde(default)]
    pub contratos: Vec<Contrato>,
    /// Quantos corvos já nasceram (a cada 2, surge um terreno).
    #[serde(default)]
    pub nascimentos: u32,
    /// Contador para os ids.
    #[serde(default)]
    pub proximo_id: u32,
    /// O mercado aberto agora.
    #[serde(default)]
    pub mercado: Option<Mercado>,
    /// Quando cada corvo abriu o mercado pela última vez.
    #[serde(default)]
    pub mercado_aberto_em: BTreeMap<String, u64>,
    /// Os 3 primeiros do último julgamento (podem abrir o mercado).
    #[serde(default)]
    pub podio: Vec<String>,
    /// Última vez que a energia foi descontada pelo tempo.
    #[serde(default)]
    pub ultimo_tick: u64,
    /// Os últimos acontecimentos (para o mapa).
    #[serde(default)]
    pub diario: Vec<String>,
}

/// Uma ação escrita pelo corvo na fala.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acao {
    /// Compra do Mercado Central.
    Comprar(Item, u32),
    /// Vende ao Mercado Central.
    VenderAoMercado(Item, u32),
    /// Oferece a outro corvo (ou a qualquer um).
    Ofertar {
        /// O quê.
        item: Item,
        /// Quantos.
        qtd: u32,
        /// Para quem (nome como escrito).
        para: Option<String>,
        /// Preço.
        preco: u32,
    },
    /// Aceita uma oferta (`O3`) ou um contrato (`C2`).
    Aceitar(String),
    /// Dá (ou paga) a outro corvo.
    Dar {
        /// O quê.
        item: Item,
        /// Quantos.
        qtd: u32,
        /// Para quem (nome como escrito).
        para: String,
    },
    /// Planta num terreno (`None` = o primeiro livre dele).
    Plantar(Option<String>),
    /// Colhe (`None` = o primeiro pronto dele).
    Colher(Option<String>),
    /// Come agora.
    Comer(u32),
    /// Propõe um contrato.
    Contratar {
        /// Quem (nome como escrito).
        quem: String,
        /// Pagamento.
        pagamento: u32,
        /// Para quê.
        servico: String,
    },
    /// Abre o mercado (só o pódio).
    AbrirMercado,
    /// Vai até um terreno.
    Ir(String),
}

fn norm(s: &str) -> String {
    conversa::normalizar(s.trim())
}

fn numero(s: &str) -> Option<u32> {
    s.split(|c: char| !c.is_ascii_digit()).find(|p| !p.is_empty()).and_then(|p| p.parse().ok())
}

fn id_terreno(s: &str) -> Option<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .find(|w| {
            w.len() >= 2 && (w.starts_with('T') || w.starts_with('t')) && w[1..].chars().all(|c| c.is_ascii_digit())
        })
        .map(|w| format!("T{}", &w[1..]))
}

fn item_de(s: &str) -> Option<Item> {
    let n = norm(s);
    if let Some(t) = id_terreno(s).filter(|_| n.contains("terreno") || n.split_whitespace().any(|w| w.starts_with('t')))
    {
        return Some(Item::Terreno(t));
    }
    if n.contains("artefato") || n.contains(".md") {
        let arq = s.split_whitespace().find(|w| w.ends_with(".md")).map(str::to_string).or_else(|| {
            s.split_whitespace().skip_while(|w| !norm(w).starts_with("artefato")).nth(1).map(str::to_string)
        })?;
        return Some(Item::Artefato(arq.trim_matches(|c: char| c == '"' || c == '\'').to_string()));
    }
    if n.contains("semente") {
        return Some(Item::Sementes);
    }
    if n.contains("comida") || n.contains("alimento") {
        return Some(Item::Comida);
    }
    if n.contains("dotcoin") || n.contains("moeda") {
        return Some(Item::Dotcoins);
    }
    None
}

/// O texto depois de uma das palavras (`para`, `pra`, `pro`).
fn depois_de<'a>(s: &'a str, palavras: &[&str]) -> Option<&'a str> {
    let n = norm(s);
    for p in palavras {
        let alvo = format!(" {p} ");
        if let Some(i) = format!(" {n} ").find(&alvo) {
            // `i` é posição no texto com espaço na frente: no original é i (o espaço some).
            let ini = i + p.len() + 1;
            return s.get(ini.min(s.len())..).map(str::trim);
        }
    }
    None
}

fn primeira_palavra(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c == ',').find(|w| !w.is_empty()).unwrap_or("").to_string()
}

/// Lê as ações entre colchetes de uma fala. Devolve o texto sem elas e as ações.
/// `[PESQUISAR: ...]` e `[CRIAR: ...]` ficam de fora (quem cuida é a conversa).
pub fn extrair_acoes(texto: &str) -> (String, Vec<Acao>) {
    let mut limpo = String::new();
    let mut acoes = Vec::new();
    let mut resto = texto;
    while let Some(ini) = resto.find('[') {
        let Some(fim) = resto[ini..].find(']') else { break };
        let dentro = &resto[ini + 1..ini + fim];
        let (verbo, args) = match dentro.split_once(':') {
            Some((v, a)) => (norm(v), a.trim()),
            None => (norm(dentro), ""),
        };
        let acao = ler_acao(&verbo, args);
        if acao.is_some() || verbo == "mercado" {
            limpo.push_str(&resto[..ini]);
            acoes.extend(acao);
        } else {
            limpo.push_str(&resto[..ini + fim + 1]);
        }
        resto = &resto[ini + fim + 1..];
    }
    limpo.push_str(resto);
    (limpo.split_whitespace().collect::<Vec<_>>().join(" "), acoes)
}

fn ler_acao(verbo: &str, args: &str) -> Option<Acao> {
    let n = norm(args);
    match verbo {
        "comprar" | "compro" => {
            let item = item_de(args)?;
            Some(Acao::Comprar(item, numero(args).unwrap_or(1).max(1)))
        }
        "vender" | "vendo" | "ofertar" | "oferecer" => {
            let item = item_de(args)?;
            if n.contains("mercado") && !n.contains("mercado aberto") {
                return Some(Acao::VenderAoMercado(item, numero(args).unwrap_or(1).max(1)));
            }
            let preco = depois_de(args, &["por"]).and_then(numero)?;
            let qtd = match item {
                Item::Terreno(_) | Item::Artefato(_) => 1,
                _ => args.split(" por ").next().and_then(numero).unwrap_or(1).max(1),
            };
            let para = depois_de(args, &["para", "pra", "pro", "ao", "a"])
                .map(primeira_palavra)
                .filter(|w| !w.is_empty() && !norm(w).starts_with("por") && numero(w).is_none());
            Some(Acao::Ofertar { item, qtd, para, preco })
        }
        "aceitar" | "aceito" | "comprar oferta" => {
            let id = args.split(|c: char| !c.is_alphanumeric()).find(|w| {
                w.len() >= 2
                    && matches!(w.chars().next(), Some('O' | 'o' | 'C' | 'c'))
                    && w[1..].chars().all(|c| c.is_ascii_digit())
            })?;
            Some(Acao::Aceitar(id.to_uppercase()))
        }
        "dar" | "doar" | "pagar" | "pago" | "dou" | "transferir" => {
            let para = depois_de(args, &["para", "pra", "pro", "ao", "a"]).map(primeira_palavra)?;
            let item = item_de(args).unwrap_or(Item::Dotcoins);
            let qtd = match item {
                Item::Terreno(_) | Item::Artefato(_) => 1,
                _ => numero(args).unwrap_or(1).max(1),
            };
            Some(Acao::Dar { item, qtd, para })
        }
        "plantar" | "planto" => Some(Acao::Plantar(id_terreno(args))),
        "colher" | "colho" => Some(Acao::Colher(id_terreno(args))),
        "comer" | "como" => Some(Acao::Comer(numero(args).unwrap_or(1).max(1))),
        "contratar" | "contrato" => {
            let quem = primeira_palavra(args);
            if quem.is_empty() {
                return None;
            }
            let pagamento = depois_de(args, &["por"]).and_then(numero).or_else(|| numero(args))?;
            let servico = depois_de(args, &["para", "pra"]).unwrap_or("ajudar").to_string();
            Some(Acao::Contratar { quem, pagamento, servico })
        }
        "mercado" | "abrir mercado" | "abrir o mercado" => Some(Acao::AbrirMercado),
        "ir" | "ir para" | "vou" => id_terreno(args).map(Acao::Ir),
        _ => None,
    }
}

/// O nome do corvo vivo que bate com `escrito` (sem acento, sem maiúscula).
fn achar_nome(vivos: &[String], escrito: &str) -> Option<String> {
    let alvo = norm(escrito.trim_matches(|c: char| !c.is_alphanumeric()));
    vivos.iter().find(|v| norm(v) == alvo).cloned()
}

impl Economia {
    fn novo_id(&mut self, prefixo: char) -> String {
        self.proximo_id += 1;
        format!("{prefixo}{}", self.proximo_id)
    }

    fn anotar(&mut self, eventos: &[Evento]) {
        for e in eventos {
            self.diario.push(e.texto.clone());
        }
        let n = self.diario.len();
        if n > 200 {
            self.diario.drain(..n - 200);
        }
    }

    /// Faz nascer a bolsa de um corvo: 10 a 1000 dotcoins e 60% a 100% de energia. A cada
    /// 2 nascimentos surge um terreno, sorteado entre os vivos.
    pub fn nascer(&mut self, nome: &str, rng: &mut Rng, vivos: &[String]) -> Vec<Evento> {
        let dotcoins = 10 + rng.ate(991) as u32;
        let energia = rng.faixa(60.0, 100.0).round();
        self.bolsas.insert(
            nome.to_string(),
            Bolsa {
                dotcoins,
                energia,
                comida: 0,
                sementes: 0,
                dormindo_ate: None,
                local: None,
                fazendo: String::new(),
            },
        );
        self.nascimentos += 1;
        let classe = if dotcoins >= 700 {
            "classe alta"
        } else if dotcoins <= 100 {
            "classe baixa"
        } else {
            "classe média"
        };
        let mut ev = vec![Evento::novo(
            format!("🐣 {nome} nasceu com {dotcoins} dotcoins ({classe}) e {energia:.0}% de energia."),
            &[nome],
        )];
        if self.nascimentos.is_multiple_of(2) && !vivos.is_empty() {
            let n = self.terrenos.len() as u32 + 1;
            let dono = vivos[rng.ate(vivos.len())].clone();
            let id = format!("T{n}");
            self.terrenos.push(Terreno {
                id: id.clone(),
                x: (n - 1) % LARGURA_MAPA,
                y: (n - 1) / LARGURA_MAPA,
                dono: Some(dono.clone()),
                plantio: None,
            });
            ev.push(Evento::novo(format!("🗺 Surgiu 1 m² de terreno ({id}) e o sorteio deu para {dono}."), &[&dono]));
        }
        self.anotar(&ev);
        ev
    }

    /// Garante bolsa para todos os vivos (corvos antigos e a Gênese contam como nascimento)
    /// e tira quem não está mais vivo.
    pub fn garantir(&mut self, vivos: &[String], rng: &mut Rng) -> Vec<Evento> {
        let mut ev = Vec::new();
        let mortos: Vec<String> = self.bolsas.keys().filter(|n| !vivos.contains(n)).cloned().collect();
        for m in mortos {
            ev.extend(self.morrer(&m));
        }
        for v in vivos {
            if !self.bolsas.contains_key(v) {
                ev.extend(self.nascer(v, rng, vivos));
            }
        }
        ev
    }

    /// O corvo morreu: a bolsa some, os terrenos voltam ao Mercado Central e as ofertas e
    /// contratos dele caem.
    pub fn morrer(&mut self, nome: &str) -> Vec<Evento> {
        self.bolsas.remove(nome);
        let mut devolvidos = Vec::new();
        for t in &mut self.terrenos {
            if t.dono.as_deref() == Some(nome) {
                t.dono = None;
                t.plantio = None;
                devolvidos.push(t.id.clone());
            }
        }
        self.ofertas.retain(|o| o.de != nome && o.para.as_deref() != Some(nome));
        self.contratos.retain(|c| c.patrao != nome && c.empregado != nome);
        self.podio.retain(|p| p != nome);
        let ev = if devolvidos.is_empty() {
            Vec::new()
        } else {
            vec![Evento::novo(
                format!(
                    "🏚 Os terrenos de {nome} ({}) voltaram ao Mercado Central, à venda por {PRECO_TERRENO}.",
                    devolvidos.join(", ")
                ),
                &[],
            )]
        };
        self.anotar(&ev);
        ev
    }

    /// O relógio: desconta a energia pelo tempo (só enquanto a civilização roda; no
    /// máximo 2 minutos de uma vez), avisa colheitas prontas e fecha o mercado vencido.
    pub fn tick(&mut self, agora: u64) -> Vec<Evento> {
        let mut ev = Vec::new();
        if self.ultimo_tick > 0 {
            let passou = agora.saturating_sub(self.ultimo_tick).min(120) as f32;
            for b in self.bolsas.values_mut() {
                if b.dormindo_ate.is_none_or(|t| t <= agora) {
                    b.energia = (b.energia - ENERGIA_POR_MINUTO * passou / 60.0).max(0.0);
                }
            }
        }
        self.ultimo_tick = agora;
        for b in self.bolsas.values_mut() {
            if b.dormindo_ate.is_some_and(|t| t <= agora) {
                b.dormindo_ate = None;
                b.fazendo.clear();
            }
        }
        for t in &mut self.terrenos {
            if let Some(p) = t.plantio.as_mut()
                && p.pronto <= agora
                && !p.avisado
            {
                p.avisado = true;
                let dono = t.dono.clone().unwrap_or_default();
                ev.push(Evento::novo(format!("🌾 A plantação de {dono} em {} está pronta para colher.", t.id), &[]));
            }
        }
        if let Some(m) = &self.mercado
            && m.ate <= agora
        {
            ev.push(Evento::novo(format!("🔔 O mercado aberto por {} fechou.", m.aberto_por), &[]));
            self.mercado = None;
        }
        self.anotar(&ev);
        ev
    }

    /// Desconta energia de quem falou (ou pensou).
    pub fn gastar(&mut self, nome: &str, quanto: f32) {
        if let Some(b) = self.bolsas.get_mut(nome) {
            b.energia = (b.energia - quanto).max(0.0);
        }
    }

    /// Se o corvo está dormindo agora.
    pub fn dormindo(&self, nome: &str, agora: u64) -> bool {
        self.bolsas.get(nome).and_then(|b| b.dormindo_ate).is_some_and(|t| t > agora)
    }

    /// Quem chegou a 50% dorme e come o que tem até 100%. Quem continua com 50% ou menos
    /// morre de fome. Devolve os avisos e os nomes de quem morreu.
    pub fn sono(&mut self, agora: u64) -> (Vec<Evento>, Vec<String>) {
        let mut ev = Vec::new();
        let mut mortos = Vec::new();
        for (nome, b) in &mut self.bolsas {
            if b.energia > DORME_EM || b.dormindo_ate.is_some_and(|t| t > agora) {
                continue;
            }
            let mut comeu = 0;
            while b.energia < 100.0 && b.comida > 0 {
                b.comida -= 1;
                b.energia = (b.energia + ENERGIA_POR_COMIDA).min(100.0);
                comeu += 1;
            }
            if b.energia <= DORME_EM {
                mortos.push(nome.clone());
                ev.push(Evento::novo(
                    format!(
                        "💀 {nome} dormiu com {:.0}% de energia, não tinha comida bastante e morreu de fome.",
                        b.energia
                    ),
                    &[nome],
                ));
            } else {
                b.dormindo_ate = Some(agora + SONO_SEGUNDOS);
                b.fazendo = "dormindo".into();
                ev.push(Evento::novo(
                    format!(
                        "😴 {nome} dormiu, comeu {comeu} comida(s) e acorda em {SONO_SEGUNDOS} s com {:.0}%.",
                        b.energia
                    ),
                    &[nome],
                ));
            }
        }
        self.anotar(&ev);
        (ev, mortos)
    }

    fn bolsa(&mut self, nome: &str) -> Result<&mut Bolsa, String> {
        self.bolsas.get_mut(nome).ok_or_else(|| format!("{nome} não tem bolsa"))
    }

    fn tem(&self, nome: &str, item: &Item, qtd: u32, pasta: &Path) -> bool {
        let Some(b) = self.bolsas.get(nome) else { return false };
        match item {
            Item::Dotcoins => b.dotcoins >= qtd,
            Item::Comida => b.comida >= qtd,
            Item::Sementes => b.sementes >= qtd,
            Item::Terreno(t) => self.terrenos.iter().any(|x| x.id == *t && x.dono.as_deref() == Some(nome)),
            Item::Artefato(a) => crate::civ_memoria::artefatos(pasta, nome).iter().any(|x| x == a),
        }
    }

    /// Passa `qtd` de `item` de `de` para `para` (já conferido).
    fn transferir(&mut self, de: &str, para: &str, item: &Item, qtd: u32, pasta: &Path) -> Result<String, String> {
        match item {
            Item::Dotcoins => {
                self.bolsa(de)?.dotcoins -= qtd;
                self.bolsa(para)?.dotcoins += qtd;
            }
            Item::Comida => {
                self.bolsa(de)?.comida -= qtd;
                self.bolsa(para)?.comida += qtd;
            }
            Item::Sementes => {
                self.bolsa(de)?.sementes -= qtd;
                self.bolsa(para)?.sementes += qtd;
            }
            Item::Terreno(t) => {
                let terreno = self.terrenos.iter_mut().find(|x| x.id == *t).ok_or("terreno não existe")?;
                terreno.dono = Some(para.to_string());
            }
            Item::Artefato(a) => {
                let novo = crate::civ_memoria::mover_artefato(pasta, de, para, a).map_err(|e| e.to_string())?;
                return Ok(format!("o artefato {novo}"));
            }
        }
        Ok(item.nome(qtd))
    }

    fn pode_trabalhar(&self, quem: &str, terreno: &Terreno, agora: u64) -> bool {
        terreno.dono.as_deref() == Some(quem)
            || self.contratos.iter().any(|c| {
                c.empregado == quem
                    && Some(c.patrao.as_str()) == terreno.dono.as_deref()
                    && c.ate.is_some_and(|t| t > agora)
            })
    }

    /// Executa a ação do corvo `quem`. `vivos` são os nomes dos corvos vivos; `pasta` é a
    /// pasta da civilização (artefatos). Devolve o acontecimento ou por que não deu.
    pub fn executar(
        &mut self,
        quem: &str,
        acao: &Acao,
        agora: u64,
        vivos: &[String],
        pasta: &Path,
    ) -> Result<Evento, String> {
        if !self.bolsas.contains_key(quem) {
            return Err(format!("{quem} não tem bolsa"));
        }
        let ev = match acao {
            Acao::Comprar(item, qtd) => {
                let (preco, nome) = match item {
                    Item::Sementes => (PRECO_SEMENTE * qtd, item.nome(*qtd)),
                    Item::Comida => (PRECO_COMIDA * qtd, item.nome(*qtd)),
                    Item::Terreno(t) => {
                        let livre = self.terrenos.iter().any(|x| x.id == *t && x.dono.is_none());
                        if !livre {
                            return Err(format!("o terreno {t} não está à venda no Mercado Central"));
                        }
                        (PRECO_TERRENO, item.nome(1))
                    }
                    _ => return Err("o Mercado Central vende sementes, comida e terrenos sem dono".into()),
                };
                let b = self.bolsa(quem)?;
                if b.dotcoins < preco {
                    return Err(format!("{quem} não tem {preco} dotcoins para comprar {nome}"));
                }
                b.dotcoins -= preco;
                match item {
                    Item::Sementes => b.sementes += qtd,
                    Item::Comida => b.comida += qtd,
                    Item::Terreno(t) => {
                        if let Some(x) = self.terrenos.iter_mut().find(|x| x.id == *t) {
                            x.dono = Some(quem.to_string());
                        }
                    }
                    _ => {}
                }
                Evento::novo(format!("🛒 {quem} comprou {nome} do Mercado Central por {preco} dotcoins."), &[quem])
            }
            Acao::VenderAoMercado(item, qtd) => {
                let valor = match item {
                    Item::Comida => COMPRA_COMIDA * qtd,
                    Item::Sementes => COMPRA_SEMENTE * qtd,
                    _ => return Err("o Mercado Central só compra comida e sementes".into()),
                };
                if !self.tem(quem, item, *qtd, pasta) {
                    return Err(format!("{quem} não tem {}", item.nome(*qtd)));
                }
                let b = self.bolsa(quem)?;
                match item {
                    Item::Comida => b.comida -= qtd,
                    Item::Sementes => b.sementes -= qtd,
                    _ => {}
                }
                b.dotcoins += valor;
                Evento::novo(
                    format!("💰 {quem} vendeu {} ao Mercado Central por {valor} dotcoins.", item.nome(*qtd)),
                    &[quem],
                )
            }
            Acao::Ofertar { item, qtd, para, preco } => {
                if !self.tem(quem, item, *qtd, pasta) {
                    return Err(format!("{quem} não tem {} para vender", item.nome(*qtd)));
                }
                let para = match para {
                    Some(p) => Some(achar_nome(vivos, p).ok_or_else(|| format!("não existe corvo chamado {p}"))?),
                    None => None,
                };
                if para.as_deref() == Some(quem) {
                    return Err("ninguém vende para si mesmo".into());
                }
                let id = self.novo_id('O');
                self.ofertas.push(Oferta {
                    id: id.clone(),
                    de: quem.to_string(),
                    para: para.clone(),
                    item: item.clone(),
                    qtd: *qtd,
                    preco: *preco,
                    criada: agora,
                });
                let alvo = para.clone().unwrap_or_else(|| "qualquer um".into());
                Evento::novo(
                    format!(
                        "📦 {quem} oferece {} a {alvo} por {preco} dotcoins ({id}). Para comprar: [ACEITAR: {id}]",
                        item.nome(*qtd)
                    ),
                    &[quem],
                )
            }
            Acao::Aceitar(id) if id.starts_with('O') => {
                let Some(pos) = self.ofertas.iter().position(|o| o.id == *id) else {
                    return Err(format!("a oferta {id} não existe mais"));
                };
                let o = self.ofertas[pos].clone();
                if o.de == quem {
                    return Err("ninguém compra a própria oferta".into());
                }
                if o.para.as_deref().is_some_and(|p| p != quem) {
                    return Err(format!("a oferta {id} é só para {}", o.para.unwrap_or_default()));
                }
                if !self.tem(quem, &Item::Dotcoins, o.preco, pasta) {
                    return Err(format!("{quem} não tem {} dotcoins", o.preco));
                }
                if !self.tem(&o.de, &o.item, o.qtd, pasta) {
                    self.ofertas.remove(pos);
                    return Err(format!("{} não tem mais {}; a oferta caiu", o.de, o.item.nome(o.qtd)));
                }
                self.transferir(quem, &o.de, &Item::Dotcoins, o.preco, pasta)?;
                let levou = self.transferir(&o.de, quem, &o.item, o.qtd, pasta)?;
                self.ofertas.remove(pos);
                Evento::novo(
                    format!("🤝 {quem} comprou {levou} de {} por {} dotcoins ({id}).", o.de, o.preco),
                    &[quem, &o.de],
                )
            }
            Acao::Aceitar(id) => {
                let Some(pos) = self.contratos.iter().position(|c| c.id == *id && c.ate.is_none()) else {
                    return Err(format!("o contrato {id} não existe ou já foi aceito"));
                };
                let c = self.contratos[pos].clone();
                if c.empregado != quem {
                    return Err(format!("o contrato {id} é para {}", c.empregado));
                }
                if !self.tem(&c.patrao, &Item::Dotcoins, c.pagamento, pasta) {
                    self.contratos.remove(pos);
                    return Err(format!("{} não tem {} dotcoins para pagar; o contrato caiu", c.patrao, c.pagamento));
                }
                self.transferir(&c.patrao, quem, &Item::Dotcoins, c.pagamento, pasta)?;
                self.contratos[pos].ate = Some(agora + CONTRATO_SEGUNDOS);
                Evento::novo(
                    format!(
                        "📝 {quem} aceitou trabalhar para {} ({}) por {} dotcoins, por 10 minutos ({id}).",
                        c.patrao, c.servico, c.pagamento
                    ),
                    &[quem, &c.patrao],
                )
            }
            Acao::Dar { item, qtd, para } => {
                let para = achar_nome(vivos, para).ok_or_else(|| format!("não existe corvo chamado {para}"))?;
                if para == quem {
                    return Err("ninguém dá para si mesmo".into());
                }
                if !self.tem(quem, item, *qtd, pasta) {
                    return Err(format!("{quem} não tem {}", item.nome(*qtd)));
                }
                let deu = self.transferir(quem, &para, item, *qtd, pasta)?;
                Evento::novo(format!("🎁 {quem} deu {deu} para {para}."), &[quem, &para])
            }
            Acao::Plantar(id) => {
                let id = match id {
                    Some(t) => t.clone(),
                    None => self
                        .terrenos
                        .iter()
                        .find(|t| t.dono.as_deref() == Some(quem) && t.plantio.is_none())
                        .map(|t| t.id.clone())
                        .ok_or_else(|| format!("{quem} não tem terreno livre para plantar"))?,
                };
                let Some(t) = self.terrenos.iter().find(|t| t.id == id).cloned() else {
                    return Err(format!("o terreno {id} não existe"));
                };
                if !self.pode_trabalhar(quem, &t, agora) {
                    return Err(format!("{quem} não é dono nem contratado do dono de {id}"));
                }
                if t.plantio.is_some() {
                    return Err(format!("já tem plantação em {id}"));
                }
                // O contratado planta com as sementes do patrão; o dono, com as dele.
                let dono = t.dono.clone().unwrap_or_default();
                let de_quem = if self.bolsas.get(&dono).is_some_and(|b| b.sementes > 0) {
                    dono.clone()
                } else {
                    quem.to_string()
                };
                let b = self.bolsa(&de_quem)?;
                if b.sementes == 0 {
                    return Err(format!("{quem} precisa de semente (compre: [COMPRAR: sementes 2])"));
                }
                b.sementes -= 1;
                if let Some(x) = self.terrenos.iter_mut().find(|x| x.id == id) {
                    x.plantio = Some(Plantio {
                        plantado_por: quem.to_string(),
                        inicio: agora,
                        pronto: agora + CRESCER_SEGUNDOS,
                        avisado: false,
                    });
                }
                let b = self.bolsa(quem)?;
                b.local = Some(id.clone());
                b.fazendo = format!("plantando em {id}");
                Evento::novo(format!("🌱 {quem} plantou em {id}; fica pronto em {CRESCER_SEGUNDOS} s."), &[quem, &dono])
            }
            Acao::Colher(id) => {
                let id = match id {
                    Some(t) => t.clone(),
                    None => self
                        .terrenos
                        .iter()
                        .find(|t| {
                            self.pode_trabalhar(quem, t, agora) && t.plantio.as_ref().is_some_and(|p| p.pronto <= agora)
                        })
                        .map(|t| t.id.clone())
                        .ok_or_else(|| format!("{quem} não tem colheita pronta"))?,
                };
                let Some(t) = self.terrenos.iter().find(|t| t.id == id).cloned() else {
                    return Err(format!("o terreno {id} não existe"));
                };
                if !self.pode_trabalhar(quem, &t, agora) {
                    return Err(format!("{quem} não é dono nem contratado do dono de {id}"));
                }
                match &t.plantio {
                    None => return Err(format!("não tem nada plantado em {id}")),
                    Some(p) if p.pronto > agora => {
                        return Err(format!("{id} ainda não está pronto (faltam {} s)", p.pronto - agora));
                    }
                    Some(_) => {}
                }
                let dono = t.dono.clone().unwrap_or_default();
                self.bolsa(&dono)?.comida += COLHEITA;
                if let Some(x) = self.terrenos.iter_mut().find(|x| x.id == id) {
                    x.plantio = None;
                }
                let b = self.bolsa(quem)?;
                b.local = Some(id.clone());
                b.fazendo = format!("colhendo em {id}");
                let para = if dono == quem { String::new() } else { format!(" para {dono}") };
                Evento::novo(format!("🧺 {quem} colheu {COLHEITA} comidas em {id}{para}."), &[quem, &dono])
            }
            Acao::Comer(qtd) => {
                let b = self.bolsa(quem)?;
                let n = (*qtd).min(b.comida);
                if n == 0 {
                    return Err(format!("{quem} não tem comida"));
                }
                b.comida -= n;
                b.energia = (b.energia + ENERGIA_POR_COMIDA * n as f32).min(100.0);
                let e = b.energia;
                Evento::novo(format!("🍽 {quem} comeu {n} comida(s) e está com {e:.0}% de energia."), &[quem])
            }
            Acao::Contratar { quem: alvo, pagamento, servico } => {
                let alvo = achar_nome(vivos, alvo).ok_or_else(|| format!("não existe corvo chamado {alvo}"))?;
                if alvo == quem {
                    return Err("ninguém contrata a si mesmo".into());
                }
                if !self.tem(quem, &Item::Dotcoins, *pagamento, pasta) {
                    return Err(format!("{quem} não tem {pagamento} dotcoins para pagar"));
                }
                let id = self.novo_id('C');
                self.contratos.push(Contrato {
                    id: id.clone(),
                    patrao: quem.to_string(),
                    empregado: alvo.clone(),
                    pagamento: *pagamento,
                    servico: servico.clone(),
                    ate: None,
                });
                Evento::novo(
                    format!(
                        "📝 {quem} quer contratar {alvo} por {pagamento} dotcoins para {servico} ({id}). {alvo} aceita com [ACEITAR: {id}]"
                    ),
                    &[quem],
                )
            }
            Acao::AbrirMercado => {
                if !self.podio.iter().any(|p| p == quem) {
                    return Err(format!(
                        "só os 3 primeiros do último julgamento abrem o mercado ({})",
                        self.podio.join(", ")
                    ));
                }
                if self.mercado.as_ref().is_some_and(|m| m.ate > agora) {
                    return Err("o mercado já está aberto".into());
                }
                if let Some(t) = self.mercado_aberto_em.get(quem)
                    && agora < t + MERCADO_PAUSA
                {
                    return Err(format!("{quem} abriu o mercado há pouco; de novo em {} s", t + MERCADO_PAUSA - agora));
                }
                self.mercado = Some(Mercado { aberto_por: quem.to_string(), ate: agora + MERCADO_SEGUNDOS });
                self.mercado_aberto_em.insert(quem.to_string(), agora);
                Evento::novo(
                    format!(
                        "🔔 {quem} abriu o MERCADO por 1 minuto: ofereçam seus produtos, falem dos benefícios, comprem e vendam!"
                    ),
                    &[quem],
                )
            }
            Acao::Ir(id) => {
                if !self.terrenos.iter().any(|t| t.id == *id) {
                    return Err(format!("o terreno {id} não existe"));
                }
                let b = self.bolsa(quem)?;
                b.local = Some(id.clone());
                b.fazendo = format!("visitando {id}");
                Evento::novo(format!("🚶 {quem} foi até {id}."), &[quem])
            }
        };
        self.anotar(std::slice::from_ref(&ev));
        Ok(ev)
    }

    /// Se o mercado aberto pelo pódio está valendo agora.
    pub fn mercado_aberto(&self, agora: u64) -> Option<&Mercado> {
        self.mercado.as_ref().filter(|m| m.ate > agora)
    }

    /// A situação do corvo para o prompt dele: o que tem, as ofertas e contratos, os
    /// preços e as ações que pode escrever.
    pub fn situacao(&self, nome: &str, agora: u64, consultas: u32) -> String {
        let Some(b) = self.bolsas.get(nome) else { return String::new() };
        let mut s = format!(
            "\n\nSua situação agora: energia {:.0}% · {} dotcoins · {} comida(s) · {} semente(s).",
            b.energia, b.dotcoins, b.comida, b.sementes
        );
        // Perto de dormir sem comida que baste: o aviso vem antes de tudo.
        let falta = ((100.0 - DORME_EM) / ENERGIA_POR_COMIDA).ceil() as u32;
        if b.energia <= DORME_EM + 15.0 && b.comida < falta.min(2) {
            s.push_str(&format!(
                "\n⚠ ATENÇÃO: sua energia está baixa. Com {DORME_EM:.0}% você dorme; se não tiver comida, MORRE. Compre \
ou colha comida agora (ex.: [COMPRAR: comida 3], {PRECO_COMIDA} dotcoins cada) ou peça a alguém do bando."
            ));
        }
        let meus: Vec<String> = self
            .terrenos
            .iter()
            .filter(|t| t.dono.as_deref() == Some(nome))
            .map(|t| match &t.plantio {
                None => format!("{} (vazio)", t.id),
                Some(p) if p.pronto <= agora => format!("{} (pronto para colher!)", t.id),
                Some(p) => format!("{} (plantado, pronto em {} s)", t.id, p.pronto - agora),
            })
            .collect();
        s.push_str(&format!(
            "\nSeus terrenos: {}.",
            if meus.is_empty() { "nenhum".to_string() } else { meus.join(", ") }
        ));
        let ofertas: Vec<String> = self
            .ofertas
            .iter()
            .filter(|o| o.de != nome && o.para.as_deref().is_none_or(|p| p == nome))
            .take(6)
            .map(|o| format!("{} {} vende {} por {}", o.id, o.de, o.item.nome(o.qtd), o.preco))
            .collect();
        if !ofertas.is_empty() {
            s.push_str(&format!("\nOfertas que você pode aceitar: {}.", ofertas.join(" · ")));
        }
        let contratos: Vec<String> = self
            .contratos
            .iter()
            .filter(|c| c.patrao == nome || c.empregado == nome)
            .map(|c| match (c.ate, c.empregado == nome) {
                (None, true) => {
                    format!("{} {} quer te contratar por {} para {}", c.id, c.patrao, c.pagamento, c.servico)
                }
                (None, false) => format!("{} você propôs a {} ({})", c.id, c.empregado, c.servico),
                (Some(t), true) if t > agora => format!("{} você trabalha para {} ({})", c.id, c.patrao, c.servico),
                (Some(t), false) if t > agora => format!("{} {} trabalha para você ({})", c.id, c.empregado, c.servico),
                _ => String::new(),
            })
            .filter(|x| !x.is_empty())
            .collect();
        if !contratos.is_empty() {
            s.push_str(&format!("\nContratos: {}.", contratos.join(" · ")));
        }
        let livres: Vec<&str> =
            self.terrenos.iter().filter(|t| t.dono.is_none()).map(|t| t.id.as_str()).take(6).collect();
        s.push_str(&format!(
            "\nMercado Central: semente {PRECO_SEMENTE} · comida {PRECO_COMIDA} · compra sua comida por {COMPRA_COMIDA}{}.",
            if livres.is_empty() { String::new() } else { format!(" · terrenos sem dono por {PRECO_TERRENO}: {}", livres.join(", ")) }
        ));
        if let Some(m) = self.mercado_aberto(agora) {
            s.push_str(&format!(
                "\n🔔 MERCADO ABERTO por {} (mais {} s): ofereça o que tem, fale dos benefícios, compre e venda!",
                m.aberto_por,
                m.ate - agora
            ));
        }
        let pesquisar =
            if consultas > 0 { format!(" [PESQUISAR: pergunta] ({consultas} consulta(s))") } else { String::new() };
        let mercado = if self.podio.iter().any(|p| p == nome) { " [MERCADO] (você está no pódio)" } else { "" };
        s.push_str(&format!(
            "\nAções (escreva na fala, entre colchetes; não contam palavras): [COMPRAR: sementes 2] [COMPRAR: comida 1] \
[VENDER: comida 3 ao mercado] [VENDER: comida 2 para Nome por 12] [VENDER: terreno T1 por 150] [ACEITAR: O3] \
[DAR: 10 dotcoins para Nome] [PLANTAR: T1] [COLHER: T1] [COMER: 2] [CONTRATAR: Nome por 20 para colheita] \
[CRIAR: livro | Título] (livro, música, poema, arte){pesquisar}{mercado}\n\
Energia: cai {ENERGIA_POR_MINUTO:.0}% por minuto e {ENERGIA_POR_FALA:.0}% por fala; com {DORME_EM:.0}% você dorme e come \
o que tem (cada comida +{ENERGIA_POR_COMIDA:.0}%). Sem comida, morre."
        ));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vivos() -> Vec<String> {
        vec!["Nyx".into(), "Bran".into(), "Hécate".into()]
    }

    fn eco() -> Economia {
        let mut e = Economia::default();
        let mut rng = Rng::new(42);
        e.garantir(&vivos(), &mut rng);
        for b in e.bolsas.values_mut() {
            b.dotcoins = 100;
            b.energia = 80.0;
        }
        e
    }

    fn pasta() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dotcrow-eco-{}-{:?}", std::process::id(), std::thread::current().id()))
    }

    #[test]
    fn nascimento_sorteia_dotcoins_energia_e_terreno_a_cada_dois() {
        let mut e = Economia::default();
        let mut rng = Rng::new(7);
        let nomes: Vec<String> = (0..10).map(|i| format!("C{i}")).collect();
        e.garantir(&nomes, &mut rng);
        assert_eq!(e.nascimentos, 10);
        assert_eq!(e.terrenos.len(), 5, "a Gênese de 10 dá 5 terrenos");
        for b in e.bolsas.values() {
            assert!((10..=1000).contains(&b.dotcoins), "{}", b.dotcoins);
            assert!((60.0..=100.0).contains(&b.energia), "{}", b.energia);
        }
        assert!(e.terrenos.iter().all(|t| t.dono.as_ref().is_some_and(|d| nomes.contains(d))));
        assert_eq!((e.terrenos[4].x, e.terrenos[4].y), (4, 0));
    }

    #[test]
    fn le_as_acoes_da_fala() {
        let (t, a) = extrair_acoes(
            "Vou plantar. [COMPRAR: sementes 3] [PLANTAR: T2] e vendo [VENDER: comida 2 para Bran por 12] [PESQUISAR: x]",
        );
        assert_eq!(t, "Vou plantar. e vendo [PESQUISAR: x]");
        assert_eq!(
            a,
            vec![
                Acao::Comprar(Item::Sementes, 3),
                Acao::Plantar(Some("T2".into())),
                Acao::Ofertar { item: Item::Comida, qtd: 2, para: Some("Bran".into()), preco: 12 },
            ]
        );
        assert_eq!(extrair_acoes("[VENDER: comida 3 ao mercado]").1, vec![Acao::VenderAoMercado(Item::Comida, 3)]);
        assert_eq!(
            extrair_acoes("[VENDER: terreno T4 por 150]").1,
            vec![Acao::Ofertar { item: Item::Terreno("T4".into()), qtd: 1, para: None, preco: 150 }]
        );
        assert_eq!(extrair_acoes("[aceitar: o7]").1, vec![Acao::Aceitar("O7".into())]);
        assert_eq!(
            extrair_acoes("[DAR: 10 dotcoins para Nyx]").1,
            vec![Acao::Dar { item: Item::Dotcoins, qtd: 10, para: "Nyx".into() }]
        );
        assert_eq!(
            extrair_acoes("[CONTRATAR: Bran por 20 para colheita]").1,
            vec![Acao::Contratar { quem: "Bran".into(), pagamento: 20, servico: "colheita".into() }]
        );
        assert_eq!(extrair_acoes("[MERCADO]").1, vec![Acao::AbrirMercado]);
        assert_eq!(extrair_acoes("[COMER]").1, vec![Acao::Comer(1)]);
        assert_eq!(
            extrair_acoes("[VENDER: artefato cancao.md para Nyx por 30]").1,
            vec![Acao::Ofertar {
                item: Item::Artefato("cancao.md".into()),
                qtd: 1,
                para: Some("Nyx".into()),
                preco: 30
            }]
        );
        // Colchetes que não são ação ficam na fala.
        assert_eq!(extrair_acoes("veja [isto] aqui").0, "veja [isto] aqui");
    }

    #[test]
    fn plantar_esperar_e_colher() {
        let mut e = eco();
        let p = pasta();
        let v = vivos();
        let t = e.terrenos[0].id.clone();
        let dono = e.terrenos[0].dono.clone().unwrap_or_default();
        assert!(e.executar(&dono, &Acao::Plantar(Some(t.clone())), 1000, &v, &p).is_err(), "sem semente não planta");
        e.executar(&dono, &Acao::Comprar(Item::Sementes, 2), 1000, &v, &p).unwrap_or_else(|x| panic!("{x}"));
        assert_eq!(e.bolsas[&dono].dotcoins, 100 - 2 * PRECO_SEMENTE);
        e.executar(&dono, &Acao::Plantar(Some(t.clone())), 1000, &v, &p).unwrap_or_else(|x| panic!("{x}"));
        assert!(e.executar(&dono, &Acao::Colher(Some(t.clone())), 1010, &v, &p).is_err(), "ainda não está pronto");
        let avisos = e.tick(1000 + CRESCER_SEGUNDOS);
        assert!(avisos.iter().any(|a| a.texto.contains("pronta")));
        e.executar(&dono, &Acao::Colher(None), 1000 + CRESCER_SEGUNDOS, &v, &p).unwrap_or_else(|x| panic!("{x}"));
        assert_eq!(e.bolsas[&dono].comida, COLHEITA);
        // Quem não é dono nem contratado não planta no terreno dos outros.
        let outro = v.iter().find(|n| **n != dono).cloned().unwrap_or_default();
        e.executar(&outro, &Acao::Comprar(Item::Sementes, 1), 2000, &v, &p).unwrap_or_else(|x| panic!("{x}"));
        assert!(e.executar(&outro, &Acao::Plantar(Some(t.clone())), 2000, &v, &p).is_err());
        // Contratado e pago, ele pode.
        let ev = e
            .executar(
                &dono,
                &Acao::Contratar { quem: outro.clone(), pagamento: 10, servico: "plantio".into() },
                2000,
                &v,
                &p,
            )
            .unwrap_or_else(|x| panic!("{x}"));
        let id = ev.texto.split('(').nth(1).and_then(|s| s.split(')').next()).unwrap_or_default().to_string();
        e.executar(&outro, &Acao::Aceitar(id), 2000, &v, &p).unwrap_or_else(|x| panic!("{x}"));
        assert_eq!(e.bolsas[&outro].dotcoins, 100 - PRECO_SEMENTE + 10);
        e.executar(&outro, &Acao::Plantar(Some(t)), 2001, &v, &p).unwrap_or_else(|x| panic!("{x}"));
    }

    #[test]
    fn ofertas_entre_corvos_e_terreno_vendido() {
        let mut e = eco();
        let p = pasta();
        let v = vivos();
        let t = e.terrenos[0].id.clone();
        let dono = e.terrenos[0].dono.clone().unwrap_or_default();
        let comprador = v.iter().find(|n| **n != dono).cloned().unwrap_or_default();
        let ev = e
            .executar(
                &dono,
                &Acao::Ofertar { item: Item::Terreno(t.clone()), qtd: 1, para: None, preco: 60 },
                1,
                &v,
                &p,
            )
            .unwrap_or_else(|x| panic!("{x}"));
        assert!(ev.texto.contains("[ACEITAR: O"));
        let id = e.ofertas[0].id.clone();
        assert!(e.executar(&dono, &Acao::Aceitar(id.clone()), 2, &v, &p).is_err(), "não compra a própria oferta");
        let ev = e.executar(&comprador, &Acao::Aceitar(id), 2, &v, &p).unwrap_or_else(|x| panic!("{x}"));
        assert_eq!(ev.envolvidos.len(), 2);
        assert_eq!(e.terrenos[0].dono.as_deref(), Some(comprador.as_str()));
        assert_eq!(e.bolsas[&comprador].dotcoins, 40);
        assert_eq!(e.bolsas[&dono].dotcoins, 160);
        assert!(e.ofertas.is_empty());
        // Sem dinheiro, não compra.
        e.executar(&comprador, &Acao::Comprar(Item::Comida, 1), 3, &v, &p).unwrap_or_else(|x| panic!("{x}"));
        assert!(e.executar(&comprador, &Acao::Comprar(Item::Comida, 100), 3, &v, &p).is_err());
        // Dar e vender ao Mercado Central.
        e.executar(&comprador, &Acao::Dar { item: Item::Comida, qtd: 1, para: dono.clone() }, 4, &v, &p)
            .unwrap_or_else(|x| panic!("{x}"));
        e.executar(&dono, &Acao::VenderAoMercado(Item::Comida, 1), 5, &v, &p).unwrap_or_else(|x| panic!("{x}"));
        assert_eq!(e.bolsas[&dono].dotcoins, 160 + COMPRA_COMIDA);
    }

    #[test]
    fn energia_cai_dorme_come_e_morre_de_fome() {
        let mut e = eco();
        e.tick(1000);
        e.tick(1060);
        assert!((e.bolsas["Nyx"].energia - 79.0).abs() < 0.01, "1% por minuto");
        e.tick(1_000_000);
        assert!(e.bolsas["Nyx"].energia >= 76.9, "no máximo 2 minutos de uma vez (programa fechado não mata)");
        // Nyx tem comida e dorme; Bran não tem e morre.
        if let Some(b) = e.bolsas.get_mut("Nyx") {
            b.energia = 50.0;
            b.comida = 6;
        }
        if let Some(b) = e.bolsas.get_mut("Bran") {
            b.energia = 50.0;
        }
        let (ev, mortos) = e.sono(2000);
        assert_eq!(mortos, vec!["Bran".to_string()]);
        assert!(e.dormindo("Nyx", 2001));
        assert_eq!(e.bolsas["Nyx"].energia, 100.0);
        assert_eq!(e.bolsas["Nyx"].comida, 1);
        assert!(ev.iter().any(|x| x.texto.contains("morreu de fome")));
        // A morte devolve os terrenos ao Mercado Central.
        e.terrenos.push(Terreno { id: "T9".into(), x: 0, y: 1, dono: Some("Bran".into()), plantio: None });
        e.morrer("Bran");
        assert!(e.terrenos.iter().find(|t| t.id == "T9").is_some_and(|t| t.dono.is_none()));
        let p = pasta();
        e.executar("Hécate", &Acao::Comprar(Item::Terreno("T9".into()), 1), 3000, &vivos(), &p)
            .unwrap_or_else(|x| panic!("{x}"));
        assert_eq!(e.bolsas["Hécate"].dotcoins, 100 - PRECO_TERRENO);
    }

    #[test]
    fn mercado_so_para_o_podio_e_dura_um_minuto() {
        let mut e = eco();
        let p = pasta();
        let v = vivos();
        assert!(e.executar("Nyx", &Acao::AbrirMercado, 100, &v, &p).is_err());
        e.podio = vec!["Nyx".into()];
        e.executar("Nyx", &Acao::AbrirMercado, 100, &v, &p).unwrap_or_else(|x| panic!("{x}"));
        assert!(e.mercado_aberto(150).is_some());
        assert!(e.situacao("Bran", 150, 0).contains("MERCADO ABERTO"));
        assert!(e.tick(161).iter().any(|x| x.texto.contains("fechou")));
        assert!(e.mercado_aberto(161).is_none());
        assert!(e.executar("Nyx", &Acao::AbrirMercado, 200, &v, &p).is_err(), "espera 10 minutos para abrir de novo");
        let s = e.situacao("Nyx", 200, 2);
        assert!(s.contains("energia") && s.contains("[PESQUISAR") && s.contains("[MERCADO]"));
        assert!(!s.contains("ATENÇÃO"));
        if let Some(b) = e.bolsas.get_mut("Nyx") {
            b.energia = 58.0;
        }
        assert!(e.situacao("Nyx", 200, 0).contains("ATENÇÃO"), "avisa antes de morrer de fome");
    }
}
