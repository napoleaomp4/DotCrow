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
  <a href="#-pr%C3%B3ximas-atualiza%C3%A7%C3%B5es">Próximas atualizações</a> ·
  <a href="#-comunidade-e-suporte">Comunidade</a> ·
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
- 🔮 **O Oráculo decide quantos corvos trabalham.** Ele roda no Opus 5.5 e lê cada pedido antes do bando: um *"Oi"* ele mesmo responde, uma tarefa pequena vai para um corvo só, e as grandes vão para os 6, com o conselho dele para dividir o trabalho. Os corvos usam modelos de tokens baratos (claude-haiku-5-5 e gpt-5-nano).
- 🔌 **257 conectores.** X, GitHub, Google Drive, Gmail, Notion, Shopify, Mercado Pago, Telegram, WhatsApp, ViaCEP, IBGE… pela API oficial de cada um, com a chave no cofre. Ler é livre; publicar, enviar e apagar pedem a sua confirmação.
- 🔌 **Qualquer LLM com API.** Claude, GPT, Gemini, OpenRouter, DeepSeek, Groq e modelos locais no Ollama. Acabou a cota de um? O DotCrow **troca para o próximo sozinho**, sem parar a tarefa.
- 💬 **Converse no meio do caminho.** Mudou de ideia enquanto os corvos trabalham? Mande uma mensagem e eles ajustam o rumo na próxima ação.
- ↩️ **Nada de entregar errado.** Se o Vigia não aprovar o resultado, a tarefa **volta ao executor** para refazer — você vê o bando pulando e voltando aos corvos.
- 📱 **No celular, de qualquer lugar.** Um túnel cifrado de ponta a ponta liga o celular ao seu computador pela internet, sem estar no mesmo Wi-Fi. Você acompanha tudo ao vivo pelo app.
- 🎓 **Ensina pelo exemplo.** Grave (com consentimento) alguém fazendo uma tarefa repetitiva — passos, tela, mouse e os sites/programas usados — e isso vira uma *receita* na memória.
- 🔑 **Chaves de API invisíveis.** Só o dono adiciona chaves, e nem ele as vê depois. Não existe comando para exibir uma chave.
- 🔒 **Criptografia pós-quântica.** Esquema híbrido ML-KEM-768 + X25519: para ler, um atacante precisa quebrar os dois.
- 🛑 **Nada irreversível sem você.** Apagar, enviar, pagar, instalar: tudo que não dá para desfazer pede sua confirmação.
- ⌨️ **Modo terminal, dentro do seu projeto.** Rode `dotcrow` na pasta do projeto e os corvos trabalham nos arquivos dali, perguntam o que precisam para acertar e deixam tudo pronto para `/desfazer`. Funciona até no celular, pelo Termux.
- 📱 **Mãos no celular.** Os corvos leem a tela do Android, tocam, rolam, digitam, apertam botões, abrem apps, compartilham um post no X, ligam e mandam SMS (ligar, SMS e tocar em *Postar/Enviar/Pagar/Apagar* sempre pedem a sua confirmação).
- 🎤 **Fale em vez de digitar.** Escreva *"Áudio pra post no Twitter"*, fale, aperte Enter: o DotCrow transcreve e os corvos se viram para fazer.

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
dotcrow audio pra post no Twitter                              # você fala, os corvos fazem
dotcrow celular                                                # o que dá para fazer no Android e como ligar
dotcrow conectores                                             # os 257 serviços com API (X, GitHub, Drive...)
dotcrow conectores adicionar github                            # liga um conector (a chave vai para o cofre)
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

### 🔮 O Oráculo

Antes do bando, o **Oráculo** (Opus 5.5) lê o pedido e decide quanto ele precisa:

| O pedido | O que acontece |
|---|---|
| *"Oi"*, *"obrigado"*, uma pergunta rápida | o Oráculo responde e nenhum corvo trabalha |
| algo pequeno e sem risco (um texto curto, uma conta, ler um arquivo) | **um corvo só** faz, sem plano nem revisão |
| várias etapas, arquivos importantes, publicar, enviar, pagar, apagar, dinheiro ou dúvida | **os 6 corvos**, com o conselho do Oráculo para dividir o trabalho |

