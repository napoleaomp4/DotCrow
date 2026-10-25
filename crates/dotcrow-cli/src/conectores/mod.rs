//! Conectores: mais de 200 serviços (X, GitHub, Google Drive, Notion, Shopify, Mercado
//! Pago, Telegram, ViaCEP...) que os corvos usam pela **API oficial** de cada um.
//!
//! - O dono liga um conector com `dotcrow conectores adicionar github`: a chave (token) vai
//!   para o mesmo cofre cifrado das chaves de LLM, com o id `con-<serviço>`, e nunca mais é
//!   mostrada. Serviços que mudam de endereço por conta (Shopify, Jira, WordPress...) guardam
//!   também a `conta` (o domínio), que não é segredo.
//! - Os corvos chamam `conector {servico, metodo, caminho, consulta, corpo}`. A chave entra
//!   no pedido só na hora, os corvos nunca a veem, e ela é apagada de qualquer resposta.
//! - Só dá para falar com o endereço oficial do serviço: o caminho não troca de site.
//! - **Ler (GET) é livre. Escrever (POST, PUT, PATCH, DELETE) pede a confirmação do dono
//!   sempre**, até no modo livre: publicar, enviar e apagar não têm volta.
//!
//! Serviços que só entregam token por OAuth (Google, Microsoft, X...) aceitam, por enquanto,
//! um token de acesso colado pelo dono (ele expira; é só adicionar de novo).

mod catalogo;

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use anyhow::{Result, bail};
use dotcrow_agents::hands::ToolOutput;
use dotcrow_providers::KeySource;
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::Value;
use zeroize::Zeroizing;

pub use catalogo::CATALOGO;

/// Como a chave vai no pedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// API pública, sem chave.
    Nenhuma,
    /// `Authorization: Bearer <chave>`.
    Bearer,
    /// Um cabeçalho com um prefixo (ex.: `Authorization: Bot <chave>`, `X-Api-Key: <chave>`).
    Cabecalho {
        /// Nome do cabeçalho.
        nome: &'static str,
        /// O que vem antes da chave.
        prefixo: &'static str,
    },
    /// HTTP Basic: a chave é `usuario:senha`.
    Basic,
    /// A chave vai num parâmetro da URL (`?nome=<chave>`), quando o serviço só aceita assim.
    Consulta(&'static str),
    /// Duas partes (`A:B`) em dois parâmetros da URL.
    ConsultaPar(&'static str, &'static str),
    /// A chave faz parte do endereço (`{chave}` na base, como no Telegram).
    Caminho,
    /// Duas partes (`A:B`) em dois cabeçalhos; a segunda com prefixo.
    Par {
        /// Cabeçalho da primeira parte.
        a: &'static str,
        /// Cabeçalho da segunda parte.
        b: &'static str,
        /// Prefixo da segunda parte.
        prefixo_b: &'static str,
    },
}

/// Categoria, para listar e buscar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cat {
    /// Código e desenvolvimento.
    Codigo,
    /// Nuvem, hospedagem e servidores.
    Nuvem,
    /// Monitoramento e métricas.
    Monitoramento,
    /// Projetos, tarefas e produtividade.
    Projetos,
    /// Arquivos e documentos.
    Arquivos,
    /// E-mail e agenda.
    Email,
    /// Mensagens e atendimento.
    Mensagens,
    /// Redes sociais.
    Social,
    /// Sites, blogs e conteúdo.
    Conteudo,
    /// Lojas e vendas.
    Lojas,
    /// Pagamentos e finanças.
    Financas,
    /// Inteligência artificial e dados.
    Ia,
    /// Busca, notícias e clima.
    Busca,
    /// Imagens, vídeo e música.
    Midia,
    /// Mapas e lugares.
    Mapas,
    /// Ciência, livros e conhecimento.
    Ciencia,
    /// Dados públicos do Brasil.
    Brasil,
    /// Automação, casa e jogos.
    Automacao,
}

