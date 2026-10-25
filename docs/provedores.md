# Provedores de LLM e troca automática

O DotCrow funciona com qualquer LLM que tenha API. O dono adiciona as chaves dos provedores que quiser, e o **roteador** decide quem responde a cada pedido.

## Usando

```bash
dotcrow chaves adicionar anthropic     # cola a chave (ela não aparece na tela)
dotcrow testar                         # manda um pedido mínimo para cada provedor
dotcrow provedores                     # lista provedores, ordem e modelos
dotcrow provedores ordem anthropic,gemini,openai
dotcrow provedores modelo anthropic claude-haiku-5-5   # "padrao" volta ao modelo padrão
dotcrow provedores url ollama http://192.168.0.10:11434/v1
dotcrow perguntar "explique o que é um corvo em uma frase"
dotcrow tarefa "escreva um e-mail para a cliente avisando o atraso"
```

Tudo isso também dá para fazer pelo [painel](painel.md) (`dotcrow painel`).

A fila do roteador é: primeiro os provedores na ordem que você definiu, depois os que têm chave e não estão na ordem, na ordem da tabela no fim desta página. O Ollama não precisa de chave, então só entra na fila quando você o coloca na ordem. A configuração fica em `config.json`, na pasta de dados.

## Como a troca funciona

O roteador tenta os provedores na ordem de preferência. Quando um falha, ele classifica o erro:

| Erro | Exemplo | Troca de provedor? | Fica "de castigo" por |
|---|---|:-:|---|
| `QuotaExhausted` | créditos acabaram | ✅ | 1 hora |
| `RateLimited` | muitas requisições | ✅ | o tempo que o provedor pedir (ou 30 s) |
| `Unauthorized` | chave revogada ou errada | ✅ | 10 minutos |
| `NotConfigured` | sem chave no cofre | ✅ | 10 minutos |
| `Misconfigured` | modelo que não existe, endereço errado | ✅ | 10 minutos |
| `Unavailable` | fora do ar, sobrecarregado, timeout | ✅ | 20 segundos |
| `BadRequest` | pedido inválido | ❌ (o próximo falharia igual) | — |

Provedores de castigo são pulados nos pedidos seguintes, sem gastar tempo. Se **todos** estiverem de castigo, o roteador tenta o que volta mais cedo, para não deixar o dono sem resposta.

Veja funcionando, com provedores simulados:

```bash
dotcrow demo troca
```

```
Pedido 1:
  ✗ anthropic  cota ou créditos esgotados → trocando
  ✗ openai     limite de requisições atingido → trocando
  ✓ gemini     [gemini] recebido: organize minha pasta Downloads

Pedido 2:
  ↷ anthropic  de castigo, pulado
  ↷ openai     de castigo, pulado
  ✓ gemini     [gemini] recebido: organize minha pasta Downloads
```

O código está em [`crates/dotcrow-providers/src/router.rs`](../crates/dotcrow-providers/src/router.rs).

## Adicionando um provedor

Esta é uma das melhores formas de começar a contribuir. Há uma issue para cada provedor que queremos, com o rótulo `provedor`.

Se o serviço segue a API de chat da OpenAI (`/chat/completions`), quase não há código: basta um `Flavor` novo em [`openai.rs`](../crates/dotcrow-providers/src/openai.rs) e uma linha no [catálogo](../crates/dotcrow-providers/src/catalog.rs). Para APIs diferentes, siga os passos abaixo.

### 1. Implemente `LlmProvider`

Crie `crates/dotcrow-providers/src/<nome>.rs`:

```rust
pub struct Ollama { base_url: String, model: String, client: reqwest::Client }

#[async_trait::async_trait]
impl LlmProvider for Ollama {
    fn id(&self) -> &str { "ollama" }

    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        // 1. converta `req.messages` para o formato da API
        // 2. faça a chamada HTTP
        // 3. converta erros com ProviderError::from_http_status(...)
        // 4. devolva CompletionResponse { text, model }
        todo!()
    }
}
```

O `id` precisa ser igual ao nome usado no cofre (`dotcrow chaves adicionar <id>`).

### 2. Use a chave só na hora

O adaptador recebe um `KeySource` (o cofre) e pede a chave no momento de montar o cabeçalho. O módulo `http` já faz isso e marca o cabeçalho como sensível:

```rust
let auth = http::secret_header(self.keys.as_ref(), "meu-provedor", |key| format!("Bearer {key}"))?;
```

Não guarde a chave em campos da struct, não coloque em mensagens de erro e não registre em log.

### 3. Classifique bem os erros

`ProviderError::from_http_status` cobre o comum (401/403, 402, 429, 5xx). Refine o que for específico do provedor. Exemplos que fazem diferença:

- alguns provedores avisam "sem créditos" com status 400 e uma mensagem no corpo: isso é `QuotaExhausted`, não `BadRequest`;
- leia o cabeçalho `retry-after` quando existir e passe em `RateLimited { retry_after }`;
- erro de conexão e timeout são `Unavailable`.

Uma classificação errada faz o roteador trocar quando não devia, ou insistir quando devia trocar.

### 4. Teste sem chamar a API

No CI não há chaves. Teste a conversão de mensagens e a classificação de erros com respostas de exemplo, como nos módulos existentes, e use o servidor HTTP falso de [`tests/fake_server.rs`](../crates/dotcrow-providers/tests/fake_server.rs) para conferir cabeçalhos e corpo da requisição. Se quiser um teste contra a API real, marque com `#[ignore]` e documente como rodar localmente.

### 5. Documente

Acrescente o provedor na tabela abaixo e no CHANGELOG.

## Provedores

| Provedor | id | Modelo padrão | Onde pegar a chave |
|---|---|---|---|
| Anthropic (Claude) | `anthropic` | `claude-haiku-5-5` | [console.anthropic.com](https://console.anthropic.com/settings/keys) |
| OpenAI (GPT) | `openai` | `gpt-5-nano` | [platform.openai.com](https://platform.openai.com/api-keys) |
| Google (Gemini) | `gemini` | `gemini-3.8-flash` | [aistudio.google.com](https://aistudio.google.com/apikey) |
| OpenRouter | `openrouter` | `openrouter/auto` | [openrouter.ai](https://openrouter.ai/keys) |
| DeepSeek | `deepseek` | `deepseek-flash` | [platform.deepseek.com](https://platform.deepseek.com/api_keys) |
| Groq | `groq` | `openai/gpt-oss-120b` | [console.groq.com](https://console.groq.com/keys) |
| Ollama (no seu computador) | `ollama` | `llama3.2` | sem chave; [instale o Ollama](https://ollama.com/download) e baixe o modelo com `ollama pull` |
| Simulado (testes) | qualquer | | `mock.rs` |

Nomes populares viram o id certo: `claude` → `anthropic`, `gpt` ou `chatgpt` → `openai`, `google` → `gemini`.

Os modelos mudam rápido. Se um provedor responder "modelo não encontrado", troque com `dotcrow provedores modelo <id> <modelo>` e abra uma issue para atualizarmos o padrão.

Detalhes de cada adaptador:

- **OpenAI:** usa `max_completion_tokens` e `reasoning_effort: "low"`, exigidos pelos modelos de raciocínio. Cota esgotada chega como 429 com o código `insufficient_quota`, e o DotCrow trata como `QuotaExhausted` (troca e deixa de castigo por 1 hora).
- **Anthropic:** "credit balance is too low" chega como 400 e vira `QuotaExhausted`; 529 (sobrecarregado) vira `Unavailable`.
- **Gemini:** a chave vai no cabeçalho `x-goog-api-key`, nunca na URL. Chave inválida chega como 400 e vira `Unauthorized`. O 429 traz `retryDelay`, que vira o tempo de castigo; limite diário vira `QuotaExhausted`.
- **OpenRouter, DeepSeek, Groq, Ollama:** API compatível com a da OpenAI. Sem saldo chega como 402.

## Faixas de modelo (Haiku → Opus)

Cada etapa de uma tarefa pede um porte de modelo:

| Faixa | Para quê | Anthropic |
|---|---|---|
| **Deep** | Planejar (Huginn) e revisar riscos (Vigia) | Opus |
| **Standard** | Executar no computador (Bico, Gralha, Pena) e conferir | Sonnet |
| **Fast** | Resumo final da Pena e teste de chave | Haiku |

Se você **fixar um modelo** para um provedor na aba Provedores, ele passa a valer para todas as faixas daquele provedor. Provedores sem faixas definidas usam o modelo padrão (ou o que você fixou) em todas as etapas.
