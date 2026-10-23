<!-- SPDX-License-Identifier: CC-BY-4.0 -->
<!-- Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio) -->
# Liste de contrôle de publication (côté propriétaire)

*Étapes que seul le propriétaire peut réaliser, dans l'ordre. Rien de ceci n'a été exécuté par l'outillage du projet : l'environnement de compilation n'avait ni accès à GitHub, ni cible wasm32, ni vrai GPU, ni juriste. Ceci n'est pas un avis juridique. Les éléments marqués **[bloquant]** doivent être traités avant toute annonce publique.*

## 1. Avant de publier quoi que ce soit

- [ ] **[bloquant]** Confirmez ou remplacez le contact de sécurité (`alexio@trom.tf` est un espace réservé dans `docs/SECURITY.md`, `--version` et les documents de licence).
- [ ] **[bloquant]** Recherchez le nom « ARMATURA » dans les registres de marques qui vous concernent (EUIPO, USPTO, OSIM) ainsi que dans les boutiques d'applications / registres de paquets. Voir `TRADEMARK.md`. En cas de conflit, changez de nom avant la première publication (le nom apparaît dans le binaire, les crates, la documentation).
- [ ] **[bloquant]** Demandez à un juriste de relire `LICENSING.md`, la répartition MPL-2.0 / AGPL-3.0, `docs/THIRD_PARTY.md` et `docs/PATENT_NOTES.md`. Décidez si vous voulez une double licence ; si oui, un CLA est nécessaire *avant* d'accepter des contributions extérieures (aujourd'hui : DCO uniquement).
- [ ] Vérifiez les conditions de la référence d'ajustement de courbes de Graphics Gems (citée de mémoire dans `CLEANROOM.md`) ; ne conservez la formulation d'implémentation indépendante que si elle est exacte.
- [ ] Exécutez `reuse lint` et `cargo deny check` sur votre machine (le projet ne dispose que de son propre test `spdx_audit`).
- [ ] Vérifiez qu'aucune donnée personnelle, aucun chemin local ni aucun jeton ne figure dans l'arborescence (`grep -r "/home/" .`, `grep -ri token`), et que le modèle fourni et les ressources de démonstration sont tous auto-générés.

## 2. Dépôt

- [ ] Créez le dépôt GitHub (ou Codeberg / votre propre Forgejo) ; poussez l'arborescence **sans** `target/` ni `out/`.
- [ ] Conservez `LICENSE`, `LICENSES/`, `REUSE.toml`, `.github/FUNDING.yml` (Buy Me a Coffee `echo.of.the.strings`, Patreon `alexio_tf`).
- [ ] Activez le signalement privé des vulnérabilités (onglet Security) et faites-y pointer `docs/SECURITY.md`.
- [ ] Passez l'URL du dépôt avec `--source-url` lors de l'exécution du studio (offre de code source AGPL, route `/source`) et testez l'offre depuis une installation neuve.

## 3. Première exécution de la CI (jamais exécutée jusqu'ici)

- [ ] Poussez et surveillez `.github/workflows/ci.yml` : tests Linux, Windows, macOS et `clippy -D warnings` ; la tâche de compilation wasm32 ; la tâche cargo-deny et REUSE.
- [ ] Attendez-vous à des échecs à la première exécution sous Windows/macOS (chemins, permissions, différences de pixels sur les bords antialiasés, L-04 / L-26). Corrigez ou assouplissez avec des tolérances, puis consignez le résultat dans `docs/LIMITATIONS.md`.
- [ ] Si la tâche wasm32 réussit, exécutez la liste de vérification de `docs/WASM.md` dans un vrai navigateur et mettez à jour L-01 / L-02.

## 4. Tests humains

- [ ] Donnez le studio à 3 à 5 animateurs pendant une semaine avec une courte tâche (rigger un personnage, réaliser un cycle de marche, exporter un GIF). Notez où ils bloquent ; ouvrez une issue pour chaque cas.
- [ ] Testez sur de vrais navigateurs (Chrome, Firefox, Safari), un appareil tactile et une session au clavier uniquement.
- [ ] Essayez le détecteur de visages / la mocap / les fonctions vocales avec des données réelles ; indiquez honnêtement qu'elles sont limitées (L-12, L-13).

## 5. Sécurité du déploiement (si vous hébergez le studio ou la collaboration)

- [ ] Placez-le derrière un reverse proxy qui termine le TLS (recette : `docs/SECURITY.md` section 12) ; n'exposez jamais le port HTTP simple à Internet.
- [ ] Utilisez des jetons par participant (`collab-token`), définissez une expiration, liez à localhost derrière le proxy.
- [ ] Commandez une revue de sécurité indépendante avant de proposer la collaboration hébergée à des inconnus.

## 6. Artefacts de publication

- [ ] Étiquetez une version, mettez à jour `CHANGELOG.md`, compilez les binaires de publication sur vos propres machines (l'environnement du projet n'a jamais compilé en `--release`), publiez les sommes de contrôle.
- [ ] Joignez le code source correspondant pour les parties AGPL (l'étiquette elle-même suffit ; gardez-le disponible aussi longtemps que vous distribuez des binaires).
- [ ] Livrez `docs/THIRD_PARTY_NOTICES.md` (à régénérer avec `scripts/licence-audit.sh`) avec chaque binaire.
- [ ] Crates.io / npm / registres de paquets : vérifiez que les noms sont libres et que les champs de licence correspondent à `LICENSING.md`.

## 7. Annonce et dons

- [ ] Premier écran du README : ce qui fonctionne, ce qui ne fonctionne pas (lien vers `docs/LIMITATIONS.md`) ; ne prétendez pas être « de classe Moho » sans nuance.
- [ ] Montrez les captures d'écran de `docs/img/` et un court GIF réalisé avec le projet de démonstration.
- [ ] Ajoutez les liens de dons (Buy Me a Coffee, Patreon) au README et à la boîte À propos du studio ; ne promettez pas de fonctionnalités en échange de dons à moins de pouvoir tenir la promesse.
- [ ] Publiez là où se trouvent les animateurs (communautés de rigging 2D / d'animation indépendante) ; demandez des rapports de bogues, pas des compliments.
- [ ] Réservez du temps pour répondre rapidement aux premières issues ; les deux premières semaines décident si les contributeurs restent.
