// DotCrow no Chrome · fundo.js — a ponte entre os corvos (painel do DotCrow) e este navegador.
//
// Conecta por WebSocket em ws://127.0.0.1:PORTA/extensao com o código de pareamento que o
// dono copiou do painel. Recebe pedidos ("ir", "ler", "clicar"...), executa só nas abas do
// grupo "DotCrow" e devolve o resultado. Os cliques e as teclas são eventos de verdade
// (chrome.debugger), e um cursor vermelho mostra na página onde o corvo vai clicar.
"use strict";

const VERSAO = chrome.runtime.getManifest().version;
const ENDERECO_PADRAO = "http://127.0.0.1:7777";
const GRUPO = "DotCrow";

let ws = null;
let estado = "desconectada"; // desconectada | conectando | conectada | recusada
let motivo = "";
let espera = 1000;
let relogioPing = null;
const depurando = new Set();

// ---------------------------------------------------------------- guarda
const guarda = {
  async ler(chaves) { return chrome.storage.local.get(chaves); },
  async abas() { return (await chrome.storage.session.get("abas")).abas || []; },
  async salvarAbas(lista) { await chrome.storage.session.set({ abas: [...new Set(lista)] }); },
  async abaAtual() { return (await chrome.storage.session.get("atual")).atual || null; },
  async definirAtual(id) { await chrome.storage.session.set({ atual: id }); },
};

// ---------------------------------------------------------------- selo no ícone
function selo() {
  chrome.storage.local.get("pausado").then(({ pausado }) => {
    const [txt, cor] = pausado ? ["❚❚", "#64748b"] : estado === "conectada" ? ["ON", "#e11d2f"] : ["", "#e11d2f"];
    chrome.action.setBadgeText({ text: txt });
    chrome.action.setBadgeBackgroundColor({ color: cor });
  });
  chrome.runtime.sendMessage({ tipo: "estado", estado, motivo }).catch(() => {});
}

// ---------------------------------------------------------------- conexão
async function conectar() {
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;
  const { endereco, codigo } = await guarda.ler(["endereco", "codigo"]);
  if (!codigo) { estado = "desconectada"; motivo = "falta o código do painel"; selo(); return; }
  const url = (endereco || ENDERECO_PADRAO).replace(/\/+$/, "").replace(/^http/, "ws") + "/extensao";
  estado = "conectando"; motivo = ""; selo();
  try { ws = new WebSocket(url); } catch (e) { agendar(String(e)); return; }
  ws.onopen = () => {
    const nav = (navigator.userAgent.match(/(Chrome|Edg|Brave)\/[\d.]+/) || ["Chrome"])[0];
    ws.send(JSON.stringify({ tipo: "ola", codigo, versao: VERSAO, navegador: nav }));
  };
  ws.onmessage = (ev) => receber(ev.data);
  ws.onclose = () => {
    if (estado !== "recusada") agendar("o painel do DotCrow não está aberto (rode: dotcrow painel)");
    ws = null;
    clearInterval(relogioPing);
    soltarTodas();
  };
  ws.onerror = () => {};
}

function agendar(porque) {
  estado = "desconectada"; motivo = porque; selo();
  setTimeout(conectar, espera);
  espera = Math.min(espera * 2, 30000);
}

function enviar(obj) {
  if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj));
}

async function receber(texto) {
  let m;
  try { m = JSON.parse(texto); } catch { return; }
  if (m.tipo === "bem_vindo") {
    estado = "conectada"; motivo = ""; espera = 1000; selo();
    clearInterval(relogioPing);
    // Um "ping" a cada 20 s mantém o service worker acordado enquanto está conectado.
    relogioPing = setInterval(() => enviar({ tipo: "ping" }), 20000);
    return;
  }
  if (m.tipo === "recusado") { estado = "recusada"; motivo = m.motivo || "código recusado"; selo(); ws && ws.close(); return; }
  if (m.tipo !== "pedido") return;
  const { pausado } = await guarda.ler("pausado");
  if (pausado) { enviar({ tipo: "resposta", id: m.id, ok: false, erro: "o dono pausou o DotCrow no Chrome (ícone do DotCrow)" }); return; }
  try {
    const resultado = await executar(m.acao, m.args || {});
    enviar({ tipo: "resposta", id: m.id, ok: true, resultado });
  } catch (e) {
    enviar({ tipo: "resposta", id: m.id, ok: false, erro: String(e && e.message ? e.message : e) });
  }
}

