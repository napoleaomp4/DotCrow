//! A memória própria de cada corvo da civilização.
//!
//! Cada corvo tem uma **pasta com o nome dele**, dentro da pasta da civilização (na
//! Downloads, para sobreviver a desligar o celular e até a reinstalar o app):
//!
//! ```text
//! civilizacao/corvos/Nyx/
//!   memoria.json      falas, vínculos pergunta→resposta, aprendizados, lições, pensamentos
//!   rag.json          os embeddings (vetores de ML) das lembranças, 1 byte por número
//!   livro-razao.json  cada acontecimento com o hash SHA-256 do anterior (uma corrente)
//!   ficha.json        a personalidade: quem ele é e o que viveu, reescrita ao refletir
//!   artefatos/        o que ele criou: livros, músicas, poemas, artes
//! ```
//!
//! - **vínculos**: perguntas que outros corvos fizeram e a resposta dele (é isso que os
//!   jurados e o oráculo avaliam);
//! - **aprendizados**: o que ele trouxe das consultas à internet;
//! - **lições** e **ficha**: o que ele concluiu ao refletir; a ficha vai em toda fala, para
//!   ele lembrar quem é e o que já viveu, mesmo numa conversa nova;
//! - **vetores**: com a chave da OpenAI, cada lembrança importante ganha um embedding
//!   (`text-embedding-3-small`, 256 dimensões) e o corvo lembra pelo significado. Sem a
//!   chave, um vetor local (das palavras) faz o mesmo papel;
//! - **livro-razão**: nascimento, notas, prêmios, consultas, economia, artefatos e morte,
//!   cada lançamento com o hash do anterior. Mudar um lançamento antigo quebra a corrente,
//!   e [`Memoria::integra`] percebe. Os artefatos entram com o hash do arquivo.
//!
//! O tamanho respeita a memória (RAG) do corvo em MB: quando passa, as lembranças mais
//! antigas saem primeiro. O livro-razão e os artefatos nunca são apagados.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Uma fala, lição ou pensamento guardado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registro {
    /// Quando (segundos Unix).
    pub quando: u64,
    /// O texto.
    pub texto: String,
    /// Embedding (hex, 1 byte por dimensão), quando houver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vetor: Option<String>,
}

/// Uma pergunta de outro corvo e a resposta deste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vinculo {
    /// Quando.
    pub quando: u64,
    /// Quem perguntou.
    pub quem_perguntou: String,
    /// A pergunta.
    pub pergunta: String,
    /// A resposta.
    pub resposta: String,
    /// Embedding (hex), quando houver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vetor: Option<String>,
}

/// Algo aprendido numa consulta à internet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aprendizado {
    /// Quando.
    pub quando: u64,
    /// O que ele pesquisou.
    pub termo: String,
    /// Título da fonte.
    pub titulo: String,
    /// Endereço da fonte.
    pub fonte: String,
    /// O resumo que entrou na memória (já aprovado pela Alfândega).
    pub texto: String,
    /// Embedding (hex), quando houver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vetor: Option<String>,
}

/// Um lançamento no livro-razão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lancamento {
    /// Número do lançamento (começa em 1).
    pub n: u64,
    /// Quando.
    pub quando: u64,
    /// Tipo: nascimento, nota, julgamento, premio, consulta, economia, artefato, morte...
    pub tipo: String,
    /// O que aconteceu.
    pub descricao: String,
    /// Hash do lançamento anterior (64 zeros no primeiro).
    pub anterior: String,
    /// Hash deste lançamento.
    pub hash: String,
}

/// A personalidade do corvo: quem ele é e o que já viveu. Vai em toda fala.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ficha {
    /// Personalidade, especialidade, geração e modelo (escrito pelo DotCrow).
    #[serde(default)]
    pub perfil: String,
    /// Quem sou e o que vivi (escrito pelo próprio corvo ao refletir).
    #[serde(default)]
    pub resumo: String,
    /// Quando o corvo reescreveu o resumo.
    #[serde(default)]
    pub atualizada: u64,
}

impl Ficha {
    fn vazia(&self) -> bool {
        self.perfil.is_empty() && self.resumo.is_empty()
    }
}

