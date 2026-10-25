<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/dotcrow-logo-dark.svg">
    <img alt="DotCrow" src="assets/brand/dotcrow-logo-light.svg" width="360">
  </picture>
</p>

<p align="center">
  <strong>Um bando de agentes de IA, open source, que trabalham no seu computador.</strong><br>
  Você delega a tarefa inteira. Os corvos planejam, executam, conferem e aprendem.
</p>

<p align="center">
  <a href="https://github.com/napoleaomp4/dotcrow/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/napoleaomp4/dotcrow/actions/workflows/ci.yml/badge.svg"></a>
  <a href="LICENSE"><img alt="Licença Apache-2.0" src="https://img.shields.io/badge/licen%C3%A7a-Apache--2.0-0D1117"></a>
  <img alt="Status: pré-alfa" src="https://img.shields.io/badge/status-pr%C3%A9--alfa-8B949E">
  <img alt="Rust" src="https://img.shields.io/badge/feito%20em-Rust-0D1117?logo=rust">
  <img alt="Pós-quântico" src="https://img.shields.io/badge/cripto-p%C3%B3s--qu%C3%A2ntica-1DB954">
  <img alt="Brasil" src="https://img.shields.io/badge/feito%20no-Maranh%C3%A3o%20%F0%9F%87%A7%F0%9F%87%B7-0D1117">
</p>

<p align="center">
  <a href="#-comece-em-2-minutos">Começar</a> ·
  <a href="#-o-que-ele-faz">O que faz</a> ·
  <a href="#-os-corvos">Os corvos</a> ·
  <a href="#-a-civiliza%C3%A7%C3%A3o-dos-corvos-v2">Civilização</a> ·
  <a href="#-quero-ajudar">Contribuir</a> ·
  <a href="docs/arquitetura.md">Arquitetura</a> ·
  <a href="README.en.md">English</a>
</p>

<br>

> [!WARNING]
> **Pré-alfa, e construído em público.** Os corvos já pensam e agem com LLMs de verdade, mas tem muita coisa pela frente e a criptografia ainda não passou por auditoria externa. Não use com dados ou chaves de produção. **Esse é o melhor momento para entrar:** o projeto é pequeno o bastante para você deixar sua marca nele. 🪶

---

## 🐦‍⬛ O que é

Em vez de pedir uma resposta a um chat, você entrega ao **DotCrow** uma tarefa inteira —

> *"organize minha pasta de downloads por cliente e me manda um resumo"*

— e um bando de agentes resolve: um planeja, um lembra como já foi feito antes, um confere os riscos, um executa e um escreve o relatório. Você é o Odin; os corvos trabalham para você.

## ✨ O que ele faz

- 🧠 **Seis corvos, cada um pensa de um jeito.** Eles se revisam entre si antes de qualquer ação, e você vê **cada um pensando ao vivo**, palavra por palavra.
- 🔌 **Qualquer LLM com API.** Claude, GPT, Gemini, OpenRouter, DeepSeek, Groq e modelos locais no Ollama. Acabou a cota de um? O DotCrow **troca para o próximo sozinho**, sem parar a tarefa.
- 💬 **Converse no meio do caminho.** Mudou de ideia enquanto os corvos trabalham? Mande uma mensagem e eles ajustam o rumo na próxima ação.
- ↩️ **Nada de entregar errado.** Se o Vigia não aprovar o resultado, a tarefa **volta ao executor** para refazer — você vê o bando pulando e voltando aos corvos.
- 📱 **No celular, de qualquer lugar.** Um túnel cifrado de ponta a ponta liga o celular ao seu computador pela internet, sem estar no mesmo Wi-Fi. Você acompanha tudo ao vivo pelo app.
- 🎓 **Ensina pelo exemplo.** Grave (com consentimento) alguém fazendo uma tarefa repetitiva — passos, tela, mouse e os sites/programas usados — e isso vira uma *receita* na memória.
- 🔑 **Chaves de API invisíveis.** Só o dono adiciona chaves, e nem ele as vê depois. Não existe comando para exibir uma chave.
- 🔒 **Criptografia pós-quântica.** Esquema híbrido ML-KEM-768 + X25519: para ler, um atacante precisa quebrar os dois.
- 🛑 **Nada irreversível sem você.** Apagar, enviar, pagar, instalar: tudo que não dá para desfazer pede sua confirmação.
- ⌨️ **Modo terminal, dentro do seu projeto.** Rode `dotcrow` na pasta do projeto e os corvos trabalham nos arquivos dali, perguntam o que precisam para acertar e deixam tudo pronto para `/desfazer`. Funciona até no celular, pelo Termux.

