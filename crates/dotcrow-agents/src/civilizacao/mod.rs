//! A Civilização dos Corvos (DotCrow v2), Fase 1: o motor.
//!
//! Isto é diferente do bando de 6 corvos que faz tarefas (ver o topo de `lib.rs`).
//! Aqui vive uma **população de 10 corvos** com genoma, modelo próprio e reputação,
//! que nasce na Gênese, compete, e a cada ciclo os piores morrem e nascem novos por
//! cruzamento e mutação — o Livro dos Mortos passa as lições adiante.
//!
//! Esta fase roda só em Sandbox (sem rede): o motor, o desenho e a mecânica de
//! vida/morte/ranking. Muralha, Alfândega e Conselho vêm nas próximas fases.
//!
//! Modelos: por padrão a Gênese distribui os corvos entre os modelos baratos que
//! você passar (ex.: metade em `gpt-5-nano`, metade em `claude-haiku-5-5`). O Opus
//! 5.5 **não** entra aqui — ele fica reservado para os pontos de controle. Dá para
//! trocar o modelo de cada corvo depois com [`Corvo::definir_modelo`].

pub mod conversa;
mod desenho;

pub use desenho::{Tamanho, cor_provedor, desenhar};

/// Quantos corvos vivem ao mesmo tempo.
pub const POPULACAO: usize = 10;
/// Quantos morrem (e nascem) a cada ciclo.
pub const RENOVADOS: usize = 5;
/// RAG inicial de cada corvo, em MB.
pub const RAG_INICIAL: f32 = 0.1;
/// Ganho-base de RAG por dia sobrevivido, em MB.
pub const RAG_POR_DIA: f32 = 1.0;

/// O modelo de LLM de um corvo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modelo {
    /// Provedor (anthropic, openai, gemini...).
    pub provedor: String,
    /// Nome do modelo (claude-haiku-5-5, gpt-5-nano...).
    pub modelo: String,
}

impl Modelo {
    /// Atalho para montar um modelo.
    pub fn novo(provedor: &str, modelo: &str) -> Self {
        Self { provedor: provedor.into(), modelo: modelo.into() }
    }
}

/// O jeito de pensar do corvo (muda o debate e o desenho).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Personalidade {
    /// Pesa riscos antes de agir.
    Cauteloso,
    /// Traz ideias novas.
    Criativo,
    /// Duvida e pede prova.
    Cetico,
    /// Faz acontecer.
    Executor,
    /// Pensa no objetivo primeiro.
    Estrategista,
    /// Busca consenso.
    Diplomata,
    /// Desafia o que está posto.
    Rebelde,
    /// Segue o passo a passo.
    Metodico,
    /// Enxerga longe.
    Visionario,
    /// Foca no que funciona agora.
    Pragmatico,
}

impl Personalidade {
    /// Todas, para sortear na Gênese.
    pub const TODAS: [Personalidade; 10] = [
        Personalidade::Cauteloso,
        Personalidade::Criativo,
        Personalidade::Cetico,
        Personalidade::Executor,
        Personalidade::Estrategista,
        Personalidade::Diplomata,
        Personalidade::Rebelde,
        Personalidade::Metodico,
        Personalidade::Visionario,
        Personalidade::Pragmatico,
    ];

    /// Nome em português.
    pub fn nome(&self) -> &'static str {
        match self {
            Personalidade::Cauteloso => "cauteloso",
            Personalidade::Criativo => "criativo",
            Personalidade::Cetico => "cético",
            Personalidade::Executor => "executor",
            Personalidade::Estrategista => "estrategista",
            Personalidade::Diplomata => "diplomata",
            Personalidade::Rebelde => "rebelde",
            Personalidade::Metodico => "metódico",
            Personalidade::Visionario => "visionário",
            Personalidade::Pragmatico => "pragmático",
        }
    }

    /// Como esse jeito aparece numa conversa.
    pub fn jeito(&self) -> &'static str {
        match self {
            Personalidade::Cauteloso => "pesa os riscos antes de concordar e lembra o que pode dar errado",
            Personalidade::Criativo => "traz ideias novas e inesperadas, gosta de imaginar possibilidades",
            Personalidade::Cetico => "duvida, pede provas e aponta furos no que os outros dizem",
            Personalidade::Executor => "vai direto ao ponto e propõe o próximo passo concreto",
            Personalidade::Estrategista => "pensa no objetivo maior e no plano antes dos detalhes",
            Personalidade::Diplomata => "busca consenso, junta as opiniões e acalma os ânimos",
            Personalidade::Rebelde => "desafia o que está posto e defende o caminho menos óbvio",
            Personalidade::Metodico => "organiza em passos, gosta de ordem e de checklist",
            Personalidade::Visionario => "fala do futuro e do que isso pode virar daqui a anos",
            Personalidade::Pragmatico => "foca no que funciona agora, com o que já existe",
        }
    }
}

