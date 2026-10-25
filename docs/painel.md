# Painel e API

O painel é o jeito visual de usar o DotCrow: guardar e testar as chaves de API, entregar tarefas aos corvos e acompanhar cada etapa ao vivo. Ele roda no seu computador e abre no navegador, inclusive no do celular.

O jeito mais fácil é **clicar no ícone do DotCrow** (menu de aplicativos no Linux, Menu Iniciar ou Área de Trabalho no Windows). O painel abre no navegador, já com o código de acesso, sem precisar do terminal. Clicar de novo com o painel aberto só traz a página de volta. Para fechar, use o botão **Fechar painel**, no fim da barra lateral.

Pelo terminal, é o mesmo que:

```bash
dotcrow          # sem comando, abre o painel
dotcrow painel   # o mesmo, com as opções abaixo
```

O terminal mostra o endereço e o **código de acesso**. `Ctrl+C` também fecha o painel.

## No celular

O celular precisa estar no mesmo Wi-Fi do computador.

```bash
dotcrow painel --rede
```

O terminal mostra um QR code. Aponte a câmera do celular: o endereço já leva o código de acesso, então não precisa digitar nada. No navegador do celular, use "Adicionar à tela de início" para abrir o painel como um app.

> [!WARNING]
> Na rede local o tráfego do painel ainda não é cifrado (HTTPS no painel está no ROADMAP). Use `--rede` só em Wi-Fi de confiança, como o de casa ou do escritório.

Outras opções:

| Opção | O que faz |
|---|---|
| `--porta 7778` | Usa outra porta (o padrão é 7777) |
| `--sem-navegador` | Não abre o navegador sozinho |
| `--novo-codigo` | Gera um código de acesso novo; o antigo para de funcionar em todos os aparelhos |

## O que o painel faz hoje

- **Tarefas:** você escreve o pedido; o Huginn planeja, o Vigia revisa os riscos, quem executa entrega o resultado, o Vigia confere e a Pena resume. Cada etapa aparece ao vivo, com o provedor e o modelo que responderam e qualquer troca automática de provedor.
- **Pergunta rápida:** uma pergunta direta, sem passar pelo fluxo dos corvos. Bom para testar as chaves.
- **Chaves de API:** guardar, testar, apagar e trocar. Nenhuma tela mostra uma chave guardada.
- **Provedores:** ordem de preferência, modelo de cada um e endereço do Ollama.
- **Atividade:** as últimas 50 tarefas, enviadas de qualquer aparelho. Uma tarefa enviada pelo computador aparece no celular, e vice-versa.

## Os corvos agem no computador

Quem executa (Bico, Gralha ou Pena) trabalha em ciclos: pede **uma ação**, vê o resultado e decide a próxima, até concluir. Cada ação aparece ao vivo no painel, com a saída.

| Ferramenta | O que faz |
|---|---|
| `terminal` | Roda comandos (bash no Linux e no macOS, PowerShell no Windows): programar, compilar, testar, git, curl |
| `ler_arquivo`, `escrever_arquivo`, `listar` | Lê, cria e altera arquivos; escreve código |
| `web` | Acessa a internet direto, sem navegador, e devolve o texto da página com os links |
| `navegador` | Abre a página num Chrome invisível que roda JavaScript; pode salvar uma captura |
| `baixar`, `abrir` | Baixa arquivos; abre arquivos, pastas e sites nos seus programas |
| `janelas`, `focar`, `mouse`, `teclado`, `tela` | Usa a tela como uma pessoa: lista e foca janelas, move e clica, digita e aperta atalhos (Linux, com `xdotool` e `wmctrl`) |

### Memória (o que o DotCrow lembra de você)

O Muninn guarda, entre conversas, o essencial do que foi feito e do que você ensina. Antes de planejar, ele busca o que já sabe e usa; no fim de cada tarefa, guarda um resumo. Tudo fica **cifrado neste computador** (só ele abre) e **o que parece senha, chave ou segredo é apagado** antes de guardar e antes de ir para a IA. Na aba **Memória** você vê, ensina à mão e esquece o que quiser. API: `GET/POST /api/memoria` e `DELETE /api/memoria/{id}`.

