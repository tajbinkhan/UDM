// Pullbay's remote-control page. It holds no data of its own: everything
// comes from /api/, with this phone's credential (kept in this browser's
// storage for this address only) in the Authorization header. Text is in
// the phone's language when the page has it (English, German, Spanish,
// French), else English.
"use strict";

const TEXT = {
  en: {
    pair_title: "Pair this phone",
    pair_help: "On the computer, open Pullbay's Options, choose Remote control and Pair a phone. Scan the QR code with this phone's camera, or type the code shown here.",
    code_label: "Pairing code",
    pair_button: "Pair",
    pairing: "Pairing…",
    add_label: "Add a link",
    add_button: "Add",
    added: "Added.",
    empty: "No downloads yet.",
    unpair: "Unpair this phone",
    unpair_confirm: "Unpair this phone? You will need a new code from the computer to use it again.",
    pause: "Pause",
    resume: "Resume",
    remove: "Remove",
    cancel: "Cancel",
    remove_question: "Remove “{name}” from the list?",
    remove_files: "Also delete the file",
    connected: "Connected",
    offline: "Cannot reach Pullbay",
    queued: "Queued", downloading: "Downloading", done: "Finished", failed: "Failed", canceled: "Stopped",
    interrupted: "Interrupted", confirming: "Waiting for the save dialog", waiting: "Waiting",
    wrong_code: "That pairing code is wrong or has run out. Show a new one on the computer.",
    too_many_tries: "Too many wrong tries. Try again in {wait} seconds.",
    not_paired: "This phone is not paired with Pullbay, or was removed. Pair it again.",
    too_many_devices: "Too many devices are paired. Remove one in Pullbay's Options first.",
    not_a_link: "That is not an http, https or ftp link.",
    no_download: "That download is no longer in the list.",
    not_running: "That download is not running.",
    daemon: "Pullbay did not answer. Is it still running on the computer?",
    off: "Remote control is turned off on the computer.",
    other: "That did not work ({error}).",
  },
  de: {
    pair_title: "Dieses Telefon koppeln",
    pair_help: "Öffnen Sie am Computer die Optionen von Pullbay, wählen Sie Fernsteuerung und Telefon koppeln. Scannen Sie den QR-Code mit der Kamera dieses Telefons oder geben Sie den angezeigten Code hier ein.",
    code_label: "Kopplungscode",
    pair_button: "Koppeln",
    pairing: "Wird gekoppelt…",
    add_label: "Link hinzufügen",
    add_button: "Hinzufügen",
    added: "Hinzugefügt.",
    empty: "Noch keine Downloads.",
    unpair: "Kopplung dieses Telefons aufheben",
    unpair_confirm: "Kopplung dieses Telefons aufheben? Um es wieder zu nutzen, brauchen Sie einen neuen Code vom Computer.",
    pause: "Anhalten",
    resume: "Fortsetzen",
    remove: "Entfernen",
    cancel: "Abbrechen",
    remove_question: "„{name}“ aus der Liste entfernen?",
    remove_files: "Auch die Datei löschen",
    connected: "Verbunden",
    offline: "Pullbay ist nicht erreichbar",
    queued: "In Warteschlange", downloading: "Wird geladen", done: "Fertig", failed: "Fehlgeschlagen", canceled: "Angehalten",
    interrupted: "Unterbrochen", confirming: "Wartet auf den Speichern-Dialog", waiting: "Wartet",
    wrong_code: "Dieser Kopplungscode ist falsch oder abgelaufen. Lassen Sie am Computer einen neuen anzeigen.",
    too_many_tries: "Zu viele Fehlversuche. Versuchen Sie es in {wait} Sekunden erneut.",
    not_paired: "Dieses Telefon ist nicht mit Pullbay gekoppelt oder wurde entfernt. Koppeln Sie es erneut.",
    too_many_devices: "Es sind zu viele Geräte gekoppelt. Entfernen Sie zuerst eines in den Optionen von Pullbay.",
    not_a_link: "Das ist kein http-, https- oder ftp-Link.",
    no_download: "Dieser Download ist nicht mehr in der Liste.",
    not_running: "Dieser Download läuft nicht.",
    daemon: "Pullbay hat nicht geantwortet. Läuft es noch auf dem Computer?",
    off: "Die Fernsteuerung ist am Computer ausgeschaltet.",
    other: "Das hat nicht geklappt ({error}).",
  },
  es: {
    pair_title: "Vincular este teléfono",
    pair_help: "En el ordenador, abra las Opciones de Pullbay, elija Control remoto y Vincular un teléfono. Escanee el código QR con la cámara de este teléfono o escriba aquí el código que se muestra.",
    code_label: "Código de vinculación",
    pair_button: "Vincular",
    pairing: "Vinculando…",
    add_label: "Añadir un enlace",
    add_button: "Añadir",
    added: "Añadido.",
    empty: "Aún no hay descargas.",
    unpair: "Desvincular este teléfono",
    unpair_confirm: "¿Desvincular este teléfono? Necesitará un código nuevo del ordenador para volver a usarlo.",
    pause: "Pausar",
    resume: "Reanudar",
    remove: "Quitar",
    cancel: "Cancelar",
    remove_question: "¿Quitar «{name}» de la lista?",
    remove_files: "Borrar también el archivo",
    connected: "Conectado",
    offline: "No se puede conectar con Pullbay",
    queued: "En cola", downloading: "Descargando", done: "Terminada", failed: "Fallida", canceled: "Detenida",
    interrupted: "Interrumpida", confirming: "Esperando el diálogo de guardar", waiting: "En espera",
    wrong_code: "Ese código de vinculación es incorrecto o ha caducado. Muestre uno nuevo en el ordenador.",
    too_many_tries: "Demasiados intentos fallidos. Vuelva a intentarlo dentro de {wait} segundos.",
    not_paired: "Este teléfono no está vinculado con Pullbay o se quitó. Vincúlelo de nuevo.",
    too_many_devices: "Hay demasiados dispositivos vinculados. Quite uno en las Opciones de Pullbay primero.",
    not_a_link: "Eso no es un enlace http, https o ftp.",
    no_download: "Esa descarga ya no está en la lista.",
    not_running: "Esa descarga no está en curso.",
    daemon: "Pullbay no respondió. ¿Sigue ejecutándose en el ordenador?",
    off: "El control remoto está desactivado en el ordenador.",
    other: "No funcionó ({error}).",
  },
  fr: {
    pair_title: "Associer ce téléphone",
    pair_help: "Sur l’ordinateur, ouvrez les Options de Pullbay, choisissez Contrôle à distance puis Associer un téléphone. Scannez le code QR avec l’appareil photo de ce téléphone, ou saisissez ici le code affiché.",
    code_label: "Code d’association",
    pair_button: "Associer",
    pairing: "Association…",
    add_label: "Ajouter un lien",
    add_button: "Ajouter",
    added: "Ajouté.",
    empty: "Aucun téléchargement pour l’instant.",
    unpair: "Dissocier ce téléphone",
    unpair_confirm: "Dissocier ce téléphone ? Il vous faudra un nouveau code de l’ordinateur pour l’utiliser à nouveau.",
    pause: "Suspendre",
    resume: "Reprendre",
    remove: "Retirer",
    cancel: "Annuler",
    remove_question: "Retirer « {name} » de la liste ?",
    remove_files: "Supprimer aussi le fichier",
    connected: "Connecté",
    offline: "Impossible de joindre Pullbay",
    queued: "En file", downloading: "En cours", done: "Terminé", failed: "Échec", canceled: "Arrêté",
    interrupted: "Interrompu", confirming: "En attente de la boîte d’enregistrement", waiting: "En attente",
    wrong_code: "Ce code d’association est faux ou a expiré. Affichez-en un nouveau sur l’ordinateur.",
    too_many_tries: "Trop d’essais erronés. Réessayez dans {wait} secondes.",
    not_paired: "Ce téléphone n’est pas associé à Pullbay ou a été retiré. Associez-le à nouveau.",
    too_many_devices: "Trop d’appareils sont associés. Retirez-en un dans les Options de Pullbay d’abord.",
    not_a_link: "Ce n’est pas un lien http, https ou ftp.",
    no_download: "Ce téléchargement n’est plus dans la liste.",
    not_running: "Ce téléchargement n’est pas en cours.",
    daemon: "Pullbay n’a pas répondu. Est-il toujours lancé sur l’ordinateur ?",
    off: "Le contrôle à distance est désactivé sur l’ordinateur.",
    other: "Cela n’a pas marché ({error}).",
  },
};