impl Cat {
    /// Nome para mostrar.
    pub fn nome(self) -> &'static str {
        match self {
            Self::Codigo => "código",
            Self::Nuvem => "nuvem",
            Self::Monitoramento => "monitoramento",
            Self::Projetos => "projetos",
            Self::Arquivos => "arquivos",
            Self::Email => "e-mail e agenda",
            Self::Mensagens => "mensagens",
            Self::Social => "redes sociais",
            Self::Conteudo => "sites e conteúdo",
            Self::Lojas => "lojas e vendas",
            Self::Financas => "finanças",
            Self::Ia => "IA e dados",
            Self::Busca => "busca, notícias e clima",
            Self::Midia => "mídia",
            Self::Mapas => "mapas",
            Self::Ciencia => "ciência e livros",
            Self::Brasil => "Brasil (dados públicos)",
            Self::Automacao => "automação e lazer",
        }
    }

    /// Todas, na ordem de exibição.
    pub const TODAS: [Cat; 18] = [
        Cat::Codigo,
        Cat::Nuvem,
        Cat::Monitoramento,
        Cat::Projetos,
        Cat::Arquivos,
        Cat::Email,
        Cat::Mensagens,
        Cat::Social,
        Cat::Conteudo,
        Cat::Lojas,
        Cat::Financas,
        Cat::Ia,
        Cat::Busca,
        Cat::Midia,
        Cat::Mapas,
        Cat::Ciencia,
        Cat::Brasil,
        Cat::Automacao,
    ];
}

/// Um serviço do catálogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conector {
    /// Id (minúsculas, números e `-`).
    pub id: &'static str,
    /// Nome.
    pub nome: &'static str,
    /// Categoria.
    pub cat: Cat,
    /// Endereço da API, sem barra no fim. Pode ter `{conta}` (domínio da conta do dono) e
    /// `{chave}` (a chave no endereço).
    pub base: &'static str,
    /// Como a chave vai no pedido.
    pub auth: Auth,
    /// Id de outro conector cuja chave este usa (ex.: o Gmail usa a do `google`).
    pub chave: &'static str,
    /// Cabeçalhos fixos que o serviço exige.
    pub extras: &'static [(&'static str, &'static str)],
    /// Um GET que só lê, para `dotcrow conectores testar`.
    pub teste: Option<&'static str>,
    /// Onde pegar a chave e o formato dela.
    pub dica: &'static str,
}

impl Conector {
    /// Um conector com chave própria, sem cabeçalhos extras e sem dica.
    pub const fn novo(
        id: &'static str,
        nome: &'static str,
        cat: Cat,
        base: &'static str,
        auth: Auth,
        teste: Option<&'static str>,
    ) -> Self {
        Self { id, nome, cat, base, auth, chave: "", extras: &[], teste, dica: "" }
    }

    /// Usa a chave de outro conector.
    pub const fn chave(mut self, de: &'static str) -> Self {
        self.chave = de;
        self
    }

    /// Cabeçalhos fixos.
    pub const fn extras(mut self, e: &'static [(&'static str, &'static str)]) -> Self {
        self.extras = e;
        self
    }

    /// Dica de onde pegar a chave e do formato.
    pub const fn dica(mut self, d: &'static str) -> Self {
        self.dica = d;
        self
    }

    /// O id da chave no cofre (`con-<id>`).
    pub fn id_da_chave(&self) -> String {
        format!("con-{}", if self.chave.is_empty() { self.id } else { self.chave })
    }

    /// Se o endereço depende da conta do dono.
    pub fn precisa_conta(&self) -> bool {
        self.base.contains("{conta}")
    }

    /// Se precisa de chave.
    pub fn precisa_chave(&self) -> bool {
        self.auth != Auth::Nenhuma
    }

    /// Como digitar a chave.
    pub fn formato(&self) -> &'static str {
        match self.auth {
            Auth::Nenhuma => "não precisa de chave",
            Auth::Basic => "usuario:senha (ou o formato da dica)",
            Auth::ConsultaPar(..) | Auth::Par { .. } => "PRIMEIRA:SEGUNDA (as duas partes separadas por dois-pontos)",
            _ => "o token ou chave de API",
        }
    }
}

fn normalizar(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' => 'u',
            'ç' => 'c',
            ' ' | '_' | '.' => '-',
            o => o,
        })
        .collect()
}

/// Acha um conector pelo id ou nome (com alguns apelidos: twitter → x, drive → google-drive).
pub fn achar(nome: &str) -> Option<&'static Conector> {
    let n = normalizar(nome);
    let n = match n.as_str() {
        "twitter" | "x-twitter" => "x",
        "drive" => "google-drive",
        "agenda" | "calendar" | "google-agenda" => "google-calendar",
        "planilhas" | "sheets" => "google-sheets",
        "facebook" => "meta",
        "zap" => "whatsapp",
        "ml" | "mercado-livre" => "mercadolivre",
        "mp" | "mercado-pago" => "mercadopago",
        "office" | "office365" | "microsoft-365" | "graph" => "microsoft",
        "cep" => "viacep",
        outro => outro,
    };
    CATALOGO.iter().find(|c| c.id == n).or_else(|| CATALOGO.iter().find(|c| normalizar(c.nome) == n))
}