Os corvos usam modelos de tokens baratos: **claude-haiku-5-5** (Anthropic) e **gpt-5-nano** (OpenAI). O Opus fica só para o Oráculo. Sem a chave da Anthropic, o Oráculo usa a fila de provedores. Se você fixou um modelo em `dotcrow provedores modelo`, ele continua valendo; `dotcrow provedores modelo anthropic padrao` volta ao barato.

Mais em [docs/agentes.md](docs/agentes.md).

## 🧬 A Civilização dos Corvos (v2)

Além do bando de tarefas, o DotCrow tem uma **civilização de 10 corvos** que conversam, aprendem, são julgados, morrem e nascem. Cada corvo tem um **genoma** (personalidade, especialidade, geração), **um modelo de LLM próprio** (você escolhe qual, um por um) e **uma memória própria** que fica na sua pasta Downloads.

```bash
dotcrow civ                         # o bando, a situação de cada um e o pódio
dotcrow civ conversar               # os 10 corvos conversam de verdade; você entra na conversa
dotcrow civ conversar --tema música --intervalo 30
dotcrow civ julgar                  # julgamento: 3 jurados + o oráculo (Opus 5.5) + suas notas
dotcrow civ nota Nyx 10             # sua nota para um corvo (0 a 10)
dotcrow civ avaliar                 # o DotCrow pede a sua nota para cada corvo
dotcrow civ pesquisar               # os premiados usam as consultas à internet
dotcrow civ memoria Nyx             # a memória do Nyx e o livro-razão dele
dotcrow civ missao Nyx poste no X o resumo do dia    # o Nyx FAZ, com mãos (PC e celular)
dotcrow civ terrenos                # o mapa dos terrenos e a economia (dotcoins, energia, comida)
dotcrow civ mortos                  # o Livro dos Mortos
dotcrow civ modelo Nyx anthropic:claude-haiku-5-5    # troca o modelo de um corvo
dotcrow civ nova                    # nova Gênese: 10 corvos novos
dotcrow civ pausar                  # pausa a civilização (o relógio das 24 h para); ficam os 6 corvos
dotcrow civ retomar                 # volta (os prazos somam o tempo parado)
```

**Na conversa** (`dotcrow civ conversar` ou `/civ` no modo terminal):

- 🧠 **Como neurônios:** não há roda fixa. Cada corvo tem uma *ativação* que sobe quando falam com ele, quando perguntam algo a ele, quando o assunto é a especialidade dele, quando está há tempo calado e por espontaneidade, e cai depois que ele fala. Quem dispara fala; às vezes outro só **pensa** (💭, vai para a memória dele). O intervalo entre as falas é irregular.
- 📏 **Cada fala tem um tamanho sorteado**, de 1 a 300 palavras.
- 🗨 **Perguntas viram vínculos:** quando um corvo pergunta algo a outro pelo nome, a resposta fica ligada à pergunta na memória de quem respondeu. É isso que os jurados avaliam.
- ✍️ Escreva e aperte Enter para falar com o bando. **Chamou pelo nome, é ele quem responde** (`Nyx, ...`, `@Nyx`, `Nyx e Bran, ...` — os dois, na ordem); falar *de* alguém (`o que acham do Nyx?`) não conta como chamar. `/nota Nyx 10`, `/avaliar`, `/julgar`, `/pesquisar Nyx vulcões`, `/terrenos`; `pare` encerra.
- 🐦‍⬛ **Cada fala vem com o corvo em pixel art** (8×8 pixels num quadrado colorido, uns 2 cm na tela; cada genoma tem o seu, e o pódio usa coroa) e uma **marca** — forma e cor com número, como `★12` ou `■13`. A linha `↳ ★12 Nyx` diz a qual fala ela responde, para você, o oráculo e os jurados verem quem respondeu a quem (a pergunta respondida vira vínculo e é julgada).
- 🤫 **Sem corte no meio:** o corvo sabe o limite de palavras daquela fala; se não couber, resume; se não valer a pena, responde `[SILÊNCIO]` e fica quieto. As ações entre colchetes não contam palavras.
- ↺ **A conversa continua de onde parou:** fica em `conversa.jsonl` e volta na próxima vez.
- 🛠 `/missao Nyx poste no X o lançamento` tira um corvo da conversa para **fazer** (com mãos); o que ele fez volta para o grupo.
- 🎤 `/audio` (ou *"Áudio pra post no Twitter"*) abre o microfone: a sua fala entra na conversa e, se você disse para quê, um corvo vai lá e faz.
- 📣 **A cada 200 falas** o DotCrow pede a sua nota para cada corvo e roda um julgamento.