// ---------------------------------------------------------------- abas do DotCrow
async function grupoDotCrow(janelaId) {
  const grupos = await chrome.tabGroups.query({ title: GRUPO, windowId: janelaId });
  return grupos.length ? grupos[0].id : null;
}

async function entregar(abaId) {
  const aba = await chrome.tabs.get(abaId);
  let g = await grupoDotCrow(aba.windowId);
  if (g === null) {
    g = await chrome.tabs.group({ tabIds: [abaId], createProperties: { windowId: aba.windowId } });
    await chrome.tabGroups.update(g, { title: GRUPO, color: "red" });
  } else {
    await chrome.tabs.group({ tabIds: [abaId], groupId: g });
  }
  await guarda.salvarAbas([...(await guarda.abas()), abaId]);
  await guarda.definirAtual(abaId);
}

async function retomar(abaId) {
  await guarda.salvarAbas((await guarda.abas()).filter((id) => id !== abaId));
  try { await chrome.tabs.ungroup(abaId); } catch {}
  await soltar(abaId);
}

// A aba em que o corvo vai agir: a pedida (se for dele), a última usada, ou uma nova.
async function abaDoCorvo(args, criarSeFaltar) {
  const minhas = await guarda.abas();
  const vivas = [];
  for (const id of minhas) { try { await chrome.tabs.get(id); vivas.push(id); } catch {} }
  if (vivas.length !== minhas.length) await guarda.salvarAbas(vivas);
  if (args.aba !== undefined && args.aba !== null) {
    const id = Number(args.aba);
    if (!vivas.includes(id)) throw new Error(`a aba ${id} não é do DotCrow: use abas para ver as suas, ou nova_aba`);
    await guarda.definirAtual(id);
    return id;
  }
  const atual = await guarda.abaAtual();
  if (atual && vivas.includes(atual)) return atual;
  if (vivas.length) { await guarda.definirAtual(vivas[vivas.length - 1]); return vivas[vivas.length - 1]; }
  if (!criarSeFaltar) throw new Error("ainda não há aba do DotCrow: use nova_aba {url} ou ir {url}");
  const nova = await chrome.tabs.create({ url: "about:blank", active: true });
  await entregar(nova.id);
  return nova.id;
}

function esperarCarregar(abaId, maxMs = 20000) {
  return new Promise((ok) => {
    const fim = setTimeout(() => { chrome.tabs.onUpdated.removeListener(ouvir); ok(); }, maxMs);
    function ouvir(id, info) {
      if (id === abaId && info.status === "complete") {
        clearTimeout(fim); chrome.tabs.onUpdated.removeListener(ouvir); setTimeout(ok, 300);
      }
    }
    chrome.tabs.onUpdated.addListener(ouvir);
    chrome.tabs.get(abaId).then((t) => { if (t.status === "complete") { clearTimeout(fim); chrome.tabs.onUpdated.removeListener(ouvir); ok(); } });
  });
}

function urlValida(url) {
  if (!/^(https?:|file:)/i.test(String(url || ""))) throw new Error("a url precisa começar com http://, https:// ou file://");
  return url;
}