## 🚀 Comece em 2 minutos

### Baixe pronto (sem compilar)

Pegue na página de [**Releases**](https://github.com/napoleaomp4/dotcrow/releases):

- 🪟 **Windows:** `dotcrow-setup.exe` (instalador de clicar) ou o `dotcrow.exe` portátil.
- 🐧 **Linux (Debian/Ubuntu/Mint):** `sudo apt install ./dotcrow_*_amd64.deb`

### Ou instale do código com um comando (Linux/macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/napoleaomp4/dotcrow/main/instalar.sh | bash
```

Isso instala o Rust, compila e cria o ícone do DotCrow no menu. Clique no ícone (ou rode `dotcrow painel`) e o painel abre no navegador. Daí é só:

1. **Chaves** → cole a chave de API de um provedor (Claude, GPT, Gemini...) e clique em **Testar**.
2. **Tarefas** → peça algo aos corvos e acompanhe cada etapa ao vivo.
3. 📱 Quer no celular? `dotcrow painel --rede` (mesmo Wi-Fi) ou `dotcrow painel --remoto` (de qualquer lugar) e leia o QR.

### ⌨️ Ou use direto no terminal, dentro do projeto

```bash
cd ~/meu-site
dotcrow
```

Num terminal, `dotcrow` abre o **modo terminal** na pasta atual (o ícone continua abrindo o painel):

```
site › troque o título do @notas.php para algo mais amigável
🧭 O Huginn quer alinhar antes de começar:
  1. Mudo só o título ou também o texto da página?
     1) Só o título   2) Os dois
  › 1
▸ 4. Bico · executa no computador…
   ⚙ 1. ler_arquivo: notas.php
   ⚙ 2. editar_arquivo: notas.php
✎ 1 arquivo(s) mexido(s): notas.php · /desfazer volta tudo
```

- **Pastas do projeto:** `/pasta + ~/assets` põe outra pasta (só leitura ou leitura e escrita); `/pastas` mostra todas.
- **Ver arquivos fácil:** `/arquivos` navega pelas pastas do aparelho com números, `/ver notas.php` mostra o arquivo, `/buscar titulo` acha por nome ou conteúdo, `@arquivo` anexa ao pedido e o Tab completa os caminhos.
- **Sempre alinhado:** na primeira vez numa pasta o Huginn faz perguntas sobre o projeto e guarda um perfil; antes de cada tarefa, pergunta o que mudaria o resultado; no meio do trabalho os corvos também podem perguntar.
- **Entregas na Downloads:** relatórios, PDFs, imagens e tudo que não é do projeto vão para a sua pasta Downloads. Fora de um projeto, o DotCrow trabalha na própria Downloads.
- **Sem medo:** os corvos editam só o trecho necessário, apagar arquivo pede confirmação e `/desfazer` volta a última tarefa.
- **No celular:** no Termux, `curl -fsSL https://raw.githubusercontent.com/napoleaomp4/dotcrow/main/instalar-termux.sh | bash`.

Tudo sobre o modo terminal em [docs/terminal.md](docs/terminal.md).

<details>
<summary>Comandos avulsos no terminal 👇</summary>

```bash
dotcrow chaves adicionar anthropic --rotulo "conta pessoal"   # guarda uma chave (sem aparecer na tela)
dotcrow testar                                                 # testa as chaves
dotcrow tarefa "escreva um e-mail avisando que a entrega atrasou dois dias"
dotcrow perguntar "o que é um corvo, em uma frase?"            # resposta direta
dotcrow provedores                                             # ordem e modelos
dotcrow agentes                                                # conheça os corvos
dotcrow civ conversar                                          # os 10 corvos da civilização conversam (v2)
dotcrow demo troca                                             # veja a troca automática de provedor
```