/// No que o corvo é bom (muda o símbolo no peito do desenho).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Especialidade {
    /// Segurança.
    Seguranca,
    /// Código.
    Codigo,
    /// Pesquisa.
    Pesquisa,
    /// Memória.
    Memoria,
    /// Escrita.
    Escrita,
    /// Organização.
    Organizacao,
    /// Dados.
    Dados,
    /// Negócios.
    Negocios,
}

impl Especialidade {
    /// Todas, para sortear na Gênese.
    pub const TODAS: [Especialidade; 8] = [
        Especialidade::Seguranca,
        Especialidade::Codigo,
        Especialidade::Pesquisa,
        Especialidade::Memoria,
        Especialidade::Escrita,
        Especialidade::Organizacao,
        Especialidade::Dados,
        Especialidade::Negocios,
    ];

    /// Nome em português.
    pub fn nome(&self) -> &'static str {
        match self {
            Especialidade::Seguranca => "segurança",
            Especialidade::Codigo => "código",
            Especialidade::Pesquisa => "pesquisa",
            Especialidade::Memoria => "memória",
            Especialidade::Escrita => "escrita",
            Especialidade::Organizacao => "organização",
            Especialidade::Dados => "dados",
            Especialidade::Negocios => "negócios",
        }
    }

    /// Símbolo que aparece no peito do corvo.
    pub fn simbolo(&self) -> char {
        match self {
            Especialidade::Seguranca => '#',
            Especialidade::Codigo => '*',
            Especialidade::Pesquisa => '?',
            Especialidade::Memoria => '@',
            Especialidade::Escrita => '~',
            Especialidade::Organizacao => '+',
            Especialidade::Dados => '%',
            Especialidade::Negocios => '$',
        }
    }
}

/// O estado atual do corvo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    /// Acordado, à toa.
    Online,
    /// Pensando numa resposta.
    Pensando,
    /// Pesquisando algo.
    Pesquisando,
    /// Em missão.
    EmMissao,
    /// Dormindo.
    Dormindo,
}

impl Estado {
    /// Rótulo em português.
    pub fn rotulo(&self) -> &'static str {
        match self {
            Estado::Online => "online",
            Estado::Pensando => "pensando",
            Estado::Pesquisando => "pesquisando",
            Estado::EmMissao => "em missão",
            Estado::Dormindo => "dormindo",
        }
    }
}

/// O genoma: o que define o corvo e o seu desenho. Determinístico pela semente.
#[derive(Debug, Clone)]
pub struct Genoma {
    /// Modelo de LLM que é o cérebro do corvo.
    pub modelo: Modelo,
    /// Jeito de pensar.
    pub personalidade: Personalidade,
    /// No que é bom.
    pub especialidade: Especialidade,
    /// Geração em que nasceu.
    pub geracao: u32,
    /// Semente que gera os detalhes do desenho.
    pub semente: u64,
}

/// Um corvo da civilização.
#[derive(Debug, Clone)]
pub struct Corvo {
    /// Nome.
    pub nome: String,
    /// Genoma.
    pub genoma: Genoma,
    /// Reputação, de 0 a 10.
    pub reputacao: f32,
    /// Memória acumulada (RAG), em MB.
    pub rag_mb: f32,
    /// Dias que já sobreviveu.
    pub idade_dias: u32,
    /// Estado atual.
    pub estado: Estado,
}

impl Corvo {
    /// Troca o modelo deste corvo. Você pode mudar quando quiser, depois da Gênese.
    pub fn definir_modelo(&mut self, modelo: Modelo) {
        self.genoma.modelo = modelo;
    }
}

