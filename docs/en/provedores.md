# LLM providers and automatic failover

DotCrow works with any LLM that has an API. The owner adds keys for whichever providers they want, and the **router** decides who answers each request.

## Usage

```bash
dotcrow chaves adicionar anthropic     # paste the key (it is not shown on screen)
dotcrow testar                         # sends a minimal request to each provider
dotcrow provedores                     # lists providers, order and models
dotcrow provedores ordem anthropic,gemini,openai
dotcrow provedores modelo anthropic claude-haiku-5-5   # "padrao" restores the default model
dotcrow provedores url ollama http://192.168.0.10:11434/v1
dotcrow perguntar "explain what a crow is in one sentence"
dotcrow tarefa "write an e-mail telling a customer about the delay"
```

You can also do all of this through the [panel](painel.md) (`dotcrow painel`).

The router's queue starts with providers in your chosen order, followed by providers that have a key but are not in that order, following the table at the end of this page. Ollama does not need a key, so it only enters the queue when you add it to the order. Settings live in `config.json` in the data directory.

## How failover works

The router tries providers in order of preference. When one fails, it classifies the error:

| Error | Example | Switch provider? | Cooldown |
|---|---|:-:|---|
| `QuotaExhausted` | credits exhausted | ✅ | 1 hour |
| `RateLimited` | too many requests | ✅ | as requested by the provider (or 30 s) |
| `Unauthorized` | revoked or incorrect key | ✅ | 10 minutes |
| `NotConfigured` | no key in the vault | ✅ | 10 minutes |
| `Misconfigured` | nonexistent model, wrong address | ✅ | 10 minutes |
| `Unavailable` | down, overloaded, timeout | ✅ | 20 seconds |
| `BadRequest` | invalid request | ❌ (the next provider would fail too) | — |

Providers on cooldown are skipped on subsequent requests, saving time. If **all** are on cooldown, the router tries the one whose cooldown ends soonest so the owner is not left without a response.

See it in action with simulated providers:

```bash
dotcrow demo troca
```

The demo output is in Portuguese (`Pedido` means "request", `trocando` means "switching", and `de castigo, pulado` means "on cooldown, skipped"):

```text
Pedido 1:
  ✗ anthropic  cota ou créditos esgotados → trocando
  ✗ openai     limite de requisições atingido → trocando
  ✓ gemini     [gemini] recebido: organize minha pasta Downloads

Pedido 2:
  ↷ anthropic  de castigo, pulado
  ↷ openai     de castigo, pulado
  ✓ gemini     [gemini] recebido: organize minha pasta Downloads
```

The first request exhausts Anthropic's quota and hits OpenAI's rate limit, then Gemini handles "organize my Downloads folder". The second skips the first two providers and goes straight to Gemini.

The code is in [`crates/dotcrow-providers/src/router.rs`](../../crates/dotcrow-providers/src/router.rs).

## Adding a provider

This is one of the best ways to start contributing. There is an issue for each provider we want, labeled `provedor`.

If the service follows OpenAI's chat API (`/chat/completions`), very little code is needed: a new `Flavor` in [`openai.rs`](../../crates/dotcrow-providers/src/openai.rs) and a line in the [catalog](../../crates/dotcrow-providers/src/catalog.rs). For other APIs, follow the steps below.

### 1. Implement `LlmProvider`

Create `crates/dotcrow-providers/src/<name>.rs`:

```rust
pub struct Ollama { base_url: String, model: String, client: reqwest::Client }

#[async_trait::async_trait]
impl LlmProvider for Ollama {
    fn id(&self) -> &str { "ollama" }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        // 1. convert `req.messages` to the API's format
        // 2. make the HTTP call
        // 3. convert errors with ProviderError::from_http_status(...)
        // 4. return CompletionResponse { text, model }
        todo!()
    }
}
```

The `id` must match the name used in the vault (`dotcrow chaves adicionar <id>`).

### 2. Use the key only when needed

The adapter receives a `KeySource` (the vault) and requests the key when building the header. The `http` module already handles this and marks the header as sensitive:

```rust
let auth = http::secret_header(self.keys.as_ref(), "my-provider", |key| format!("Bearer {key}"))?;
```

Do not store the key in struct fields, put it in error messages or log it.

### 3. Classify errors correctly

`ProviderError::from_http_status` covers common cases (401/403, 402, 429, 5xx). Refine provider-specific behavior. Examples that matter:

- Some providers report "no credits" with status 400 and a message in the body: that is `QuotaExhausted`, not `BadRequest`.
- Read the `retry-after` header when present and pass it as `RateLimited { retry_after }`.
- Connection errors and timeouts are `Unavailable`.

An incorrect classification makes the router switch when it should not, or keep retrying when it should switch.

### 4. Test without calling the API

CI has no keys. Test message conversion and error classification with sample responses, as in the existing modules, and use the fake HTTP server in [`tests/fake_server.rs`](../../crates/dotcrow-providers/tests/fake_server.rs) to check request headers and body. If you want a test against the real API, mark it `#[ignore]` and document how to run it locally.

### 5. Document it

Add the provider to the table below and to the CHANGELOG.

## Providers

| Provider | id | Default model | Where to get a key |
|---|---|---|---|
| Anthropic (Claude) | `anthropic` | `claude-sonnet-5-5` | [console.anthropic.com](https://console.anthropic.com/settings/keys) |
| OpenAI (GPT) | `openai` | `gpt-6.1-sol` | [platform.openai.com](https://platform.openai.com/api-keys) |
| Google (Gemini) | `gemini` | `gemini-3.8-flash` | [aistudio.google.com](https://aistudio.google.com/apikey) |
| OpenRouter | `openrouter` | `openrouter/auto` | [openrouter.ai](https://openrouter.ai/keys) |
| DeepSeek | `deepseek` | `deepseek-flash` | [platform.deepseek.com](https://platform.deepseek.com/api_keys) |
| Groq | `groq` | `openai/gpt-oss-120b` | [console.groq.com](https://console.groq.com/keys) |
| Ollama (on your computer) | `ollama` | `llama3.2` | no key; [install Ollama](https://ollama.com/download) and download a model with `ollama pull` |
| Simulated (tests) | any | | `mock.rs` |

Popular names map to the correct id: `claude` → `anthropic`, `gpt` or `chatgpt` → `openai`, `google` → `gemini`.

Models change quickly. If a provider responds "model not found", change it with `dotcrow provedores modelo <id> <model>` and open an issue so we can update the default.

Adapter details:

- **OpenAI:** uses `max_completion_tokens` and `reasoning_effort: "low"`, required by reasoning models. Exhausted quota arrives as 429 with the code `insufficient_quota`, which DotCrow treats as `QuotaExhausted` (switches and applies a 1-hour cooldown).
- **Anthropic:** "credit balance is too low" arrives as 400 and becomes `QuotaExhausted`; 529 (overloaded) becomes `Unavailable`.
- **Gemini:** the key goes in the `x-goog-api-key` header, never in the URL. An invalid key arrives as 400 and becomes `Unauthorized`. A 429 includes `retryDelay`, which sets the cooldown; a daily limit becomes `QuotaExhausted`.
- **OpenRouter, DeepSeek, Groq, Ollama:** OpenAI-compatible API. Insufficient balance arrives as 402.

## Model tiers (Haiku → Opus)

Agent names here are Huginn (thought), Vigia (lookout), Bico (beak), Gralha (jay) and Pena (feather/quill); their full profiles are in [the agents](agentes.md).

Each stage of a task requests a model tier:

| Tier | Purpose | Anthropic |
|---|---|---|
| **Deep** | Planning (Huginn) and risk review (Vigia) | Opus |
| **Standard** | Computer execution (Bico, Gralha, Pena) and verification | Sonnet |
| **Fast** | Pena's final summary and key testing | Haiku |

If you **pin a model** for a provider in the Providers tab, it applies to all tiers for that provider. Providers without defined tiers use the default model (or the one you pinned).
