# CodeBetter

> **Coden lernen und behalten – trotz KI.**

---

## 1. Problemstellung

Generative KI hat die Softwareentwicklung disruptiv verändert. Viele Entwickler wollen eigentlich selbst coden, greifen aber immer häufiger auf KI zurück – und verlernen dadurch Schritt für Schritt ihr Handwerk.

**Die Folge:**

- Entwickler verstehen ihren eigenen Code nicht mehr vollständig
- Software lässt sich schlecht **maintainen**
- Fehler lassen sich schlecht **debuggen**
- Abhängigkeit von KI statt echter Problemlösungskompetenz

---

## 2. Unsere Lösung

**CodeBetter** ist eine Plattform, auf der Entwickler das Coden nicht durch KI-Nutzung verlernen, sondern **Schritt für Schritt selbstständig implementieren**.

Statt fertigen Code zu liefern, führt die Plattform durch den Lösungsweg:

1. Aufgabe oder Projektidee wird in kleine, verständliche Teilschritte zerlegt
2. Der Entwickler implementiert jeden Schritt selbst
3. Feedback und Hinweise unterstützen – ohne die Lösung vorwegzunehmen
4. Verständnis wird geprüft, bevor der nächste Schritt freigeschaltet wird

---

## 3. Anwendungsfälle

### 🛠️ Eigene Projekte
Der Entwickler bringt seine eigene Projektidee mit. CodeBetter zerlegt sie in Meilensteine und begleitet die Umsetzung, ohne den Code zu schreiben. Das Ergebnis: ein Projekt, das der Entwickler vollständig versteht, pflegen und debuggen kann.

### 🎓 Hochschulen
Lehrende nutzen CodeBetter als Begleiter für Programmierkurse und Praktika. Studierende lösen Aufgaben selbstständig, Lehrende sehen den Lernfortschritt und können Aufgaben anpassen. So werden Grundlagen gefestigt, statt dass Abgaben per KI generiert werden.

### 📚 Eigenes Lernen
Selbstlernende wählen Themen und Lernpfade (z. B. Python, Web, Algorithmen) und arbeiten sie in ihrem Tempo durch – mit abgestufter Hilfe statt fertiger Lösungen.

---

## 4. So funktioniert es

| Schritt | Beschreibung |
|---------|--------------|
| **1. Ziel wählen** | Eigenes Projekt, Kursaufgabe oder Lernpfad |
| **2. Zerlegen** | Plattform teilt das Problem in kleine Meilensteine |
| **3. Selbst implementieren** | Entwickler schreibt den Code eigenständig |
| **4. Feedback** | Tests, Hinweise und Code-Review statt fertiger Lösung |
| **5. Verstehen** | Kurze Verständnisfragen zum eigenen Code |

---

## 5. Kernfeatures

- **Guided Implementation** – Schritt-für-Schritt-Anleitung statt Copy & Paste
- **Projekt-Modus** – Eigene Ideen in umsetzbare Meilensteine zerlegen
- **Abgestufte Hilfe** – Hinweis → Denkanstoß → Konzept-Erklärung (Lösung nur als letzter Ausweg)
- **Automatische Tests** – Sofortiges Feedback zur Korrektheit
- **Debugging-Training** – Gezielte Aufgaben mit versteckten Bugs
- **Maintainability-Checks** – Lesbarkeit und Struktur des eigenen Codes werden bewertet
- **Kurs-Dashboard für Lehrende** – Fortschritt und Schwierigkeiten der Studierenden im Blick
- **Fortschritts-Tracking** – Lernkurve und Skill-Entwicklung sichtbar machen

---

## 6. Zielgruppe

- **Studierende** der Informatik, die Grundlagen festigen wollen
- **Hochschulen und Lehrende**, die Programmierkompetenz sichern wollen
- **Hobby- und Projektentwickler**, die eigene Ideen selbst umsetzen möchten
- **Selbstlernende und Junior-Entwickler**, die nicht von KI abhängig werden wollen

---

## 7. Nutzen

| Für Lernende und Entwickler | Für Hochschulen |
|-----------------------------|-----------------|
| Echtes Verständnis des eigenen Codes | Nachweisbar erworbene Kompetenz |
| Bessere Debugging-Fähigkeiten | Transparenz über den Lernfortschritt |
| Eigene Projekte, die man wirklich beherrscht | Weniger KI-generierte Abgaben ohne Verständnis |

---

## 8. Abgrenzung

| | KI-Codeassistenten | **CodeBetter** |
|---|---|---|
| Ziel | Code schnell generieren | Code verstehen und selbst schreiben |
| Rolle der KI | Löst das Problem | Begleitet den Lernprozess |
| Ergebnis | Fertiger Code | Gefestigte Fähigkeit |

> Hinweis: CodeBetter nutzt KI selbst für Hinweise und Feedback, aber nie, um dem Nutzer die Arbeit abzunehmen.

---

## 9. Vision

Eine Zukunft, in der Entwickler KI als **Werkzeug** nutzen, weil sie ihr Handwerk beherrschen – nicht weil sie es verlernt haben.

---

## 10. Prototyp

[`codebetter-terminal/`](codebetter-terminal/) ist der erste Prototyp: eine Desktop-App
(Tauri + Rust) im Retro-Terminal-Stil mit einem Rust-Kurs in 10 Lektionen,
dem Editor CBEditor und automatischen Tests. Start und Aufbau stehen in der
[README der App](codebetter-terminal/README.md).

## 11. Nächste Schritte

- [x] MVP mit einem Themenbereich: Rust-Grundkurs in der [CodeBetter Terminal](codebetter-terminal/)-App
- [ ] Projekt-Modus für eigene Ideen
- [ ] Pilotphase mit einer Hochschule bzw. einem Kurs
- [ ] Feedback auswerten und Lernpfade erweitern

---

**CodeBetter – Verstehe, was du baust.**