/// A memória de um corvo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memoria {
    /// Nome do corvo.
    pub nome: String,
    /// O que ele disse.
    #[serde(default)]
    pub falas: Vec<Registro>,
    /// Perguntas de outros corvos e as respostas dele.
    #[serde(default)]
    pub vinculos: Vec<Vinculo>,
    /// O que aprendeu na internet.
    #[serde(default)]
    pub aprendizados: Vec<Aprendizado>,
    /// Lições que tirou ao refletir.
    #[serde(default)]
    pub licoes: Vec<Registro>,
    /// O que pensou sozinho.
    #[serde(default)]
    pub pensamentos: Vec<Registro>,
    /// O livro-razão, com hashes encadeados (fica em `livro-razao.json`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub livro_razao: Vec<Lancamento>,
    /// A personalidade (fica em `ficha.json`).
    #[serde(default, skip_serializing_if = "Ficha::vazia")]
    pub ficha: Ficha,
}

/// Um embedding guardado no `rag.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ItemRag {
    tipo: String,
    chave: String,
    vetor: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ArquivoRag {
    #[serde(default)]
    modelo: String,
    #[serde(default)]
    itens: Vec<ItemRag>,
}

/// Segundos Unix agora.
pub fn agora() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// O nome do corvo como nome de pasta (só letras, números, `-` e `_`).
pub fn nome_de_arquivo(nome: &str) -> String {
    let limpo: String =
        nome.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if limpo.is_empty() { "corvo".into() } else { limpo }
}

/// A pasta de um corvo vivo: `corvos/<Nome>/`.
pub fn pasta_do_corvo(pasta: &Path, nome: &str) -> PathBuf {
    pasta.join("corvos").join(nome_de_arquivo(nome))
}

/// A pasta dos artefatos de um corvo.
pub fn pasta_de_artefatos(pasta: &Path, nome: &str) -> PathBuf {
    pasta_do_corvo(pasta, nome).join("artefatos")
}

/// Onde ficava a memória nas versões antigas (um arquivo só); serve para migrar.
pub fn arquivo_antigo(pasta: &Path, nome: &str) -> PathBuf {
    pasta.join("corvos").join(format!("{}.json", nome_de_arquivo(nome)))
}

fn hash_de(n: u64, quando: u64, tipo: &str, descricao: &str, anterior: &str) -> String {
    dotcrow_crypto::sha256_hex(format!("{n}\u{1f}{quando}\u{1f}{tipo}\u{1f}{descricao}\u{1f}{anterior}").as_bytes())
}

fn chave_rag(tipo: &str, quando: u64, texto: &str) -> String {
    format!("{tipo}:{quando}:{}", &dotcrow_crypto::sha256_hex(texto.as_bytes())[..12])
}

/// Guarda um embedding em 1 byte por dimensão (hex), para caber na memória do corvo.
pub fn compactar(v: &[f32]) -> String {
    let max = v.iter().fold(0.0f32, |m, x| m.max(x.abs())).max(1e-9);
    v.iter().map(|x| format!("{:02x}", ((x / max * 127.0).round() as i32).clamp(-127, 127) as i8 as u8)).collect()
}

/// Volta o embedding compactado para números.
pub fn expandir(hex: &str) -> Vec<f32> {
    (0..hex.len() / 2)
        .filter_map(|i| hex.get(i * 2..i * 2 + 2))
        .filter_map(|b| u8::from_str_radix(b, 16).ok())
        .map(|b| f32::from(b as i8) / 127.0)
        .collect()
}

/// Semelhança de cosseno entre dois vetores (0 se tiverem tamanhos diferentes).
pub fn cosseno(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut ab, mut aa, mut bb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa == 0.0 || bb == 0.0 { 0.0 } else { ab / (aa.sqrt() * bb.sqrt()) }
}

fn sem_acento(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        o => o,
    }
}

fn palavras(s: &str) -> Vec<String> {
    s.to_lowercase()
        .chars()
        .map(sem_acento)
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4)
        .map(str::to_string)
        .collect()
}

/// Dimensões do vetor local.
pub const DIM_LOCAL: usize = 256;

/// Um vetor local (sem internet nem chave): cada palavra cai numa das 256 posições pelo
/// hash dela. Textos com palavras em comum ficam perto. Serve para lembrar e para
/// desenhar a mente do corvo quando não há embedding da OpenAI.
pub fn vetor_local(texto: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; DIM_LOCAL];
    for w in palavras(texto) {
        // FNV-1a: rápido e estável entre versões.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in w.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        let i = (h % DIM_LOCAL as u64) as usize;
        v[i] += if (h >> 32) & 1 == 0 { 1.0 } else { -1.0 };
    }
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
    v
}

