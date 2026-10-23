<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio) -->
# Release-Checkliste (Seite des Inhabers)

*Schritte, die nur der Inhaber ausführen kann, in dieser Reihenfolge. Nichts davon wurde von den Werkzeugen des Projekts ausgeführt: Die Build-Umgebung hatte keinen GitHub-Zugang, kein wasm32-Target, keine echte GPU und keinen Anwalt. Keine Rechtsberatung. Mit **[blocker]** markierte Punkte sollten vor einer öffentlichen Ankündigung erledigt sein.*

## 1. Bevor irgendetwas veröffentlicht wird

- [ ] **[blocker]** Den Sicherheitskontakt bestätigen oder ersetzen (`alexio@trom.tf` ist ein Platzhalter in `docs/SECURITY.md`, in `--version` und in den Lizenzdokumenten).
- [ ] **[blocker]** Den Namen „ARMATURA" in den Markenregistern recherchieren, die dir wichtig sind (EUIPO, USPTO, OSIM), sowie in App-Stores / Paketregistern. Siehe `TRADEMARK.md`. Bei einer Kollision vor dem ersten Release umbenennen (der Name steckt im Binary, in Crates und Dokumentation).
- [ ] **[blocker]** Einen Anwalt bitten, `LICENSING.md`, die Aufteilung MPL-2.0 / AGPL-3.0, `docs/THIRD_PARTY.md` und `docs/PATENT_NOTES.md` zu prüfen. Entscheiden, ob du Dual-Lizenzierung willst; wenn ja, wird ein CLA *vor* der Annahme externer Beiträge benötigt (heute: nur DCO).
- [ ] Die Bedingungen der Graphics-Gems-Referenz zur Kurvenanpassung prüfen (in `CLEANROOM.md` aus dem Gedächtnis zitiert); die Formulierung zur unabhängigen Implementierung nur beibehalten, wenn sie zutrifft.
- [ ] `reuse lint` und `cargo deny check` auf deinem Rechner ausführen (das Projekt hat nur seinen eigenen Test `spdx_audit`).
- [ ] Prüfen, dass im Baum keine personenbezogenen Daten, lokalen Pfade oder Tokens liegen (`grep -r "/home/" .`, `grep -ri token`) und dass das mitgelieferte Modell und die Demo-Assets alle selbst erzeugt sind.

## 2. Repository

- [ ] Das Repository auf GitHub (oder Codeberg / eigenem Forgejo) anlegen; den Baum **ohne** `target/` und `out/` pushen.
- [ ] `LICENSE`, `LICENSES/`, `REUSE.toml`, `.github/FUNDING.yml` behalten (Buy Me a Coffee `echo.of.the.strings`, Patreon `alexio_tf`).
- [ ] Privates Melden von Sicherheitslücken aktivieren (Reiter Security) und `docs/SECURITY.md` darauf verweisen lassen.
- [ ] Beim Start des Studios die Repository-URL mit `--source-url` übergeben (AGPL-Quellcode-Angebot, Route `/source`) und das Angebot an einer frischen Installation testen.

## 3. Erster CI-Lauf (bisher nie ausgeführt)

- [ ] Pushen und `.github/workflows/ci.yml` beobachten: Tests unter Linux, Windows, macOS und `clippy -D warnings`; der wasm32-Build-Job; der Job für cargo-deny und REUSE.
- [ ] Beim ersten Lauf mit Fehlern unter Windows/macOS rechnen (Pfade, Berechtigungen, Pixelunterschiede an Anti-Aliasing-Kanten, L-04 / L-26). Beheben oder mit Toleranzen entschärfen, dann das Ergebnis in `docs/LIMITATIONS.md` vermerken.
- [ ] Wenn der wasm32-Job besteht, die Verifikationsliste in `docs/WASM.md` in einem echten Browser durchgehen und L-01 / L-02 aktualisieren.

## 4. Tests mit Menschen

- [ ] Das Studio 3 bis 5 Animatoren eine Woche lang mit einer kurzen Aufgabe geben (eine Figur riggen, einen Walk Cycle erstellen, ein GIF exportieren). Festhalten, wo sie stecken bleiben; jeden Fall als Issue anlegen.
- [ ] Auf echten Browsern (Chrome, Firefox, Safari), einem Touch-Gerät und in einer Sitzung nur mit Tastatur testen.
- [ ] Gesichtsdetektor / Mocap / Sprachfunktionen mit echten Daten ausprobieren; ehrlich berichten, dass sie begrenzt sind (L-12, L-13).

## 5. Sicherheit beim Betrieb (wenn du das Studio oder die Zusammenarbeit hostest)

- [ ] Hinter einen Reverse-Proxy mit TLS-Terminierung stellen (Rezept: `docs/SECURITY.md` Abschnitt 12); den reinen HTTP-Port niemals ins Internet exponieren.
- [ ] Tokens pro Teilnehmer verwenden (`collab-token`), ein Ablaufdatum setzen, hinter dem Proxy an localhost binden.
- [ ] Eine unabhängige Sicherheitsprüfung in Auftrag geben, bevor gehostete Zusammenarbeit Fremden angeboten wird.

## 6. Release-Artefakte

- [ ] Eine Version taggen, `CHANGELOG.md` aktualisieren, Release-Binaries auf deinen eigenen Rechnern bauen (die Projektumgebung hat nie `--release` gebaut), Prüfsummen veröffentlichen.
- [ ] Den passenden Quellcode für die AGPL-Teile beilegen (der Tag selbst genügt; ihn verfügbar halten, solange du Binaries verteilst).
- [ ] `docs/THIRD_PARTY_NOTICES.md` (mit `scripts/licence-audit.sh` neu generieren) mit jedem Binary ausliefern.
- [ ] Crates.io / npm / Paketregister: prüfen, dass die Namen frei sind und die Lizenzfelder zu `LICENSING.md` passen.

## 7. Ankündigung und Spenden

- [ ] Erster Bildschirm des README: was funktioniert, was nicht (auf `docs/LIMITATIONS.md` verlinken); nicht ohne Einschränkung „Moho-Klasse" behaupten.
- [ ] Die Screenshots aus `docs/img/` und ein kurzes GIF zeigen, das mit dem Demo-Projekt erstellt wurde.
- [ ] Die Spendenlinks (Buy Me a Coffee, Patreon) im README und im About-Fenster des Studios ergänzen; keine Funktionen als Gegenleistung für Spenden versprechen, es sei denn, du kannst das Versprechen halten.
- [ ] Dort posten, wo Animatoren sind (Communities für 2D-Rigging / Indie-Animation); um Fehlerberichte bitten, nicht um Lob.
- [ ] Zeit einplanen, um die ersten Issues schnell zu beantworten; die ersten zwei Wochen entscheiden, ob Mitwirkende bleiben.
