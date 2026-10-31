// DotCrow no Chrome · janela.js — a janelinha do ícone: conectar, pausar e entregar abas.
"use strict";

const $ = (id) => document.getElementById(id);
const NOMES = { conectada: "Conectada ao painel", conectando: "Conectando…", desconectada: "Desconectada", recusada: "Código recusado" };
let abaAtiva = null;

async function atualizar() {
  const s = await chrome.runtime.sendMessage({ tipo: "status" });
  const { endereco, codigo } = await chrome.storage.local.get(["endereco", "codigo"]);
  $("ponto").dataset.e = s.estado;
  $("estado").textContent = s.pausado ? "Pausada pelo dono" : NOMES[s.estado] || s.estado;
  $("motivo").textContent = s.pausado ? "o corvo não age no Chrome até você retomar" : (s.motivo || (s.estado === "conectada" ? `${s.abas} aba(s) do DotCrow` : ""));
  if (document.activeElement !== $("codigo") && codigo) $("codigo").value = codigo.length === 8 ? `${codigo.slice(0, 4)}-${codigo.slice(4)}` : codigo;
  if (document.activeElement !== $("endereco")) $("endereco").value = endereco || "http://127.0.0.1:7777";
  $("conectar").textContent = s.estado === "conectada" ? "Reconectar" : "Conectar";
  $("ajuda").hidden = s.estado === "conectada";
  $("pausar").textContent = s.pausado ? "Retomar" : "Pausar";
  $("pausar").dataset.pausado = s.pausado ? "1" : "";
  abaAtiva = s.abaAtiva;
  const ok = abaAtiva && /^(https?|file):/.test(abaAtiva.url || "");
  $("aba").hidden = !ok;
  if (ok) {
    $("abaTitulo").textContent = abaAtiva.titulo || abaAtiva.url;
    $("entregar").hidden = abaAtiva.doCorvo;
    $("retomar").hidden = !abaAtiva.doCorvo;
  }
}

$("form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const codigo = $("codigo").value.replace(/[^a-z0-9]/gi, "").toUpperCase();
  if (codigo.length !== 8) { $("motivo").textContent = "o código tem 8 letras e números (ex.: K7QF-29XM)"; return; }
  const endereco = $("endereco").value.trim() || "http://127.0.0.1:7777";
  if (!/^https?:\/\/(127\.0\.0\.1|localhost|\[::1\])(:\d+)?\/?$/.test(endereco)) {
    $("motivo").textContent = "por segurança, o painel precisa estar neste computador (127.0.0.1)";
    return;
  }
  await chrome.runtime.sendMessage({ tipo: "conectar", codigo, endereco });
  setTimeout(atualizar, 600);
});
$("desconectar").addEventListener("click", async () => { await chrome.runtime.sendMessage({ tipo: "desconectar" }); $("codigo").value = ""; atualizar(); });
$("pausar").addEventListener("click", async () => { await chrome.runtime.sendMessage({ tipo: "pausar", pausado: !$("pausar").dataset.pausado }); atualizar(); });
$("entregar").addEventListener("click", async () => { if (abaAtiva) await chrome.runtime.sendMessage({ tipo: "entregar", aba: abaAtiva.id }); atualizar(); });
$("retomar").addEventListener("click", async () => { if (abaAtiva) await chrome.runtime.sendMessage({ tipo: "retomar", aba: abaAtiva.id }); atualizar(); });
chrome.runtime.onMessage.addListener((m) => { if (m.tipo === "estado") atualizar(); });
atualizar();