const LANG = (() => {
  for (const tag of navigator.languages || [navigator.language || "en"]) {
    const code = String(tag).toLowerCase().split("-")[0];
    if (TEXT[code]) return code;
  }
  return "en";
})();

function t(key, values) {
  let text = TEXT[LANG][key] || TEXT.en[key] || key;
  for (const [name, value] of Object.entries(values || {})) text = text.split("{" + name + "}").join(String(value));
  return text;
}

const KEY = "pullbay.credential";
const $ = (id) => document.getElementById(id);

function credential() {
  try { return localStorage.getItem(KEY); } catch (_) { return null; }
}

function keep(value) {
  try {
    if (value) localStorage.setItem(KEY, value); else localStorage.removeItem(KEY);
  } catch (_) { /* private mode: paired until the tab closes */ }
  memory = value;
}

let memory = null;

class Refused extends Error {
  constructor(status, body) {
    super(body.error || String(status));
    this.status = status;
    this.body = body;
  }
}

async function api(method, path, body) {
  const headers = {};
  const cred = memory || credential();
  if (cred) headers["Authorization"] = "Bearer " + cred;
  if (body !== undefined) headers["Content-Type"] = "application/json";
  const response = await fetch(path, {
    method, headers, body: body === undefined ? undefined : JSON.stringify(body),
    cache: "no-store", credentials: "omit", redirect: "error",
  });
  let data = {};
  try { data = await response.json(); } catch (_) { data = {}; }
  if (!response.ok) throw new Refused(response.status, data);
  return data;
}