/// Busca pelo id, nome ou categoria. Termo vazio = todos.
pub fn buscar(termo: &str) -> Vec<&'static Conector> {
    let t = normalizar(termo);
    if t.is_empty() {
        return CATALOGO.iter().collect();
    }
    CATALOGO
        .iter()
        .filter(|c| c.id.contains(&t) || normalizar(c.nome).contains(&t) || normalizar(c.cat.nome()).contains(&t))
        .collect()
}

/// Situação de um conector para este dono.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Situacao {
    /// Pronto (tem chave e, se precisa, conta).
    Pronto,
    /// Público: funciona sem chave.
    Publico,
    /// Falta a chave.
    FaltaChave,
    /// Falta a conta (o domínio).
    FaltaConta,
}

/// As chaves e contas do dono, para as mãos usarem.
#[derive(Clone)]
pub struct Conexoes {
    keys: Arc<dyn KeySource>,
    contas: BTreeMap<String, String>,
    chaves: HashSet<String>,
}

impl Conexoes {
    /// Monta a partir de uma fonte de chaves, das contas e dos ids de chave que existem.
    pub fn new(keys: Arc<dyn KeySource>, contas: BTreeMap<String, String>, chaves: HashSet<String>) -> Self {
        Self { keys, contas, chaves }
    }

    /// As do cofre e da configuração do DotCrow.
    pub fn do_store(store: &crate::store::Store) -> Self {
        let chaves = store.list_keys().into_iter().map(|k| k.provider).collect();
        Self::new(store.keys(), store.config().conectores, chaves)
    }

    /// A conta (domínio) guardada para o conector.
    pub fn conta(&self, c: &Conector) -> Option<&str> {
        self.contas.get(c.id).map(String::as_str)
    }

    /// Se o conector está pronto para uso.
    pub fn situacao(&self, c: &Conector) -> Situacao {
        if c.precisa_conta() && self.conta(c).is_none() {
            return Situacao::FaltaConta;
        }
        if !c.precisa_chave() {
            return Situacao::Publico;
        }
        if self.chaves.contains(&c.id_da_chave()) { Situacao::Pronto } else { Situacao::FaltaChave }
    }

    /// Os conectores com chave prontos (não inclui os públicos).
    pub fn prontos(&self) -> Vec<&'static Conector> {
        CATALOGO.iter().filter(|c| self.situacao(c) == Situacao::Pronto).collect()
    }

    fn segredo(&self, c: &Conector) -> Option<Zeroizing<String>> {
        let mut out = None;
        self.keys.with_key(&c.id_da_chave(), &mut |k| out = Some(Zeroizing::new(k.to_string())));
        out
    }
}

/// Confere a conta (um domínio, com porta opcional): sem barras, espaços nem `@`.
pub fn validar_conta(conta: &str) -> Result<String, String> {
    let c = conta.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
    let ok = !c.is_empty()
        && c.len() <= 200
        && c.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | ':' | '_'))
        && !c.starts_with(['.', '-', ':']);
    if ok {
        Ok(c.to_lowercase())
    } else {
        Err(format!("conta inválida: \"{conta}\" (use só o domínio, ex.: minhaloja.myshopify.com)"))
    }
}

/// Confere o caminho: começa com `/` ou `?`, sem trocar de site e sem `..`.
pub fn validar_caminho(caminho: &str) -> Result<(), String> {
    let c = caminho.trim();
    if c.is_empty() {
        return Ok(());
    }
    if !(c.starts_with('/') || c.starts_with('?')) {
        return Err("o caminho começa com / (ex.: /user)".into());
    }
    if c.contains("://") || c.starts_with("//") || c.contains('\\') || c.contains('#') {
        return Err("o caminho não pode apontar para outro site".into());
    }
    if c.split(['/', '?', '&']).any(|seg| seg == "..") {
        return Err("o caminho não pode ter ..".into());
    }
    if c.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return Err("o caminho não pode ter espaços (use %20)".into());
    }
    Ok(())
}

/// Marca da chave dentro do endereço (`Auth::Caminho`), trocada só na hora de enviar.
const MARCA: &str = "DOTCROWCHAVE";