/// Uma entrada no Livro dos Mortos: o que o corvo deixou ao morrer.
#[derive(Debug, Clone)]
pub struct Biografia {
    /// Nome do corvo.
    pub nome: String,
    /// Geração em que morreu.
    pub geracao: u32,
    /// Reputação final.
    pub reputacao: f32,
    /// A lição que fica para os próximos.
    pub licao: String,
}

/// A civilização: a população viva e a memória dos que já morreram.
#[derive(Debug, Clone)]
pub struct Civilizacao {
    /// Os corvos vivos.
    pub corvos: Vec<Corvo>,
    /// Geração atual.
    pub geracao: u32,
    /// O que os mortos deixaram.
    pub livro_dos_mortos: Vec<Biografia>,
}

/// Nomes possíveis (sem repetir os 6 corvos do bando de tarefas).
const NOMES: [&str; 16] = [
    "Nyx",
    "Ícaro",
    "Vesper",
    "Morrigana",
    "Bran",
    "Corvus",
    "Lenore",
    "Grimm",
    "Sombra",
    "Azrael",
    "Noturna",
    "Rávena",
    "Hécate",
    "Órion",
    "Fuligem",
    "Zéfiro",
];

impl Civilizacao {
    /// A Gênese: cria 10 corvos novos, distribuindo-os entre os `modelos` dados
    /// (round-robin — com 2 modelos, 5 e 5). Mesma `semente` → mesma geração.
    ///
    /// O Opus não entra aqui: ele é reservado para os pontos de controle.
    pub fn genese(semente: u64, modelos: &[Modelo]) -> Civilizacao {
        let mut rng = Rng::new(semente ^ 0x00C0_FFEE);
        let mut nomes: Vec<&str> = NOMES.to_vec();
        for i in (1..nomes.len()).rev() {
            let j = rng.ate(i + 1);
            nomes.swap(i, j);
        }
        let padrao = Modelo::novo("anthropic", "claude-haiku-5-5");
        let mut corvos = Vec::with_capacity(POPULACAO);
        for i in 0..POPULACAO {
            let modelo = if modelos.is_empty() {
                padrao.clone()
            } else {
                modelos.get(i % modelos.len()).cloned().unwrap_or_else(|| padrao.clone())
            };
            let genoma = Genoma {
                modelo,
                personalidade: Personalidade::TODAS[rng.ate(Personalidade::TODAS.len())],
                especialidade: Especialidade::TODAS[rng.ate(Especialidade::TODAS.len())],
                geracao: 1,
                semente: rng.prox(),
            };
            corvos.push(Corvo {
                nome: nomes.get(i).copied().unwrap_or("Corvo").to_string(),
                genoma,
                reputacao: rng.faixa(4.0, 7.0),
                rag_mb: RAG_INICIAL,
                idade_dias: 0,
                estado: Estado::Online,
            });
        }
        Civilizacao { corvos, geracao: 1, livro_dos_mortos: Vec::new() }
    }

    /// Encontra um corvo pelo nome (para trocar o modelo, por exemplo).
    pub fn por_nome_mut(&mut self, nome: &str) -> Option<&mut Corvo> {
        self.corvos.iter_mut().find(|c| c.nome.eq_ignore_ascii_case(nome))
    }

