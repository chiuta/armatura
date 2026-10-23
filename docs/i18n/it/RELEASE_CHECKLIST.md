<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio) -->
# Checklist di rilascio (a cura del titolare)

*Passaggi che solo il titolare può compiere, in ordine. Nulla di quanto segue è stato eseguito dagli strumenti del progetto: l'ambiente di build non aveva accesso a GitHub, né il target wasm32, né una vera GPU, né un avvocato. Non è consulenza legale. Le voci contrassegnate con **[blocker]** vanno completate prima di un annuncio pubblico.*

## 1. Prima di pubblicare qualsiasi cosa

- [ ] **[blocker]** Conferma o sostituisci il contatto per la sicurezza (`alexio@trom.tf` è un segnaposto in `docs/SECURITY.md`, in `--version` e nei documenti di licenza).
- [ ] **[blocker]** Cerca il nome "ARMATURA" nei registri dei marchi che ti interessano (EUIPO, USPTO, OSIM) e negli app store / registri dei pacchetti. Vedi `TRADEMARK.md`. Se c'è un conflitto, cambia nome prima della prima release (il nome compare nel binario, nei crate, nella documentazione).
- [ ] **[blocker]** Chiedi a un avvocato di rivedere `LICENSING.md`, la suddivisione MPL-2.0 / AGPL-3.0, `docs/THIRD_PARTY.md` e `docs/PATENT_NOTES.md`. Decidi se vuoi la doppia licenza; in tal caso serve un CLA *prima* di accettare contributi esterni (oggi: solo DCO).
- [ ] Verifica i termini del riferimento sull'approssimazione di curve di Graphics Gems (citato a memoria in `CLEANROOM.md`); mantieni la formulazione sull'implementazione indipendente solo se è accurata.
- [ ] Esegui `reuse lint` e `cargo deny check` sulla tua macchina (il progetto ha solo il proprio test `spdx_audit`).
- [ ] Verifica che nell'albero non ci siano dati personali, percorsi locali o token (`grep -r "/home/" .`, `grep -ri token`), e che il modello incluso e gli asset della demo siano tutti autogenerati.

## 2. Repository

- [ ] Crea il repository su GitHub (o Codeberg / un Forgejo tuo); invia l'albero **senza** `target/` e `out/`.
- [ ] Conserva `LICENSE`, `LICENSES/`, `REUSE.toml`, `.github/FUNDING.yml` (Buy Me a Coffee `echo.of.the.strings`, Patreon `alexio_tf`).
- [ ] Abilita la segnalazione privata delle vulnerabilità (scheda Security) e fai puntare `docs/SECURITY.md` ad essa.
- [ ] Passa l'URL del repository con `--source-url` quando avvii lo studio (offerta del sorgente AGPL, route `/source`) e prova l'offerta da un'installazione pulita.

## 3. Primo giro di CI (mai eseguito finora)

- [ ] Fai il push e osserva `.github/workflows/ci.yml`: test su Linux, Windows, macOS e `clippy -D warnings`; il job di build wasm32; il job cargo-deny e REUSE.
- [ ] Aspettati fallimenti alla prima esecuzione su Windows/macOS (percorsi, permessi, differenze di pixel sui bordi con anti-aliasing, L-04 / L-26). Correggi o allenta con tolleranze, poi annota il risultato in `docs/LIMITATIONS.md`.
- [ ] Se il job wasm32 passa, esegui l'elenco di verifica di `docs/WASM.md` in un vero browser e aggiorna L-01 / L-02.

## 4. Prove con persone reali

- [ ] Dai lo studio a 3-5 animatori per una settimana con un breve compito (fare il rig di un personaggio, realizzare un ciclo di camminata, esportare una GIF). Registra dove si bloccano; apri una issue per ciascun caso.
- [ ] Prova su browser reali (Chrome, Firefox, Safari), su un dispositivo touch e in una sessione solo da tastiera.
- [ ] Prova le funzioni di rilevamento del volto / mocap / voce con dati reali; riporta onestamente che sono limitate (L-12, L-13).

## 5. Sicurezza del deployment (se ospiti lo studio o la collaborazione)

- [ ] Mettilo dietro un reverse proxy che termina il TLS (ricetta: `docs/SECURITY.md` sezione 12); non esporre mai su internet la porta HTTP in chiaro.
- [ ] Usa token per partecipante (`collab-token`), imposta una scadenza, fai il bind su localhost dietro il proxy.
- [ ] Commissiona una revisione di sicurezza indipendente prima di offrire la collaborazione ospitata a sconosciuti.

## 6. Artefatti di release

- [ ] Crea un tag di versione, aggiorna `CHANGELOG.md`, compila i binari di release sulle tue macchine (l'ambiente del progetto non ha mai compilato con `--release`), pubblica i checksum.
- [ ] Allega il sorgente corrispondente per le parti AGPL (il tag stesso basta; tienilo disponibile finché distribuisci i binari).
- [ ] Distribuisci `docs/THIRD_PARTY_NOTICES.md` (rigeneralo con `scripts/licence-audit.sh`) con ogni binario.
- [ ] Crates.io / npm / registri dei pacchetti: verifica che i nomi siano liberi e che i campi di licenza corrispondano a `LICENSING.md`.

## 7. Annuncio e donazioni

- [ ] Prima schermata del README: cosa funziona, cosa no (collega `docs/LIMITATIONS.md`); non dichiarare "di classe Moho" senza precisazioni.
- [ ] Mostra le schermate in `docs/img/` e una breve GIF realizzata con il progetto demo.
- [ ] Aggiungi i link per le donazioni (Buy Me a Coffee, Patreon) al README e alla finestra Informazioni dello studio; non promettere funzionalità in cambio di donazioni a meno che tu possa mantenere la promessa.
- [ ] Pubblica dove stanno gli animatori (community di rigging 2D / animazione indipendente); chiedi segnalazioni di bug, non elogi.
- [ ] Riserva del tempo per rispondere in fretta alle prime issue; le prime due settimane decidono se i contributori restano.
