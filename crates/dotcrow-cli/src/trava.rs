//! A trava da Civilização: no modo Civilização (`dotcrow civ conversar`, `julgar`,
//! `pesquisar` e `missao`), o processo do DotCrow — e tudo o que ele rodar, comandos dos
//! corvos inclusive — só enxerga a pasta `Downloads/DotCrow` e a pasta de dados do
//! DotCrow (o cofre das chaves e a configuração). O resto do disco fica invisível.
//!
//! No Linux isso é o **Landlock**, do próprio kernel (5.13 ou mais novo): é aplicado
//! antes de qualquer thread existir e não tem volta para o processo nem para os filhos.
//! Para conseguir rodar, o sistema fica só para leitura (programas, bibliotecas,
//! certificados, DNS); nada de `/home`, `/tmp`, `/proc` dos outros processos ou
//! `/media`. A internet continua: os corvos falam com os modelos e pesquisam.
//!
//! Fora do Linux (Android, macOS, Windows) a trava não existe e o DotCrow avisa.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static TEMPORARIA: OnceLock<PathBuf> = OnceLock::new();

/// A pasta temporária: com a trava, `Downloads/DotCrow/tmp` (o `/tmp` fica de fora).
pub fn temporaria() -> PathBuf {
    TEMPORARIA.get().cloned().unwrap_or_else(std::env::temp_dir)
}

/// Se a trava está valendo neste processo.
pub fn ativa() -> bool {
    TEMPORARIA.get().is_some()
}

/// Como ficou a trava.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum Situacao {
    /// O kernel aplicou tudo.
    Completa,
    /// O kernel é antigo: aplicou só parte.
    Parcial,
    /// Este sistema não tem Landlock (ou ele está desligado).
    Indisponivel(String),
}

/// As pastas do sistema que ficam só para leitura (para os programas rodarem).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub const SISTEMA: [&str; 22] = [
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib32",
    "/lib64",
    "/libx32",
    "/opt",
    "/etc/ssl",
    "/etc/ca-certificates",
    "/etc/pki",
    "/etc/alternatives",
    "/etc/resolv.conf",
    "/etc/hosts",
    "/etc/host.conf",
    "/etc/nsswitch.conf",
    "/etc/gai.conf",
    "/etc/localtime",
    "/etc/ld.so.cache",
    "/etc/services",
    "/run/systemd/resolve",
    "/proc/self",
];

/// Dispositivos que os programas usam (terminal, números aleatórios, som do microfone).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub const DISPOSITIVOS: [&str; 7] =
    ["/dev/null", "/dev/zero", "/dev/urandom", "/dev/random", "/dev/tty", "/dev/pts", "/dev/snd"];

/// Fecha o processo nas pastas `livres` (leitura e escrita), mais o sistema só para
/// leitura. Chame antes de criar o runtime do tokio (a trava vale para a thread que a
/// aplica e para as que nascem depois). `tmp` vira a pasta temporária.
pub fn aplicar(livres: &[PathBuf], tmp: &Path) -> Situacao {
    let _ = std::fs::create_dir_all(tmp);
    for p in livres {
        let _ = std::fs::create_dir_all(p);
    }
    let s = aplicar_no_sistema(livres, tmp, &certificados());
    if !matches!(s, Situacao::Indisponivel(_)) {
        let _ = TEMPORARIA.set(tmp.to_path_buf());
    }
    s
}

/// Onde o sistema guarda os certificados das conexões HTTPS, quando foi apontado para
/// fora de `/etc/ssl` (`SSL_CERT_FILE`, `SSL_CERT_DIR`): ficam só para leitura.
fn certificados() -> Vec<PathBuf> {
    ["SSL_CERT_FILE", "SSL_CERT_DIR"]
        .iter()
        .filter_map(std::env::var_os)
        .flat_map(|v| std::env::split_paths(&v).collect::<Vec<_>>())
        .filter(|p| p.exists())
        .collect()
}