/// Monta a URL do pedido (sem a chave) e confere que ela fica no site oficial.
pub fn montar_url(
    c: &Conector,
    conta: Option<&str>,
    caminho: &str,
    consulta: &[(String, String)],
) -> Result<reqwest::Url, String> {
    validar_caminho(caminho)?;
    let mut base = c.base.to_string();
    if c.precisa_conta() {
        let Some(conta) = conta else {
            return Err(format!(
                "o conector {} precisa da sua conta (o domínio): dotcrow conectores adicionar {} --conta ...",
                c.id, c.id
            ));
        };
        base = base.replace("{conta}", &validar_conta(conta)?);
    }
    let base = base.replace("{chave}", MARCA);
    let esperado = reqwest::Url::parse(&base).map_err(|e| format!("endereço inválido: {e}"))?;
    let mut url =
        reqwest::Url::parse(&format!("{base}{}", caminho.trim())).map_err(|e| format!("caminho inválido: {e}"))?;
    if url.scheme() != "https" || url.host_str() != esperado.host_str() || url.port() != esperado.port() {
        return Err("o caminho saiu do site oficial do serviço".into());
    }
    if !consulta.is_empty() {
        let mut q = url.query_pairs_mut();
        for (k, v) in consulta {
            q.append_pair(k, v);
        }
    }
    Ok(url)
}

/// Se o método só lê.
pub fn eh_leitura(metodo: &str) -> bool {
    matches!(metodo.to_uppercase().as_str(), "GET" | "HEAD")
}

/// Tira a chave (e as partes dela) de um texto.
pub fn ocultar(texto: &str, segredo: &str) -> String {
    let mut t = texto.to_string();
    let mut partes: Vec<&str> = vec![segredo];
    if let Some((a, b)) = segredo.split_once(':') {
        partes.push(a);
        partes.push(b);
    }
    for p in partes {
        if p.len() >= 6 {
            t = t.replace(p, "••••");
        }
    }
    t
}

fn cabecalho(nome: &str, valor: &str) -> Result<(HeaderName, HeaderValue), String> {
    let n = HeaderName::from_bytes(nome.as_bytes()).map_err(|_| format!("cabeçalho inválido: {nome}"))?;
    let mut v =
        HeaderValue::from_str(valor).map_err(|_| "a chave tem caracteres que não cabem num cabeçalho".to_string())?;
    v.set_sensitive(true);
    Ok((n, v))
}

/// Tamanho máximo da resposta mostrada ao corvo.
const MAX_RESPOSTA: usize = 24_000;