// ---------------------------------------------------------------- olhos (pagina.js) e mãos (debugger)
async function olhos(abaId, func, args = []) {
  const aba = await chrome.tabs.get(abaId);
  if (/^(chrome|edge|about|chrome-extension|devtools):/.test(aba.url || "") || /chromewebstore\.google\.com|chrome\.google\.com\/webstore/.test(aba.url || "")) {
    throw new Error(`o Chrome não deixa extensões mexerem nesta página (${aba.url}). Abra outra com ir {url}.`);
  }
  await chrome.scripting.executeScript({ target: { tabId: abaId }, files: ["pagina.js"] });
  const [r] = await chrome.scripting.executeScript({
    target: { tabId: abaId },
    func: (nome, a) => window.__dotcrow[nome](...a),
    args: [func, args],
  });
  return r ? r.result : null;
}

async function depurar(abaId) {
  if (depurando.has(abaId)) return;
  await chrome.debugger.attach({ tabId: abaId }, "1.3");
  depurando.add(abaId);
}
async function soltar(abaId) {
  if (!depurando.has(abaId)) return;
  depurando.delete(abaId);
  try { await chrome.debugger.detach({ tabId: abaId }); } catch {}
}
function soltarTodas() { for (const id of [...depurando]) soltar(id); }
chrome.debugger.onDetach.addListener((src) => depurando.delete(src.tabId));

async function cdp(abaId, metodo, params) {
  await depurar(abaId);
  return chrome.debugger.sendCommand({ tabId: abaId }, metodo, params);
}

