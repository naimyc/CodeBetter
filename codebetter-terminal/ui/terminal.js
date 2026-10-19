"use strict";

const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);
const crt = $("crt");
const output = $("output");
const inputLine = $("input-line");
const promptEl = $("prompt");
const beforeEl = $("before");
const cursorEl = $("cursor");
const afterEl = $("after");
const busyEl = $("busy");

// UI strings for the terminal and CBEditor; everything else is translated by the backend.
const STRINGS = {
  de: {
    working: "arbeite",
    save: "Speichern",
    exit: "Beenden",
    indent: "Einrücken",
    check: "Prüfen",
    hint: "Tipp",
    panel: "Panel",
    learn: "LERNEN",
    yourTask: "DEINE AUFGABE",
    part: (n, m, title) => `Lektion ${n}/${m} · ${title}`,
    hintsHeader: (n, m) => `TIPPS ${n}/${m}`,
    noHints: "Für diese Lektion gibt es keine Tipps.",
    moreHints: "F2 deckt den nächsten Tipp auf.",
    allHints: "Alle Tipps aufgedeckt. Du schaffst das.",
    checkHeader: "PRÜFUNG",
    checkIdle: "F5 speichert und prüft deine Lösung gegen die Tests.",
    checking: "prüfe",
    noTask: "Diese Datei gehört zu keinem Kurs.\n\nÖffne eine Datei in ~/Tasks.cb/<kurs>/, um hier Lektion und Tipps zu sehen.",
    modified: "Geändert",
    newFile: "[ Neue Datei ]",
    read: (n) => `[ ${n} Zeilen gelesen ]`,
    wrote: (n) => `[ ${n} Zeilen geschrieben ]`,
    writeError: (e) => `[ Fehler beim Schreiben: ${e} ]`,
    askSave: "Geänderten Puffer speichern?  J = ja   N = nein   Esc = abbrechen",
    cancelled: "[ Abgebrochen ]",
    pos: (l, c) => `Zeile ${l}, Spalte ${c}`,
    internalError: "interner Fehler",
    boot: [
      "CB-BIOS v0.1  (C) 1987 CodeBetter Systems",
      "Speichertest ...... 640K OK",
      "Suche Gehirn ...... gefunden",
      "Autopilot (KI) .... deaktiviert",
      "Mounte Home ....... ok",
    ],
  },
  en: {
    working: "working",
    save: "Save",
    exit: "Exit",
    indent: "Indent",
    check: "Check",
    hint: "Hint",
    panel: "Panel",
    learn: "LEARN",
    yourTask: "YOUR TASK",
    part: (n, m, title) => `Lesson ${n}/${m} · ${title}`,
    hintsHeader: (n, m) => `HINTS ${n}/${m}`,
    noHints: "No hints for this lesson.",
    moreHints: "F2 reveals the next hint.",
    allHints: "All hints revealed. You've got this.",
    checkHeader: "CHECK",
    checkIdle: "F5 saves and checks your solution against the tests.",
    checking: "checking",
    noTask: "This file doesn't belong to a course.\n\nOpen a file in ~/Tasks.cb/<course>/ to see its lesson and hints here.",
    modified: "Modified",
    newFile: "[ New File ]",
    read: (n) => `[ Read ${n} lines ]`,
    wrote: (n) => `[ Wrote ${n} lines ]`,
    writeError: (e) => `[ Error writing: ${e} ]`,
    askSave: "Save modified buffer?  Y = yes   N = no   Esc = cancel",
    cancelled: "[ Cancelled ]",
    pos: (l, c) => `line ${l}, col ${c}`,
    internalError: "internal error",
    boot: [
      "CB-BIOS v0.1  (C) 1987 CodeBetter Systems",
      "Memory test ........ 640K OK",
      "Detecting brain .... found",
      "Disabling autopilot  done",
      "Mounting home ...... ok",
    ],
  },
};

let lang = "de";
const t = (key, ...args) => {
  const s = (STRINGS[lang] ?? STRINGS.de)[key];
  return typeof s === "function" ? s(...args) : s;
};

function setLang(next) {
  if (STRINGS[next]) lang = next;
  document.documentElement.lang = next;
  document.querySelectorAll("[data-i18n]").forEach((el) => (el.textContent = t(el.dataset.i18n)));
}

const term = {
  line: "",
  pos: 0,
  prompt: "",
  history: [],
  historyIndex: 0,
  draft: "",
  busy: true,
};

// ---------------------------------------------------------------- output

