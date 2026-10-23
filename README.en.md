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
> **Pre-alpha, built in public.** The crows already think and act with real LLMs, but there is a lot ahead and the cryptography has not been externally audited. Do not use it with production data or keys. It is a great time to jump in and leave your mark. 🪶

## What it is

**DotCrow** ([dotcrow.si](https://dotcrow.si)) is an open source flock of AI agents that runs on your own machine. Instead of asking for an answer, you delegate a complete task, and the crows get it done: one plans, one remembers how it was done before, one reviews the risks, one executes and one writes the report.

- **Six crows, each thinking differently**, reviewing each other before acting — and you watch **each one think live**, token by token.
- **Any LLM with an API.** Claude, GPT, Gemini, OpenRouter, DeepSeek, Groq, local models through Ollama. When one runs out of quota or goes down, DotCrow **fails over to the next automatically**.
- **Talk to them mid-task.** Change your mind while they work and they adjust on the next action.
- **Nothing ships wrong.** If the Vigia doesn't approve the result, the task **goes back to the executor** to redo it.
- **On your phone, from anywhere.** An end-to-end encrypted tunnel links your phone to your computer over the internet, no shared Wi-Fi needed — watch everything live. See [docs/remoto.md](docs/remoto.md).
- **Teach by example.** Record (with consent) someone doing a repetitive task — steps, screen, mouse and the apps/sites used — and it becomes a recipe in memory.
- **Invisible API keys.** Only the owner adds keys, and not even the owner can read them back. No command reveals a key.
- **Post-quantum cryptography.** Hybrid ML-KEM-768 + X25519 envelopes: an attacker has to break both.
- **Nothing irreversible without you.** Deleting, sending, paying or installing always asks first.

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
dotcrow panel --rede     # on your phone (same Wi-Fi): scan the QR code
dotcrow panel --remoto   # on your phone from anywhere (encrypted tunnel): scan the QR code
```

In the panel, paste an API key (Claude, GPT, Gemini, OpenRouter, DeepSeek, Groq, or a local Ollama), test it, and hand a task to the crows to watch each step live. Prefer the terminal?

```bash
dotcrow keys add anthropic
dotcrow test
dotcrow task "write an e-mail telling a customer the delivery is two days late"
dotcrow ask "what is a crow, in one sentence?"
```

To build by hand you need [Rust](https://rustup.rs) 1.88+ and a C compiler: `cargo build --release`. The panel's JSON API, used by the page, phone apps and the upcoming WhatsApp bridge, is documented in [Panel and API](docs/en/painel.md).

**Status:** pre-alpha. The crows plan, review, write and act on the computer with real LLMs. Still ahead: a WhatsApp bridge, a native mobile app and a marketplace of recipes — see the [ROADMAP](ROADMAP.md).

## Contributing

Documentation is available in [English](docs/en/README.md) and [Brazilian Portuguese](docs/README.md). Issues and pull requests in English are welcome. Code identifiers are in English. Start with [CONTRIBUTING.md](CONTRIBUTING.md) (Portuguese) and the issues labeled `boa-primeira-issue` (good first issue).

## License

[Apache License 2.0](LICENSE).
