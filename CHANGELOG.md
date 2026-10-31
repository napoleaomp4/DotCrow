# Changelog

Todas as mudanças relevantes do DotCrow ficam registradas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o projeto usa [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Adicionado

- **Modo terminal.** Num terminal, `dotcrow` (ou `dotcrow terminal`) abre o DotCrow dentro da
  pasta atual e os corvos trabalham nos arquivos do projeto. O ícone (o `.exe` e o `.deb`)
  continua abrindo o painel; no Windows, só conta como terminal quando o `dotcrow` foi
  digitado num (PowerShell, CMD, Git Bash, VS Code). Veja [docs/terminal.md](docs/terminal.md).
  - **Pastas do projeto:** `/pasta +` põe outras pastas, cada uma "só leitura" ou "leitura e
    escrita", com uma anotação de para que serve; ficam salvas por projeto.
  - **Ver arquivos:** `/arquivos` (navegador numerado, com atalhos para a pasta pessoal, a
    Downloads e o armazenamento do celular), `/ver`, `/buscar`, `@arquivo` para anexar ao
    pedido e Tab completando comandos e caminhos.
  - **Alinhamento:** entrevista do projeto na primeira vez (vira o perfil do projeto, que vai
    em toda tarefa), perguntas do Huginn antes de cada tarefa (`/perguntas sempre|direto`) e
    a ferramenta `perguntar` para os corvos tirarem dúvidas no meio do trabalho.
  - **Entregas na Downloads:** o que não é do projeto (relatórios, PDFs, imagens) vai para a
    pasta Downloads, sem perguntar; fora de um projeto, o DotCrow trabalha na Downloads.
  - **Desfazer:** cada arquivo alterado ganha uma cópia (fora do projeto) e `/desfazer` volta
    a última tarefa. Apagar arquivos do projeto por comando sempre pede confirmação.
  - Texto dos corvos ao vivo, `!comando`, `/modo`, `/provedor`, `/chave`, `/testar`, `/nova`
    e Ctrl+C para a tarefa em andamento.
- **Ferramentas novas dos corvos:** `editar_arquivo` (troca só um trecho exato, tolerando
  arquivos do Windows), `buscar` (por nome, com `*`, e por conteúdo) e `perguntar`.
- **Android (Termux).** `instalar-termux.sh` baixa o binário pronto para celular (arm64) ou
  compila no próprio Termux; o shell, o abrir arquivos/links e a pasta Downloads do celular
  funcionam lá. O CI compila para `aarch64-linux-android` e a release publica o
  `dotcrow-android-arm64`.
- `dotcrow tarefa` agora também mostra o texto dos corvos ao vivo.

- **Relay em PHP (`relay.php`).** Alternativa ao binário Rust `dotcrow-relay`: um único
  arquivo para subir num domínio PHP comum e servir de ponto de encontro entre o computador
  e o app (HTTP com long-poll). Continua zero-conhecimento — só repassa base64 cifrado.
  Fica em `relay-php/`.
- **App iOS (esqueleto, SwiftUI) + build no Codemagic.** O celular acessa o computador de
  qualquer lugar pelo túnel cifrado: lê o QR, abre o túnel e acompanha os corvos ao vivo.
  A rede e a tela ficam em Swift; **toda a criptografia é reusada do Rust** (crate novo
  `dotcrow-mobile`, ABI C) para o celular e o computador falarem byte a byte o mesmo
  protocolo — nada de reimplementar cripto em Swift. `apps/ios/` tem o esqueleto, o
  `project.yml` (XcodeGen) e o `build-rust.sh`; o `codemagic.yaml` da raiz compila a lib
  Rust para iOS, monta o XCFramework e exporta o IPA.
- **`dotcrow-tunel`**: o núcleo do túnel (identidade, handshake de ponta a ponta e sessão
  cifrada) virou um crate próprio, compartilhado pelo computador (`dotcrow-cli`) e pelo app
  (via FFI). A mesma criptografia dos dois lados, sem duplicar.
- **Navegador com cursor próprio (CDP).** Os corvos agora navegam numa janela própria e
  clicam/digitam por eventos sintéticos do DevTools Protocol — **o mouse do dono não se
  mexe**, então ele pode usar o computador ao mesmo tempo. Ações novas em `navegador`:
  `ir`, `ler`, `clicar` (por seletor ou x,y), `digitar`, `rolar` e `captura`.
- **QR de pareamento que troca a cada 10 s.** O QR do túnel remoto agora leva só um token de
  curta duração (não o código durável), então um print antigo não serve para ninguém. No 1º
  pareamento o celular recebe o código durável cifrado (frame `CRED`) e guarda; depois
  reconecta de qualquer rede sem ler o QR de novo. Protocolo em `docs/remoto.md`.
- **Triagem de dados sensíveis antes da IA.** Antes de qualquer texto ir para uma LLM — o
  pedido, o contexto e o que os corvos leem de arquivos durante a tarefa (um `.env`, por
  exemplo) —, o DotCrow tira CPF e CNPJ (validados pelo dígito verificador), chaves de API e
  tokens conhecidos (OpenAI, Google, AWS, GitHub, Slack, JWT), blocos de chave privada e
  valores de senha/segredo em atribuições. O prompt dos corvos não é tocado.

### Corrigido

- **Termux: o teste da chave e toda conexão HTTPS derrubavam o programa.** O verificador de
  certificados do sistema, no Android, só funciona dentro de um app Android (com Java). No
  Android o DotCrow agora confere os certificados com a lista de autoridades da Mozilla
  embutida no binário.
- **O treino de RAG não trava mais a tela.** A borda verde virou uma moldura que ocupa só as
  beiradas (o centro da tela nem faz parte da janela), então o resto do computador continua
  livre para usar enquanto grava. O selo do canto agora é **clicável**: tocar nele encerra a
  gravação na hora (o botão "Parar" do painel continua valendo).
- **Texto com acento ou de outro alfabeto não derruba mais o DotCrow.** A ferramenta `web`
  caía ao decodificar `&amp;`, `&lt;` e afins quando vinha acento ou letra não latina logo
  depois (como em "Pesquisa &amp; Inovação"), e um comando com barra desconhecido e acentuado,
  como `/ação`, fechava o modo terminal. Os dois cortavam o texto no meio de uma letra.

## [0.7.0] - 2026-10-23

### Adicionado

- **Ver cada corvo pensando ao vivo.** O texto dos corvos aparece token a token no painel,
  por streaming de verdade (SSE) nos provedores Claude e compatíveis com a OpenAI, no fluxo
  normal e no mural da equipe.
- **O Vigia pode mandar refazer.** Se a conferência não passar, a tarefa volta ao executor
  para uma nova rodada (até 2), e o painel mostra o bando voltando ao corvo em vez de
  sobrescrever a etapa — dá para ver os corvos pulando e voltando.
- **Conversa ao vivo durante a tarefa.** Uma caixa no painel manda mensagens para a tarefa
  em andamento; o executor leva em conta antes da próxima ação e pode mudar o rumo.
- **Treino de RAG com borda verde.** Enquanto grava, a tela ganha uma borda verde e o selo
  "Em treinamento de RAG", e o DotCrow registra o que está em foco (navegadores e outros
  programas) junto com os passos — tudo isso vira memória.
- **Celular pela internet (túnel remoto).** `dotcrow painel --remoto` deixa o celular
  acessar o computador de qualquer lugar (Wi-Fi ou dados móveis), sem estar na mesma rede
  e sem abrir porta. Passa por um relay que só repassa bytes; tudo vai cifrado de ponta a
  ponta (envelope ML-KEM-768 + X25519 e chave de sessão XChaCha20-Poly1305). O celular
  acompanha os corvos **ao vivo** (o fluxo SSE do painel vai inteiro pelo túnel). Pareamento
  por QR (`dotcrow://p?...`), uma vez só. Protocolo em `docs/remoto.md`.
- **`dotcrow-relay`**: o servidor de encontro, para rodar na VPS. Zero-conhecimento (não lê
  nem guarda nada); fica atrás do TLS do nginx.
- Escolher o **modelo** de uma tarefa/pergunta só para aquela vez, no painel (ao lado do
  provedor), sem mexer na configuração.

### Corrigido

- O navegador invisível usa um perfil novo e descartável a cada chamada. Um perfil
  compartilhado deixava um `SingletonLock` quando o navegador anterior não fechava direito,
  e aí toda busca seguinte falhava (era a causa da busca de empresas não retornar nada).
- A busca orienta a usar `lite.duckduckgo.com` e o navegador invisível quando um buscador
  pede CAPTCHA, em vez de travar.

## [0.6.1] - 2026-10-21

### Alterado

- O Anthropic agora usa **Opus 5.5 em todas as etapas** (sem Haiku), por escolha do dono. Fixar outro modelo na aba Provedores continua valendo. Para variar por faixa de novo, basta trocar em `anthropic::pick`.

## [0.6.0] - 2026-10-21

### Adicionado

- **Exportar e importar RAG.** Dá para treinar uma memória e salvá-la como um arquivo `.dotrag` (JSON com ficha: nome, autor, licença, preço e chave Pix do vendedor) e importar o de outra pessoa. Base para o marketplace, que o dono fará depois. Segredos são removidos de novo na exportação.
- **Modo treinamento (com consentimento).** Grava uma pessoa fazendo uma tarefa repetitiva — passos rotulados, capturas de tela e movimentos do mouse — e transforma a sessão em memória (RAG). A gravação **só começa com o consentimento explícito** de quem é gravado (nome + "autorizo"); nada de gravação escondida. Abas **Memória** (exportar/importar) e **Treino** no painel.
- API: `POST /api/memoria/exportar`, `POST /api/memoria/importar`, `GET /api/treino`, `POST /api/treino/{iniciar,passo,tela,parar}`.

## [0.5.0] - 2026-10-21

### Adicionado

- **Memória do DotCrow (o RAG do Muninn).** O bando lembra do dono entre conversas: antes de planejar, o Muninn busca o que já sabe e põe no contexto; no fim, guarda o essencial da tarefa. Vale no fluxo normal e na equipe.
- A memória fica **cifrada neste computador** (XChaCha20-Poly1305 com a chave-mestra); só este computador abre. Nada sai para nuvem por conta própria.
- **Remoção de segredos:** antes de guardar e antes de ir para a IA, o que parece senha, chave, token ou bloco de chave privada é apagado. Uma nota que seja só segredo não é guardada.
- Aba **Memória** no painel: ver, ensinar algo à mão e esquecer. API: `GET/POST /api/memoria`, `DELETE /api/memoria/{id}`.
- Contrato `Memory` (recall/remember) no núcleo; a implementação cifrada fica no app.

## [0.4.0] - 2026-10-21

### Adicionado

- **Equipe de corvos em paralelo.** Ligando "Equipe", o Huginn divide o pedido em subtarefas independentes e vários corvos trabalham ao mesmo tempo (até 7 de uma vez), cada um na sua faixa. No painel, um **mural** mostra uma coluna por corvo, com o que cada um está fazendo e pensando ao vivo; a Pena junta tudo num resumo só. Se o pedido for melhor feito por um corvo só, cai no fluxo normal.
- Eventos novos: `team_planned` (as faixas) e `lane_event` (um evento dentro de uma faixa). API: `"equipe": true` em `POST /api/tarefas`.

## [0.3.0] - 2026-10-21

### Adicionado

- **Cada etapa usa o modelo do porte certo.** Planejar e revisar (Huginn e Vigia) usam a faixa forte (Deep); executar, a equilibrada (Standard); o resumo da Pena, a leve (Fast). No Anthropic isso vira Opus, Sonnet e Haiku. Quem fixa um modelo na aba Provedores continua mandando. O `tier` entra no `CompletionRequest`; cada provedor escolhe o modelo da sua faixa.

### Alterado

- Testar as chaves usa a faixa leve (mais barato).

### Segurança

- Comandos que destroem o sistema ou um disco (`rm -rf /`, `mkfs`, `dd of=/dev/...`, `wipefs`, fork bomb, `format c:`) agora são **recusados em qualquer modo**, não só confirmados.

## [0.2.0] - 2026-10-21

### Adicionado

- **Os corvos agem no computador.** Quem executa trabalha em ciclos (pede uma ação, vê o resultado, decide a próxima) com as ferramentas `terminal`, `ler_arquivo`, `escrever_arquivo`, `listar`, `web` (internet sem navegador), `navegador` (Chrome invisível com JavaScript e captura), `baixar`, `abrir`, `janelas`, `focar`, `mouse`, `teclado` e `tela`. Cada ação aparece ao vivo no painel e no `dotcrow tarefa`.
- Três modos por conversa: **Computador** (pergunta o que é sensível), **Livre** (não pergunta, exceto o que destrói o sistema) e **Sandbox** (caixa isolada com bubblewrap; só a pasta da conversa muda).
- Confirmações no painel (**Permitir**, **Permitir sempre nesta conversa**, **Negar**) e no terminal. O cofre de chaves nunca é lido pelos corvos.
- Pastas por conversa em `~/DotCrow/conversas/`, com `conversa.md`; pedidos seguidos na mesma conversa lembram o que já foi feito. O painel lista os arquivos e abre a pasta.
- O painel abre numa janela própria (modo aplicativo do Chrome/Edge/Brave), sem barra de endereço.
- API: `modo` e `conversa` em `POST /api/tarefas`, `POST /api/aprovacoes/{id}`, `GET /api/conversas/{id}/arquivos`, `POST /api/abrir` e os eventos `action_*` e `approval_*`.

### Alterado

- Visual em preto e branco.
- Com o provedor só ocupado (limite de uso ou fora do ar), os corvos esperam e tentam de novo em vez de parar a tarefa; o erro final diz o motivo de cada provedor.
- O `.deb` recomenda `xdotool`, `wmctrl`, `bubblewrap` e um programa de captura de tela.

### Corrigido

- Recusa do modelo (Anthropic `stop_reason: refusal`) aparece como recusa, não como "resposta sem texto".

### Corrigido

- O ícone do Linux (`.deb`) chama `/usr/bin/dotcrow` pelo caminho completo. Antes, um `dotcrow` antigo em `~/.local/bin` passava na frente, e clicar no ícone não abria nada.
- Quando o painel não consegue abrir pelo ícone (sem terminal), o erro aparece na tela: notificação no Linux e janela de aviso no Windows e no macOS. Antes, o erro sumia sem ninguém ver.

### Adicionado

- `instalar.sh` cria o ícone do DotCrow no menu de aplicativos do Linux, apontando para o comando que ele acabou de instalar.

- O ícone do DotCrow (Linux e Windows) abre o painel no navegador sem passar pelo terminal. `dotcrow` sem comando abre o painel; clicar de novo com ele aberto só traz a página de volta; o botão "Fechar painel" (`POST /api/desligar`) encerra.
- Pacotes prontos para instalar sem compilar: `.deb` (Linux), `dotcrow.exe` portátil e `dotcrow-setup.exe` (instalador do Windows, NSIS), além do `instalar.ps1` para Windows. Gerados pelo workflow `release.yml` a cada tag `v*`.

- Painel visual (`dotcrow painel`): chaves de API, testes, tarefas com cada etapa ao vivo, pergunta rápida, provedores e atividade. Funciona no celular com `--rede` e QR code, e pode ser instalado na tela de início.
- API JSON do painel (estado, chaves, provedores, testes, perguntas, tarefas e eventos ao vivo por SSE), com código de acesso, checagem de `Host` e CSP. Documentada em `docs/painel.md`.
- Adaptadores reais de LLM: Anthropic (Claude), OpenAI, Google Gemini e os compatíveis com a OpenAI (OpenRouter, DeepSeek, Groq e Ollama), com classificação de erros específica de cada provedor.
- `catalog` de provedores, `KeySource` para os adaptadores pegarem a chave do cofre só na hora e o erro `Misconfigured` (modelo ou endereço errado), que também troca de provedor.
- Orquestrador dos corvos com LLM real: Huginn planeja, Vigia revisa e pode bloquear, Pena/Gralha/Bico entregam, Vigia confere e Pena resume. Ainda sem executar nada no computador.
- Comandos `testar`, `perguntar`, `tarefa`, `provedores` (ordem, modelo, url) e `painel`. Apelidos como `claude`, `gpt` e `google` viram o id certo.
- `instalar.sh`: instala Rust, compilador e o DotCrow com um comando.
- Documentação inicial: README, guia de contribuição, código de conduta, política de segurança, governança e roadmap.
- `dotcrow-crypto`: envelope híbrido ML-KEM-768 + X25519 com XChaCha20-Poly1305 e cifra simétrica para dados em repouso.
- `dotcrow-vault`: cofre de chaves de API somente-escrita (adicionar, listar, apagar), cifrado em disco.
- `dotcrow-providers`: interface comum de LLM e roteador com troca automática de provedor.
- `dotcrow-core`: permissões, níveis de risco e política de confirmação.
- `dotcrow-agents`: os seis agentes (Huginn, Muninn, Gralha, Bico, Pena e Vigia) e o fluxo padrão.
- `dotcrow-cli`: comandos `chaves`, `agentes`, `fluxo` e `demo troca`.
- Identidade visual (logo, favicon, ícone e capa do repositório).