### Treinar, exportar e importar RAG

Na aba **Treino** você grava uma pessoa fazendo uma tarefa repetitiva (passos, capturas de tela e mouse) e transforma a sessão em memória. A gravação **só começa com o consentimento** de quem é gravado (nome + "autorizo"); ela vê a gravação e pode parar quando quiser. Na aba **Memória**, **Exportar RAG** salva a memória como um arquivo `.dotrag` (com ficha: nome, preço e chave Pix do vendedor) e **Importar RAG** traz o de outra pessoa. O marketplace (cobrança/repasse) fica para depois e exige um backend. API: `POST /api/memoria/exportar` e `/importar`, `GET /api/treino`, `POST /api/treino/iniciar|passo|tela|parar`.

### Pastas das conversas

Cada conversa tem a sua pasta em `~/DotCrow/conversas/` (por exemplo, `2026-10-21_001_crie-um-site`). É ali que o terminal começa e onde vai tudo o que os corvos criam. O arquivo `conversa.md` guarda cada pedido e cada entrega. No painel, **Abrir pasta** mostra a pasta no gerenciador de arquivos, e clicar num arquivo abre ele. Para continuar a mesma conversa (mesma pasta, com a memória do que já foi feito), é só mandar outro pedido; **Nova conversa** começa outra.

### Modos

| Modo | O que os corvos podem |
|---|---|
| **Computador** (padrão) | Usar o computador inteiro. Pedem confirmação antes do que é sensível (veja abaixo). |
| **Livre** | Tudo, sem perguntar. Só o que pode destruir o sistema (como `rm -rf /` ou formatar disco) ainda pergunta. |
| **Sandbox** | Trabalhar numa caixa isolada (bubblewrap): o sistema fica só para leitura, só a pasta da conversa pode mudar, o cofre e as chaves SSH ficam escondidos. Sem mouse, teclado nem janelas. |

### Equipe (vários corvos ao mesmo tempo)

Ligue **Equipe** no formulário. O Huginn divide o pedido em subtarefas independentes e vários corvos trabalham em paralelo (até 7 de uma vez). O painel mostra um **mural**: uma coluna por corvo, com as ações e o pensamento de cada um ao vivo. A Pena junta tudo num resumo. Se as partes dependem uma da outra, o DotCrow usa um corvo só. Na API, mande `"equipe": true` em `POST /api/tarefas`; os eventos vêm como `team_planned` e `lane_event`.

### O que pede confirmação (modo Computador)

Instalar ou remover programas, `sudo`, ligar e desligar serviços; apagar fora da pasta da conversa; publicar ou enviar para fora (`git push`, `scp`, `ssh`, formulários e POST); tocar em senhas, chaves e certificados; gravar fora da pasta da conversa; assumir o mouse e o teclado. O pedido aparece no painel com **Permitir**, **Permitir sempre nesta conversa** e **Negar**. Sem resposta em 30 minutos, vale como negado. O cofre de chaves do DotCrow nunca é lido, nem com confirmação.

As tarefas ficam na memória enquanto o painel está aberto; os arquivos ficam nas pastas.

### Janela própria

O ícone abre o painel numa janela própria do DotCrow (Chrome, Edge ou Brave em modo aplicativo, sem abas nem barra de endereço). Sem esses navegadores, abre no navegador padrão.

## Segurança do painel

| Proteção | Como funciona |
|---|---|
| Só este computador por padrão | O painel escuta em `127.0.0.1`. Com `--rede`, aceita aparelhos da rede local. |
| Código de acesso | 16 símbolos (80 bits), guardado em `painel.token` na pasta de dados, com permissão 0600. Toda chamada a `/api/` exige o código. |
| Contra sites maliciosos | O painel confere o cabeçalho `Host` (barra *DNS rebinding*) e não libera CORS, então uma página aberta no navegador não consegue falar com ele por trás. |
| Sem vazar chaves | Nenhuma rota devolve uma chave. As chaves entram pelo painel e vão direto para o cofre cifrado. |
| Conteúdo da IA é texto | A página mostra as respostas dos modelos sempre como texto puro, nunca como HTML. A política de segurança de conteúdo (CSP) bloqueia scripts e conexões de fora. |
| Código fora da barra de endereço | Depois de abrir pelo QR code, a página guarda o código e o tira da URL. |

