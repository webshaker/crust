# Guide : convertir un projet Rust vers Crust (`.rsc`)

Destinataire : un agent (ou un développeur) qui travaille sur un projet Rust existant et doit le restructurer avec la syntaxe Crust. La spécification complète est dans `rsc_cahier_des_charges_codex.md`, dans ce même dossier. Ce guide se suffit pour la migration.

---

## 1. À lire avant de commencer : état de Crust (2026-10-21)

| Fonction | État |
|---|---|
| `cargo build`, `cargo run`, `cargo test`, `cargo clippy` sur un projet `.rsc` | ✅ fonctionne |
| `class`, `over`, `impl Trait` dans une classe, generics, `import` | ✅ |
| Mélange `.rs` / `.rsc` dans une même crate, crate lib + bin | ✅ |
| `cargo crust build/check/run/test/clippy` : erreurs et warnings affichés dans les `.rsc`, au format rustc | ✅ (phase P3) |
| `cargo build` simple : bonne ligne et bonne colonne, mais chemin de la copie générée dans `target/` | ✅ (voir 6) |
| `cargo crust new`, `cargo crust init` (installation automatique), `cargo crust explain CRxxxx` | ✅ |
| **VS Code** (rust-analyzer via `crust-lsp` + extension « Crust ») : complétion, survol, définition, références, renommage, diagnostics, coloration dans les `.rsc` | ✅ (phases P4–P5), après activation dans le workspace (voir 2.5) |
| `cargo crust fmt` (et formatage à l'enregistrement dans VS Code) | ✅ |
| `src/bin/*.rsc` (avec `src/bin/<nom>.rs` d'une ligne, créé par `cargo crust init`) | ✅ |
| `cargo crust eject` : retour à du Rust pur | ✅ |
| `.rsc` dans `tests/`, `examples/`, `benches/` | ❌ : ces fichiers restent en `.rs` |
| Publication sur crates.io | ❌ : `crust-build` n'est pas publié, dépendance par chemin local uniquement |

**Conséquence** : la migration est possible, le binaire produit est identique, et l'éditeur fonctionne sur les `.rsc`. Crust reste jeune. Faire la migration :

- **sur une branche git dédiée**, jamais directement sur la branche principale;
- **progressivement**, module par module, en compilant et en testant après chaque étape;
- **seulement si l'utilisateur l'a validé**, en connaissant ces limites.

---

## 2. Installer Crust dans le projet

Chemin local de Crust : `/home/rtx/crust`. La commande `cargo crust` est installée sur cette machine. Si elle manque : `cargo install --path /home/rtx/crust/crates/cargo-crust`.

**Le plus simple** : à la racine du paquet, lancer `cargo crust init`. La commande fait les étapes 2.1 à 2.3 ci-dessous (option A) et ne fait rien si Crust est déjà installé. Les sections suivantes détaillent ce qu'elle fait.

### 2.1. `Cargo.toml` du paquet

Dans chaque paquet qui contiendra des `.rsc` :

```toml
[build-dependencies]
crust-build = { path = "/home/rtx/crust/crates/crust-build" }
```

L'édition doit être 2018 ou plus (2021 ou 2024 recommandées).

### 2.2. `build.rs`

S'il n'existe pas, le créer à la racine du paquet :

```rust
fn main() {
    crust_build::run();
}
```

S'il existe, ajouter `crust_build::run();` au début de son `main`.

### 2.3. Racine de crate

Cargo exige toujours `src/main.rs` et/ou `src/lib.rs`. Deux options :

**A. Garder la racine en `.rs`** (recommandé pour commencer). Ajouter cette ligne dans `src/main.rs`, ou `root_lib.rs` dans `src/lib.rs` :

```rust
include!(concat!(env!("OUT_DIR"), "/crust/root_main.rs"));
```

**B. Racine en `.rsc`.** Déplacer le contenu dans `src/main.rsc` (ou `src/lib.rsc`) et réduire `src/main.rs` à :

```rust
// Attributs de crate éventuels (#![...]) ici, jamais dans main.rsc.
include!(concat!(env!("OUT_DIR"), "/crust/root_main.rs"));
```

Pour une crate lib + bin : `src/lib.rs` inclut `root_lib.rs`, `src/main.rs` inclut `root_main.rs`. Les modules `.rsc` appartiennent à la lib.

### 2.4. Commandes utiles

```sh
cargo crust check            # comme cargo check, erreurs affichées dans les .rsc
cargo crust build | run | test | clippy   # idem ; tous les arguments de cargo sont acceptés
cargo crust expand src/robot/Moteur.rsc          # Rust équivalent, formaté
cargo crust expand --lines src/robot/Moteur.rsc  # version réellement compilée
cargo crust explain CR0002                       # explication d'une erreur Crust
cargo crust fmt [--check]                        # formate les .rsc, puis cargo fmt
cargo crust eject [--dry-run]                    # convertit tout le paquet en Rust standard
```

**Utiliser `cargo crust check` / `cargo crust test` plutôt que `cargo check` / `cargo test`** pendant la migration : les messages pointent directement vers le `.rsc` à corriger. `--message-format=json` est accepté et renvoie le JSON de cargo, avec les positions déjà converties vers les `.rsc`.

### 2.5. VS Code

1. L'extension « Crust » doit être installée (`code --install-extension /home/rtx/crust/editors/vscode/crust-0.1.0.vsix`), ainsi que `crust-lsp` (`cargo install --path /home/rtx/crust/crates/crust-lsp`).
2. À l'ouverture d'un projet Crust, l'extension propose de faire passer rust-analyzer par `crust-lsp` : répondre « Activer ». Sinon, utiliser la commande « Crust : utiliser crust-lsp dans ce workspace ». Elle écrit `"rust-analyzer.server.path": "…/crust-lsp"` dans `.vscode/settings.json` du workspace.
3. Bouton « afficher le Rust généré » dans la barre d'un `.rsc`.

Un agent sans éditeur n'a pas besoin de cette étape.

### 2.6. Vérification de l'installation

Avant de convertir le moindre fichier, lancer `cargo build && cargo test`. Ça doit passer exactement comme avant, avec les mêmes warnings.

---

## 3. Règles de conversion

Principe : **ne jamais changer la sémantique.** Chaque construction Crust est une écriture plus courte d'un Rust identique, et le binaire final doit être le même.

### 3.1. `struct` + `impl` → `class`

Avant (`src/robot/moteur.rs`) :

```rust
/// Un moteur.
#[derive(Debug, Clone)]
pub struct Moteur {
    pub vitesse: u32,
    couple: f32,
}

impl Moteur {
    pub fn new() -> Self { Self { vitesse: 0, couple: 1.0 } }
}

impl Default for Moteur {
    fn default() -> Self { Self::new() }
}
```

Après (`src/robot/Moteur.rsc`) :

```rust
/// Un moteur.
#[derive(Debug, Clone)]
pub class Moteur {
    pub vitesse: u32,
    couple: f32,

    pub fn new() -> Self { Self { vitesse: 0, couple: 1.0 } }

    impl Default {
        fn default() -> Self { Self::new() }
    }
}
```

Règles :

- Les attributs et la doc de la `struct` vont sur la `class`. `#[cfg]` et `#[allow]` s'appliquent aussi aux `impl` générés.
- Les champs s'écrivent `nom: Type,` : la virgule est obligatoire, y compris quand une méthode suit.
- Dans une classe, `impl Trait { }` signifie `impl Trait for LaClasse { }`. Les generics supplémentaires s'écrivent `impl<U> From<U> where U: Into<u32> { }`.
- `unsafe impl Send { }` est accepté.
- Les generics et le `where` de la classe s'écrivent comme pour une struct : `pub class Pile<T: Clone> where T: Default { ... }`. Ils sont recopiés sur chaque `impl`.

### 3.2. Ce qui reste en Rust ordinaire (même dans un `.rsc`)

Tout Rust valide reste valide dans un `.rsc`. **Ne pas convertir** :

| Cas | Pourquoi |
|---|---|
| `enum`, `trait`, `type`, fonctions libres, `const`, `static`, `macro_rules!` | `class` ne concerne que les structs à champs nommés |
| Struct tuple `struct Metres(f64);`, struct unité `struct Marqueur;` | non pris en charge (et une classe sans champ devient `struct X {}`, pas `struct X;`) |
| `impl` partiel : `impl Pile<i32> { }`, `impl<T: Copy> Pile<T> { }` | les `impl` de la classe reprennent exactement ses generics : écrire ces blocs **hors** de la classe, dans le même fichier |
| `impl Trait for AutreType` | hors de la classe |
| Blocs `impl` générés par des macros | à laisser tels quels |
| `type X = …;` dans une classe | interdit (inherent associated types instables) ; seulement dans `impl Trait { }` |
| Struct avec un attribut macro qui doit voir les méthodes | une macro attribut sur la classe ne voit que la `struct` générée |

### 3.3. Composition : `over`

Un champ composant peut s'écrire avec `over` :

```rust
pub class Servomoteur over Moteur {      // génère `pub moteur: Moteur,` en premier champ
    position: f32,
}
pub class Axe over Moteur(motorisation) { }   // nom explicite
```

- Le champ est placé **en premier**, ce qui compte pour l'ordre de drop et pour `#[repr(C)]`. Si l'ordre des champs importe, garder un champ ordinaire.
- Sa visibilité est celle de la classe. On peut la préciser : `over pub(crate) Moteur`.
- Aucune délégation : `s.moteur.accelerer()`, jamais `s.accelerer()`.
- `over` est **facultatif**. Ne l'utiliser que si le champ est bien un composant et qu'il garde son nom déduit ou explicite. Sinon, laisser un champ normal.

### 3.4. Fichiers et modules

- **Un `.rsc` sous `src/` est automatiquement un module.** `src/robot/Moteur.rsc` devient `crate::robot::moteur` : le nom de fichier est converti en snake_case.
- Pas de `mod.rs`, ni de déclaration `mod` pour les `.rsc`.
- **Chemin court** : les items `pub` d'un `.rsc` sont aussi accessibles par le répertoire, `crate::robot::Moteur`.
- **Convention** : un fichier par classe, nommé comme la classe (`Moteur.rsc`). Plusieurs items par fichier restent permis.
- **Conflits interdits** : deux fichiers ne doivent pas donner le même module (`Moteur.rsc` + `moteur.rsc`, ou `Moteur.rsc` + un `moteur.rs` déjà monté). Sinon : erreur CR0009.
- **Répertoire mixte** : si un répertoire contenant des `.rsc` correspond à un module `.rs` existant (`src/outils.rs` ou `src/outils/mod.rs`), ce fichier doit contenir :
  ```rust
  include!(concat!(env!("OUT_DIR"), "/crust/dir/outils.rs"));
  ```
  Sinon : erreur CR0010, qui donne la ligne exacte. Pour un sous-répertoire : `/crust/dir/a/b.rs`.
- **Supprimer un `mod.rs`** devenu inutile, c'est-à-dire qui ne faisait que déclarer des sous-modules désormais en `.rsc`, et retirer les `mod x;` correspondants.
- **Modules `.rs` existants** : leurs `mod x;` continuent de fonctionner. Un `mod x;` écrit dans un `.rsc` est aussi pris en charge, avec la même sémantique qu'en Rust.

### 3.5. `import`

Pour les fichiers de la crate, on peut remplacer les `use crate::…` :

```rust
import Moteur from "./Moteur.rsc";                  // relatif au fichier courant
import { Etat, Mode as ModeMoteur } from "@/robot/Moteur";   // @/ = src/, extension facultative
import * from "../outils/Pince.rsc";
pub import Moteur from "./Moteur.rsc";              // réexport (pub use)
import { lire } from "./capteur.rs";                // fonctionne aussi vers un .rs
```

- `use crate::…`, `use super::…` et `use` vers les crates externes restent valides et restent nécessaires pour les dépendances (`use serde::Serialize;`).
- Un `.rs` importé mais monté nulle part est monté automatiquement.
- **Ne pas mélanger** les deux mécanismes : déclarer un `.rs` par `mod` **et** l'importer est valide, mais le déclarer par **deux** `mod` puis l'importer donne l'erreur CR0011.
- Dans `main.rsc` d'une crate lib + bin, `import` vers un module de la lib produit `use nom_de_la_lib::…` automatiquement.
- Les fichiers `.rs` ne peuvent pas utiliser `import` : ils gardent `use crate::robot::Moteur;`.

### 3.6. Racine `main.rsc` / `lib.rsc`

- Interdit : `#![...]` de premier niveau (erreur CR0015). Les mettre dans `src/main.rs` / `src/lib.rs`, au-dessus de la ligne `include!`.
- La doc de crate `//!` y est ignorée. Pour une lib documentée, la garder dans `src/lib.rs`.

### 3.7. Mots réservés

`class`, `over`, `import` et `from` ne sont **pas** réservés : `let from = …;`, `T::from(x)` et `fn class()` restent valides.

---

## 4. Procédure recommandée pour l'agent

1. Créer une branche : `git switch -c crust-migration`.
2. Faire l'installation (section 2). `cargo build && cargo test` doivent donner exactement le même résultat qu'avant. Committer.
3. Relever la référence :
   - sortie de `cargo test`;
   - liste des warnings de `cargo clippy --all-targets`;
   - si c'est une lib, l'API publique (`cargo doc` ou la liste des `pub use` de `lib.rs`).
4. Convertir **un module à la fois**, en commençant par les feuilles (modules sans dépendants dans la crate) :
   1. renommer ou créer le `.rsc`, convertir `struct` + `impl` en `class`;
   2. retirer le `mod x;` et le `mod.rs` devenus inutiles;
   3. adapter les chemins si le nom de module change (par exemple `moteur.rs` → `Moteur.rsc` garde `crate::…::moteur`, mais `motor_ctrl.rs` → `MoteurCtrl.rsc` donne `moteur_ctrl`);
   4. `cargo crust check && cargo crust test && cargo crust clippy --all-targets`;
   5. committer, un commit par module.
5. **Ne pas** profiter de la migration pour changer la logique ou renommer l'API publique : ce sont des diffs à faire séparément. Lancer `cargo crust fmt` sur les fichiers convertis est en revanche recommandé.
6. À la fin : mêmes tests au vert, mêmes warnings clippy, même API publique. `cargo crust expand` sur quelques fichiers permet de vérifier que le Rust généré correspond à l'original.

---

## 5. Exemple de structure cible

```text
mon_projet/
├── Cargo.toml          # + [build-dependencies] crust-build
├── build.rs            # crust_build::run();
└── src/
    ├── main.rs         # include!(concat!(env!("OUT_DIR"), "/crust/root_main.rs"));
    ├── main.rsc        # fn main() { ... } + import ...
    ├── config.rs       # Rust ordinaire, importé depuis un .rsc ou monté par `mod`
    └── robot/
        ├── Moteur.rsc
        ├── Servomoteur.rsc
        └── Capteur.rsc
```

---

## 6. Lire les erreurs du compilateur

Avec `cargo crust check` (ou `build`, `test`, `clippy`), les erreurs s'affichent comme d'habitude, directement dans le `.rsc` :

```text
error[E0308]: mismatched types
  --> src/robot/Moteur.rsc:11:24
   |
11 |         self.vitesse = "abc";
   |         ------------   ^^^^^ expected `u32`, found `&str`
```

Une suggestion de rustc qui porterait sur du code généré (par exemple la `struct`) n'est pas affichée, car elle ne s'appliquerait pas au `.rsc`.

Avec `cargo build` simple, les erreurs affichent le chemin de la copie générée :

```text
error[E0308]: mismatched types
  --> target/debug/build/mon_projet-1a2b3c/out/crust/src/robot/Moteur.rsc:11:24
```

Pour trouver le fichier à corriger, **retirer le préfixe `…/out/crust/`** : c'est `src/robot/Moteur.rsc`, ligne 11, colonne 24.

- **Code écrit à la main** (corps de méthodes, signatures, items Rust) : ligne et colonne sont exactes.
- **Erreur sur un champ ou un `#[derive]`** : avec `cargo build`, la position peut tomber sur la ligne de l'accolade fermante de la classe, car la `struct` générée y est écrite. `cargo crust` la ramène sur le champ.
- **Erreurs propres à Crust** : elles apparaissent sous la forme `error: [CR0002] …` à la bonne ligne, avec une ligne `= help:` qui donne la correction. Liste complète dans la spec, section 19.2.
  - Les plus courantes : CR0001 (syntaxe, souvent une virgule manquante après un champ), CR0002 (champ en collision avec `over`), CR0005 (élément interdit dans une classe).

---

## 7. Revenir en arrière

- Tout est sur la branche `crust-migration` : `git switch main` suffit.
- Pour garder le travail mais revenir à du Rust pur : `cargo crust eject`. Il convertit chaque `.rsc` en `.rs` lisible, remplace les lignes `include!` par de vrais `mod` et retire `crust-build`. Il refuse de tourner si le dépôt git a des modifications non committées (`--force` pour passer outre). `--dry-run` montre ce qui serait fait.

---

## 8. Signaler un problème de Crust

Crust est jeune. Si le code généré ne compile pas alors que l'équivalent Rust compilerait, ou si une erreur pointe au mauvais endroit :

1. Ne pas contourner en silence : réduire le cas à un `.rsc` minimal.
2. Joindre la sortie de `cargo crust expand --lines <fichier>` et le message du compilateur.
3. Le transmettre à l'utilisateur, qui le remontera au projet Crust (`/home/rtx/crust`).