Para compilar na mão você precisa do [Rust](https://rustup.rs) 1.88+ e de um compilador C:

```bash
cargo build --release && ./target/release/dotcrow painel
```
</details>

## 🪶 Os corvos

Corvos estão entre os animais mais inteligentes que existem: usam ferramentas, resolvem problemas em etapas e **ensinam uns aos outros**. Na mitologia nórdica, Odin tinha dois corvos — **Huginn** (pensamento) e **Muninn** (memória) — que voavam pelo mundo e voltavam contando o que viram.

| Corvo | Papel | Como pensa |
|---|---|---|
| 🧭 **Huginn** | Estrategista | Entende o objetivo e o que significa "pronto" antes de qualquer passo |
| 🧠 **Muninn** | Memória | "Já vi algo parecido, funcionou assim" |
| 🗂️ **Gralha** | Organizadora | Faz o inventário antes de mover qualquer coisa |
| ⚙️ **Bico** | Executor | Faz, confere o resultado real, segue |
| ✍️ **Pena** | Escritora | Escreve pensando em quem vai ler |
| 🛡️ **Vigia** | Guardião | "O que pode dar errado aqui?" |

```mermaid
flowchart LR
    V0([Você]) --> H[Huginn<br/>planeja]
    H --> M[Muninn<br/>lembra]
    M --> G1{Vigia<br/>revisa}
    G1 -- risco alto --> V0
    G1 -- aprovado --> X[Bico / Gralha / Pena<br/>executam]
    X --> G2{Vigia<br/>confere}
    G2 -- ajustar --> X
    G2 -- ok --> P[Pena<br/>resume]
    P --> V1([Você])
    P --> M2[Muninn<br/>guarda a receita]
```

Mais em [docs/agentes.md](docs/agentes.md).

## 🧬 A Civilização dos Corvos (v2)

Além do bando de tarefas, o DotCrow está ganhando uma **civilização de 10 corvos** que nascem, competem, morrem e evoluem. Cada corvo tem um **genoma** (personalidade, especialidade, geração) e **um modelo de LLM próprio** — você escolhe qual, um por um, e pode trocar quando quiser.

```bash
dotcrow civ                         # mostra o bando (é sempre o mesmo: fica salvo)
dotcrow civ conversar               # os 10 corvos conversam de verdade; você entra na conversa
dotcrow civ conversar --tema música --intervalo 30   # com tema e uma fala a cada 30 s
dotcrow civ modelo Nyx anthropic:claude-haiku-5-5    # troca o modelo de um corvo
dotcrow civ --modelos openai:gpt-5-nano,anthropic:claude-haiku-5-5   # troca o rodízio de todos
dotcrow civ nova                    # nova Gênese: 10 corvos novos
```

Na conversa, cada corvo fala com **o próprio modelo**, do seu jeito (personalidade e especialidade), respondendo ao que os outros disseram. **Escreva e aperte Enter** para falar com o bando; chame um corvo pelo nome (`Nyx, o que você acha?`) para ele responder; Enter vazio pede a próxima fala na hora; **`pare`** (ou Ctrl+C) encerra. Dentro do modo terminal, é só digitar **`/civ`** (ou `/civ música` para dar um tema). Se o modelo de um corvo falhar ou recusar, ele fala com o modelo do outro provedor.

```
Sombra  ★ 6.8 [online]  · gpt-5-nano        · organização
Nyx     ★ 5.4 [online]  · claude-haiku-5-5  · memória
Zéfiro  ★ 6.8 [online]  · claude-haiku-5-5  · pesquisa
...
Conexões externas de hoje: Zéfiro · 3   Morrigana · 2   Sombra · 1
```

- 🌱 **Gênese:** 10 corvos, metade em `gpt-5-nano` e metade em `claude-haiku-5-5` por padrão (os modelos baratos). O **Opus 5.5 não entra no bando**: fica nos pontos de controle (Guardião, triagem e refino).
- 🎨 **Cada corvo é desenhado a partir do genoma:** a cor vem do provedor, o símbolo no peito vem da especialidade, as estrelas vêm da reputação.
- 🌐 **Internet só para os 3 melhores do dia:** 3 conexões para o 1º, 2 para o 2º, 1 para o 3º. Do 4º para baixo, ficam na Sandbox.
- 💀 **Ciclo de vida:** a cada ciclo os 5 piores morrem e vão para o **Livro dos Mortos** com a lição que deixaram; nascem 5 novos por cruzamento e mutação dos sobreviventes.
- 🧠 **Memória que cresce:** cada corvo nasce com **0,1 MB** de RAG e ganha **+1 MB por dia sobrevivido** (mais rápido quanto melhor a reputação). Essa memória é a **alma** do corvo — a ideia é poder exportá-la para outra simulação.

| Fase | O que entra | Estado |
|---|---|---|
| 1 | Motor da civilização: Gênese, genoma, desenho, ranking, ciclo de morte/nascimento, RAG por dia, modelo por corvo | ✅ no código |
| 1.5 | Conversa de verdade entre os 10 corvos (cada um com o seu modelo) e com você, bando salvo, modelo por corvo | ✅ no código |
| 2 | Muralha (anonimização antes de qualquer LLM) e Alfândega (Opus triando tudo que vem de fora) | 🟡 próxima |
| 3 | Conselho com voto (>60%), notas 10/5 do usuário, avaliador por amostragem a cada ~100 mensagens | 🟡 |
| 4 | Estado salvo entre sessões, alma exportável, Modo Dispositivo e painel estilo WhatsApp | 🟡 |

## 🧩 O que já funciona

| Parte | Estado |
|---|---|
| Painel no navegador e no celular, com os corvos pensando ao vivo | ✅ |
| Celular pela internet, com túnel cifrado de ponta a ponta ([docs](docs/remoto.md)) | ✅ |
| Provedores reais: Claude, GPT, Gemini, OpenRouter, DeepSeek, Groq, Ollama | ✅ |
| Troca automática entre LLMs quando um falha | ✅ |
| Corvos agindo no computador (terminal, arquivos, navegador, mouse, teclado) | ✅ |
| Modo terminal na pasta do projeto, com perguntas, @anexos, pastas extras e desfazer ([docs](docs/terminal.md)) | ✅ |
| Android pelo Termux | ✅ |
| Conversa ao vivo e o Vigia mandando refazer | ✅ |
| Modo treinamento (vira memória/RAG), com consentimento | ✅ |
| Cofre de chaves cifrado + envelope pós-quântico (**não auditado**) | ✅ |
| Civilização dos Corvos v2: 10 corvos com genoma e modelo próprio, conversando entre si e com você (`dotcrow civ conversar` ou `/civ`) | ✅ |
| WhatsApp, app nativo e marketplace de RAGs | 🟡 a caminho — veja o [ROADMAP](ROADMAP.md) |

## 🤝 Quero ajudar

Toda ajuda conta: código, testes, documentação, tradução, revisão de segurança ou só testar e abrir issues. Não precisa ser sênior — precisa ter vontade. 💚

1. Leia o [guia de contribuição](CONTRIBUTING.md) (é curtinho).
2. Pegue uma issue [`boa-primeira-issue`](https://github.com/napoleaomp4/dotcrow/labels/boa-primeira-issue) ou [`ajuda-bem-vinda`](https://github.com/napoleaomp4/dotcrow/labels/ajuda-bem-vinda).
3. Comente na issue que vai pegar, para ninguém fazer em dobro.

**Bons primeiros lugares:** um provedor novo compatível com a OpenAI (Mistral, xAI...), a ponte com o WhatsApp pela [API do painel](docs/painel.md), o app nativo falando o [protocolo remoto](docs/remoto.md), ou testes de *prompt injection* para o Vigia.

## 🗂️ Estrutura

```
crates/
  dotcrow-core/        permissões, riscos e política de confirmação
  dotcrow-crypto/      envelope híbrido pós-quântico e cifra simétrica
  dotcrow-vault/       cofre de chaves de API (só escreve, nunca mostra)
  dotcrow-providers/   adaptadores de LLM + roteador com troca automática
  dotcrow-agents/      os seis corvos, o fluxo, o orquestrador e a civilização dos 10 corvos
  dotcrow-cli/         o comando dotcrow, o painel e o túnel remoto
  dotcrow-relay/       o relay do túnel (roda na nuvem; só repassa bytes)
docs/                  arquitetura, agentes, segurança, painel e API, remoto
```

## 🔐 Segurança

Achou uma falha? **Não abra issue pública.** Siga o [SECURITY.md](SECURITY.md). O modelo de ameaças e os detalhes da criptografia estão em [docs/seguranca.md](docs/seguranca.md).

## 📄 Licença

Código sob a [Licença Apache 2.0](LICENSE). O nome DotCrow e o logo identificam o projeto oficial; veja [assets/brand/README.md](assets/brand/README.md).

---

<p align="center">
  Criado por <a href="https://github.com/napoleaomp4">Napoleão Junior</a> em Buriticupu, Maranhão 🇧🇷<br>
  <a href="https://x.com/napoleaomp4">@napoleaomp4</a> · <a href="https://dotcrow.si">dotcrow.si</a><br>
  <sub>Se curtir a ideia, deixa uma ⭐ — ajuda demais um projeto pequeno a crescer.</sub>
</p>