    /// Ranking do dia: os corvos ordenados por reputação (melhor primeiro).
    pub fn ranking(&self) -> Vec<&Corvo> {
        let mut v: Vec<&Corvo> = self.corvos.iter().collect();
        v.sort_by(|a, b| b.reputacao.partial_cmp(&a.reputacao).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    /// Conexões externas do dia: 3 para o 1º, 2 para o 2º, 1 para o 3º, 0 para o resto.
    pub fn conexoes_do_dia(&self) -> Vec<(String, u8)> {
        self.ranking()
            .iter()
            .enumerate()
            .map(|(i, c)| {
                (
                    c.nome.clone(),
                    match i {
                        0 => 3,
                        1 => 2,
                        2 => 1,
                        _ => 0,
                    },
                )
            })
            .collect()
    }

    /// Fecha um ciclo: os piores morrem (vão para o Livro dos Mortos), os melhores
    /// sobrevivem (envelhecem e ganham RAG, com o mérito acelerando) e nascem novos
    /// por cruzamento e mutação dos sobreviventes.
    pub fn ciclo(&mut self, modelos: &[Modelo]) {
        self.corvos.sort_by(|a, b| b.reputacao.partial_cmp(&a.reputacao).unwrap_or(std::cmp::Ordering::Equal));
        let corte = self.corvos.len().saturating_sub(RENOVADOS);
        let mortos: Vec<Corvo> = self.corvos.split_off(corte);
        for m in &mortos {
            self.livro_dos_mortos.push(Biografia {
                nome: m.nome.clone(),
                geracao: m.genoma.geracao,
                reputacao: m.reputacao,
                licao: format!(
                    "corvo {} de {} caiu com reputação {:.1}",
                    m.genoma.personalidade.nome(),
                    m.genoma.especialidade.nome(),
                    m.reputacao
                ),
            });
        }
        for c in &mut self.corvos {
            c.idade_dias += 1;
            let mult = 1.0 + (c.reputacao / 10.0);
            c.rag_mb += RAG_POR_DIA * mult;
        }
        self.geracao += 1;
        let pais: Vec<Corvo> = self.corvos.clone();
        let nascer = POPULACAO.saturating_sub(self.corvos.len());
        let mut rng = Rng::new(u64::from(self.geracao) ^ 0x0000_BEEF);
        for k in 0..nascer {
            let filho = nascer_corvo(&mut rng, &pais, modelos, self.geracao, k, &self.corvos);
            self.corvos.push(filho);
        }
    }
}

/// Faz nascer um corvo cruzando dois sobreviventes, com uma pitada de mutação.
fn nascer_corvo(
    rng: &mut Rng,
    pais: &[Corvo],
    modelos: &[Modelo],
    geracao: u32,
    indice: usize,
    ja_vivos: &[Corvo],
) -> Corvo {
    let padrao = Modelo::novo("anthropic", "claude-haiku-5-5");
    let (p1, p2) =
        if pais.is_empty() { (None, None) } else { (pais.get(rng.ate(pais.len())), pais.get(rng.ate(pais.len()))) };
    // Herda a personalidade de um pai e a especialidade do outro; muta de vez em quando.
    let mut personalidade =
        p1.map(|c| c.genoma.personalidade).unwrap_or(Personalidade::TODAS[rng.ate(Personalidade::TODAS.len())]);
    let mut especialidade =
        p2.map(|c| c.genoma.especialidade).unwrap_or(Especialidade::TODAS[rng.ate(Especialidade::TODAS.len())]);
    if rng.ate(100) < 15 {
        personalidade = Personalidade::TODAS[rng.ate(Personalidade::TODAS.len())];
    }
    if rng.ate(100) < 15 {
        especialidade = Especialidade::TODAS[rng.ate(Especialidade::TODAS.len())];
    }
    let modelo = if modelos.is_empty() {
        padrao.clone()
    } else {
        modelos.get(indice % modelos.len()).cloned().unwrap_or_else(|| padrao.clone())
    };
    let genoma = Genoma { modelo, personalidade, especialidade, geracao, semente: rng.prox() };
    Corvo {
        nome: nome_livre(rng, ja_vivos, indice),
        genoma,
        reputacao: rng.faixa(3.5, 6.0),
        rag_mb: RAG_INICIAL,
        idade_dias: 0,
        estado: Estado::Online,
    }
}

/// Escolhe um nome ainda não usado; se acabarem, inventa um com sufixo.
fn nome_livre(rng: &mut Rng, vivos: &[Corvo], indice: usize) -> String {
    let usados: Vec<&str> = vivos.iter().map(|c| c.nome.as_str()).collect();
    let livres: Vec<&&str> = NOMES.iter().filter(|n| !usados.contains(*n)).collect();
    if let Some(n) = livres.get(rng.ate(livres.len().max(1))) { (**n).to_string() } else { format!("Corvo-{indice}") }
}

/// Gerador determinístico (splitmix64): mesma semente → mesma sequência.
struct Rng(u64);

impl Rng {
    fn new(semente: u64) -> Self {
        Self(semente)
    }

    fn prox(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Um número de 0 a n-1 (0 se n for 0).
    fn ate(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.prox() % n as u64) as usize }
    }

    /// Um real entre lo e hi.
    fn faixa(&mut self, lo: f32, hi: f32) -> f32 {
        let unidade = (self.prox() >> 40) as f32 / (1u64 << 24) as f32;
        lo + unidade * (hi - lo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dois_modelos() -> Vec<Modelo> {
        vec![Modelo::novo("openai", "gpt-5-nano"), Modelo::novo("anthropic", "claude-haiku-5-5")]
    }

    #[test]
    fn genese_cria_dez_corvos_cinco_e_cinco() {
        let c = Civilizacao::genese(42, &dois_modelos());
        assert_eq!(c.corvos.len(), POPULACAO);
        let nano = c.corvos.iter().filter(|x| x.genoma.modelo.modelo == "gpt-5-nano").count();
        let haiku = c.corvos.iter().filter(|x| x.genoma.modelo.modelo == "claude-haiku-5-5").count();
        assert_eq!(nano, 5);
        assert_eq!(haiku, 5);
    }

    #[test]
    fn opus_nunca_vai_para_um_corvo() {
        let c = Civilizacao::genese(7, &dois_modelos());
        assert!(c.corvos.iter().all(|x| !x.genoma.modelo.modelo.contains("opus")));
    }

    #[test]
    fn nomes_nao_repetem() {
        let c = Civilizacao::genese(99, &dois_modelos());
        let mut nomes: Vec<&str> = c.corvos.iter().map(|x| x.nome.as_str()).collect();
        nomes.sort_unstable();
        nomes.dedup();
        assert_eq!(nomes.len(), POPULACAO);
    }

    #[test]
    fn mesma_semente_mesma_geracao() {
        let a = Civilizacao::genese(123, &dois_modelos());
        let b = Civilizacao::genese(123, &dois_modelos());
        let na: Vec<&str> = a.corvos.iter().map(|x| x.nome.as_str()).collect();
        let nb: Vec<&str> = b.corvos.iter().map(|x| x.nome.as_str()).collect();
        assert_eq!(na, nb);
    }

    #[test]
    fn corvo_comeca_com_rag_inicial() {
        let c = Civilizacao::genese(1, &dois_modelos());
        assert!(c.corvos.iter().all(|x| (x.rag_mb - RAG_INICIAL).abs() < f32::EPSILON));
    }

    #[test]
    fn conexoes_vao_para_os_tres_melhores() {
        let c = Civilizacao::genese(5, &dois_modelos());
        let con = c.conexoes_do_dia();
        let valores: Vec<u8> = con.iter().map(|(_, v)| *v).collect();
        assert_eq!(valores.first().copied(), Some(3));
        assert_eq!(valores.get(1).copied(), Some(2));
        assert_eq!(valores.get(2).copied(), Some(1));
        assert!(valores.iter().skip(3).all(|v| *v == 0));
    }

    #[test]
    fn ciclo_mantem_dez_e_renova_cinco() {
        let mut c = Civilizacao::genese(3, &dois_modelos());
        let antes: Vec<String> = c.ranking().iter().take(RENOVADOS).map(|x| x.nome.clone()).collect();
        c.ciclo(&dois_modelos());
        assert_eq!(c.corvos.len(), POPULACAO);
        assert_eq!(c.geracao, 2);
        assert_eq!(c.livro_dos_mortos.len(), RENOVADOS);
        // os melhores de antes continuam vivos
        for nome in antes {
            assert!(c.corvos.iter().any(|x| x.nome == nome));
        }
    }

    #[test]
    fn sobreviventes_ganham_rag() {
        let mut c = Civilizacao::genese(8, &dois_modelos());
        c.ciclo(&dois_modelos());
        // quem sobreviveu tem mais que o RAG inicial
        assert!(c.corvos.iter().any(|x| x.rag_mb > RAG_INICIAL + 0.5));
    }

    #[test]
    fn da_para_trocar_o_modelo_depois() {
        let mut c = Civilizacao::genese(2, &dois_modelos());
        let nome = c.corvos.first().map(|x| x.nome.clone()).unwrap_or_default();
        if let Some(corvo) = c.por_nome_mut(&nome) {
            corvo.definir_modelo(Modelo::novo("gemini", "gemini-3.8-flash"));
        }
        assert!(c.corvos.iter().any(|x| x.genoma.modelo.modelo == "gemini-3.8-flash"));
    }
}
