# Crust

Rust, avec des **classes** : une extension syntaxique de Rust (fichiers `.rsc`)
qui regroupe `struct` + `impl` + `impl Trait`, simplifie les modules, et reste
**100 % compatible Rust** (toolchain officielle, crates.io, binaire identique).

```rust
// src/robot/Servomoteur.rsc
import Moteur from "./Moteur.rsc";

pub class Servomoteur over Moteur(parent) {   // champ `pub parent: Moteur`
    pub position: f32,

    pub fn aller_a(&mut self, position: f32) { self.position = position; }

    impl Default {
        fn default() -> Self { Self { parent: Moteur::new(), position: 0.0 } }
    }
}
```

## Démarrer

```sh
cargo install --path crates/cargo-crust   # cargo crust …
cargo install --path crates/crust-lsp     # rust-analyzer pour les .rsc
code --install-extension editors/vscode/crust-0.1.1.vsix

cargo crust new mon_projet                # ou `cargo crust init` dans un projet existant
cd mon_projet && cargo crust run
```

Exemple complet : `exemples/robot/` (l'ouvrir dans sa propre fenêtre VS Code).

## Commandes

| Commande | Rôle |
|---|---|
| `cargo crust build / check / run / test / clippy` | comme cargo, erreurs affichées dans les `.rsc` |
| `cargo crust new`, `cargo crust init` | créer un projet / installer Crust dans un projet |
| `cargo crust expand <fichier.rsc>` | voir le Rust généré |
| `cargo crust explain CR0002` | expliquer une erreur Crust |
| `cargo crust fmt [--check]` | formater les `.rsc` (et les `.rs` via `cargo fmt`) |
| `cargo crust eject` | revenir à du Rust standard (retire Crust du paquet) |

`cargo build` seul fonctionne aussi : il suffit de `crust-build` en dépendance de build.

## Projets qui utilisent Crust

- [RustUI](https://github.com/webshaker/RustUI) : widgets iced partagés, sources en `.rsc`.
  Pour s'en servir, rien à installer : cargo récupère `crust-build` depuis ce dépôt.
- Simulateur électronique.

Ces projets dépendent de `crates/crust-build` par `git` : ne pas déplacer ce crate.

## Dépôt

| Chemin | Contenu |
|---|---|
| `crates/crust-syntax` | lexer, parseur léger, transpileur `.rsc` → Rust, source maps |
| `crates/crust-build` | intégration Cargo (`build.rs`), arbre de modules |
| `crates/cargo-crust` | sous-commande `cargo crust` |
| `crates/crust-lsp` | proxy LSP devant rust-analyzer |
| `editors/vscode` | extension VS Code |
| `Specs/` | cahier des charges, guide de migration |
| `spikes/` | prototypes de validation et rapport |

Tests : `cargo test --workspace` (le test de l'éditeur demande un rust-analyzer, trouvé automatiquement).