/// Chama a API do serviço com a chave do cofre. Não pergunta nada: quem chama já decidiu
/// (as mãos pedem a confirmação do dono antes de escrever).
pub async fn chamar(
    http: &reqwest::Client,
    cx: &Conexoes,
    c: &Conector,
    metodo: &str,
    caminho: &str,
    consulta: &[(String, String)],
    corpo: Option<&Value>,
) -> ToolOutput {
    let metodo = metodo.trim().to_uppercase();
    let Ok(m) = reqwest::Method::from_bytes(metodo.as_bytes()) else {
        return ToolOutput::err("metodo: GET, POST, PUT, PATCH, DELETE ou HEAD");
    };
    if !matches!(metodo.as_str(), "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE") {
        return ToolOutput::err("metodo: GET, POST, PUT, PATCH, DELETE ou HEAD");
    }
    let segredo = if c.precisa_chave() {
        match cx.segredo(c) {
            Some(s) => s,
            None => {
                return ToolOutput::err(format!(
                    "o conector {} ({}) ainda não tem chave. O dono liga com: dotcrow conectores adicionar {}",
                    c.id, c.nome, c.id
                ));
            }
        }
    } else {
        Zeroizing::new(String::new())
    };
    let url = match montar_url(c, cx.conta(c), caminho, consulta) {
        Ok(u) => u,
        Err(e) => return ToolOutput::err(e),
    };
    let exibir = url.as_str().to_string();
    let mut final_url = url;
    match c.auth {
        Auth::Caminho => match reqwest::Url::parse(&final_url.as_str().replace(MARCA, &segredo)) {
            Ok(u) => final_url = u,
            Err(_) => return ToolOutput::err("a chave não cabe no endereço"),
        },
        Auth::Consulta(nome) => {
            final_url.query_pairs_mut().append_pair(nome, &segredo);
        }
        Auth::ConsultaPar(a, b) => {
            let Some((va, vb)) = segredo.split_once(':') else {
                return ToolOutput::err(format!("a chave de {} tem o formato PRIMEIRA:SEGUNDA", c.id));
            };
            final_url.query_pairs_mut().append_pair(a, va).append_pair(b, vb);
        }
        _ => {}
    }
    let mut req = http.request(m, final_url);
    for (k, v) in c.extras {
        req = req.header(*k, *v);
    }
    let auth = match c.auth {
        Auth::Bearer => cabecalho("authorization", &format!("Bearer {}", segredo.as_str())).map(|h| vec![h]),
        Auth::Cabecalho { nome, prefixo } => {
            cabecalho(nome, &format!("{prefixo}{}", segredo.as_str())).map(|h| vec![h])
        }
        Auth::Par { a, b, prefixo_b } => match segredo.split_once(':') {
            Some((va, vb)) => {
                cabecalho(a, va).and_then(|h1| cabecalho(b, &format!("{prefixo_b}{vb}")).map(|h2| vec![h1, h2]))
            }
            None => Err(format!("a chave de {} tem o formato PRIMEIRA:SEGUNDA", c.id)),
        },
        _ => Ok(Vec::new()),
    };
    match auth {
        Ok(hs) => {
            for (k, v) in hs {
                req = req.header(k, v);
            }
        }
        Err(e) => return ToolOutput::err(e),
    }
    if c.auth == Auth::Basic {
        let (u, s) = segredo.split_once(':').unwrap_or((segredo.as_str(), ""));
        req = req.basic_auth(u, Some(s));
    }
    if let Some(corpo) = corpo {
        req = match corpo {
            Value::String(s) => req.body(s.clone()),
            v => req.json(v),
        };
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let motivo = if e.is_timeout() { "tempo esgotado".to_string() } else { "falha de rede".to_string() };
            return ToolOutput::err(format!("{} {} {exibir}: {motivo}", c.nome, metodo));
        }
    };
    let status = resp.status();
    let bytes = resp.bytes().await.unwrap_or_default();
    let mut texto = String::from_utf8_lossy(&bytes[..bytes.len().min(400_000)]).to_string();
    if texto.chars().count() > MAX_RESPOSTA {
        texto = format!("{}\n… (resposta cortada)", texto.chars().take(MAX_RESPOSTA).collect::<String>());
    }
    let texto = format!("HTTP {} · {} {} {exibir}\n{texto}", status.as_u16(), c.nome, metodo);
    let texto = ocultar(&texto, &segredo);
    ToolOutput { ok: status.is_success(), text: texto }
}

/// Manual das ferramentas de conectores, para o prompt dos corvos.
pub fn manual(cx: &Conexoes) -> String {
    let prontos: Vec<&str> = cx.prontos().iter().map(|c| c.id).collect();
    let prontos = if prontos.is_empty() {
        "nenhum ainda (o dono liga com dotcrow conectores adicionar)".to_string()
    } else {
        prontos.join(", ")
    };
    format!(
        "- conectores {{busca?}}: mostra o catálogo de {} serviços (X, GitHub, Google Drive, Notion, Shopify, \
Mercado Pago, Telegram, ViaCEP...) com o endereço da API e se o dono já ligou.\n\
- conector {{servico, metodo?, caminho, consulta?, corpo?}}: chama a API OFICIAL do serviço com a chave do cofre \
(você nunca vê a chave). Ex.: conector {{servico: \"github\", caminho: \"/user/repos\"}}. metodo padrão GET; \
consulta = objeto com os parâmetros da URL; corpo = JSON. Ler (GET) é livre; POST, PUT, PATCH e DELETE pedem a \
confirmação do dono toda vez. Ligados agora: {prontos}. Públicos (sem chave): viacep, brasilapi, ibge, bcb, \
open-meteo, wikipedia, nominatim e outros (veja conectores {{busca: \"brasil\"}}). Prefira o conector à ferramenta \
web quando o serviço tiver um.\n",
        CATALOGO.len()
    )
}

