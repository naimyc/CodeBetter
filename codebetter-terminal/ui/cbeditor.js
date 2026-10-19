"use strict";

// CBEditor: a code editor with the current lesson (theory + exercise), its
// hints and the test results attached next to the code, like Codecademy. Uses invoke/t/ansiToHtml/render
// from terminal.js.

const editor = {
  el: $("editor"),
  text: $("ed-text"),
  gutter: $("ed-gutter"),
  status: $("ed-status"),
  path: null,
  saved: "",
  task: null,
  asking: false,
  checking: false,

  open({ path, name, content, task }) {
    this.path = path;
    this.saved = content;
    this.text.value = content;
    $("ed-name").textContent = name;
    $("p-check").innerHTML = escapeHtml(t("checkIdle"));
    this.showTask(task);
    this.el.classList.toggle("no-panel", !task);
    this.el.classList.remove("hidden");
    this.setStatus(content ? t("read", content.split("\n").length) : t("newFile"));
    this.text.setSelectionRange(0, 0);
    this.text.scrollTop = 0;
    this.text.focus();
    this.update();
  },

  get modified() {
    return this.text.value !== this.saved;
  },

  /** Fills the side panel. `revealed` marks a freshly shown hint. */
  showTask(task, revealed = false) {
    const unlocked = this.task && task && this.task.id === task.id && task.part !== this.task.part;
    this.task = task;
    $("ed-panel").classList.toggle("no-task", !task);
    if (!task) {
      $("p-task").textContent = "";
      $("p-part").textContent = "";
      $("pane-learn").classList.add("hidden");
      $("p-desc").textContent = t("noTask");
      return;
    }
    $("p-task").textContent = `${task.id} · ${task.title}`;
    $("p-part").textContent = task.done ? `★ ${task.part_title}` : t("part", task.part, task.parts, task.part_title);
    $("p-part").classList.toggle("flash", unlocked);
    $("p-lesson").textContent = task.lesson;
    $("pane-learn").classList.toggle("hidden", !task.lesson);
    $("p-desc").textContent = task.description;
    if (unlocked) $("ed-panel").scrollTop = 0;

    const list = $("p-hints");
    list.innerHTML = "";
    task.hints.forEach((h, i) => {
      const li = document.createElement("li");
      li.textContent = h;
      if (revealed && i === task.hints.length - 1) li.classList.add("flash");
      list.appendChild(li);
    });
    $("pane-hints").classList.toggle("hidden", task.done);
    $("p-hints-title").textContent = t("hintsHeader", task.hints.length, task.hints_total);
    $("p-hints-more").textContent =
      task.hints_total === 0 ? t("noHints") : task.hints.length < task.hints_total ? t("moreHints") : t("allHints");
  },

  update() {
    const lines = this.text.value.split("\n").length;
    this.gutter.textContent = Array.from({ length: lines }, (_, i) => i + 1).join("\n") + "\n\n";
    this.gutter.scrollTop = this.text.scrollTop;
    $("ed-mod").textContent = this.modified ? t("modified") : "";
    const before = this.text.value.slice(0, this.text.selectionStart).split("\n");
    $("ed-pos").textContent = t("pos", before.length, before[before.length - 1].length + 1);
  },

  setStatus(msg, ask = false) {
    this.status.textContent = msg;
    this.status.classList.toggle("ask", ask);
  },

  async save() {
    try {
      const content = this.text.value;
      await invoke("save", { path: this.path, content });
      this.saved = content;
      this.setStatus(t("wrote", content.split("\n").length));
      return true;
    } catch (err) {
      this.setStatus(t("writeError", err));
      return false;
    } finally {
      this.update();
    }
  },

  async hint() {
    if (!this.task || this.task.done) return;
    const before = this.task.hints.length;
    const task = await invoke("editor_hint", { path: this.path });
    this.showTask(task, task && task.hints.length > before);
    $("pane-hints").scrollIntoView({ block: "nearest" });
  },

  /** Saves, then runs the current part's tests on this file. */
  async check() {
    if (this.checking || !this.task) return;
    if (!(await this.save())) return;
    this.checking = true;
    const out = $("p-check");
    out.innerHTML = `<span class="d">${escapeHtml(t("checking"))}<span class="dots"></span></span>`;
    $("pane-check").scrollIntoView({ block: "nearest" });
    try {
      const res = await invoke("editor_check", { path: this.path });
      out.innerHTML = ansiToHtml(res.output);
      if (res.task) this.showTask(res.task);
    } catch (err) {
      out.innerHTML = ansiToHtml(`\x1b[31m${t("internalError")}: ${err}\x1b[0m`);
    }
    this.checking = false;
    // A pass shows the next lesson from the top (showTask); otherwise keep the result in view.
    if ($("ed-panel").scrollTop !== 0) $("pane-check").scrollIntoView({ block: "nearest" });
  },

  togglePanel() {
    this.el.classList.toggle("no-panel");
  },

  close() {
    this.asking = false;
    this.el.classList.add("hidden");
    this.setStatus("");
    render();
  },

  /** Replaces the selection, keeping the browser's undo history intact. */
  type(str) {
    if (!document.execCommand("insertText", false, str)) this.text.setRangeText(str, this.text.selectionStart, this.text.selectionEnd, "end");
  },

  indent(outdent) {
    const t = this.text;
    const start = t.value.lastIndexOf("\n", t.selectionStart - 1) + 1;
    const end = t.selectionEnd;
    const block = t.value.slice(start, end);
    if (!outdent && !block.includes("\n")) {
      this.type("    ");
      return;
    }
    const changed = block
      .split("\n")
      .map((l) => (outdent ? l.replace(/^ {1,4}/, "") : "    " + l))
      .join("\n");
    t.setSelectionRange(start, end);
    this.type(changed);
    t.setSelectionRange(start, start + changed.length);
  },

  async onKey(e) {
    const key = e.key.toLowerCase();
    if (this.asking) {
      e.preventDefault();
      if (key === "y" || key === "j") {
        if (await this.save()) this.close();
      } else if (key === "n") {
        this.close();
      } else if (key === "escape" || (e.ctrlKey && key === "c")) {
        this.asking = false;
        this.setStatus(t("cancelled"));
      }
      return;
    }
    if (e.ctrlKey && key === "s") {
      e.preventDefault();
      this.save();
    } else if (e.ctrlKey && key === "x") {
      e.preventDefault();
      if (this.modified) {
        this.asking = true;
        this.setStatus(t("askSave"), true);
      } else {
        this.close();
      }
    } else if (key === "f5") {
      e.preventDefault();
      this.check();
    } else if (key === "f2") {
      e.preventDefault();
      this.hint();
    } else if (key === "f1") {
      e.preventDefault();
      this.togglePanel();
    } else if (key === "tab") {
      e.preventDefault();
      this.indent(e.shiftKey);
    } else if (key === "enter" && !e.ctrlKey) {
      e.preventDefault();
      const t = this.text;
      const lineStart = t.value.lastIndexOf("\n", t.selectionStart - 1) + 1;
      const indent = t.value.slice(lineStart).match(/^[ \t]*/)[0];
      const extra = /[{([]\s*$/.test(t.value.slice(lineStart, t.selectionStart)) ? "    " : "";
      this.type("\n" + indent + extra);
    }
  },
};

editor.text.addEventListener("input", () => editor.update());
editor.text.addEventListener("scroll", () => (editor.gutter.scrollTop = editor.text.scrollTop));
editor.text.addEventListener("keyup", () => editor.update());
editor.text.addEventListener("click", () => editor.update());
document.querySelectorAll("[data-key]").forEach((el) =>
  el.addEventListener("click", () => {
    const action = { check: "check", hint: "hint", panel: "togglePanel", save: "save" }[el.dataset.key];
    editor[action]();
    editor.text.focus();
  }),
);