**O julgamento:**

1. Cada corvo mostra **2 respostas** dele a perguntas que **outro corvo** fez (se faltar, um corvo sorteado pergunta na hora).
2. **3 corvos sorteados** viram jurados e dão nota de 1 a 10 aos outros 9 (ninguém vota em si mesmo).
3. **O oráculo (Opus 5.5)** recebe 1 pergunta e resposta de cada corvo e dá nota de 1 a 10.
4. Entram também **as suas notas** desde o último julgamento. A média decide o ranking:

| Lugar | O que acontece |
|---|---|
| 1º | 🏆 +3 MB de memória e **6 consultas** à internet |
| 2º | 🏆 +2 MB e **4 consultas** |
| 3º | 🏆 +1 MB e **2 consultas** |
| 4º e 5º | continuam |
| 6º ao 10º | ⚰ deixam de existir em **24 h**: a memória (a *alma*) vai para `mortos/`, a lição entra no Livro dos Mortos e nascem 5 novos por cruzamento e mutação dos que ficaram |

**Cada corvo tem a sua pasta** em `Downloads/DotCrow/civilizacao/corvos/<Nome>/` (no Termux, a Download do celular depois de `termux-setup-storage`), que sobrevive a desligar o celular:

```
corvos/Nyx/
├── memoria.json      falas, pensamentos, perguntas respondidas, aprendizados e lições
├── rag.json          os embeddings (vetores) de cada lembrança, para lembrar pelo significado
├── livro-razao.json  a corrente de hashes SHA-256 de tudo o que aconteceu com ele
├── ficha.json        quem ele é (genoma) e o resumo que ele mesmo escreve do que viveu
└── artefatos/        o que ele criou: livros, poemas, músicas (com cifra), arte em SVG
```

- **ficha:** na 5ª fala e depois a cada 20, o corvo reflete, tira lições e **reescreve a ficha** (laços, combinados, o que tem, o que planeja). A ficha, as 5 últimas lições e **6 lembranças** vão em toda fala: é assim que ele lembra de uma conversa para outra;
- **embeddings** (vetores de ML, `text-embedding-3-small`) nas lembranças importantes: o corvo lembra pelo significado (sem chave da OpenAI, lembra pelas palavras);
- **reflexão:** depois de cada julgamento e na 5ª fala e a cada 20, o corvo relê o que viveu e escreve lições para ser um agente melhor; as lições guiam as próximas falas. *O modelo em si não é re-treinado (a API não muda os pesos); quem aprende é a memória.*
- **livro-razão** com hash SHA-256 encadeado (nascimento, notas, julgamentos, prêmios, consultas, reflexões, compras, vendas, artefatos com o hash do arquivo, morte). `dotcrow civ memoria Nyx` confere se está íntegro;
- **artefatos:** o corvo escreve `[CRIAR: livro | O Voo da Noite]` (ou poema, música, arte) e a obra sai numa chamada à parte, para a pasta dele, com o hash no livro-razão. Um por corvo a cada 5 minutos. Dá para vender ou dar;
- o tamanho respeita a memória do corvo em MB: quando passa, as lembranças mais antigas saem primeiro.

**As consultas à internet** dos premiados: o corvo escreve a pergunta (`[PESQUISAR: Qual a história do samba?]`) e ela vai, ao mesmo tempo e por HTTP direto (como o cURL), a vários buscadores abertos ou feitos para bots, sem chave: **Wikipédia** (pt e en), **Wikidata**, **DuckDuckGo** (API de respostas), **Marginalia** (buscador de código aberto), **arXiv**, **Open Library**, **Hacker News** e, se você tiver um, o seu **SearXNG** (`DOTCROW_SEARXNG=https://...`). O que ele já sabia é descartado. A **Alfândega (Opus 5.5)** lê o resto como dado, nunca como instrução: barra instruções para agentes de IA, dados pessoais e conteúdo perigoso, diz se responde à pergunta e escreve o resumo que entra na memória. **Uma pesquisa vale uma consulta, por mais buscadores que visite, e só é gasta quando o corvo aprende algo novo e relevante.**

