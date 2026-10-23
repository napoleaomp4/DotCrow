# API key vault

## The rule

> Only the computer's owner adds keys. Once stored, **nobody can view the key**, not even the owner. You can list, delete and create another one.

Why: API keys are money. If the interface, CLI or local API could display a key, anything able to use those interfaces (a script, an extension, an agent tricked by prompt injection, someone looking at the screen) could copy it. Without a read operation, that path simply does not exist.

## What you can do

The commands and example output below retain their Portuguese spelling:

```bash
dotcrow chaves adicionar anthropic --rotulo "conta pessoal"   # reads without echoing to the screen
dotcrow chaves listar
dotcrow chaves apagar 1a2b3c4d
```

`listar` (list) displays:

```text
1a2b3c4d  anthropic   conta pessoal        8cfd-d087-daa7-e5ef  adicionada em 2026-10-19
```

Here, `conta pessoal` means "personal account" and `adicionada em` means "added on".

The **fingerprint** (`8cfd-d087-...`) is a truncated, domain-separated hash of the key. It lets you check which key is which (for example, by comparing it with a fingerprint generated on another computer) without revealing any part of the key.

To **replace** a key: delete the old one and add the new one.

## How it works internally

- The entire vault is JSON encrypted with **XChaCha20-Poly1305** and written atomically to `vault.bin`, with `0600` permissions on Linux and macOS.
- The **master key** that encrypts the vault currently lives in `master.key`, also `0600`, in the same directory. **This is temporary.** The phase 1 issue moves it to the operating system's keychain (Keychain on macOS, Credential Manager on Windows, Secret Service on Linux).
- In memory, keys live in `Zeroizing<String>` and are erased when no longer in use.
- The only access to a key's plaintext is `Vault::lease(provedor)`, which returns a `Lease` with `with_secret(|chave| ...)`. The provider adapter uses it solely to build the request header.
- `Lease` and `Vault` have redacted `Debug` implementations. Tests ensure that keys never appear in `Debug`, listings or the file on disk.
- Duplicate keys (same provider and fingerprint) are rejected.

## Guarantees and limitations

| We guarantee | We do not guarantee protection against |
|---|---|
| No command, screen or local API returns a key. | A program with administrator privileges reading Ninho's memory while the key is in use. |
| Keys are not stored in plaintext on disk. | Someone copying both `vault.bin` **and** `master.key` (resolved when the master key moves to the operating system's keychain). |
| Keys do not appear in logs or error messages. | What an LLM provider does with the key on its end. |

In phase 1, the vault moves inside **Ninho** (the local service, Portuguese for "nest"). The CLI and app communicate with Ninho over an authenticated local channel whose protocol **has no** key-reading verb. This turns the "nobody sees the key" rule from a coding convention into a process boundary.

## Contributing here

Vault changes are sensitive: they require tests for secret leakage and two approvals. **Never** add a "show key" command, even for debugging.