/// Texto do `conectores {busca}` para o corvo.
pub fn listar_para_corvo(cx: &Conexoes, busca: &str) -> String {
    let achados = buscar(busca);
    let mut linhas = Vec::new();
    for c in achados.iter().take(60) {
        let estado = match cx.situacao(c) {
            Situacao::Pronto => "ligado",
            Situacao::Publico => "público",
            Situacao::FaltaChave => "sem chave",
            Situacao::FaltaConta => "sem conta",
        };
        let teste = c.teste.map(|t| format!(" · ex.: GET {t}")).unwrap_or_default();
        linhas.push(format!("{} — {} [{}] {} · {estado}{teste}", c.id, c.nome, c.cat.nome(), c.base));
    }
    if achados.len() > 60 {
        linhas.push(format!("… e mais {} (busque por nome ou categoria)", achados.len() - 60));
    }
    if linhas.is_empty() {
        return format!(
            "Nenhum conector com \"{busca}\". Busque por categoria: código, nuvem, projetos, arquivos, \
e-mail, mensagens, redes sociais, lojas, finanças, IA, busca, mídia, mapas, Brasil."
        );
    }
    linhas.join("\n")
}

/// Liga um conector: guarda a conta (se precisa) e a chave (se precisa).
pub fn adicionar(
    store: &crate::store::Store,
    c: &Conector,
    conta: Option<&str>,
    segredo: Option<Zeroizing<String>>,
    rotulo: Option<String>,
) -> Result<()> {
    if c.precisa_conta() {
        let Some(conta) = conta else {
            bail!("o {} precisa da sua conta (o domínio): use --conta. {}", c.nome, c.dica);
        };
        let conta = validar_conta(conta).map_err(anyhow::Error::msg)?;
        store.update_config(|cfg| {
            cfg.conectores.insert(c.id.to_string(), conta);
        })?;
    }
    if c.precisa_chave() {
        let Some(s) = segredo else { bail!("faltou a chave do {}", c.nome) };
        if matches!(c.auth, Auth::Basic | Auth::ConsultaPar(..) | Auth::Par { .. }) && !s.contains(':') {
            bail!("a chave do {} tem duas partes separadas por dois-pontos ({})", c.nome, c.formato());
        }
        store.add_key(&c.id_da_chave(), s, rotulo)?;
    }
    Ok(())
}

/// Desliga um conector: apaga a chave e a conta.
pub fn remover(store: &crate::store::Store, c: &Conector) -> Result<usize> {
    let id = c.id_da_chave();
    let mut n = 0;
    for k in store.list_keys().into_iter().filter(|k| k.provider == id) {
        store.delete_key(&k.id)?;
        n += 1;
    }
    store.update_config(|cfg| {
        cfg.conectores.remove(c.id);
    })?;
    Ok(n)
}

/// `dotcrow conectores [busca]`: o catálogo por categoria, com o que já está ligado.
pub fn listar(store: &crate::store::Store, busca: &str, p: &crate::Paint) {
    let cx = Conexoes::do_store(store);
    let achados = buscar(busca);
    let prontos = CATALOGO.iter().filter(|c| cx.situacao(c) == Situacao::Pronto).count();
    let publicos = CATALOGO.iter().filter(|c| cx.situacao(c) == Situacao::Publico).count();
    println!(
        "\n  {} {}",
        p.bold(&format!("🔌 Conectores ({} no catálogo)", CATALOGO.len())),
        p.dim(&format!("· {prontos} ligado(s) · {publicos} público(s), funcionam sem chave"))
    );
    if achados.is_empty() {
        println!("  {}\n", p.yellow(&format!("Nenhum conector com \"{busca}\".")));
        return;
    }
    for cat in Cat::TODAS {
        let da_cat: Vec<&&Conector> = achados.iter().filter(|c| c.cat == cat).collect();
        if da_cat.is_empty() {
            continue;
        }
        println!("\n  {}", p.bold(cat.nome()));
        for c in da_cat {
            let marca = match cx.situacao(c) {
                Situacao::Pronto => p.green("✓ ligado"),
                Situacao::Publico => p.cyan("● público"),
                Situacao::FaltaConta => p.dim("· falta a conta"),
                Situacao::FaltaChave => p.dim("·"),
            };
            println!("    {:<20} {:<36} {marca}", c.id, c.nome);
        }
    }
    println!(
        "\n  {}\n",
        p.dim("Ligar: dotcrow conectores adicionar github · detalhes: dotcrow conectores ver github · testar: dotcrow conectores testar github")
    );
}