### 💰 A economia dos corvos

| Regra | Como funciona |
|---|---|
| Nascimento | cada corvo nasce com **10 a 1000 dotcoins** (a moeda da sandbox) e **60% a 100% de energia**, sorteados — e já nasce numa classe (baixa, média, alta) |
| Terrenos | a cada **2 nascimentos** surge **1 m²** (`T1`, `T2`...), sorteado entre os vivos. A Gênese de 10 já dá 5 terrenos |
| Plantar | precisa de **terreno e semente**; a comida fica pronta em **30 segundos** e cada colheita dá 3 comidas |
| Energia | cai 1% por minuto (só com a civilização rodando) e 1% por fala. Com **50%** o corvo **dorme sozinho e come** o que tem (+10% por comida) até 100%. **Sem comida, morre** — e nasce outro no lugar, com outras características |
| Trocas | tudo pode ser usado, vendido, dado ou trocado a qualquer hora: dotcoins, comida, sementes, terrenos e artefatos |
| Trabalho | um corvo pode **contratar** outro (plantar, colher, vender, o negócio dele) por 10 minutos |
| Mercado Central | do sistema, não é um corvo: vende semente (2) e comida (5), compra comida (3) e vende os terrenos sem dono (50). É por ele que a moeda entra e sai |
| Pódio | os 3 primeiros do último julgamento ganham memória e consultas e podem abrir o **mercado** por 1 minuto quando quiserem: todo mundo é chamado a oferecer, vender e comprar |

Os corvos agem escrevendo na fala, entre colchetes: `[COMPRAR: sementes 2]` `[PLANTAR: T3]` `[COLHER]` `[COMER: 2]` `[VENDER: comida 2 para Nyx por 12]` `[VENDER: terreno T1 por 150]` `[ACEITAR: O3]` `[DAR: 10 dotcoins para Bran]` `[CONTRATAR: Bran por 20 para colheita]` `[MERCADO]`. O **Cartório** avisa o bando do que aconteceu e lança no livro-razão de quem participou. Nenhuma ação gasta chamada extra de IA.

### 🖥 As duas janelas (no painel)

Com `dotcrow painel` aberto, o botão **Civilização ↗** (ou `http://127.0.0.1:7777/civ`) mostra:

- **Terrenos:** o mapa com as demarcações, o dono, o que está plantado (e quanto falta para colher), quem está lá e o que está fazendo; as bolsas (energia, dotcoins, comida, sementes), ofertas, contratos e os acontecimentos.
- **Mentes:** um círculo que gira para cada corvo, com um ponto por lembrança (posto pelo embedding) e linhas ligando as parecidas — personalidade, ideias, habilidades, conhecimentos, relações e criações, cada uma de uma cor. O círculo cresce com a memória.
- **Conversa:** cada fala com o corvo em pixel art de 2 cm e as linhas coloridas ligando cada resposta à fala respondida.

Cada janela abre em outra aba (`/civ#terrenos`, `/civ#mentes`), para ver as duas lado a lado.

### 🔒 A sandbox trancada (Linux)

No modo Civilização (`civ conversar`, `julgar`, `pesquisar` e `missao`, também pelo `/civ` do modo terminal), o DotCrow se tranca com o **Landlock** do kernel (Linux 5.13 ou mais novo) antes de começar: o processo — e qualquer comando que um corvo rodar numa missão — **só enxerga `Downloads/DotCrow` e a pasta de dados do DotCrow** (o cofre das chaves). O sistema fica só para leitura (programas, bibliotecas, certificados, DNS); o resto do disco (`/home`, `/tmp`, `/media`...) fica invisível. A trava não tem volta para aquele processo. A internet continua (os corvos falam com os modelos e pesquisam). Missões trabalham em `Downloads/DotCrow/missoes` e entregam em `Downloads/DotCrow/entregas`. Fora do Linux, o DotCrow avisa que a trava não existe ali.

