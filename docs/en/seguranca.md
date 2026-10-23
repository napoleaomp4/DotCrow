# Security and threat model

DotCrow will operate on people's computers, store API keys that cost money and share knowledge between installations. This document explains **what we protect, how, and what we cannot protect**. If you find a vulnerability, follow [SECURITY.md](../../SECURITY.md).

> **Current status:** the cryptography uses established libraries (RustCrypto and dalek), but how we combine them **has not been externally audited**. An audit is planned before version 1.0.

## What we want to protect

| Asset | Why |
|---|---|
| API keys | They cost money and grant access to the owner's provider accounts. |
| The owner's files and accounts | DotCrow will be able to read, write and act on them. |
| Personal data | Must not leave the machine, including for Revoada. |
| Integrity of actions | Nothing may happen without passing through the policy and, for serious actions, the owner. |
| Installation identity | Nobody may impersonate another DotCrow on Revoada. |

## Who we defend against

1. **Malicious content** (the most likely threat): a page, e-mail, PDF or Revoada recipe containing hidden instructions for the agent, known as *prompt injection*.
2. **An LLM that makes a mistake or "hallucinates"** a dangerous command.
3. **Other unprivileged programs and users** on the same machine trying to read the vault or talk to Ninho (the local service, Portuguese for "nest").
4. **The network**: attackers intercepting traffic today to decrypt it later with a quantum computer ("harvest now, decrypt later").
5. **A compromised Revoada server**, or poisoned recipes sent by malicious installations.

## What we **cannot** protect against

- **Malware with administrator privileges** on the owner's machine. It can read any process's memory, including a key while it is being used. No local program solves this; the operating system must be clean.
- **The owner choosing to do something harmful.** DotCrow asks for confirmation and explains the risk, but the computer's owner makes the decision.
- **What LLM providers do with what they receive.** Content sent to Claude, GPT, etc. is subject to each provider's policy. For sensitive data, use a local model (Ollama).

## Defenses

### 1. Permission policy

Every action proposed by an agent becomes an `Action` with permissions (`ler` = read, `escrever` = write, `apagar` = delete, `executar` = execute, `rede` = network, `sistema` = system), a risk level and whether it is reversible. The `Policy` (in `dotcrow-core`) decides:

- **Irreversible** action → always asks for confirmation.
- Risk above the threshold → asks for confirmation.
- Blocked permission → refuses, even with confirmation.
- Factory default: only **read** runs automatically; **system** is blocked.

Each agent also has a fixed list of what it may request (see [agentes.md](agentes.md)).

### 2. External content is data, not instructions

LLM responses, pages, e-mails, files and recipes enter the context marked as **untrusted**. Vigia (Portuguese for "lookout") reviews each step with that in mind, and the policy applies regardless: even if a text persuades an agent, it cannot grant permissions the agent lacks or bypass confirmations. We will maintain a suite of known attacks as automated tests (phase 2 issue).

### 3. Write-only key vault

Summary (details in [cofre-de-chaves.md](cofre-de-chaves.md)):

- No operation returns a key to the CLI, app or local API.
- On disk, everything is a blob encrypted with XChaCha20-Poly1305.
- In memory, secrets live in `Zeroizing` and are erased when no longer in use.
- Redacted `Debug` and tests ensure keys never appear in listings or logs.

### 4. Hybrid post-quantum cryptography

**Envelope (messages between identities)** — implemented in `dotcrow-crypto::envelope`:

| Stage | Algorithm |
|---|---|
| Post-quantum key agreement | ML-KEM-768 (FIPS 203) |
| Classical key agreement | Ephemeral X25519 |
| Combination | HKDF-SHA256 over both secrets, binding the KEM ciphertext and public keys |
| Encryption | XChaCha20-Poly1305, with the entire header authenticated |

To read a message, an attacker must break **both** key agreements. If a flaw in ML-KEM is discovered, X25519 still provides protection; conversely, ML-KEM protects against quantum computers. The decision is recorded in [ADR 0002](adr/0002-criptografia-hibrida.md).

**Data at rest** — `dotcrow-crypto::sym`: XChaCha20-Poly1305 with a random 24-byte nonce and context-specific associated data.

**Planned:**

- Hybrid **Ed25519 + ML-DSA-65** signatures (FIPS 204) for recipes and certificates.
- **A digital certificate per installation**: each DotCrow generates its key pair locally, and Revoada issues a certificate binding the installation to its public keys; the private key never leaves the machine.
- **TLS with hybrid key exchange** (X25519 + ML-KEM-768) between Ninho and Revoada.
- **True end-to-end encryption** for synchronization between the same owner's devices: the server stores only encrypted blobs it cannot read.

### 5. End-to-end encryption: where it applies and where it does not

To avoid promising what does not exist:

- **Your private data** (history, local memory, synchronization between your devices) will be end-to-end encrypted: only your devices have the keys.
- **Recipes shared on Revoada** must be readable by the server so search works for the whole flock. They are therefore **anonymous by construction**: they pass through the privacy filter and the owner's review before leaving, and travel encrypted over hybrid TLS. See [revoada.md](revoada.md).

### 6. Auditing

Every policy decision, owner confirmation, executed action and provider switch goes into a local hash-chained log so later changes can be detected (phase 1).

## Development practices

- `unsafe` is forbidden throughout the workspace.
- Dependencies are checked by `cargo-deny` in CI (licenses and known vulnerabilities) and updated by Dependabot.
- Changes in sensitive areas require two approvals (see [GOVERNANCE.md](../../GOVERNANCE.md)).