/// `dotcrow conectores ver <id>`: endereço, como vai a chave e a dica.
pub fn ver(store: &crate::store::Store, id: &str, p: &crate::Paint) -> Result<()> {
    let Some(c) = achar(id) else { bail!("não conheço o conector \"{id}\" (veja: dotcrow conectores)") };
    let cx = Conexoes::do_store(store);
    let auth = match c.auth {
        Auth::Nenhuma => "sem chave (API pública)".to_string(),
        Auth::Bearer => "Authorization: Bearer <chave>".to_string(),
        Auth::Cabecalho { nome, prefixo } => format!("{nome}: {prefixo}<chave>"),
        Auth::Basic => "HTTP Basic (chave = usuario:senha)".to_string(),
        Auth::Consulta(n) => format!("parâmetro ?{n}=<chave>"),
        Auth::ConsultaPar(a, b) => format!("parâmetros ?{a}=<A>&{b}=<B> (chave = A:B)"),
        Auth::Caminho => "a chave vai no endereço".to_string(),
        Auth::Par { a, b, prefixo_b } => format!("{a}: <A> e {b}: {prefixo_b}<B> (chave = A:B)"),
    };
    println!("\n  {} {}", p.bold(c.nome), p.dim(&format!("({} · {})", c.id, c.cat.nome())));
    println!("    API:      {}", c.base);
    if let Some(conta) = cx.conta(c) {
        println!("    conta:    {conta}");
    }
    println!("    chave:    {auth}");
    if !c.chave.is_empty() {
        println!("    usa a chave de: {}", c.chave);
    }
    if !c.dica.is_empty() {
        println!("    dica:     {}", c.dica);
    }
    if let Some(t) = c.teste {
        println!("    teste:    GET {t}");
    }
    let estado = match cx.situacao(c) {
        Situacao::Pronto => p.green("ligado"),
        Situacao::Publico => p.cyan("público, já funciona"),
        Situacao::FaltaConta => p.yellow(&format!("falta a conta: dotcrow conectores adicionar {} --conta ...", c.id)),
        Situacao::FaltaChave => p.yellow(&format!("falta a chave: dotcrow conectores adicionar {}", c.id)),
    };
    println!("    situação: {estado}\n");
    Ok(())
}

/// `dotcrow conectores testar <id>`: faz o GET de teste e mostra o começo da resposta.
pub async fn testar(store: &crate::store::Store, id: &str, p: &crate::Paint) -> Result<()> {
    let Some(c) = achar(id) else { bail!("não conheço o conector \"{id}\" (veja: dotcrow conectores)") };
    let Some(caminho) = c.teste else {
        bail!("o {} não tem teste automático (a API dele não tem um GET simples). {}", c.nome, c.dica);
    };
    let cx = Conexoes::do_store(store);
    let r = chamar(&cliente()?, &cx, c, "GET", caminho, &[], None).await;
    let curto: String = r.text.chars().take(600).collect();
    if r.ok {
        println!("  {} {}\n{}\n", p.green("✓"), p.bold(&format!("{} respondeu.", c.nome)), p.dim(&curto));
    } else {
        println!("  {} {}\n{}\n", p.red("✗"), p.bold(&format!("{} não respondeu bem.", c.nome)), curto);
    }
    Ok(())
}