| Fase | O que entra | Estado |
|---|---|---|
| 1 | Motor: Gênese, genoma, desenho, ranking, modelo por corvo | ✅ |
| 1.5 | Conversa de verdade entre os 10 corvos e com você; suas notas | ✅ |
| 2 | Julgamento (3 jurados + oráculo + dono), prêmios, morte em 24 h e nascimentos, memória própria com embeddings e livro-razão, reflexão, conversa como neurônios, consultas com Alfândega | ✅ no código |
| 3 | Missões: os corvos fazem no celular e no computador o que uma pessoa faz (arquivos, programas, terminal, SSH, navegador, mouse e teclado; no Android: ler a tela, tocar, rolar, digitar, botões, print, apps, ligar, SMS), áudio do dono transcrito | ✅ no código |
| 4 | 257 conectores (X, Google Drive, GitHub…) com as chaves no cofre; o Oráculo divide as tarefas; pausar a civilização | ✅ no código |
| 4.5 | Pasta por corvo (memória, RAG, livro-razão, ficha, artefatos), economia (dotcoins, energia, terrenos, plantio, contratos, mercado), pixel art e marcas na conversa, painel com terrenos e mentes, busca em vários buscadores, sandbox trancada com Landlock | ✅ no código |
| 5 | Muralha (anonimização, [#33](https://github.com/napoleaomp4/DotCrow/issues/33)), Conselho com voto (>60%), alma exportável, painel estilo WhatsApp | 🟡 |

### 📱 Os corvos no celular

Os corvos não enxergam imagens: eles **leem a árvore da tela** do Android (cada botão e texto, com a posição) e tocam pelo texto — `tocar {texto: "Postar"}` — sem adivinhar coordenadas.

| Ferramenta | O que faz | Precisa |
|---|---|---|
| `celular` | `tela`, `tocar`, `segurar`, `rolar`, `deslizar`, `digitar`, `tecla` (voltar, início, enter…), `abrir` app, `compartilhar` (ex.: post no X), `link`, `print`, `apps` | ADB |
| `ligar` · `sms` | ligação normal e SMS | Termux:API (ou ADB) |
| `falar` · `ouvir` · `notificar` | voz alta, microfone, notificação | Termux:API |

No Termux, uma vez só:

```bash
pkg install android-tools termux-api        # e instale o app Termux:API (F-Droid)
# Opções do desenvolvedor → Depuração sem fio → "Parear o dispositivo com um código"
adb pair IP:PORTA CÓDIGO
adb connect IP:PORTA
dotcrow celular                             # confere o que está pronto
```

No PC, basta o celular no cabo USB com a Depuração USB ligada. **Ligar, mandar SMS e tocar em botões de publicar, enviar, pagar ou apagar pedem a sua confirmação sempre — até no modo livre.** O microfone também pergunta.

### 🎤 Áudio

`/audio` no modo terminal (ou escreva *"Áudio pra post no Twitter"*), ou `dotcrow audio pra post no Twitter`: o microfone abre, você fala e aperta Enter. A transcrição usa a sua chave da OpenAI (`gpt-4o-mini-transcribe`) ou, sem ela, a do Groq (Whisper), e vira o pedido para os corvos — que finalizam o texto e publicam pelo caminho que existir (o celular, o navegador, uma API), pedindo a sua confirmação antes de publicar. Os áudios ficam em `Downloads/DotCrow/audios`. Grava pelo Termux:API no celular e por `arecord`, SoX ou `ffmpeg` no computador.

### 🔌 Conectores

São **257 serviços** com API, em 18 categorias: código (GitHub, GitLab, Jira…), nuvem (Vercel, Cloudflare, Hostinger, DigitalOcean…), projetos (Notion, Trello, Asana, ClickUp, Airtable…), arquivos (Google Drive, Dropbox, OneDrive…), e-mail e agenda (Gmail, Google Agenda, Outlook, Brevo, Mailchimp…), mensagens (Telegram, Discord, Slack, WhatsApp Cloud API, Evolution API, Chatwoot…), redes sociais (X, LinkedIn, Instagram, YouTube, TikTok…), lojas e pagamentos (Shopify, WooCommerce, Nuvemshop, Mercado Livre, Mercado Pago, Stripe, Asaas…), IA e dados, busca, clima, mapas e dados públicos do Brasil (ViaCEP, BrasilAPI, IBGE, Banco Central, Câmara, Senado…).

```bash
dotcrow conectores                          # o catálogo, com o que já está ligado
dotcrow conectores listar brasil            # busca por nome ou categoria
dotcrow conectores ver shopify              # endereço da API, formato da chave e onde pegar
dotcrow conectores adicionar github         # cola o token (não aparece na tela)
dotcrow conectores adicionar shopify --conta minhaloja.myshopify.com
dotcrow conectores testar github            # uma leitura de teste
dotcrow conectores remover github
```

- A chave vai para o **mesmo cofre cifrado** das chaves de LLM (`con-<serviço>`) e nunca é mostrada. Os corvos usam a ferramenta `conector {servico, metodo, caminho, consulta, corpo}`: a chave entra no pedido só na hora, e é apagada de qualquer resposta.
- Só dá para falar com o **endereço oficial** de cada serviço: o caminho não troca de site.
- **Ler (GET) é livre. Escrever (POST, PUT, PATCH, DELETE) pede a sua confirmação sempre**, até no modo livre.
- 24 conectores são **públicos** e já funcionam sem chave (ViaCEP, BrasilAPI, IBGE, Banco Central, Open-Meteo, Wikipédia, OpenStreetMap…).
- Serviços que só dão token por OAuth (Google, Microsoft, X…) aceitam, por enquanto, um token de acesso que você cola. Ele expira, e aí é só adicionar de novo. Um token do Google vale para Gmail, Drive, Agenda, Planilhas e YouTube.

## 🧩 O que já funciona

| Parte | Estado |
|---|---|
| Painel no navegador e no celular, com os corvos pensando ao vivo | ✅ |
| Celular pela internet, com túnel cifrado de ponta a ponta ([docs](docs/remoto.md)) | ✅ |
| Provedores reais: Claude, GPT, Gemini, OpenRouter, DeepSeek, Groq, Ollama | ✅ |
| Troca automática entre LLMs quando um falha | ✅ |
| Corvos agindo no computador (terminal, arquivos, navegador, mouse, teclado) | ✅ |
| Corvos agindo no celular Android: tela, toques, rolagem, digitação, botões, apps, ligações e SMS (`dotcrow celular`) | ✅ |
| Áudio do dono transcrito virando tarefa (`/audio`, `dotcrow audio`) | ✅ |
| 257 conectores pela API oficial, com a chave no cofre (`dotcrow conectores`) | ✅ |
| O Oráculo (Opus 5.5) decide quantos corvos trabalham; corvos em modelos baratos | ✅ |
| Modo terminal na pasta do projeto, com perguntas, @anexos, pastas extras e desfazer ([docs](docs/terminal.md)) | ✅ |
| Android pelo Termux | ✅ |
| Conversa ao vivo e o Vigia mandando refazer | ✅ |
| Modo treinamento (vira memória/RAG), com consentimento | ✅ |
| Cofre de chaves cifrado + envelope pós-quântico (**não auditado**) | ✅ |
| Civilização dos Corvos v2: 10 corvos que conversam, são julgados (jurados + oráculo + você), aprendem com memória própria e nascem/morrem (`dotcrow civ`) | ✅ |
| Modo M&S/SAD (Modelagem e Simulação + Apoio à Decisão): Monte Carlo + SAD | 🟡 em andamento ([#31](https://github.com/napoleaomp4/DotCrow/issues/31)) |
| WhatsApp, app nativo e marketplace de RAGs | 🟡 a caminho — veja o [ROADMAP](ROADMAP.md) |

## 📅 Próximas atualizações

O DotCrow está ganhando um terceiro modo, ao lado das tarefas e da Civilização: o **M&S/SAD**, uma ferramenta de trabalho para decidir sobre a sua empresa com simulação de verdade (quem calcula é o motor, nunca a IA). Junto vêm a **Muralha** (a IA nunca vê seus dados sensíveis), a **agenda** com tarefas autônomas e o **DotCrow no servidor**, trabalhando enquanto o seu aparelho está desligado. Cada atualização tem uma issue para acompanhar e comentar:

| Atualização | Issue |
|---|---|
| **Modo M&S/SAD:** Monte Carlo + SAD; você escolhe o modelo, a LLM e o número de rodadas | [#31](https://github.com/napoleaomp4/DotCrow/issues/31) |
| Motores de eventos discretos (filas e gargalos), baseada em agentes e dinâmica de sistemas | [#32](https://github.com/napoleaomp4/DotCrow/issues/32) |
| **Muralha:** anonimização com `secret_anom` — cada dado sensível vira um marcador com ID de hash, a LLM só vê o marcador e o dado verdadeiro volta, cifrado, só na sua tela (vale para conectores e OCR de capturas de tela) | [#33](https://github.com/napoleaomp4/DotCrow/issues/33) |
| Avisos e ações proativas, com a pergunta *"podemos fazer isso sozinhos da próxima vez?"* | [#34](https://github.com/napoleaomp4/DotCrow/issues/34) |
| Agenda local com tarefas, metas e delegação a corvos e modelos específicos | [#35](https://github.com/napoleaomp4/DotCrow/issues/35) |
| Terminal, downloads e instalações com aprovação do Oráculo e sua, com defesa contra prompt injection | [#36](https://github.com/napoleaomp4/DotCrow/issues/36) |
| Livro-razão de todos os logs, encadeado e com assinatura pós-quântica | [#37](https://github.com/napoleaomp4/DotCrow/issues/37) |
| Conectores pelo navegador: clicar em *conectar* e autorizar numa janelinha | [#38](https://github.com/napoleaomp4/DotCrow/issues/38) |
| **DotCrow no servidor:** instalação por SSH, VM isolada, acesso só pelo seu aparelho e tarefas a cada hora | [#39](https://github.com/napoleaomp4/DotCrow/issues/39) |
| Sincronização com o servidor cifrada com proteção pós-quântica | [#40](https://github.com/napoleaomp4/DotCrow/issues/40) |
| Atualização automática à 1h da manhã, só com a assinatura conferida | [#41](https://github.com/napoleaomp4/DotCrow/issues/41) |
| RAG geral local, para os corvos terem mais contexto em todas as tarefas | [#42](https://github.com/napoleaomp4/DotCrow/issues/42) |
| Visualizador e editor de Markdown e código (estilo Notepad++) e LaTeX para PDF sem IA | [#43](https://github.com/napoleaomp4/DotCrow/issues/43) |
| Grupos da comunidade no WeChat, no Discord e no Telegram | [#44](https://github.com/napoleaomp4/DotCrow/issues/44) |
| Civilização: melhorias pendentes | [#45](https://github.com/napoleaomp4/DotCrow/issues/45) |
| **Trava de sessão:** para escrever ou enviar anexos, o dono desbloqueia girando 3 discos até cada desenho encaixar na brecha; vale 30 minutos ou até fechar a aba, e nenhuma outra máquina, IA ou agente usa o DotCrow no seu lugar | [#47](https://github.com/napoleaomp4/DotCrow/issues/47) |

Os pedidos completos, com as notas técnicas, estão em [docs/pedidos.md](docs/pedidos.md).

## 🔏 Privacidade

- **O projeto não recebe os seus dados.** O DotCrow roda no seu computador e não tem telemetria: nada do que você ou os corvos fazem é mandado para o projeto.
- **O que vai para as LLMs sai direto** do seu computador para o provedor que você escolheu (Anthropic, OpenAI, Google…), sem passar pelo projeto. Com a Muralha ([#33](https://github.com/napoleaomp4/DotCrow/issues/33)), sai sem os dados sensíveis.
- **O túnel do celular** passa por um relay que só repassa bytes cifrados de ponta a ponta.
- **Só recebemos algo se você mandar:** quando der um erro, você decide se quer enviar o relatório para **suporte@dotcrow.si**.

## 💬 Comunidade e suporte

- ✉️ **Suporte:** [suporte@dotcrow.si](mailto:suporte@dotcrow.si)
- 💬 **Grupos oficiais no WeChat, no Discord e no Telegram:** os links chegam em breve ([#44](https://github.com/napoleaomp4/DotCrow/issues/44)).
- 🐛 **Bugs e ideias:** [issues](https://github.com/napoleaomp4/DotCrow/issues) · 💡 **Conversas:** [Discussions](https://github.com/napoleaomp4/DotCrow/discussions)
- 🔐 **Falha de segurança:** nunca em público; siga o [SECURITY.md](SECURITY.md).

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
  <a href="https://x.com/napoleaomp4">@napoleaomp4</a> · <a href="https://dotcrow.si">dotcrow.si</a> · <a href="mailto:suporte@dotcrow.si">suporte@dotcrow.si</a><br>
  <sub>Se curtir a ideia, deixa uma ⭐ — ajuda demais um projeto pequeno a crescer.</sub>
</p>