const escapeHtml = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** Converts text with ANSI SGR colour codes into HTML spans. */
function ansiToHtml(text) {
  text = text.replace(/\x1b\[[\d;?]*[A-Za-ln-z]/g, ""); // non-colour escapes
  const style = { fg: null, bold: false, dim: false, underline: false };
  const re = /\x1b\[([\d;]*)m/g;
  let html = "";
  let last = 0;

  const wrap = (s) => {
    if (!s) return "";
    const cls = [];
    if (style.fg !== null) cls.push(`c${style.fg}`);
    if (style.bold) cls.push("b");
    if (style.dim) cls.push("d");
    if (style.underline) cls.push("u");
    return cls.length ? `<span class="${cls.join(" ")}">${escapeHtml(s)}</span>` : escapeHtml(s);
  };

  for (let m; (m = re.exec(text)); ) {
    html += wrap(text.slice(last, m.index));
    last = re.lastIndex;
    const codes = m[1] === "" ? [0] : m[1].split(";").map(Number);
    for (let i = 0; i < codes.length; i++) {
      const c = codes[i];
      if (c === 0) Object.assign(style, { fg: null, bold: false, dim: false, underline: false });
      else if (c === 1) style.bold = true;
      else if (c === 2) style.dim = true;
      else if (c === 4) style.underline = true;
      else if (c === 22) style.bold = style.dim = false;
      else if (c === 24) style.underline = false;
      else if (c >= 30 && c <= 37) style.fg = c - 30;
      else if (c === 39) style.fg = null;
      else if (c >= 90 && c <= 97) style.fg = c - 90 + 8;
      else if (c === 38 && codes[i + 1] === 5) {
        const n = codes[i + 2];
        if (n < 16) style.fg = n;
        i += 2;
      }
    }
  }
  return html + wrap(text.slice(last));
}

function scrollToBottom() {
  crt.scrollTop = crt.scrollHeight;
}

function print(text) {
  if (!text) return;
  output.insertAdjacentHTML("beforeend", ansiToHtml(text));
  scrollToBottom();
}

function promptHtml(prompt) {
  const i = prompt.indexOf(":");
  return `<span class="p-user">${escapeHtml(prompt.slice(0, i))}</span>:<span class="p-path">${escapeHtml(prompt.slice(i + 1))}</span>$ `;
}

function setPrompt(prompt) {
  term.prompt = prompt;
  promptEl.innerHTML = promptHtml(prompt);
}

function render() {
  beforeEl.textContent = term.line.slice(0, term.pos);
  cursorEl.textContent = term.line[term.pos] ?? " ";
  afterEl.textContent = term.line.slice(term.pos + 1);
  scrollToBottom();
}

function echoLine(suffix = "") {
  output.insertAdjacentHTML("beforeend", promptHtml(term.prompt) + escapeHtml(term.line) + suffix + "\n");
}

function setBusy(busy) {
  term.busy = busy;
  inputLine.classList.toggle("hidden", busy);
  busyEl.classList.add("hidden");
  if (busy) {
    // Only show the indicator for slow commands (compiling, running tests).
    clearTimeout(setBusy.timer);
    setBusy.timer = setTimeout(() => term.busy && busyEl.classList.remove("hidden"), 250);
  }
  scrollToBottom();
}

/** Prints words in columns, like `ls` or bash completion. */
function columns(words) {
  const width = Math.max(...words.map((w) => w.length)) + 2;
  const charWidth = cursorEl.getBoundingClientRect().width || 10;
  const perRow = Math.max(1, Math.floor((crt.clientWidth - 60) / charWidth / width));
  let s = "";
  words.forEach((w, i) => {
    s += w.padEnd(width);
    if ((i + 1) % perRow === 0 || i === words.length - 1) s = s.trimEnd() + "\n";
  });
  return s;
}

// --------------------------------------------------------------- commands

async function submitLine() {
  const line = term.line;
  echoLine();
  term.line = "";
  term.pos = 0;
  term.draft = "";
  if (line.trim() && term.history[term.history.length - 1] !== line) term.history.push(line);
  term.historyIndex = term.history.length;

  if (line.trim() === "history") {
    print(term.history.map((h, i) => `${String(i + 1).padStart(5)}  ${h}\n`).join(""));
    render();
    return;
  }
  if (!line.trim()) {
    render();
    return;
  }

  setBusy(true);
  try {
    const res = await invoke("exec", { line });
    setLang(res.lang);
    if (res.action?.type === "clear") output.innerHTML = "";
    print(res.output);
    setPrompt(res.prompt);
    if (res.action?.type === "edit") {
      setBusy(false);
      editor.open(res.action);
      return;
    }
  } catch (err) {
    print(`\x1b[31m${t("internalError")}: ${err}\x1b[0m\n`);
  }
  setBusy(false);
  render();
}

async function tabComplete() {
  const head = term.line.slice(0, term.pos);
  const tail = term.line.slice(term.pos);
  const res = await invoke("complete", { line: head });
  if (res.candidates.length) {
    echoLine();
    print(columns(res.candidates));
  }
  term.line = res.line + tail;
  term.pos = res.line.length;
  render();
}

function historyMove(delta) {
  if (!term.history.length) return;
  if (term.historyIndex === term.history.length) term.draft = term.line;
  term.historyIndex = Math.min(term.history.length, Math.max(0, term.historyIndex + delta));
  term.line = term.historyIndex === term.history.length ? term.draft : term.history[term.historyIndex];
  term.pos = term.line.length;
  render();
}

function insert(text) {
  term.line = term.line.slice(0, term.pos) + text + term.line.slice(term.pos);
  term.pos += text.length;
  render();
}

function onTerminalKey(e) {
  if (term.busy) {
    e.preventDefault();
    return;
  }
  const ctrl = e.ctrlKey && !e.altKey;
  const key = e.key;

  if (ctrl && key.toLowerCase() === "c" && !window.getSelection().toString()) {
    echoLine("^C");
    term.line = "";
    term.pos = 0;
    render();
  } else if (ctrl && key.toLowerCase() === "l") {
    output.innerHTML = "";
    render();
  } else if (ctrl && key.toLowerCase() === "u") {
    term.line = term.line.slice(term.pos);
    term.pos = 0;
    render();
  } else if (ctrl && key.toLowerCase() === "k") {
    term.line = term.line.slice(0, term.pos);
    render();
  } else if (ctrl && key.toLowerCase() === "w") {
    const head = term.line.slice(0, term.pos).replace(/\S+\s*$/, "");
    term.line = head + term.line.slice(term.pos);
    term.pos = head.length;
    render();
  } else if ((ctrl && key.toLowerCase() === "a") || key === "Home") {
    term.pos = 0;
    render();
  } else if ((ctrl && key.toLowerCase() === "e") || key === "End") {
    term.pos = term.line.length;
    render();
  } else if (ctrl && key.toLowerCase() === "v") {
    return; // handled by the paste event
  } else if (key === "Enter") {
    submitLine();
  } else if (key === "Backspace") {
    if (term.pos > 0) {
      term.line = term.line.slice(0, term.pos - 1) + term.line.slice(term.pos);
      term.pos--;
      render();
    }
  } else if (key === "Delete") {
    term.line = term.line.slice(0, term.pos) + term.line.slice(term.pos + 1);
    render();
  } else if (key === "ArrowLeft") {
    term.pos = Math.max(0, term.pos - 1);
    render();
  } else if (key === "ArrowRight") {
    term.pos = Math.min(term.line.length, term.pos + 1);
    render();
  } else if (key === "ArrowUp") {
    historyMove(-1);
  } else if (key === "ArrowDown") {
    historyMove(1);
  } else if (key === "Tab") {
    tabComplete();
  } else if (key.length === 1 && !e.ctrlKey && !e.metaKey) {
    insert(key);
  } else {
    return;
  }
  e.preventDefault();
}

// ----------------------------------------------------------------- wiring

const editorOpen = () => !editor.el.classList.contains("hidden");

document.addEventListener("keydown", (e) => {
  if (editorOpen()) editor.onKey(e);
  else onTerminalKey(e);
});

document.addEventListener("paste", (e) => {
  if (editorOpen() || term.busy) return;
  e.preventDefault();
  insert(e.clipboardData.getData("text").replace(/\r?\n/g, " "));
});

window.addEventListener("focus", () => document.body.classList.remove("unfocused"));
window.addEventListener("blur", () => document.body.classList.add("unfocused"));

// ------------------------------------------------------------------- boot

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function boot() {
  const res = await invoke("welcome");
  setLang(res.lang);
  for (const l of t("boot")) {
    print(`\x1b[2m${l}\x1b[0m\n`);
    await sleep(110);
  }
  print("\n" + res.output);
  setPrompt(res.prompt);
  setBusy(false);
  render();
}

boot();
