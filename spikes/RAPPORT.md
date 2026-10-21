# Rapport des spikes S1–S7

Date : 2026-10-21. Environnement : rustc/cargo 1.92.0 stable, rust-analyzer 0.3.3089 (binaire de l'extension VS Code), CodeLLDB 1.12.1, Linux (WSL2).

| Spike | Résultat | Décision |
|---|---|---|
| S1 | ✅ | Architecture `build.rs` + `include!(OUT_DIR)` + `#[path]` absolu validée |
| S2 | ⚠️ | Nuancé : deux contraintes découvertes (voir ci-dessous) |
| S3 | ✅ | Overlays `didOpen` dans `OUT_DIR` acceptés, quelle que soit l'extension |
| S4 | ✅ (statique) | Possible ; test de bout en bout à faire en P4 |
| S5 | ✅ | Points d'arrêt dans les `.rsc` fonctionnels |
| S6 | ✅ | Surcoût de build nul |
| S7 | ✅ | Aucun warning sur les `impl` vides |

Le plan B `.crust/gen/` n'est **pas** nécessaire : on garde `OUT_DIR`.

---

## S1 — `include!` + `#[path]` absolu avec rustc stable

Projet : `spikes/s1/`. Un `build.rs` simule `crust-build` en copiant des fichiers transpilés à la main (`gen_fixtures/`) vers `$OUT_DIR/crust/src/**.rsc`.

- `cargo run` compile et s'exécute (`1200 0`).
- Des fichiers d'extension `.rsc` sont acceptés par `rustc` via `#[path]`.
- **Erreur de type injectée (`--features err`)** :
  ```text
  --> …/out/crust/src/robot/Moteur.rsc:11:24
  11 |         self.vitesse = "abc";
  ```
  `11:24` est **exactement** la position dans `src/robot/Moteur.rsc`. La disposition préservant les lignes fonctionne : même sans remappage, seul le préfixe du chemin diffère.

## S2 — rust-analyzer suit-il ce schéma ?

Scripts : `spikes/lsp/s2.mjs`, `s3.mjs`, `s3b.mjs` (client LSP Node minimal dans `lsp.mjs`).

1. **Fichiers `.rsc` non chargés depuis le disque.** Le VFS de rust-analyzer ne charge que les `*.rs`. Avec des fichiers générés `.rsc`, `workspace/symbol` ne trouve rien tant que les fichiers ne sont pas envoyés en overlay. Les mêmes fichiers nommés `.rs` (`spikes/s1rs`) sont trouvés, et goto definition renvoie les bonnes lignes.
   → `crust-lsp` **doit** envoyer en overlay (`didOpen`) **tous** les fichiers générés au démarrage, ce que la spec 20.2 prévoyait déjà. C'est désormais obligatoire.
2. **Le texte inclus par `include!` n'a pas de complétion.** Dans le corps de `main.rsc` inclus via `include!`, le hover fonctionne mais la complétion renvoie une liste vide (`isIncomplete: true`), pour `.rs` comme pour `.rsc`. En revanche, les **modules déclarés** (`#[path] mod x;`) depuis un fichier inclus fonctionnent pleinement.
   → Le corps de `main.rsc` / `lib.rsc` ne doit **pas** être inclus textuellement. Il devient un module caché `__crust_root` (voir la spec 17.2).
   → Vérifié : `use __crust_root::main;` (fonction `main` importée) compile et s'exécute en Rust **stable 1.92** (`spikes/imain`).
   → **Révisé en P2** : `__crust_root` changeait la sémantique (items privés de la racine invisibles ailleurs). Retenu à la place : `include!` textuel pour la compilation, et overlay du `.rs` racine par `crust-lsp` pour rust-analyzer (spec 17.2 et 20.2).

## S3 — overlays sur des fichiers d'`OUT_DIR`

- `didOpen` / `didChange` sur `…/out/crust/src/**.rsc` : **hover, complétion et diagnostics natifs fonctionnent**. Par exemple, la complétion `m.` dans `Servomoteur.rsc` propose `vitesse` et `accelerer`.
- Les **diagnostics natifs** de rust-analyzer passent par le modèle *pull* (`textDocument/diagnostic`), pas par `publishDiagnostics`. Le serveur répond parfois `-32802 ServerCancelled` avec `retriggerRequest: true` : il faut relancer la requête. Les diagnostics `cargo check` (flycheck) arrivent, eux, en *push*.
  → `crust-lsp` doit remapper les deux canaux.
- **Plusieurs `OUT_DIR`** coexistent pour un même paquet (6 observés : build, check, clippy, features…). `crust-lsp` doit utiliser celui de rust-analyzer : `cargo check --message-format=json --all-targets`, message `build-script-executed`.
- Les numéros de ligne des diagnostics sur les overlays correspondent à ceux des `.rsc` source.

## S4 — extension rust-analyzer avec proxy

Analyse statique de l'extension 0.3.3089 :

- `documentSelector: [{ scheme: "file", language: "rust" }]`. Le filtrage se fait **uniquement par languageId** (`languageId === "rust" && scheme === "file"`), jamais par l'extension de fichier. Avec `files.associations: { "*.rsc": "rust" }`, les `.rsc` sont donc envoyés au serveur.
- `rust-analyzer.server.path` est respecté. L'extension lance `<binaire> --version` au démarrage : **`crust-lsp --version` doit répondre**.
- Test de bout en bout dans VS Code à faire en P4.

## S5 — débogage

lldb de CodeLLDB, `spikes/s1` :

```text
settings set target.source-map <OUT_DIR>/crust/src /…/s1/src
breakpoint set --file /…/s1/src/robot/Moteur.rsc --line 6
→ stop reason = breakpoint 1.1 … at Moteur.rsc:6:9
-> 6   	        self.vitesse += delta;
```

Le point d'arrêt est atteint, le **vrai fichier `.rsc`** est affiché et les variables sont lisibles.

## S6 — coût de `rerun-if-changed` (500 fichiers, 25 répertoires)

Projet : `spikes/s6/`.

| Mesure | Temps |
|---|---|
| `cargo build` sans changement | 40 ms |
| `build.rs` seul (parcours + comparaison de 500 fichiers) | 7 ms |
| Modifier un `.rsc` (build.rs + rustc) | 140–149 ms |
| Référence : modifier `main.rs` sans build.rs | 139–153 ms |
| Ajout d'un nouveau fichier | 172 ms, détecté via `rerun-if-changed` sur le répertoire |

Le surcoût n'est pas mesurable.

## S7 — `impl` vides et lints

`cargo clippy -W pedantic -W nursery -W restriction` sur le code généré :

- aucun warning sur les `impl Moteur { }` vides;
- seul `clippy::multiple_inherent_impl` (« multiple implementations of this structure ») apparaît, et il fait partie du groupe `restriction`, désactivé par défaut;
- observation : la suggestion de `clippy::derivable_impls` porte sur une zone synthétisée (`} impl Default for Moteur {`). Cela confirme la règle 18.2 de la spec : retirer ou retraduire les suggestions qui tombent dans du code synthétisé.

## Autres observations

- Avec rustc 1.92, rust-analyzer 0.3.3089 affichait « toolchain obsolète, 1.94+ supporté ». **Toolchain mise à jour en 1.99.1 : l'avertissement a disparu et S1 a été revérifié (même position `Moteur.rsc:11:24`).** Malgré tout, `crust-lsp` devrait préférer le rust-analyzer du composant `rustup` quand l'extension est plus récente que la toolchain.
- rust-analyzer ne signale pas nativement un champ inexistant (`self.vitess`). Seul `cargo check` le fait. Le remappage du flycheck est donc indispensable.
