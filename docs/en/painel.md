# Panel and API

The panel is the visual way to use DotCrow: store and test API keys, hand tasks to the crows and follow every stage live. It runs on your computer and opens in a browser, including your phone's browser.

The easiest way is to **click the DotCrow icon** (the applications menu on Linux, the Start Menu or Desktop on Windows). The panel opens in the browser with the access code already included, without needing a terminal. Clicking again while the panel is open simply brings the page back. To close it, use **Fechar painel** (Close panel) at the bottom of the sidebar.

From the terminal, the equivalent is:

```bash
dotcrow          # with no command, opens the panel
dotcrow painel   # the same, with the options below
```

The terminal shows the address and **access code**. `Ctrl+C` also closes the panel.

## On your phone

Your phone must be on the same Wi-Fi as the computer.

```bash
dotcrow painel --rede
```

The terminal shows a QR code. Point your phone's camera at it: the address already includes the access code, so there is nothing to type. In your phone's browser, use "Add to Home Screen" to open the panel like an app.

> [!WARNING]
> Panel traffic on the local network is not encrypted yet (HTTPS for the panel is on the ROADMAP). Use `--rede` only on trusted Wi-Fi, such as at home or in the office.

Other options:

| Option | What it does |
|---|---|
| `--porta 7778` | Uses another port (the default is 7777) |
| `--sem-navegador` | Does not open the browser automatically |
| `--novo-codigo` | Generates a new access code; the old one stops working on all devices |

## What the panel does today