#[cfg(target_os = "linux")]
fn aplicar_no_sistema(livres: &[PathBuf], tmp: &Path, certificados: &[PathBuf]) -> Situacao {
    use landlock::{
        ABI, Access, AccessFs, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetStatus, path_beneath_rules,
    };
    let abi = ABI::V5;
    let mut rw: Vec<PathBuf> = livres.to_vec();
    rw.push(tmp.to_path_buf());
    let leitura = AccessFs::from_read(abi);
    let tudo = AccessFs::from_all(abi);
    let r = (|| -> Result<RulesetStatus, landlock::RulesetError> {
        let st = Ruleset::default()
            .handle_access(tudo)?
            .create()?
            .add_rules(path_beneath_rules(&rw, tudo))?
            .add_rules(path_beneath_rules(SISTEMA, leitura))?
            .add_rules(path_beneath_rules(certificados, leitura))?
            // Os dispositivos: ler e escrever (terminal, som), sem criar nada em /dev.
            .add_rules(path_beneath_rules(DISPOSITIVOS, leitura | AccessFs::WriteFile))?
            .restrict_self()?;
        Ok(st.ruleset)
    })();
    match r {
        Ok(RulesetStatus::FullyEnforced) => Situacao::Completa,
        Ok(RulesetStatus::PartiallyEnforced) => Situacao::Parcial,
        Ok(RulesetStatus::NotEnforced) => {
            Situacao::Indisponivel("o kernel não tem o Landlock ligado (precisa do Linux 5.13 ou mais novo)".into())
        }
        Err(e) => Situacao::Indisponivel(format!("o Landlock recusou: {e}")),
    }
}

#[cfg(not(target_os = "linux"))]
fn aplicar_no_sistema(_livres: &[PathBuf], _tmp: &Path, _certificados: &[PathBuf]) -> Situacao {
    Situacao::Indisponivel("a trava de pastas (Landlock) só existe no Linux".into())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    // A trava não tem volta: o teste roda num processo filho (o próprio binário de teste).
    #[test]
    fn trava_so_deixa_a_pasta_dotcrow() {
        if std::env::var_os("DOTCROW_TESTE_TRAVA").is_some() {
            let raiz = PathBuf::from(std::env::var("DOTCROW_TESTE_TRAVA").unwrap_or_default());
            let livre = raiz.join("DotCrow");
            let fora = raiz.join("fora");
            let s = aplicar(std::slice::from_ref(&livre), &livre.join("tmp"));
            if let Situacao::Indisponivel(m) = s {
                println!("SEM-LANDLOCK {m}");
                return;
            }
            assert!(std::fs::write(livre.join("ok.txt"), "ok").is_ok(), "dentro da pasta DotCrow grava");
            assert!(std::fs::read_to_string(livre.join("ok.txt")).is_ok());
            assert!(std::fs::read_to_string(fora.join("segredo.txt")).is_err(), "fora não lê");
            assert!(std::fs::write(fora.join("novo.txt"), "x").is_err(), "fora não grava");
            assert!(std::fs::read_dir("/home").is_err() || !Path::new("/home").exists(), "nem lista /home");
            assert!(std::fs::read_to_string("/etc/hostname").is_err() || !Path::new("/etc/hostname").exists());
            assert!(Path::new("/usr").read_dir().is_ok(), "o sistema fica para leitura");
            assert_eq!(temporaria(), livre.join("tmp"));
            println!("TRAVA-OK");
            return;
        }
        let raiz = std::env::temp_dir().join(format!("dotcrow-trava-{}", std::process::id()));
        let _ = std::fs::create_dir_all(raiz.join("fora"));
        let _ = std::fs::write(raiz.join("fora").join("segredo.txt"), "segredo");
        let exe = std::env::current_exe().unwrap_or_default();
        let out = std::process::Command::new(exe)
            .args(["trava::tests::trava_so_deixa_a_pasta_dotcrow", "--exact", "--nocapture", "--test-threads=1"])
            .env("DOTCROW_TESTE_TRAVA", &raiz)
            .output()
            .unwrap_or_else(|e| panic!("{e}"));
        let txt = String::from_utf8_lossy(&out.stdout);
        let _ = std::fs::remove_dir_all(&raiz);
        assert!(out.status.success(), "{txt}\n{}", String::from_utf8_lossy(&out.stderr));
        assert!(txt.contains("TRAVA-OK") || txt.contains("SEM-LANDLOCK"), "{txt}");
    }
}