## API

A mesma API que a página usa serve para os seus apps e, no futuro, para a ponte com o WhatsApp. Todas as rotas abaixo de `/api/` falam JSON e exigem:

```
Authorization: Bearer <código de acesso>
```

O código pode ir com ou sem hífens. Erros sempre vêm como `{"error": "mensagem"}` com o status HTTP adequado (400 pedido inválido, 401 código errado, 403 endereço não permitido, 404 não encontrado, 502 todos os provedores falharam).

### Estado

`GET /api/estado`

```json
{
  "version": "0.1.0",
  "providers": [
    { "id": "anthropic", "name": "Anthropic (Claude)", "model": "claude-sonnet-5-5", "default_model": true,
      "catalog_model": "claude-sonnet-5-5", "custom_url": null, "needs_key": true, "has_key": true,
      "position": 1, "key_url": "https://console.anthropic.com/settings/keys" }
  ],
  "keys": [
    { "id": "8365fa75", "provider": "anthropic", "label": "conta pessoal",
      "fingerprint": "cae4-728a-25d6-b4f2", "created_at": 1792548562 }
  ],
  "order": ["anthropic"],
  "warnings": [],
  "lan": false
}
```

### Chaves

| Rota | Corpo | Resposta |
|---|---|---|
| `POST /api/chaves` | `{"provider": "anthropic", "key": "sk-...", "label": "opcional"}` | Metadados da chave (sem a chave) |
| `DELETE /api/chaves/{id}` | | Metadados da chave apagada |

Nomes populares viram o id do catálogo: `claude` → `anthropic`, `gpt` → `openai`, `google` → `gemini`.

### Provedores

`PUT /api/provedores`

```json
{ "order": ["anthropic", "gemini"], "models": { "anthropic": "claude-haiku-5-5", "gemini": null }, "urls": { "ollama": "http://192.168.0.10:11434/v1" } }
```

Todos os campos são opcionais. `null` ou texto vazio volta ao padrão. Responde com o estado atualizado.

`POST /api/testar` com `{}` testa todos os provedores com chave; com `{"provider": "gemini"}`, só um. Responde uma lista:

```json
[{ "provider": "gemini", "ok": false, "model": "gemini-3.8-flash", "ms": 412,
   "error": "chave de API recusada", "hint": "A chave foi recusada. Confira se copiou inteira..." }]
```

### Pergunta rápida

`POST /api/perguntar` com `{"text": "...", "provider": null}` responde:

```json
{ "text": "...", "provider": "anthropic", "model": "claude-sonnet-5-5", "failovers": [] }
```

### Tarefas

`POST /api/tarefas`

```json
{ "text": "escreva um e-mail para a cliente avisando o atraso", "kind": null, "provider": null }
```

`kind` pode ser `escrever`, `organizar`, `operar` (ou `null` para o Huginn decidir). Responde `201` com `{"id": "27b5d91b01"}` e a tarefa começa na hora.

| Rota | O que devolve |
|---|---|
| `GET /api/tarefas` | As últimas tarefas, da mais nova para a mais antiga: `id`, `request`, `created_at`, `status` |
| `GET /api/tarefas/{id}` | A tarefa com todos os eventos até agora |
| `GET /api/tarefas/{id}/eventos` | Os eventos como *Server-Sent Events*: primeiro os que já aconteceram, depois os novos, ao vivo, até o fim |

`status` é `running`, `finished`, `blocked` (o Vigia bloqueou) ou `failed`.

Cada evento é uma linha `data: {json}`. O campo `type` diz o que aconteceu:

| `type` | Campos | Quando |
|---|---|---|
| `started` | `request` | A tarefa começou |
| `kind_chosen` | `kind` (`write`, `organize`, `operate`) | O tipo foi definido |
| `stage_started` | `step`, `agent`, `name`, `what` | Um corvo começou |
| `stage_finished` | `step`, `agent`, `name`, `text`, `provider`, `model`, `failovers` | Um corvo terminou |
| `stage_skipped` | `step`, `agent`, `name`, `reason` | Uma etapa foi pulada |
| `blocked` | `reason` | O Vigia bloqueou o plano |
| `finished` | `summary` | Fim, com o resumo da Pena |
| `failed` | `error` | A tarefa parou por erro |

Exemplo com `curl`:

```bash
CODIGO=abcd-efgh-jkmn-pqrs
ID=$(curl -s -X POST -H "Authorization: Bearer $CODIGO" -H 'Content-Type: application/json' \
  -d '{"text":"escreva uma legenda para o Instagram sobre corvos"}' \
  http://127.0.0.1:7777/api/tarefas | jq -r .id)
curl -N -H "Authorization: Bearer $CODIGO" http://127.0.0.1:7777/api/tarefas/$ID/eventos
```

No navegador, `EventSource` não manda cabeçalhos; leia o fluxo com `fetch` e `response.body.getReader()`, como a página do painel faz.

### Ações, confirmações e arquivos

`POST /api/tarefas` aceita também `"modo": "computador" | "livre" | "sandbox"` e `"conversa": "<id>"` (continua a conversa; sem isso, abre uma nova). A resposta traz `{"id", "conversa"}`.

Eventos novos da tarefa: `action_started` (`n`, `tool`, `title`, `why`), `action_finished` (`n`, `ok`, `output`), `approval_needed` (`id`, `n`, `title`, `reason`) e `approval_resolved` (`id`, `answer`).

| Rota | Corpo | O que faz |
|---|---|---|
| `POST /api/aprovacoes/{id}` | `{"resposta": "sim" \| "sempre" \| "nao"}` | Responde a um pedido de confirmação |
| `GET /api/conversas/{id}/arquivos` | | A pasta da conversa, o modo e a lista de arquivos |
| `POST /api/abrir` | `{"conversa": "<id>", "caminho": "opcional"}` | Abre a pasta (ou um arquivo dela) no computador |

### Fechar o painel

`POST /api/desligar` com `{}` fecha o painel (é o botão "Fechar painel").

### Celular

`GET /api/celular` responde `{"lan": true, "url": "...", "qr_svg": "<svg ...>"}` quando o painel foi aberto com `--rede`.

### Civilização

A página `/civ` (botão **Civilização ↗** no painel) tem três janelas: **Terrenos** (`/civ#terrenos`), **Mentes** (`/civ#mentes`) e **Conversa** (`/civ#conversa`). Ela só lê; quem muda a civilização é a conversa no terminal. Usa o mesmo código de acesso guardado no navegador.

`GET /api/civ` responde, entre outros: `corvos` (cada um com `nome`, `personalidade`, `especialidade`, `modelo`, `reputacao`, `energia`, `dotcoins`, `comida`, `sementes`, `dormindo`, `local`, `fazendo`, `artefatos`, `sprite` — o desenho em SVG — e `mente`, com `pontos` `{x, y, c, t}` no círculo e `ligacoes` `[i, j, força]`), `terrenos` (`id`, `x`, `y`, `dono`, `plantio`), `ofertas`, `contratos`, `mercado`, `podio`, `diario` e `conversa` (as últimas 80 falas com `n`, `quem`, `texto`, `responde`, `forma` e `cor` da marca).

## Próximos passos

- **WhatsApp:** uma ponte que recebe mensagens, cria tarefas por `POST /api/tarefas` e devolve o resumo quando chega o evento `finished`.
- **Ver a tela ao vivo:** quando os corvos ganharem mãos no computador (fase 2), o painel vai mostrar a tela em tempo real, no computador e no celular, com confirmação de cada ação séria.
- **HTTPS no painel** para o modo `--rede`.
