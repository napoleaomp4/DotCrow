# DotCrow no Chrome

Extensão que deixa os corvos do DotCrow usarem o **seu** Chrome (com as contas que você já usa):
abrir páginas, ler, clicar com o mouse, digitar e tirar capturas de tela.

## Instalar

1. No terminal, rode `dotcrow extensao`. Ele grava a extensão em `~/DotCrow/extensao-chrome` e mostra o
   código de pareamento. Também dá para fazer pelo painel, em **Extensão do Chrome → Instalar**.
2. No Chrome, abra `chrome://extensions`.
3. Ligue o **Modo do desenvolvedor** (canto superior direito).
4. Clique em **Carregar sem compactação** e escolha a pasta `~/DotCrow/extensao-chrome`.
5. Fixe o ícone do DotCrow na barra (ícone de quebra-cabeça → alfinete).

## Conectar

1. Deixe o painel aberto: `dotcrow painel`.
2. Clique no ícone do DotCrow no Chrome.
3. Cole o código do painel (ex.: `K7QF-29XM`) e clique em **Conectar**. O ícone mostra **ON** em vermelho.

O código fica guardado: depois disso a extensão se conecta sozinha sempre que o painel estiver aberto.

## O que o corvo pode fazer

- Ele só mexe nas abas do grupo vermelho **DotCrow**: as que ele mesmo abre e as que você entregar.
  Para entregar, abra a aba, clique no ícone do DotCrow e em **Entregar esta aba ao DotCrow**.
- Um **cursor vermelho** mostra na página onde ele vai clicar. Enquanto ele usa uma aba, o Chrome mostra
  o aviso "DotCrow no Chrome começou a depurar este navegador". Esse aviso é do próprio Chrome e é
  esperado.
- Digitar em campo de **senha ou cartão** sempre pede a sua confirmação no painel.
- **Pausar**, no ícone, para tudo na hora. **Desconectar** apaga o código guardado.

## Segurança

- A extensão só fala com o painel neste computador (`127.0.0.1`) e só com o código certo.
- O painel só aceita conexão de extensões do Chrome, nunca de sites.
- Para trocar o código, use o painel: **Extensão do Chrome → Trocar código**. A conexão atual cai.