/// Um cliente HTTP para a linha de comando (`dotcrow conectores testar`).
pub fn cliente() -> Result<reqwest::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(dotcrow_providers::tls::preparar(
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(concat!("DotCrow/", env!("CARGO_PKG_VERSION"), " (+https://github.com/napoleaomp4/DotCrow)")),
    )
    .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Uma(&'static str, &'static str);
    impl KeySource for Uma {
        fn with_key(&self, provider: &str, f: &mut dyn FnMut(&str)) -> bool {
            if provider == self.0 {
                f(self.1);
                true
            } else {
                false
            }
        }
    }

    #[test]
    fn catalogo_tem_mais_de_200_e_e_consistente() {
        assert!(CATALOGO.len() >= 200, "só {} conectores", CATALOGO.len());
        let mut ids = HashSet::new();
        for c in CATALOGO {
            assert!(ids.insert(c.id), "id repetido: {}", c.id);
            assert!(
                !c.id.is_empty()
                    && c.id.len() <= 36
                    && c.id.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'),
                "id inválido para o cofre: {}",
                c.id
            );
            assert!(c.base.starts_with("https://") && !c.base.ends_with('/'), "base: {}", c.base);
            assert_eq!(c.auth == Auth::Caminho, c.base.contains("{chave}"), "{{chave}} em {}", c.id);
            if let Some(t) = c.teste {
                assert!(validar_caminho(t).is_ok(), "teste de {}: {t}", c.id);
            }
            if !c.chave.is_empty() {
                assert!(
                    CATALOGO.iter().any(|o| o.id == c.chave),
                    "{} usa a chave de {}, que não existe",
                    c.id,
                    c.chave
                );
            }
            if matches!(c.auth, Auth::ConsultaPar(..) | Auth::Par { .. }) || c.precisa_conta() {
                assert!(!c.dica.is_empty(), "{} precisa de dica do formato", c.id);
            }
            // Com conta de exemplo, a base monta e fica em https.
            assert!(montar_url(c, Some("exemplo.com"), c.teste.unwrap_or(""), &[]).is_ok(), "base de {}", c.id);
        }
        for cat in Cat::TODAS {
            assert!(CATALOGO.iter().any(|c| c.cat == cat), "categoria vazia: {}", cat.nome());
        }
    }

    #[test]
    fn url_fica_no_site_oficial() {
        let gh = achar("github").unwrap_or_else(|| panic!("github"));
        let u =
            montar_url(gh, None, "/user/repos", &[("per_page".into(), "5".into())]).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(u.as_str(), "https://api.github.com/user/repos?per_page=5");
        for ruim in ["//evil.com/x", "/../x", "https://evil.com", "/a\\b", "/x#y", "user", "/a b"] {
            assert!(montar_url(gh, None, ruim, &[]).is_err(), "deveria barrar {ruim}");
        }
        let shop = achar("shopify").unwrap_or_else(|| panic!("shopify"));
        assert!(montar_url(shop, None, "/shop.json", &[]).is_err());
        let u = montar_url(shop, Some("https://Minha-Loja.myshopify.com/"), "/shop.json", &[])
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(u.host_str(), Some("minha-loja.myshopify.com"));
        assert!(validar_conta("evil.com/x").is_err() && validar_conta("a@b.com").is_err());
        let tg = achar("telegram").unwrap_or_else(|| panic!("telegram"));
        let u = montar_url(tg, None, "/getMe", &[]).unwrap_or_else(|e| panic!("{e}"));
        assert!(u.as_str().contains(MARCA) && !u.as_str().contains("{chave}"));
    }

    #[test]
    fn apelidos_busca_e_situacao() {
        assert_eq!(achar("Twitter").map(|c| c.id), Some("x"));
        assert_eq!(achar("Google Drive").map(|c| c.id), Some("google-drive"));
        assert!(buscar("brasil").iter().any(|c| c.id == "viacep"));
        let cx = Conexoes::new(
            Arc::new(Uma("con-github", "ghp_segredo123")),
            BTreeMap::new(),
            ["con-github".to_string()].into(),
        );
        let gh = achar("github").unwrap_or_else(|| panic!("github"));
        assert_eq!(cx.situacao(gh), Situacao::Pronto);
        assert_eq!(cx.situacao(achar("viacep").unwrap_or_else(|| panic!("viacep"))), Situacao::Publico);
        assert_eq!(cx.situacao(achar("shopify").unwrap_or_else(|| panic!("shopify"))), Situacao::FaltaConta);
        assert_eq!(cx.situacao(achar("notion").unwrap_or_else(|| panic!("notion"))), Situacao::FaltaChave);
        // O Gmail usa a chave do google.
        assert_eq!(achar("gmail").map(|c| c.id_da_chave()).as_deref(), Some("con-google"));
        assert!(manual(&cx).contains("Ligados agora: github"));
    }

    #[test]
    fn ocultar_tira_a_chave_e_as_partes() {
        assert_eq!(ocultar("token=ghp_segredo123 ok", "ghp_segredo123"), "token=•••• ok");
        assert_eq!(ocultar("id AC123456 e tok abcdef99", "AC123456:abcdef99"), "id •••• e tok ••••");
        assert!(eh_leitura("get") && !eh_leitura("POST") && !eh_leitura("delete"));
    }

    #[tokio::test]
    async fn sem_chave_avisa_como_ligar() {
        let cx = Conexoes::new(Arc::new(dotcrow_providers::NoKeys), BTreeMap::new(), HashSet::new());
        let http = cliente().unwrap_or_else(|e| panic!("{e}"));
        let r = chamar(&http, &cx, achar("notion").unwrap_or_else(|| panic!("notion")), "GET", "/users/me", &[], None)
            .await;
        assert!(!r.ok && r.text.contains("dotcrow conectores adicionar notion"), "{}", r.text);
        let r = chamar(&http, &cx, achar("viacep").unwrap_or_else(|| panic!("viacep")), "TRACE", "/", &[], None).await;
        assert!(!r.ok);
    }
}