- **Tasks:** you write a request; Huginn ("thought", one of Odin's ravens) plans, Vigia (Portuguese for "lookout") reviews the risks, the executor delivers the result, Vigia checks it and Pena ("feather" or "quill") summarizes it. Each stage appears live with the provider and model that responded and any automatic provider failover.
- **Quick question:** a direct question without going through the crows' workflow. Useful for testing keys.
- **API keys:** store, test, delete and replace. No screen displays a stored key.
- **Providers:** order of preference, each provider's model and the Ollama address.
- **Activity:** the last 50 tasks, submitted from any device. A task sent from the computer appears on the phone, and vice versa.

## The crows act on the computer

The executor (Bico, Portuguese for "beak"; Gralha, "jay"; or Pena) works in cycles: requests **one action**, sees the result and decides the next one until finished. Every action appears live in the panel with its output.

Tool identifiers retain their original spelling:

| Tool | What it does |
|---|---|
| `terminal` | Runs commands (bash on Linux and macOS, PowerShell on Windows): programming, compiling, testing, git, curl |
| `ler_arquivo`, `escrever_arquivo`, `listar` | Reads, creates and edits files; writes code |
| `web` | Accesses the internet directly without a browser and returns page text with links |
| `navegador` | Opens a page in an invisible Chrome instance that runs JavaScript; can save a screenshot |
| `baixar`, `abrir` | Downloads files; opens files, folders and websites in your programs |
| `janelas`, `focar`, `mouse`, `teclado`, `tela` | Uses the screen like a person: lists and focuses windows, moves and clicks, types and presses shortcuts (Linux, with `xdotool` and `wmctrl`) |

### Memory (what DotCrow remembers about you)

Muninn ("memory", Odin's other raven) retains the essentials of what was done and what you teach it across conversations. Before planning, it retrieves and uses what it already knows; at the end of each task, it saves a summary. Everything stays **encrypted on this computer** (only this computer can open it), and **anything resembling a password, key or secret is removed** before storage and before being sent to the AI. In the **Memória** (Memory) tab, you can view, manually teach and forget whatever you want. API: `GET/POST /api/memoria` and `DELETE /api/memoria/{id}`.

### Conversation folders

Each conversation has its own folder under `~/DotCrow/conversas/` (for example, `2026-10-21_001_crie-um-site`). This is where the terminal starts and where everything the crows create goes. The `conversa.md` file stores each request and delivery. In the panel, **Abrir pasta** (Open folder) shows the folder in the file manager, and clicking a file opens it. To continue the same conversation (same folder, with memory of what has already been done), just send another request; **Nova conversa** (New conversation) starts a new one.

### Modes

| Mode | What the crows can do |
|---|---|
| **Computador** (Computer, default) | Use the whole computer. Ask for confirmation before sensitive actions (see below). |
| **Livre** (Free) | Everything, without asking. Only actions that could destroy the system (such as `rm -rf /` or formatting a disk) still ask. |
| **Sandbox** | Work in an isolated environment (bubblewrap): the system is read-only, only the conversation folder can change, and the vault and SSH keys are hidden. No mouse, keyboard or windows. |

### Team (several crows at once)

Enable **Equipe** (Team) in the form. Huginn splits the request into independent subtasks, and several crows work in parallel (up to 7 at once). The panel shows a **board**: one column per crow, with each one's actions and thoughts live. Pena brings everything together in a summary. If the parts depend on one another, DotCrow uses a single crow. In the API, send `"equipe": true` in `POST /api/tarefas`; events arrive as `team_planned` and `lane_event`.

### What asks for confirmation (Computer mode)

Installing or removing programs, `sudo`, starting and stopping services; deleting outside the conversation folder; publishing or sending externally (`git push`, `scp`, `ssh`, forms and POST); accessing passwords, keys and certificates; writing outside the conversation folder; taking control of the mouse and keyboard. The request appears in the panel with **Permitir** (Allow), **Permitir sempre nesta conversa** (Always allow in this conversation) and **Negar** (Deny). No response within 30 minutes counts as a denial. DotCrow's key vault is never read, even with confirmation.

Tasks remain in memory while the panel is open; files remain in their folders.

### Dedicated window

The icon opens the panel in its own DotCrow window (Chrome, Edge or Brave in application mode, without tabs or an address bar). If none of these browsers is available, it opens in the default browser.

## Panel security

| Protection | How it works |
|---|---|
| This computer only by default | The panel listens on `127.0.0.1`. With `--rede`, it accepts devices on the local network. |
| Access code | 16 symbols (80 bits), stored in `painel.token` in the data directory with 0600 permissions. Every `/api/` call requires the code. |
| Protection against malicious sites | The panel checks the `Host` header (blocking *DNS rebinding*) and does not enable CORS, so a page open in the browser cannot talk to it in the background. |
| No key leakage | No route returns a key. Keys enter through the panel and go straight into the encrypted vault. |
| AI content is text | The page always displays model responses as plain text, never HTML. The content security policy (CSP) blocks external scripts and connections. |
| Code removed from the address bar | After opening via QR code, the page stores the code and removes it from the URL. |

## API

The same API used by the page is available for your apps and, in the future, the WhatsApp bridge. All routes under `/api/` use JSON and require:

```text
Authorization: Bearer <access code>
```

The code may include or omit hyphens. Errors always use `{"error": "message"}` with the appropriate HTTP status (400 invalid request, 401 wrong code, 403 address not allowed, 404 not found, 502 all providers failed).

Routes, JSON field names and enum values retain their original spelling. Examples of server error messages remain in Portuguese.

### State

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
    { "id": "8365fa75", "provider": "anthropic", "label": "personal account",
      "fingerprint": "cae4-728a-25d6-b4f2", "created_at": 1792548562 }
  ],
  "order": ["anthropic"],
  "warnings": [],
  "lan": false
}
```

### Keys

| Route | Body | Response |
|---|---|---|
| `POST /api/chaves` | `{"provider": "anthropic", "key": "sk-...", "label": "optional"}` | Key metadata (without the key) |
| `DELETE /api/chaves/{id}` | | Deleted key's metadata |

Popular names map to the catalog id: `claude` → `anthropic`, `gpt` → `openai`, `google` → `gemini`.

### Providers

`PUT /api/provedores`

```json
{ "order": ["anthropic", "gemini"], "models": { "anthropic": "claude-haiku-5-5", "gemini": null }, "urls": { "ollama": "http://192.168.0.10:11434/v1" } }
```

All fields are optional. `null` or an empty string restores the default. Returns the updated state.

`POST /api/testar` with `{}` tests all providers that have keys; with `{"provider": "gemini"}`, just one. Returns a list:

```json
[{ "provider": "gemini", "ok": false, "model": "gemini-3.8-flash", "ms": 412,
   "error": "chave de API recusada", "hint": "A chave foi recusada. Confira se copiou inteira..." }]