function explain(error) {
  if (error instanceof Refused) {
    const kind = error.body.error;
    if (kind === "too_many_tries") return t("too_many_tries", { wait: error.body.wait || 60 });
    if (kind === "refused" && error.body.message) return error.body.message;
    if (TEXT.en[kind]) return t(kind);
    return t("other", { error: kind || error.status });
  }
  return t("offline");
}

function say(text) {
  const box = $("message");
  box.textContent = text || "";
  box.hidden = !text;
}

function show(which) {
  $("pair").hidden = which !== "pair";
  $("list").hidden = which !== "list";
}

function deviceName() {
  const ua = navigator.userAgent;
  if (/iPhone/.test(ua)) return "iPhone";
  if (/iPad/.test(ua)) return "iPad";
  if (/Android/.test(ua)) return "Android";
  if (/Windows/.test(ua)) return "Windows";
  if (/Macintosh/.test(ua)) return "Mac";
  if (/Linux/.test(ua)) return "Linux";
  return "Browser";
}

async function pair(code) {
  say(t("pairing"));
  try {
    const answer = await api("POST", "/api/pair", { code, name: deviceName() });
    keep(answer.credential);
    say("");
    start();
  } catch (e) {
    say(explain(e));
    show("pair");
  }
}

function unpaired(error) {
  keep(null);
  stop();
  show("pair");
  say(explain(error));
}

// The list.
let timer = null;
const last = new Map();

function stop() {
  if (timer) clearTimeout(timer);
  timer = null;
}

function start() {
  show("list");
  stop();
  refresh();
}

function size(bytes) {
  if (bytes == null) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let n = bytes, i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return (i === 0 ? n : n.toFixed(1)) + " " + units[i];
}

function button(label, action, extra) {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = t(label);
  if (extra) b.className = extra;
  b.addEventListener("click", async () => {
    b.disabled = true;
    try { await action(); say(""); } catch (e) {
      if (e instanceof Refused && e.status === 401) return unpaired(e);
      say(explain(e));
    }
    refresh();
  });
  return b;
}