async function clicarEm(abaId, x, y) {
  await olhos(abaId, "cursor", [x, y]);
  await new Promise((r) => setTimeout(r, 380));
  await cdp(abaId, "Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  const base = { x, y, button: "left", buttons: 1, clickCount: 1 };
  await cdp(abaId, "Input.dispatchMouseEvent", { ...base, type: "mousePressed" });
  await cdp(abaId, "Input.dispatchMouseEvent", { ...base, type: "mouseReleased" });
}

const TECLAS = {
  enter: ["Enter", "Enter", 13], return: ["Enter", "Enter", 13], tab: ["Tab", "Tab", 9], escape: ["Escape", "Escape", 27],
  esc: ["Escape", "Escape", 27], backspace: ["Backspace", "Backspace", 8], delete: ["Delete", "Delete", 46],
  space: [" ", "Space", 32], arrowup: ["ArrowUp", "ArrowUp", 38], arrowdown: ["ArrowDown", "ArrowDown", 40],
  arrowleft: ["ArrowLeft", "ArrowLeft", 37], arrowright: ["ArrowRight", "ArrowRight", 39], up: ["ArrowUp", "ArrowUp", 38],
  down: ["ArrowDown", "ArrowDown", 40], left: ["ArrowLeft", "ArrowLeft", 37], right: ["ArrowRight", "ArrowRight", 39],
  home: ["Home", "Home", 36], end: ["End", "End", 35], pageup: ["PageUp", "PageUp", 33], pagedown: ["PageDown", "PageDown", 34],
};

async function apertar(abaId, nome) {
  const t = TECLAS[String(nome || "").toLowerCase().replace(/\s+/g, "")];
  if (!t) throw new Error(`não conheço a tecla "${nome}" (use Enter, Tab, Escape, setas, Backspace...)`);
  const [key, code, vk] = t;
  await cdp(abaId, "Input.dispatchKeyEvent", { type: "keyDown", key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, text: key === "Enter" ? "\r" : undefined });
  await cdp(abaId, "Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk });
}

async function porNumero(abaId, n) {
  const a = await olhos(abaId, "alvo", [Number(n)]);
  if (!a) throw new Error(`o elemento [${n}] não existe mais: use ler de novo (a página mudou)`);
  return a;
}

// ---------------------------------------------------------------- as ações
async function executar(acao, a) {
  switch (String(acao || "").toLowerCase()) {
    case "abas": {
      const minhas = await guarda.abas();
      const atual = await guarda.abaAtual();
      const linhas = [];
      for (const id of minhas) {
        try { const t = await chrome.tabs.get(id); linhas.push(`${id === atual ? "▶" : " "} aba ${id}: ${t.title || "(sem título)"} — ${t.url}`); } catch {}
      }
      const todas = await chrome.tabs.query({});
      return { texto: (linhas.length ? `Abas do DotCrow:\n${linhas.join("\n")}` : "O DotCrow ainda não tem abas.")
        + `\n(O dono tem ${todas.length - linhas.length} outra(s) aba(s); o corvo só usa as do grupo DotCrow ou as que o dono entregar.)` };
    }
    case "nova_aba": {
      const url = a.url ? urlValida(a.url) : "about:blank";
      const t = await chrome.tabs.create({ url, active: true });
      await entregar(t.id);
      await esperarCarregar(t.id);
      const info = await chrome.tabs.get(t.id);
      return { texto: `Abri a aba ${t.id}: ${info.title || ""} — ${info.url}`, aba: t.id };
    }
    case "ir": case "abrir": case "navegar": {
      const url = urlValida(a.url);
      const id = await abaDoCorvo(a, true);
      await chrome.tabs.update(id, { url, active: true });
      await esperarCarregar(id);
      const info = await chrome.tabs.get(id);
      return { texto: `Abri ${info.url} na aba ${id} (${info.title || "sem título"}). Use ler para ver os elementos.`, aba: id };
    }
    case "ler": case "ver": case "olhar": {
      const id = await abaDoCorvo(a, false);
      return { texto: await olhos(id, "ler"), aba: id };
    }
    case "clicar": {
      const id = await abaDoCorvo(a, false);
      if (a.n !== undefined) {
        const alvo = await porNumero(id, a.n);
        await clicarEm(id, alvo.x, alvo.y);
        await new Promise((r) => setTimeout(r, 500));
        return { texto: `Cliquei em [${a.n}] ${alvo.tipo} "${alvo.rotulo}".` };
      }
      if (a.x !== undefined && a.y !== undefined) {
        await clicarEm(id, Number(a.x), Number(a.y));
        return { texto: `Cliquei em ${a.x},${a.y}.` };
      }
      throw new Error("clicar precisa de n (o número do elemento, veja ler) ou de x e y");
    }
    case "digitar": {
      if (typeof a.texto !== "string") throw new Error("faltou texto");
      const id = await abaDoCorvo(a, false);
      if (a.n !== undefined) {
        const info = await olhos(id, "focar", [Number(a.n), a.limpar !== false]);
        if (!info) throw new Error(`o elemento [${a.n}] não existe mais: use ler de novo`);
        if (info.sensivel && !a.confirmado) {
          return { precisa_confirmar: true, motivo: `digitar no campo sensível "${info.rotulo}" (senha ou cartão)` };
        }
      }
      await cdp(id, "Input.insertText", { text: a.texto });
      return { texto: `Digitei ${a.texto.length} caractere(s)${a.n !== undefined ? ` em [${a.n}]` : ""}.` };
    }
    case "tecla": case "apertar": {
      const id = await abaDoCorvo(a, false);
      await apertar(id, a.tecla);
      await new Promise((r) => setTimeout(r, 400));
      return { texto: `Apertei ${a.tecla}.` };
    }
    case "escolher": case "selecionar": {
      const id = await abaDoCorvo(a, false);
      const r = await olhos(id, "escolher", [Number(a.n), String(a.opcao || "")]);
      if (!r) throw new Error(`[${a.n}] não é um menu de opções: clique nele e escolha na lista`);
      if (r.erro) throw new Error(`${r.erro}. Opções: ${(r.opcoes || []).join(", ")}`);
      return { texto: `Escolhi "${r.escolhida}" no menu [${a.n}].` };
    }
    case "passar_mouse": case "passar": {
      const id = await abaDoCorvo(a, false);
      const p = a.n !== undefined ? await porNumero(id, a.n) : { x: Number(a.x), y: Number(a.y) };
      if (Number.isNaN(p.x)) throw new Error("passar_mouse precisa de n ou de x e y");
      await olhos(id, "cursor", [p.x, p.y]);
      await cdp(id, "Input.dispatchMouseEvent", { type: "mouseMoved", x: p.x, y: p.y });
      return { texto: `Passei o mouse por ${a.n !== undefined ? `[${a.n}]` : `${p.x},${p.y}`}.` };
    }
    case "rolar": {
      const id = await abaDoCorvo(a, false);
      const dy = Number(a.quantidade ?? 600);
      await cdp(id, "Input.dispatchMouseEvent", { type: "mouseWheel", x: 400, y: 300, deltaX: 0, deltaY: dy });
      await new Promise((r) => setTimeout(r, 350));
      return { texto: `Rolei ${dy} px.` };
    }
    case "voltar": {
      const id = await abaDoCorvo(a, false);
      await chrome.tabs.goBack(id);
      await esperarCarregar(id, 8000);
      return { texto: "Voltei uma página." };
    }
    case "esperar": {
      const s = Math.min(30, Math.max(1, Number(a.segundos || 2)));
      await new Promise((r) => setTimeout(r, s * 1000));
      return { texto: `Esperei ${s} s.` };
    }
    case "captura": case "tela": {
      const id = await abaDoCorvo(a, false);
      const t = await chrome.tabs.update(id, { active: true });
      await chrome.windows.update(t.windowId, { focused: true });
      await new Promise((r) => setTimeout(r, 250));
      const png = await chrome.tabs.captureVisibleTab(t.windowId, { format: "png" });
      return { png_base64: png.replace(/^data:image\/png;base64,/, "") };
    }
    case "fechar_aba": {
      const id = await abaDoCorvo(a, false);
      await soltar(id);
      await chrome.tabs.remove(id);
      await guarda.salvarAbas((await guarda.abas()).filter((x) => x !== id));
      return { texto: `Fechei a aba ${id}.` };
    }
    default:
      throw new Error(`ação "${acao}" não existe. Use: abas, nova_aba, ir, ler, clicar, digitar, tecla, escolher, passar_mouse, rolar, voltar, esperar, captura, fechar_aba.`);
  }
}

// ---------------------------------------------------------------- conversa com a janelinha (popup)
chrome.runtime.onMessage.addListener((m, _remetente, responder) => {
  (async () => {
    if (m.tipo === "status") {
      const [aba] = await chrome.tabs.query({ active: true, currentWindow: true });
      const minhas = await guarda.abas();
      responder({ estado, motivo, pausado: (await guarda.ler("pausado")).pausado || false, abas: minhas.length,
        abaAtiva: aba ? { id: aba.id, titulo: aba.title, url: aba.url, doCorvo: minhas.includes(aba.id) } : null });
    } else if (m.tipo === "conectar") {
      await chrome.storage.local.set({ endereco: m.endereco || ENDERECO_PADRAO, codigo: m.codigo });
      if (ws) ws.close();
      estado = "desconectada"; espera = 1000;
      await conectar();
      responder({ ok: true });
    } else if (m.tipo === "desconectar") {
      await chrome.storage.local.remove("codigo");
      if (ws) ws.close();
      estado = "desconectada"; motivo = "desconectada pelo dono"; selo();
      responder({ ok: true });
    } else if (m.tipo === "pausar") {
      await chrome.storage.local.set({ pausado: !!m.pausado });
      if (m.pausado) soltarTodas();
      selo();
      responder({ ok: true });
    } else if (m.tipo === "entregar") {
      await entregar(m.aba); responder({ ok: true });
    } else if (m.tipo === "retomar") {
      await retomar(m.aba); responder({ ok: true });
    }
  })();
  return true;
});

chrome.tabs.onRemoved.addListener(async (id) => {
  depurando.delete(id);
  await guarda.salvarAbas((await guarda.abas()).filter((x) => x !== id));
});

// Acorda e reconecta: ao abrir o Chrome, ao instalar e a cada 30 s (se tiver caído).
chrome.runtime.onStartup.addListener(conectar);
chrome.runtime.onInstalled.addListener(conectar);
chrome.alarms.create("manter", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((a) => { if (a.name === "manter") conectar(); });
conectar();