```

The example error means "API key rejected"; the hint says "The key was rejected. Check that you copied it in full...".

### Quick question

`POST /api/perguntar` with `{"text": "...", "provider": null}` returns:

```json
{ "text": "...", "provider": "anthropic", "model": "claude-sonnet-5-5", "failovers": [] }
```

### Tasks

`POST /api/tarefas`

```json
{ "text": "write an e-mail telling a customer about the delay", "kind": null, "provider": null }
```

`kind` can be `escrever` (write), `organizar` (organize), `operar` (operate), or `null` to let Huginn decide. Returns `201` with `{"id": "27b5d91b01"}`, and the task starts immediately.

| Route | What it returns |
|---|---|
| `GET /api/tarefas` | Recent tasks, newest first: `id`, `request`, `created_at`, `status` |
| `GET /api/tarefas/{id}` | The task with all events so far |
| `GET /api/tarefas/{id}/eventos` | Events as *Server-Sent Events*: existing events first, then new ones live until completion |

`status` is `running`, `finished`, `blocked` (Vigia blocked it) or `failed`.

Each event is a `data: {json}` line. The `type` field says what happened:

| `type` | Fields | When |
|---|---|---|
| `started` | `request` | The task started |
| `kind_chosen` | `kind` (`write`, `organize`, `operate`) | The type was chosen |
| `stage_started` | `step`, `agent`, `name`, `what` | A crow started |
| `stage_finished` | `step`, `agent`, `name`, `text`, `provider`, `model`, `failovers` | A crow finished |
| `stage_skipped` | `step`, `agent`, `name`, `reason` | A stage was skipped |
| `blocked` | `reason` | Vigia blocked the plan |
| `finished` | `summary` | Completed, with Pena's summary |
| `failed` | `error` | The task stopped because of an error |

Example with `curl`:

```bash
ACCESS_CODE=abcd-efgh-jkmn-pqrs
TASK_ID=$(curl -s -X POST -H "Authorization: Bearer $ACCESS_CODE" -H 'Content-Type: application/json' \
  -d '{"text":"write an Instagram caption about crows"}' \
  http://127.0.0.1:7777/api/tarefas | jq -r .id)
curl -N -H "Authorization: Bearer $ACCESS_CODE" http://127.0.0.1:7777/api/tarefas/$TASK_ID/eventos
```

In the browser, `EventSource` does not send headers; read the stream with `fetch` and `response.body.getReader()`, as the panel page does.

### Actions, confirmations and files

`POST /api/tarefas` also accepts `"modo": "computador" | "livre" | "sandbox"` and `"conversa": "<id>"` (continues the conversation; without it, starts a new one). The response includes `{"id", "conversa"}`.

New task events: `action_started` (`n`, `tool`, `title`, `why`), `action_finished` (`n`, `ok`, `output`), `approval_needed` (`id`, `n`, `title`, `reason`) and `approval_resolved` (`id`, `answer`).

| Route | Body | What it does |
|---|---|---|
| `POST /api/aprovacoes/{id}` | `{"resposta": "sim" \| "sempre" \| "nao"}` | Answers a confirmation request: yes, always or no |
| `GET /api/conversas/{id}/arquivos` | | The conversation folder, mode and file list |
| `POST /api/abrir` | `{"conversa": "<id>", "caminho": "optional"}` | Opens the folder (or a file inside it) on the computer |

### Closing the panel

`POST /api/desligar` with `{}` closes the panel (the "Fechar painel" button).

### Phone

`GET /api/celular` returns `{"lan": true, "url": "...", "qr_svg": "<svg ...>"}` when the panel was opened with `--rede`.

## Next steps

- **WhatsApp:** a bridge that receives messages, creates tasks through `POST /api/tarefas` and returns the summary when the `finished` event arrives.
- **Live screen view:** when the crows gain the ability to operate the computer (phase 2), the panel will show the screen in real time on both computer and phone, with confirmation for every serious action.
- **HTTPS for the panel** in `--rede` mode.