function draw(downloads) {
  const list = $("downloads");
  list.replaceChildren();
  $("empty").hidden = downloads.length > 0;
  const now = Date.now();
  for (const d of downloads.slice().reverse()) {
    const li = document.createElement("li");
    const name = document.createElement("div");
    name.className = "name";
    name.textContent = d.name;
    const meta = document.createElement("div");
    meta.className = "meta";
    const parts = [t(d.status), d.host];
    const before = last.get(d.id);
    if (d.status === "downloading" && before && now > before.at) {
      const speed = Math.max(0, (d.done - before.done) * 1000 / (now - before.at));
      parts.push(size(Math.round(speed)) + "/s");
    }
    parts.push(d.total ? size(d.done) + " / " + size(d.total) : size(d.done));
    last.set(d.id, { done: d.done, at: now });
    meta.textContent = parts.filter(Boolean).join(" · ");
    li.append(name, meta);
    const bar = document.createElement("div");
    bar.className = "bar";
    const fill = document.createElement("div");
    const share = d.status === "done" ? 1 : d.total ? Math.min(1, d.done / d.total) : 0;
    fill.style.width = (share * 100).toFixed(1) + "%";
    bar.append(fill);
    li.append(bar);
    if (d.error) {
      const error = document.createElement("div");
      error.className = "error";
      error.textContent = d.error;
      li.append(error);
    }
    const actions = document.createElement("div");
    actions.className = "actions";
    const running = ["queued", "downloading", "confirming"].includes(d.status);
    if (running) actions.append(button("pause", () => api("POST", "/api/pause", { id: d.id })));
    if (["failed", "canceled", "interrupted", "waiting", "confirming"].includes(d.status)) {
      actions.append(button("resume", () => api("POST", "/api/resume", { id: d.id }), "primary"));
    }
    if (!running || d.status === "confirming") actions.append(button("remove", () => removing(d), "danger"));
    li.append(actions);
    list.append(li);
  }
}

function removing(d) {
  const dialog = $("remove-dialog");
  $("remove-question").textContent = t("remove_question", { name: d.name });
  $("remove-files").checked = false;
  return new Promise((resolve, reject) => {
    dialog.addEventListener("close", () => {
      if (dialog.returnValue !== "remove") return resolve();
      api("POST", "/api/remove", { id: d.id, delete_files: $("remove-files").checked }).then(resolve, reject);
    }, { once: true });
    dialog.returnValue = "";
    dialog.showModal();
  });
}

async function refresh() {
  stop();
  try {
    const answer = await api("GET", "/api/downloads");
    $("state").textContent = t("connected");
    draw(answer.downloads || []);
  } catch (e) {
    if (e instanceof Refused && e.status === 401) return unpaired(e);
    $("state").textContent = t("offline");
  }
  timer = setTimeout(refresh, document.hidden ? 10000 : 2000);
}

document.addEventListener("visibilitychange", () => {
  if (!document.hidden && !$("list").hidden) refresh();
});

function init() {
  document.documentElement.lang = LANG;
  for (const el of document.querySelectorAll("[data-t]")) el.textContent = t(el.dataset.t);
  memory = credential();

  $("pair-form").addEventListener("submit", (event) => {
    event.preventDefault();
    pair($("code").value);
  });
  $("add-form").addEventListener("submit", async (event) => {
    event.preventDefault();
    const field = $("url");
    try {
      await api("POST", "/api/add", { url: field.value.trim() });
      field.value = "";
      say(t("added"));
    } catch (e) {
      if (e instanceof Refused && e.status === 401) return unpaired(e);
      say(explain(e));
    }
    refresh();
  });
  $("unpair").addEventListener("click", async () => {
    if (!confirm(t("unpair_confirm"))) return;
    try { await api("POST", "/api/unpair", {}); } catch (_) { /* gone either way */ }
    keep(null);
    stop();
    show("pair");
  });

  // The QR code's link carries the code after #, which the browser never
  // sends to a server; take it out of the address bar and history at once.
  const hash = new URLSearchParams(location.hash.slice(1));
  const code = hash.get("pair");
  if (location.hash) history.replaceState(null, "", location.pathname);
  if (code) return pair(code);
  if (memory) return start();
  show("pair");
}

init();
