<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/dotcrow-logo-dark.svg">
    <img alt="DotCrow" src="assets/brand/dotcrow-logo-light.svg" width="380">
  </picture>
</p>

<p align="center">
  <strong>Open source agents that work on your computer.</strong><br>
  You hand over the whole task. The crows plan, act, check and learn.
</p>

<p align="center"><a href="README.md">Português</a> · <a href="CONTRIBUTING.md">Contributing</a> · <a href="SECURITY.md">Security</a></p>

> [!WARNING]
> **Pre-alpha.** DotCrow does not control the computer yet, and the cryptography has not been externally audited. Do not use it with production data or keys.

## What it is

**DotCrow** ([dotcrow.si](https://dotcrow.si)) is an open source agent that runs on your own machine. Instead of asking for an answer, you delegate a complete task, and a flock of agents gets it done: one plans, one remembers how it was done before, one reviews the risks, one executes and one writes the report.

- **Several agents, each thinking differently.** Six crows with distinct reasoning and organizing styles that review each other before acting.
- **Any LLM with an API.** Claude, GPT, Gemini, OpenRouter, local models through Ollama. When one provider runs out of quota or goes down, DotCrow **fails over to the next one automatically**.
- **Invisible API keys.** Only the machine owner can add keys, and not even the owner can read them back. Keys can be listed (provider + fingerprint), deleted and replaced. There is no command that reveals a key.
- **Collective learning mode.** When one DotCrow learns a task, the *recipe* (the steps, with no personal data) can be shared with the **Revoada**, the collective memory. Opt-in, off by default.
- **Post-quantum cryptography.** Hybrid ML-KEM-768 + X25519 envelopes: an attacker has to break both. Each install has its own cryptographic identity, to be backed by a digital certificate.
- **Nothing irreversible without you.** Deleting, sending, paying or installing always asks first. Instructions found inside web pages, e-mails or files never count as orders from the owner.

## The crows

| Agent | Role | Thinks | Organizes |
|---|---|---|---|
| **Huginn** | Strategist | Top-down, goal first | As a dependency tree |
| **Muninn** | Memory | By precedent | By similarity and timeline |
| **Gralha** | Organizer | By category and inventory | As folders and tags |
| **Bico** | Executor | Step by step, verifying each one | As a checklist queue |
| **Pena** | Writer | From the reader's point of view | As a narrative |
| **Vigia** | Guardian | Adversarially | By risk level |

Huginn and Muninn are Odin's ravens in Norse mythology. *Gralha* is the Portuguese word for jay, *Bico* means beak, *Pena* means feather (and quill) and *Vigia* means lookout.

## Quick start

On Linux or macOS, one command installs everything (Rust, a C compiler and DotCrow) and puts `dotcrow` on your PATH:

```bash
curl -fsSL https://raw.githubusercontent.com/napoleaomp4/dotcrow/main/instalar.sh | bash
```

On Linux, the installer also adds a DotCrow icon to your applications menu: just click it. Or open a new terminal and start the visual panel:

```bash
dotcrow panel            # opens in your browser
dotcrow panel --rede     # also on your phone (same Wi-Fi): scan the QR code
```

In the panel, paste an API key (Claude, GPT, Gemini, OpenRouter, DeepSeek, Groq, or a local Ollama), test it, and hand a task to the crows to watch each step live. Prefer the terminal?

```bash
dotcrow keys add anthropic
dotcrow test
dotcrow task "write an e-mail telling a customer the delivery is two days late"
dotcrow ask "what is a crow, in one sentence?"
```

To build by hand you need [Rust](https://rustup.rs) 1.88+ and a C compiler: `cargo build --release`. The panel's JSON API, used by the page, phone apps and the upcoming WhatsApp bridge, is documented in [Panel and API](docs/en/painel.md).

**Status:** pre-alpha. The crows plan, review and write with real LLMs, but they don't control the computer yet.

## Contributing

Documentation is available in [English](docs/en/README.md) and [Brazilian Portuguese](docs/README.md). Issues and pull requests in English are welcome. Code identifiers are in English. Start with [CONTRIBUTING.md](CONTRIBUTING.md) (Portuguese) and the issues labeled `boa-primeira-issue` (good first issue).

## License

[Apache License 2.0](LICENSE).
