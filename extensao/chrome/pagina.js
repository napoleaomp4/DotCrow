// DotCrow · pagina.js — os "olhos" do corvo numa página.
//
// Numera os elementos que dá para usar (links, botões, campos, menus) e descreve a
// página em texto curto, para o corvo decidir onde clicar pelo número, sem adivinhar
// coordenadas. O mesmo arquivo é usado em dois lugares:
//   - pela extensão do Chrome (chrome.scripting.executeScript, arquivo pagina.js);
//   - pelo navegador próprio do DotCrow (o Rust embute este arquivo e roda pelo CDP).
// Tudo fica em window.__dotcrow. Rodar de novo não duplica nada.
(() => {
  "use strict";
  if (window.__dotcrow && window.__dotcrow.versao === 2) return;

  const ALVOS = [
    "a[href]", "button", "input:not([type=hidden])", "select", "textarea", "summary", "label[for]",
    "[role=button]", "[role=link]", "[role=tab]", "[role=menuitem]", "[role=checkbox]", "[role=radio]",
    "[role=option]", "[role=switch]", "[role=combobox]", "[role=textbox]", "[role=searchbox]",
    "[contenteditable=''], [contenteditable=true]", "[onclick]", "[tabindex]:not([tabindex='-1'])",
  ].join(",");
  const MAX_ELEMENTOS = 160;
  const MAX_TEXTO = 5000;

  const limpa = (s, n) => {
    const t = String(s || "").replace(/\s+/g, " ").trim();
    return t.length > n ? t.slice(0, n - 1) + "…" : t;
  };

  function visivel(el) {
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) return false;
    const st = getComputedStyle(el);
    if (st.visibility === "hidden" || st.display === "none" || Number(st.opacity) === 0) return false;
    // A tela atual e mais duas telas abaixo: o corvo enxerga o que está perto.
    return r.bottom > 0 && r.right > 0 && r.left < innerWidth && r.top < innerHeight * 3;
  }

  // Campos onde o corvo não digita sem a confirmação do dono: senhas e dados de cartão.
  function sensivel(el) {
    const tipo = (el.getAttribute("type") || "").toLowerCase();
    const auto = (el.getAttribute("autocomplete") || "").toLowerCase();
    const nome = `${el.name || ""} ${el.id || ""}`.toLowerCase();
    return tipo === "password" || auto.startsWith("cc-") || auto === "current-password" || auto === "new-password"
      || /(senha|password|passwd|cvv|cvc|card.?number|numero.?cartao|pin\b)/.test(nome);
  }

  function rotulo(el) {
    const aria = el.getAttribute("aria-label") || el.getAttribute("aria-labelledby") && (
      el.getAttribute("aria-labelledby").split(/\s+/).map((id) => document.getElementById(id)?.innerText || "").join(" "));
    if (aria) return limpa(aria, 90);
    if (el.labels && el.labels.length) return limpa([...el.labels].map((l) => l.innerText).join(" "), 90);
    const ph = el.getAttribute("placeholder") || el.getAttribute("alt") || el.getAttribute("title");
    const texto = el.innerText || el.value || "";
    if (texto && el.tagName !== "INPUT" && el.tagName !== "TEXTAREA" && el.tagName !== "SELECT") return limpa(texto, 90);
    if (ph) return limpa(ph, 90);
    const img = el.querySelector && el.querySelector("img[alt]");
    if (img) return limpa(img.alt, 90);
    return limpa(el.name || el.id || "", 60);
  }

  function tipo(el) {
    const t = el.tagName.toLowerCase();
    const role = el.getAttribute("role");
    if (t === "a" || role === "link") return "link";
    if (t === "button" || role === "button" || t === "summary") return "botão";
    if (t === "select" || role === "combobox" || role === "option") return "menu";
    if (t === "textarea" || el.isContentEditable || role === "textbox" || role === "searchbox") return "campo de texto";
    if (t === "input") {
      const it = (el.getAttribute("type") || "text").toLowerCase();
      if (it === "checkbox" || role === "checkbox") return el.checked ? "caixa marcada" : "caixa";
      if (it === "radio") return el.checked ? "opção marcada" : "opção";
      if (it === "submit" || it === "button" || it === "image" || it === "reset") return "botão";
      if (it === "file") return "enviar arquivo";
      return `campo ${it}`;
    }
    if (role) return role;
    return "clicável";
  }

  function valor(el) {
    if (sensivel(el)) return el.value ? "(preenchido)" : "";
    if (el.tagName === "SELECT") return limpa(el.options[el.selectedIndex]?.text || "", 60);
    if (el.tagName === "INPUT" || el.tagName === "TEXTAREA") return limpa(el.value, 60);
    return "";
  }

  // Numera os elementos usáveis e devolve a página descrita em texto.
  function ler() {
    document.querySelectorAll("[data-dotcrow-n]").forEach((e) => e.removeAttribute("data-dotcrow-n"));
    const vistos = new Set();
    const linhas = [];
    let n = 0;
    for (const el of document.querySelectorAll(ALVOS)) {
      if (n >= MAX_ELEMENTOS) break;
      if (vistos.has(el) || !visivel(el) || el.disabled) continue;
      // Um botão dentro de um link (ou o contrário) conta uma vez só.
      if (el.parentElement && el.parentElement.closest("[data-dotcrow-n]") && !/^(INPUT|SELECT|TEXTAREA)$/.test(el.tagName)) continue;
      vistos.add(el);
      n += 1;
      el.setAttribute("data-dotcrow-n", String(n));
      const r = el.getBoundingClientRect();
      const fora = r.top >= innerHeight ? " (abaixo, role para ver)" : "";
      const v = valor(el);
      const href = el.tagName === "A" && el.getAttribute("href") && !el.getAttribute("href").startsWith("javascript:")
        ? ` → ${limpa(el.href, 80)}` : "";
      const s = sensivel(el) ? " ⚠ sensível" : "";
      linhas.push(`[${n}] ${tipo(el)} "${rotulo(el)}"${v ? ` = "${v}"` : ""}${href}${s}${fora}`);
    }
    const maxY = Math.max(0, document.documentElement.scrollHeight - innerHeight);
    const pct = maxY ? Math.round((scrollY / maxY) * 100) : 100;
    const corpo = limpa(document.body ? document.body.innerText : "", MAX_TEXTO);
    return [
      `Página: ${limpa(document.title, 120)} — ${location.href}`,
      `Rolagem: ${pct}% (${Math.round(scrollY)} de ${maxY} px) · janela ${innerWidth}×${innerHeight}`,
      "",
      linhas.length ? `Elementos que dá para usar (aja pelo número n):\n${linhas.join("\n")}` : "Nenhum elemento clicável visível.",
      "",
      `Texto da página:\n${corpo}`,
    ].join("\n");
  }

  // Centro do elemento n, já rolado para a vista. Devolve null se o número não existe mais.
  function alvo(n) {
    const el = document.querySelector(`[data-dotcrow-n="${Number(n)}"]`);
    if (!el) return null;
    el.scrollIntoView({ block: "center", inline: "center", behavior: "instant" });
    const r = el.getBoundingClientRect();
    return {
      x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2),
      sensivel: sensivel(el), tipo: tipo(el), rotulo: rotulo(el),
    };
  }

  // Põe o foco no elemento n (para digitar) e seleciona o que já estava escrito.
  function focar(n, limpar) {
    const el = document.querySelector(`[data-dotcrow-n="${Number(n)}"]`);
    if (!el) return null;
    el.scrollIntoView({ block: "center", behavior: "instant" });
    el.focus();
    if (limpar && "value" in el) { el.select && el.select(); }
    return { sensivel: sensivel(el), tipo: tipo(el), rotulo: rotulo(el) };
  }

  // Escolhe uma opção de um <select> pelo texto (ou valor) — clicar em menus nativos não abre a lista.
  function escolher(n, opcao) {
    const el = document.querySelector(`[data-dotcrow-n="${Number(n)}"]`);
    if (!el || el.tagName !== "SELECT") return null;
    const alvoTxt = String(opcao).toLowerCase().trim();
    const op = [...el.options].find((o) => o.text.toLowerCase().trim() === alvoTxt || o.value === opcao)
      || [...el.options].find((o) => o.text.toLowerCase().includes(alvoTxt));
    if (!op) return { erro: `não achei a opção "${opcao}"`, opcoes: [...el.options].map((o) => o.text).slice(0, 30) };
    el.value = op.value;
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
    return { escolhida: op.text };
  }

  // Um cursor visível (vermelho) que mostra ao dono onde o corvo vai clicar.
  function cursor(x, y) {
    let c = document.getElementById("__dotcrow_cursor");
    if (!c) {
      c = document.createElement("div");
      c.id = "__dotcrow_cursor";
      c.setAttribute("aria-hidden", "true");
      c.style.cssText = "position:fixed;z-index:2147483647;pointer-events:none;width:22px;height:22px;"
        + "margin:-3px 0 0 -3px;transition:left .35s ease,top .35s ease;left:-40px;top:-40px;"
        + "filter:drop-shadow(0 1px 2px rgba(0,0,0,.5))";
      c.innerHTML = '<svg viewBox="0 0 24 24" width="22" height="22"><path d="M3 2l16 9-7 2-3 7z" fill="#e11d2f" stroke="#fff" stroke-width="1.5"/></svg>';
      (document.body || document.documentElement).appendChild(c);
    }
    c.style.left = `${x}px`;
    c.style.top = `${y}px`;
    return true;
  }

  window.__dotcrow = { versao: 2, ler, alvo, focar, escolher, cursor };
})();