fn gravar(caminho: &Path, bytes: &[u8]) -> Result<()> {
    // Grava ao lado e troca de nome: um desligamento no meio não corrompe a memória.
    let tmp = caminho.with_extension("tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("gravando {}", tmp.display()))?;
    std::fs::rename(&tmp, caminho).with_context(|| format!("gravando {}", caminho.display()))?;
    Ok(())
}

/// Nome de arquivo seguro para um artefato.
pub fn slug(titulo: &str) -> String {
    let s: String = titulo
        .to_lowercase()
        .chars()
        .map(sem_acento)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    let s: String = s.chars().take(60).collect();
    if s.is_empty() { "artefato".into() } else { s }
}

impl Memoria {
    /// Abre a memória do corvo (ou começa uma nova, com o lançamento de nascimento).
    /// Traz a memória do formato antigo (um arquivo só), se houver.
    pub fn abrir(pasta: &Path, nome: &str) -> Memoria {
        let dir = pasta_do_corvo(pasta, nome);
        if let Some(m) = Self::ler_pasta(&dir) {
            return m;
        }
        if let Ok(bytes) = std::fs::read(arquivo_antigo(pasta, nome))
            && let Ok(m) = serde_json::from_slice::<Memoria>(&bytes)
        {
            return m;
        }
        let mut m = Memoria { nome: nome.to_string(), ..Default::default() };
        m.registrar("nascimento", &format!("{nome} nasceu na civilização"));
        m
    }

    fn ler_pasta(dir: &Path) -> Option<Memoria> {
        let mut m: Memoria = serde_json::from_slice(&std::fs::read(dir.join("memoria.json")).ok()?).ok()?;
        m.livro_razao = std::fs::read(dir.join("livro-razao.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        m.ficha = std::fs::read(dir.join("ficha.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let rag: ArquivoRag =
            std::fs::read(dir.join("rag.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mapa: std::collections::HashMap<String, String> =
            rag.itens.into_iter().map(|i| (i.chave, i.vetor)).collect();
        let pega = |tipo: &str, quando: u64, texto: &str| mapa.get(&chave_rag(tipo, quando, texto)).cloned();
        for r in &mut m.falas {
            r.vetor = pega("fala", r.quando, &r.texto);
        }
        for r in &mut m.pensamentos {
            r.vetor = pega("pensamento", r.quando, &r.texto);
        }
        for r in &mut m.licoes {
            r.vetor = pega("licao", r.quando, &r.texto);
        }
        for v in &mut m.vinculos {
            v.vetor = pega("vinculo", v.quando, &format!("{} {}", v.pergunta, v.resposta));
        }
        for a in &mut m.aprendizados {
            a.vetor = pega("aprendizado", a.quando, &a.texto);
        }
        Some(m)
    }

    /// Acrescenta um lançamento ao livro-razão, encadeado ao anterior.
    pub fn registrar(&mut self, tipo: &str, descricao: &str) {
        let n = self.livro_razao.last().map(|l| l.n + 1).unwrap_or(1);
        let anterior = self.livro_razao.last().map(|l| l.hash.clone()).unwrap_or_else(|| "0".repeat(64));
        let quando = agora();
        let hash = hash_de(n, quando, tipo, descricao, &anterior);
        self.livro_razao.push(Lancamento { n, quando, tipo: tipo.into(), descricao: descricao.into(), anterior, hash });
    }

    /// Confere a corrente de hashes do livro-razão.
    pub fn integra(&self) -> bool {
        let mut anterior = "0".repeat(64);
        for l in &self.livro_razao {
            if l.anterior != anterior || l.hash != hash_de(l.n, l.quando, &l.tipo, &l.descricao, &l.anterior) {
                return false;
            }
            anterior = l.hash.clone();
        }
        true
    }

    /// Guarda uma fala.
    pub fn falou(&mut self, texto: &str) {
        self.falas.push(Registro { quando: agora(), texto: texto.into(), vetor: None });
    }

    /// Guarda um pensamento.
    pub fn pensou(&mut self, texto: &str) {
        self.pensamentos.push(Registro { quando: agora(), texto: texto.into(), vetor: None });
    }

    /// Guarda as lições de uma reflexão e lança no livro-razão.
    pub fn refletiu(&mut self, licoes: &[String], vetores: Option<Vec<Vec<f32>>>) {
        let vetores = vetores.unwrap_or_default();
        for (i, l) in licoes.iter().enumerate() {
            self.licoes.push(Registro {
                quando: agora(),
                texto: l.clone(),
                vetor: vetores.get(i).map(|v| compactar(v)),
            });
        }
        if !licoes.is_empty() {
            self.registrar("reflexao", &format!("aprendeu: {}", licoes.join(" | ")));
        }
    }

    /// Reescreve o resumo de quem ele é e do que viveu.
    pub fn atualizar_ficha(&mut self, resumo: &str) {
        let r = resumo.trim();
        if r.is_empty() {
            return;
        }
        self.ficha.resumo = r.chars().take(1500).collect();
        self.ficha.atualizada = agora();
    }

    /// As últimas `n` lições, da mais antiga para a mais nova.
    pub fn ultimas_licoes(&self, n: usize) -> Vec<String> {
        let ini = self.licoes.len().saturating_sub(n);
        self.licoes[ini..].iter().map(|l| l.texto.clone()).collect()
    }

    /// Se alguma lembrança tem embedding da OpenAI (vale a pena vetorizar a busca).
    pub fn tem_vetores(&self) -> bool {
        self.vinculos.iter().any(|v| v.vetor.is_some())
            || self.aprendizados.iter().any(|a| a.vetor.is_some())
            || self.licoes.iter().any(|l| l.vetor.is_some())
    }

    /// Guarda um vínculo (pergunta de outro corvo → resposta deste).
    pub fn vincular(&mut self, quem_perguntou: &str, pergunta: &str, resposta: &str) {
        self.vinculos.push(Vinculo {
            quando: agora(),
            quem_perguntou: quem_perguntou.into(),
            pergunta: pergunta.into(),
            resposta: resposta.into(),
            vetor: None,
        });
    }

    /// Guarda o que aprendeu numa consulta e lança no livro-razão.
    pub fn aprendeu(&mut self, a: Aprendizado) {
        self.registrar("consulta", &format!("pesquisou \"{}\" e guardou: {} ({})", a.termo, a.titulo, a.fonte));
        self.aprendizados.push(a);
    }

    /// Tamanho aproximado da memória em bytes.
    pub fn tamanho(&self) -> usize {
        serde_json::to_vec(self).map(|v| v.len()).unwrap_or(0)
    }

    /// Corta as lembranças mais antigas até caber em `limite_mb` (falas primeiro, depois
    /// pensamentos, aprendizados e vínculos, sempre deixando alguns). O livro-razão fica.
    pub fn caber(&mut self, limite_mb: f32) {
        let limite = (f64::from(limite_mb.max(0.01)) * 1024.0 * 1024.0) as usize;
        while self.tamanho() > limite {
            if self.falas.len() > 10 {
                self.falas.remove(0);
            } else if !self.pensamentos.is_empty() {
                self.pensamentos.remove(0);
            } else if !self.falas.is_empty() {
                self.falas.remove(0);
            } else if !self.aprendizados.is_empty() {
                self.aprendizados.remove(0);
            } else if self.vinculos.len() > 2 {
                self.vinculos.remove(0);
            } else if self.licoes.len() > 5 {
                self.licoes.remove(0);
            } else {
                break;
            }
        }
    }

    /// Grava na pasta do corvo (memoria, rag, livro-razão e ficha), respeitando a memória
    /// dele em MB. Some com o arquivo do formato antigo depois de migrar.
    pub fn salvar(&mut self, pasta: &Path, limite_mb: f32) -> Result<()> {
        self.caber(limite_mb);
        let dir = pasta_do_corvo(pasta, &self.nome);
        std::fs::create_dir_all(dir.join("artefatos")).with_context(|| format!("criando {}", dir.display()))?;
        let mut itens = Vec::new();
        let mut copia = self.clone();
        let mut tira = |tipo: &str, quando: u64, texto: &str, vetor: &mut Option<String>| {
            if let Some(v) = vetor.take() {
                itens.push(ItemRag { tipo: tipo.into(), chave: chave_rag(tipo, quando, texto), vetor: v });
            }
        };
        for r in &mut copia.falas {
            tira("fala", r.quando, &r.texto.clone(), &mut r.vetor);
        }
        for r in &mut copia.pensamentos {
            tira("pensamento", r.quando, &r.texto.clone(), &mut r.vetor);
        }
        for r in &mut copia.licoes {
            tira("licao", r.quando, &r.texto.clone(), &mut r.vetor);
        }
        for v in &mut copia.vinculos {
            let t = format!("{} {}", v.pergunta, v.resposta);
            tira("vinculo", v.quando, &t, &mut v.vetor);
        }
        for a in &mut copia.aprendizados {
            tira("aprendizado", a.quando, &a.texto.clone(), &mut a.vetor);
        }
        let livro = std::mem::take(&mut copia.livro_razao);
        let ficha = std::mem::take(&mut copia.ficha);
        gravar(&dir.join("memoria.json"), &serde_json::to_vec_pretty(&copia)?)?;
        gravar(
            &dir.join("rag.json"),
            &serde_json::to_vec_pretty(&ArquivoRag { modelo: "text-embedding-3-small/256".into(), itens })?,
        )?;
        gravar(&dir.join("livro-razao.json"), &serde_json::to_vec_pretty(&livro)?)?;
        gravar(&dir.join("ficha.json"), &serde_json::to_vec_pretty(&ficha)?)?;
        let antigo = arquivo_antigo(pasta, &self.nome);
        if antigo.exists() {
            let _ = std::fs::remove_file(antigo);
        }
        Ok(())
    }

    /// O RAG do corvo: até `n` lembranças (aprendizados, vínculos, lições, pensamentos e
    /// falas antigas) que mais têm a ver com `contexto`. Com `vetor` (o embedding do
    /// contexto) e embeddings guardados, mede pelo significado; senão, pelo vetor local.
    pub fn lembrancas(&self, contexto: &str, n: usize, vetor: Option<&[f32]>) -> Vec<String> {
        let local = vetor_local(contexto);
        let pontua = |texto: &str, emb: &Option<String>| -> f32 {
            match (vetor, emb) {
                (Some(q), Some(e)) => cosseno(q, &expandir(e)) * 10.0,
                _ => cosseno(&local, &vetor_local(texto)) * 10.0,
            }
        };
        let mut itens: Vec<(f32, u64, String)> = Vec::new();
        for a in &self.aprendizados {
            let texto = format!("Aprendi na internet ({}): {}", a.titulo, a.texto);
            let pontos = pontua(&format!("{} {} {}", a.termo, a.titulo, a.texto), &a.vetor);
            itens.push((pontos + 0.5, a.quando, texto));
        }
        for v in &self.vinculos {
            let texto = format!("{} me perguntou \"{}\" e eu respondi: {}", v.quem_perguntou, v.pergunta, v.resposta);
            itens.push((pontua(&format!("{} {}", v.pergunta, v.resposta), &v.vetor), v.quando, texto));
        }
        for l in &self.licoes {
            itens.push((pontua(&l.texto, &l.vetor), l.quando, format!("Lição minha: {}", l.texto)));
        }
        for p in &self.pensamentos {
            itens.push((pontua(&p.texto, &p.vetor) * 0.8, p.quando, format!("Pensei: {}", p.texto)));
        }
        // As falas mais recentes já estão na conversa; aqui entram as de antes.
        let ate = self.falas.len().saturating_sub(4);
        for f in &self.falas[..ate] {
            itens.push((pontua(&f.texto, &f.vetor) * 0.8, f.quando, format!("Eu disse antes: {}", f.texto)));
        }
        itens.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(b.1.cmp(&a.1)));
        itens.into_iter().filter(|(p, _, _)| *p > 0.9).take(n).map(|(_, _, t)| t.chars().take(500).collect()).collect()
    }

    /// Cria um artefato (livro, música, poema, arte...) na pasta do corvo e lança no
    /// livro-razão com o hash SHA-256 do arquivo. Devolve o caminho e o hash.
    pub fn criar_artefato(
        &mut self,
        pasta: &Path,
        tipo: &str,
        titulo: &str,
        conteudo: &str,
    ) -> Result<(PathBuf, String)> {
        let dir = pasta_de_artefatos(pasta, &self.nome);
        std::fs::create_dir_all(&dir).with_context(|| format!("criando {}", dir.display()))?;
        let base = slug(titulo);
        // Arte vem como SVG (só desenho: sem script nem nada de fora); o resto, Markdown.
        let svg = svg_seguro(conteudo);
        let ext = if svg.is_some() { "svg" } else { "md" };
        let mut caminho = dir.join(format!("{base}.{ext}"));
        let mut k = 2;
        while caminho.exists() {
            caminho = dir.join(format!("{base}-{k}.{ext}"));
            k += 1;
        }
        let texto = match svg {
            Some(s) => format!(
                "<!-- {} · {} de {} · {} -->\n{s}\n",
                titulo.trim().replace("--", "—"),
                tipo.trim(),
                self.nome,
                data(agora())
            ),
            None => format!(
                "# {}\n\n*{} de {} · {}*\n\n{}\n",
                titulo.trim(),
                tipo.trim(),
                self.nome,
                data(agora()),
                conteudo.trim()
            ),
        };
        gravar(&caminho, texto.as_bytes())?;
        let hash = dotcrow_crypto::sha256_hex(texto.as_bytes());
        let arquivo = caminho.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.registrar("artefato", &format!("criou {tipo} \"{}\" ({arquivo}) sha256 {hash}", titulo.trim()));
        Ok((caminho, hash))
    }

    /// O corvo deixou de existir: lança a morte, grava e move a pasta para `mortos/` (a
    /// alma fica guardada para poder ser exportada depois).
    pub fn enterrar(mut self, pasta: &Path, motivo: &str) -> Result<PathBuf> {
        self.registrar("morte", motivo);
        self.salvar(pasta, 1000.0)?;
        let origem = pasta_do_corvo(pasta, &self.nome);
        let destino = pasta.join("mortos").join(format!("{}-{}", nome_de_arquivo(&self.nome), agora()));
        if let Some(dir) = destino.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::rename(&origem, &destino).with_context(|| format!("movendo {} para mortos/", origem.display()))?;
        Ok(destino)
    }
}

/// O `<svg>…</svg>` do texto, se for só desenho: sem script, sem eventos (`onload=`...),
/// sem `javascript:`, sem `foreignObject` e sem nada carregado de fora.
pub fn svg_seguro(texto: &str) -> Option<String> {
    let ini = texto.find("<svg")?;
    let fim = texto.rfind("</svg>")? + "</svg>".len();
    if fim <= ini {
        return None;
    }
    let svg = &texto[ini..fim];
    let baixo = svg.to_lowercase();
    let perigoso =
        ["<script", "javascript:", "<foreignobject", "<iframe", "<image", "href=\"http", "href='http", "@import"]
            .iter()
            .any(|x| baixo.contains(x));
    let evento = baixo.match_indices(" on").any(|(i, _)| {
        let resto = &baixo[i + 3..];
        let nome: String = resto.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        !nome.is_empty() && resto[nome.len()..].trim_start().starts_with('=')
    });
    (!perigoso && !evento).then(|| svg.to_string())
}

/// Os artefatos de um corvo (nomes de arquivo).
pub fn artefatos(pasta: &Path, nome: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(pasta_de_artefatos(pasta, nome))
        .map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
        .unwrap_or_default();
    v.retain(|n| !n.ends_with(".tmp"));
    v.sort();
    v
}

/// Passa um artefato de um corvo para outro (venda ou troca). Devolve o novo nome.
pub fn mover_artefato(pasta: &Path, de: &str, para: &str, arquivo: &str) -> Result<String> {
    let nome = std::path::Path::new(arquivo).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if nome.is_empty() || nome.starts_with('.') {
        bail!("artefato inválido");
    }
    let origem = pasta_de_artefatos(pasta, de).join(&nome);
    if !origem.is_file() {
        bail!("{de} não tem o artefato {nome}");
    }
    let dir = pasta_de_artefatos(pasta, para);
    std::fs::create_dir_all(&dir)?;
    let mut destino = dir.join(&nome);
    let mut k = 2;
    while destino.exists() {
        destino = dir.join(format!("{k}-{nome}"));
        k += 1;
    }
    std::fs::rename(&origem, &destino)?;
    Ok(destino.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(nome))
}

/// Data (UTC) AAAA-MM-DD de segundos Unix.
pub fn data(segundos: u64) -> String {
    let dias = (segundos / 86_400) as i64;
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{m:02}-{d:02}", if m <= 2 { y + 1 } else { y })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasta() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "dotcrow-mem-{}-{}-{:?}",
            std::process::id(),
            agora(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&d).unwrap_or_default();
        d
    }

    #[test]
    fn livro_razao_encadeado_e_detecta_adulteracao() {
        let mut m = Memoria::abrir(&pasta(), "Nyx");
        m.registrar("nota", "recebeu 9 do dono");
        m.registrar("premio", "+3 MB e 6 consultas");
        assert!(m.integra());
        assert_eq!(m.livro_razao.len(), 3);
        assert_eq!(m.livro_razao[1].anterior, m.livro_razao[0].hash);
        m.livro_razao[1].descricao = "recebeu 10 do dono".into();
        assert!(!m.integra(), "mudar um lançamento antigo quebra a corrente");
    }

    #[test]
    fn grava_numa_pasta_por_corvo_e_volta_igual() {
        let d = pasta();
        let mut m = Memoria::abrir(&d, "Ícaro");
        m.falou("oi bando");
        m.refletiu(&["Ser direto.".into()], Some(vec![vec![0.5, -0.5, 0.25]]));
        m.vincular("Nyx", "Você voa?", "Voo sim.");
        m.vinculos[0].vetor = Some(compactar(&[1.0, 0.0]));
        m.atualizar_ficha("Sou o Ícaro, curioso. Prometi ao Nyx ajudar na colheita.");
        m.ficha.perfil = "curioso, pesquisa".into();
        m.salvar(&d, 1.0).unwrap_or_else(|e| panic!("{e}"));
        let dir = pasta_do_corvo(&d, "Ícaro");
        for f in ["memoria.json", "rag.json", "livro-razao.json", "ficha.json"] {
            assert!(dir.join(f).is_file(), "falta {f}");
        }
        assert!(dir.join("artefatos").is_dir());
        // Os vetores ficam no rag.json, não na memoria.json.
        let mem = std::fs::read_to_string(dir.join("memoria.json")).unwrap_or_default();
        assert!(!mem.contains("\"vetor\"") && !mem.contains("livro_razao"));
        let de_volta = Memoria::abrir(&d, "Ícaro");
        assert_eq!(de_volta, m, "volta igual, com vetores, ficha e livro-razão");
        assert!(de_volta.integra());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn migra_o_arquivo_antigo() {
        let d = pasta();
        let mut velho = Memoria { nome: "Bran".into(), ..Default::default() };
        velho.registrar("nascimento", "Bran nasceu");
        velho.falou("fala antiga");
        std::fs::create_dir_all(d.join("corvos")).unwrap_or_default();
        std::fs::write(arquivo_antigo(&d, "Bran"), serde_json::to_vec(&velho).unwrap_or_default()).unwrap_or_default();
        let mut m = Memoria::abrir(&d, "Bran");
        assert_eq!(m.falas.len(), 1);
        m.salvar(&d, 1.0).unwrap_or_else(|e| panic!("{e}"));
        assert!(!arquivo_antigo(&d, "Bran").exists(), "depois de migrar, o arquivo antigo some");
        assert_eq!(Memoria::abrir(&d, "Bran").livro_razao.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn grava_volta_e_respeita_o_tamanho() {
        let d = pasta();
        let mut m = Memoria::abrir(&d, "Ícaro");
        for i in 0..400 {
            m.falou(&format!("fala número {i} {}", "x".repeat(400)));
        }
        m.vincular("Nyx", "Você voa?", "Voo sim.");
        m.salvar(&d, 0.05).unwrap_or_else(|e| panic!("{e}"));
        let de_volta = Memoria::abrir(&d, "Ícaro");
        assert!(de_volta.tamanho() <= (0.05 * 1024.0 * 1024.0) as usize + 1);
        assert!(de_volta.falas.len() < 400, "as falas mais antigas saem primeiro");
        assert_eq!(de_volta.vinculos.len(), 1, "o vínculo fica");
        assert!(de_volta.integra());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn lembrancas_trazem_o_que_tem_a_ver() {
        let mut m = Memoria::abrir(&pasta(), "Bran");
        m.aprendeu(Aprendizado {
            quando: 1,
            termo: "vulcões".into(),
            titulo: "Vulcão".into(),
            fonte: "https://pt.wikipedia.org/wiki/Vulc%C3%A3o".into(),
            texto: "Um vulcão é uma abertura na crosta terrestre.".into(),
            vetor: None,
        });
        m.vincular("Nyx", "Qual o melhor café?", "O passado na hora.");
        let l = m.lembrancas("vamos falar de vulcão e crosta", 3, None);
        assert!(l.first().is_some_and(|t| t.contains("Vulcão")));
        assert!(m.integra());
    }

    #[test]
    fn vetor_local_aproxima_textos_parecidos() {
        let a = vetor_local("plantar sementes no terreno");
        let b = vetor_local("o terreno tem sementes para plantar");
        let c = vetor_local("música e poesia ao luar");
        assert!(cosseno(&a, &b) > cosseno(&a, &c));
        assert_eq!(a.len(), DIM_LOCAL);
    }

    #[test]
    fn embedding_compactado_volta_parecido() {
        let v = vec![0.3, -0.9, 0.0, 0.45];
        let hex = compactar(&v);
        assert_eq!(hex.len(), 8);
        let w = expandir(&hex);
        assert!(cosseno(&v, &w) > 0.999);
        assert!((cosseno(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
    }

    #[test]
    fn lembra_pelo_significado_com_vetores() {
        let mut m = Memoria::abrir(&pasta(), "Lenore");
        m.refletiu(&["Ser mais curta.".into(), "Citar fontes.".into()], Some(vec![vec![1.0, 0.0], vec![0.0, 1.0]]));
        assert!(m.tem_vetores());
        let l = m.lembrancas("qualquer coisa", 1, Some(&[0.0, 1.0]));
        assert_eq!(l, vec!["Lição minha: Citar fontes.".to_string()]);
        assert_eq!(m.ultimas_licoes(1), vec!["Citar fontes.".to_string()]);
        assert!(m.livro_razao.iter().any(|x| x.tipo == "reflexao"));
    }

    #[test]
    fn arte_em_svg_so_se_for_desenho() {
        let bom = "Aqui: <svg viewBox=\"0 0 10 10\"><circle cx=\"5\" cy=\"5\" r=\"4\" fill=\"red\"/></svg> pronto";
        assert!(svg_seguro(bom).is_some_and(|s| s.starts_with("<svg") && s.ends_with("</svg>")));
        assert!(svg_seguro("<svg><script>alert(1)</script></svg>").is_none());
        assert!(svg_seguro("<svg><rect onload=\"x()\"/></svg>").is_none());
        assert!(svg_seguro("<svg><image href=\"http://x/y.png\"/></svg>").is_none());
        assert!(svg_seguro("só texto").is_none());
        let d = pasta();
        let mut m = Memoria::abrir(&d, "Corvina");
        let (c, _) = m.criar_artefato(&d, "arte", "Lua", bom).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(c.extension().and_then(|x| x.to_str()), Some("svg"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn artefato_fica_na_pasta_dele_com_hash_no_livro() {
        let d = pasta();
        let mut m = Memoria::abrir(&d, "Vesper");
        let (caminho, hash) = m
            .criar_artefato(&d, "música", "Canção do Corvo / Lua", "Voa, voa, corvo...")
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(caminho.starts_with(pasta_de_artefatos(&d, "Vesper")));
        assert_eq!(caminho.file_name().and_then(|n| n.to_str()), Some("cancao-do-corvo-lua.md"));
        let conteudo = std::fs::read(&caminho).unwrap_or_default();
        assert_eq!(dotcrow_crypto::sha256_hex(&conteudo), hash);
        assert!(m.livro_razao.last().is_some_and(|l| l.tipo == "artefato" && l.descricao.contains(&hash)));
        assert_eq!(artefatos(&d, "Vesper"), vec!["cancao-do-corvo-lua.md".to_string()]);
        let novo = mover_artefato(&d, "Vesper", "Nyx", "cancao-do-corvo-lua.md").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(artefatos(&d, "Nyx"), vec![novo]);
        assert!(mover_artefato(&d, "Vesper", "Nyx", "../../segredo").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn enterrar_move_a_pasta_para_mortos() {
        let d = pasta();
        let mut m = Memoria::abrir(&d, "Grimm");
        m.salvar(&d, 0.1).unwrap_or_else(|e| panic!("{e}"));
        let destino = m.enterrar(&d, "ficou entre os últimos").unwrap_or_else(|e| panic!("{e}"));
        assert!(destino.join("livro-razao.json").is_file());
        assert!(!pasta_do_corvo(&d, "Grimm").exists());
        assert_eq!(data(0), "1970-01-01");
        let _ = std::fs::remove_dir_all(&d);
    }
}
