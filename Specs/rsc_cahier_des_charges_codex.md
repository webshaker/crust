# Cahier des charges — Crust : extension syntaxique Rust `.rsc`

Version 2 du cahier des charges. La version 1 d'origine est conservée dans `rsc_cahier_des_charges_codex.v1.md`.

---

# 0. Résumé

## 0.1. Ce que veut l'utilisateur final

1. **Rendre le développement Rust moins pénible** : moins de cérémonie (`struct` + `impl` + `impl Trait for` dispersés, `mod.rs`, `mod` + `use` en double), sans perdre ce qui fait Rust.
2. **Un équivalent de rust-analyzer dans VS Code** pour les `.rsc` : complétion, navigation, renommage, hover, diagnostics en temps réel, run/debug.
3. **Rapide à l'usage** : aucun ralentissement perceptible ni à la compilation ni dans l'éditeur.

## 0.2. Contrainte absolue

**Crust doit rester compatible Rust.** Concrètement (détaillé en section 2.2) :

- tout fichier `.rs` valide est un fichier `.rsc` valide, avec exactement le même sens;
- on utilise la toolchain **officielle, stable et non modifiée** (`rustc`, `cargo`, `rust-analyzer`, `rustfmt`, `clippy`);
- une crate utilisant des `.rsc` se compile avec un simple `cargo build` et peut être publiée sur crates.io;
- le binaire produit est identique à celui du Rust équivalent écrit à la main (zéro coût à l'exécution, zéro dépendance runtime).

## 0.3. Décisions principales

| Sujet | Décision | Section |
|---|---|---|
| Approche | **Transpileur** `.rsc` → `.rs`, pas de fork de `rustc` | 16 |
| Intégration Cargo | `build.rs` + crate `crust-build` (build-dependency) + une ligne `include!` | 17 |
| Diagnostics | Disposition du code généré **préservant lignes et colonnes** + source maps | 15, 18 |
| Erreurs propres à Crust | Émises sous forme de `compile_error!` à la bonne ligne : un seul canal d'erreurs | 19 |
| rust-analyzer | **Proxy LSP** (`crust-lsp`) devant le rust-analyzer officiel, pas de fork | 20 |
| VS Code | Petite extension qui s'appuie sur l'extension rust-analyzer officielle | 20 |
| Modules | Arbre de modules déduit des fichiers `.rsc`, plus de `mod.rs` | 12 |
| Visibilité du champ `over` | Hérite de la visibilité de la classe (et non plus « toujours `pub` ») | 8.3 |
| Porte de sortie | `cargo crust eject` convertit le projet en Rust standard lisible | 17.6 |

## 0.4. Changements par rapport à la v1

- Sections 15 (« intégration rustc ») et 18 (« fork de rust-analyzer ») de la v1 remplacées par un transpileur et un proxy LSP.
- Le champ `over` n'est plus toujours `pub` : il prend la visibilité de la classe, modifiable explicitement.
- Ajout : generics, lifetimes, `where`, attributs, constantes associées, `unsafe impl`, macros dans les classes.
- Ajout : mots-clés contextuels, règle de sur-ensemble strict de Rust.
- Ajout : règles précises de l'arbre de modules, cohabitation `mod` / `import`, crates lib + bin.
- Ajout : variantes d'`import` (`{ A, B as C }`, `*`, `pub import`, extension facultative, chemin `@/`).
- Ajout : codes d'erreur `CRxxxx`, objectifs de performance chiffrés, débogage, plan de tests, risques et spikes.

---

# 1. Objectif général

Créer une extension syntaxique de Rust permettant d'écrire certains types de manière plus lisible, sans modifier les règles sémantiques de Rust.

Le projet ne cherche **pas** à transformer Rust en langage orienté objet.

L'objectif est uniquement d'ajouter du sucre syntaxique autour de constructions Rust existantes :

- regrouper dans une même déclaration :
  - les champs d'une `struct`,
  - les méthodes `impl`,
  - les implémentations de `trait`;
- simplifier l'organisation des fichiers;
- permettre un fichier par « classe »;
- simplifier les imports entre fichiers;
- fournir une syntaxe légère de composition avec `over`;
- conserver une compatibilité totale avec Rust standard;
- permettre à des fichiers `.rsc` et `.rs` de cohabiter dans le même projet et dans la même crate;
- fournir une expérience éditeur équivalente à rust-analyzer.

Les fichiers `.rs` conservent **exactement** la syntaxe Rust standard et ne sont jamais transformés.

Les fichiers `.rsc` acceptent la syntaxe Rust habituelle plus les extensions définies ici.

---

# 2. Principe fondamental

## 2.1. Traduction

Toute construction `.rsc` a une traduction claire, déterministe et documentée vers du Rust standard.

Le compilateur Rust officiel continue à appliquer ses règles pour :

- le typage;
- le borrow checker;
- les lifetimes;
- les generics;
- les traits;
- la visibilité;
- la génération de code;
- les optimisations;
- l'ABI Rust.

Le projet n'introduit pas de nouveau modèle objet.

## 2.2. Garanties de compatibilité (exigences testables)

| # | Garantie | Comment c'est vérifié |
|---|---|---|
| C1 | **Sur-ensemble strict** : tout fichier `.rs` valide, renommé en `.rsc`, est accepté et sa traduction est **identique octet pour octet** à l'entrée. | Test sur un corpus de fichiers Rust réels (section 27.4) |
| C2 | **Toolchain officielle** : aucune modification de `rustc`, `cargo`, `rust-analyzer`, `rustfmt`, `clippy`. Fonctionne en Rust **stable**. | CI sur stable uniquement |
| C3 | **`cargo build` suffit** : une crate `.rsc` se compile sans `cargo crust`, avec la seule build-dependency `crust-build`. | Test CI sans `cargo-crust` installé |
| C4 | **Publication** : une crate `.rsc` peut être publiée sur crates.io et utilisée par une crate Rust ordinaire. | Test `cargo package` + dépendance par chemin |
| C5 | **Zéro coût** : aucune dépendance runtime; le code généré est celui qu'un humain écrirait. | Comparaison du MIR/binaire avec l'équivalent manuel |
| C6 | **Transparence** : un type `.rsc` est un type Rust normal pour le reste de la crate et pour les autres crates. | Tests de coexistence (section 26) |
| C7 | **Réversibilité** : `cargo crust eject` produit un projet 100 % `.rs` lisible et formaté. | Test : eject → `cargo build` → tests identiques |
| C8 | **Éditions** : éditions Rust 2018, 2021 et 2024 supportées. L'édition 2015 est refusée (les chemins `crate::` sont requis). | Tests par édition |

---

# 3. Extension de fichier

Les fichiers utilisant la nouvelle syntaxe portent l'extension `.rsc`.

```text
src/
    main.rs          (stub d'une ligne, voir 17.2)
    main.rsc
    Moteur.rsc
    Servomoteur.rsc
    Capteur.rs
```

- Les `.rs` restent du Rust standard, jamais réécrits.
- Les `.rsc` passent par le transpileur Crust.
- Les deux peuvent se référencer mutuellement.
- Encodage : UTF-8 obligatoire (comme Rust). Les fins de ligne `\n` et `\r\n` sont acceptées et préservées.

---

# 4. Syntaxe `class`

## 4.1. Exemple minimal

```rust
pub class Moteur {
    pub vitesse: u32,

    pub fn new() -> Self {
        Self { vitesse: 0 }
    }

    pub fn accelerer(&mut self, delta: u32) {
        self.vitesse += delta;
    }
}
```

Traduction conceptuelle (la disposition réelle du code généré est décrite en section 15) :

```rust
pub struct Moteur {
    pub vitesse: u32,
}

impl Moteur {
    pub fn new() -> Self {
        Self { vitesse: 0 }
    }

    pub fn accelerer(&mut self, delta: u32) {
        self.vitesse += delta;
    }
}
```

`class` n'introduit aucune nouvelle sémantique : c'est une `struct` à champs nommés et ses blocs `impl`, écrits ensemble.

## 4.2. Grammaire

Notation EBNF. Les non-terminaux non définis ici (`Type`, `BlockExpr`, `OuterAttribute`, `Visibility`, `GenericParams`, `WhereClause`, `TypePath`, `MacroInvocationSemi`, etc.) sont ceux de la [Rust Reference](https://doc.rust-lang.org/reference/).

```ebnf
ClassDecl     = OuterAttribute* Visibility? "class" IDENTIFIER GenericParams?
                OverClause? WhereClause? "{" ClassMember* "}" ;

OverClause    = "over" OverItem ( "," OverItem )* ;
OverItem      = Visibility? OverType ( "(" IDENTIFIER ")" )? ;
OverType      = TypePath | Type ;          (* voir 8.4 *)

ClassMember   = ClassField
              | ClassFunction
              | ClassConst
              | ClassTraitImpl
              | MacroInvocationSemi ;

ClassField    = OuterAttribute* Visibility? IDENTIFIER ":" Type ( "," | &"}" ) ;
ClassFunction = OuterAttribute* Visibility? Function ;      (* fn, const fn, async fn, unsafe fn, extern "abi" fn *)
ClassConst    = OuterAttribute* Visibility? ConstantItem ;  (* const NOM: T = expr; *)

ClassTraitImpl = OuterAttribute* "unsafe"? "impl" GenericParams? "!"? TypePath
                 WhereClause? "{" InnerAttribute* AssociatedItem* "}" ;
```

Pour distinguer les membres (après les attributs et la visibilité éventuelle), le parseur regarde au plus deux tokens :

| Tokens | Membre |
|---|---|
| `IDENT :` (pas `::`) | champ |
| `fn`, `const fn`, `async`, `unsafe fn`, `extern`, `default fn` | fonction / méthode |
| `const IDENT :` | constante associée |
| `impl`, `unsafe impl` | implémentation de trait |
| `chemin !` | invocation de macro (placée dans l'`impl` inhérent) |
| `type`, `struct`, `enum`, `union`, `trait`, `mod`, `use`, `static`, `extern crate`, `class`, `import` | **erreur CR0005** (voir 4.6) |

## 4.3. Ordre des membres

- Champs, méthodes et `impl` de traits peuvent être **librement entremêlés**.
- L'ordre relatif des champs est conservé dans la `struct` générée, ce qui détermine l'ordre de drop et la disposition `#[repr(C)]`.
- Les champs générés par `over` sont placés **avant** les champs déclarés, dans l'ordre de la clause `over` (voir 8.5).

## 4.4. Generics, lifetimes, `where`

```rust
pub class Pile<'a, T: Clone, const N: usize = 16>
where
    T: Default,
{
    items: [T; N],
    nom: &'a str,

    pub fn len(&self) -> usize { N }
}
```

Traduction :

```rust
pub struct Pile<'a, T: Clone, const N: usize = 16>
where
    T: Default,
{
    items: [T; N],
    nom: &'a str,
}

impl<'a, T: Clone, const N: usize> Pile<'a, T, N>
where
    T: Default,
{
    pub fn len(&self) -> usize { N }
}
```

Règles :

1. La `struct` reçoit les `GenericParams` et la `WhereClause` **tels quels**.
2. Chaque `impl` généré reçoit les mêmes paramètres **sans les valeurs par défaut** (interdites dans un `impl`), avec leurs bornes.
3. Le type `Self` de chaque `impl` est `NomClasse<arguments>`, où les arguments sont les noms des paramètres dans l'ordre de déclaration (lifetimes, types, consts).
4. La `WhereClause` de la classe est recopiée sur chaque `impl` généré.

## 4.5. Attributs

| Attribut sur la classe | Destination |
|---|---|
| `#[cfg(...)]`, `#[cfg_attr(...)]` | **recopié** sur la `struct` et sur chaque `impl` généré |
| `#[allow]`, `#[warn]`, `#[deny]`, `#[forbid]`, `#[expect]` | **recopié** sur la `struct` et sur chaque `impl` généré |
| Commentaires de doc `///` et `#[doc]` | `struct` seulement |
| `#[derive]`, `#[repr]`, `#[non_exhaustive]`, `#[must_use]`, attributs d'outils (`#[serde(...)]`…), macros attributs | `struct` seulement |

- Les attributs sur un champ restent sur ce champ dans la `struct`.
- Les attributs sur une méthode, une constante ou un `impl Trait` restent sur cet élément.
- Une macro attribut sur la classe (`#[ma_macro] pub class X {...}`) ne s'applique qu'à la `struct` générée. C'est documenté comme limitation : la macro ne voit pas les méthodes.

## 4.6. Interdits dans le corps d'une classe (CR0005)

- Les items imbriqués (`struct`, `enum`, `trait`, `mod`, `fn` hors méthode…) : à déclarer au niveau du module, à côté de la classe.
- Les `type` associés inhérents : ils sont instables en Rust. Ils restent autorisés dans un `impl Trait { }`.
- `use` et `import` : à placer au niveau du fichier.
- `impl Trait for AutreType` : à écrire hors de la classe.

Le message d'erreur propose toujours la correction.

## 4.7. Cas particuliers

- **Classe sans champ** : `class Marqueur { ... }` donne `struct Marqueur {}`, une struct à accolades vides. La valeur s'écrit donc `Marqueur {}` et non `Marqueur`.
- **Classe sans méthode** : aucun `impl` inhérent superflu n'est exigé. L'`impl` vide éventuellement généré (section 15) ne doit produire aucun warning.
- **Struct tuple ou unité** : pas de syntaxe `class` en V1. On utilise `struct` + `impl`, qui restent disponibles dans les `.rsc`.
- **Tout Rust reste valide** dans un `.rsc` en dehors des classes : `struct`, `enum`, `trait`, `impl`, `macro_rules!`, `mod` inline, `#[cfg(test)] mod tests { }`, etc.
- Une classe peut aussi être complétée par des blocs `impl NomClasse { }` ou `impl Trait for NomClasse { }` écrits hors de la classe, dans le même fichier ou ailleurs.
- **Emplacement** : une `class` est autorisée au niveau d'un module (fichier ou `mod x { }` inline), pas dans le corps d'une fonction.

---

# 5. Traits dans une `class`

```rust
pub class Moteur {
    pub vitesse: u32,

    pub fn new() -> Self { Self { vitesse: 0 } }

    impl Default {
        fn default() -> Self { Self::new() }
    }
}
```

Traduction :

```rust
impl Default for Moteur {
    fn default() -> Self { Self::new() }
}
```

Dans une classe, `impl Trait { ... }` signifie `impl Trait for NomDeLaClasse { ... }`.

## 5.1. Traits génériques et paramètres supplémentaires

```rust
pub class Pile<T> {
    items: Vec<T>,

    impl<U> From<Vec<U>> where U: Into<T> {
        fn from(v: Vec<U>) -> Self {
            Self { items: v.into_iter().map(Into::into).collect() }
        }
    }
}
```

Traduction :

```rust
impl<T, U> From<Vec<U>> for Pile<T>
where
    U: Into<T>,
{
    fn from(v: Vec<U>) -> Self { ... }
}
```

Règles de fusion :

- Paramètres : d'abord les lifetimes (classe puis impl), ensuite les types et consts (classe puis impl). Rust exige les lifetimes en premier.
- La clause `where` de la classe est suivie de celle de l'`impl`.
- Un nom de paramètre en double donne l'erreur `rustc` E0403, ramenée vers le `.rsc`.

## 5.2. Variantes

- `unsafe impl Send { }` donne `unsafe impl Send for X { }`.
- `impl !Trait { }` donne `impl !Trait for X { }` (nightly uniquement, comme en Rust).
- Les `type` et `const` associés au trait sont autorisés dans le bloc.
- `impl Trait for Autre { }` dans une classe donne l'erreur CR0005.

---

# 6. Pas d'héritage objet

Le projet n'introduit pas l'héritage de classes au sens C++/Java.

En particulier, il ne faut pas ajouter automatiquement :

- de vtable;
- de méthodes virtuelles;
- de conversion implicite classe fille → classe parent;
- de surcharge dynamique;
- de polymorphisme objet supplémentaire;
- de RTTI supplémentaire;
- de nouveau système de dispatch.

Rust conserve ses mécanismes habituels : composition, traits, generics, `dyn Trait`.

---

# 7. Syntaxe `over` : composition automatique

`over` déclare un **champ composant**. Ce n'est pas un héritage.

```rust
pub class Servomoteur over Moteur {
    position: f32,
}
```

Traduction :

```rust
pub struct Servomoteur {
    pub moteur: Moteur,
    position: f32,
}
```

---

# 8. Règles de `over`

## 8.1. Nom déduit

Sans nom explicite, le nom du champ est le **dernier segment du chemin du type**, sans ses arguments génériques, converti en `snake_case`.

L'algorithme découpe aux frontières de mots : minuscule ou chiffre suivi d'une majuscule, et sigle suivi d'un mot (`HTTPServer`). Le résultat est toujours accepté par le lint `non_snake_case` de rustc. Un nom qui tombe sur un mot-clé est émis en identifiant brut, par exemple `Box` donne `r#box`.

```text
Moteur              -> moteur
MoteurBrushless     -> moteur_brushless
RobotArm            -> robot_arm
HTTPServer          -> http_server
crate::io::Uart2    -> uart2
Wrapper<T>          -> wrapper
```

## 8.2. Nom explicite

```rust
pub class Axe over Moteur(motorisation) {
    position: f32,
}
```

donne :

```rust
pub struct Axe {
    pub motorisation: Moteur,
    position: f32,
}
```

`over Type` équivaut à `over Type(nom_deduit_en_snake_case)`. Un nom explicite peut être un identifiant brut (`r#type`).

## 8.3. Visibilité du champ généré

**Changement par rapport à la v1** : le champ prend par défaut **la visibilité de la classe**.

| Déclaration | Champ généré |
|---|---|
| `pub class S over Moteur` | `pub moteur: Moteur` (comme la v1) |
| `pub(crate) class S over Moteur` | `pub(crate) moteur: Moteur` |
| `class S over Moteur` | `moteur: Moteur` (privé) |
| `pub class S over pub(crate) Moteur` | `pub(crate) moteur: Moteur` (explicite) |

Raison : dans le cas courant (`pub class`), le résultat est identique à la v1, et on évite d'exposer des champs privés contre la volonté du développeur.

## 8.4. Types acceptés

- **Type chemin**, éventuellement générique : `Moteur`, `crate::a::Moteur`, `Vec<T>`. Le nom peut être déduit.
- **Autre type** (référence, tableau, tuple, `Box<dyn Trait>`, …) : le nom explicite est **obligatoire**, par exemple `over [u8; 4](octets)` ou `over &'a Config(config)`. Sans nom : erreur CR0004.
- Pour lever l'ambiguïté du parseur, `Fn(A) -> B` n'est pas accepté comme `OverType` nu. Il faut l'envelopper, par exemple `over Box<dyn Fn(u32)>(callback)`. La parenthèse qui suit un `OverType` introduit toujours le nom du champ.

## 8.5. Composants multiples

```rust
pub class Robot over Moteur(gauche), Moteur(droite), Capteur {
    nom: String,
}
```

donne :

```rust
pub struct Robot {
    pub gauche: Moteur,
    pub droite: Moteur,
    pub capteur: Capteur,
    nom: String,
}
```

C'est de la composition pure : plusieurs champs, sans aucune délégation. Il n'y a donc pas d'héritage multiple. Les champs `over` sont placés **en tête**, dans l'ordre de la clause. Ils sont donc droppés **avant** les champs déclarés (règle Rust : ordre de déclaration).

## 8.6. Collisions

- Un champ déclaré porte le même nom qu'un champ `over` : **CR0002**.
- Deux composants `over` ont le même nom (déduit ou explicite) : **CR0003**. Par exemple, `over Moteur, Moteur` impose des noms explicites.

L'erreur pointe sur le `.rsc` d'origine, avec les deux positions.

---

# 9. Accès au composant

```rust
let mut s = Servomoteur::new();
s.moteur.vitesse = 1200;
s.moteur.accelerer(100);
```

Aucune délégation implicite : `s.accelerer()` **n'appelle pas** `s.moteur.accelerer()`. On écrit explicitement l'accès au composant.

---

# 10. Constructeurs

Pas de constructeur spécial. Les fonctions associées Rust restent la norme.

```rust
pub class Servomoteur over Moteur {
    position: f32,

    pub fn new() -> Self {
        Self { moteur: Moteur::new(), position: 0.0 }
    }
}
```

Le champ `over` s'initialise comme n'importe quel champ. Aucun constructeur n'est généré automatiquement. `#[derive(Default)]` fonctionne normalement si les types le permettent.

---

# 11. Mots-clés contextuels

`class`, `over`, `import` et `from` **ne sont pas des mots-clés réservés**. Ils ne sont reconnus qu'à des positions précises :

| Mot | Reconnu uniquement… |
|---|---|
| `class` | en position d'item, après attributs et visibilité, **suivi d'un identifiant** |
| `import` | en position d'item, après la visibilité éventuelle, suivi d'un identifiant, de `{` ou de `*`, avec un `from "…"` avant le `;` |
| `over` | dans l'en-tête d'une classe, après les paramètres génériques |
| `from` | dans une instruction `import` |

Justification : en Rust, une suite `IDENT IDENT` en position d'item n'est jamais valide (seules les macros `ident!` commencent par un identifiant). Ces constructions ne peuvent donc **jamais** entrer en conflit avec du Rust valide, ce qui garantit C1.

Restent donc valides dans un `.rsc` : `let from = 3;`, `T::from(x)`, `fn class(&self)`, `struct S { over: u8, import: bool }`, `class!()`, etc.

---

# 12. Organisation des fichiers et arbre de modules

Objectif : **un fichier par classe, sans `mod.rs` ni déclarations `mod`**.

## 12.1. Montage automatique des `.rsc`

Tout fichier `.rsc` sous `src/` est automatiquement monté comme module. Son chemin de module est déduit de son emplacement :

| Fichier | Module |
|---|---|
| `src/main.rsc` / `src/lib.rsc` | racine de la crate |
| `src/Moteur.rsc` | `crate::moteur` |
| `src/robot/RobotArm.rsc` | `crate::robot::robot_arm` |
| `src/robot/` (répertoire) | `crate::robot` (module généré) |

Règles :

1. Le nom de module d'un fichier est son nom sans extension, converti en `snake_case` (même algorithme que 8.1). Aucun warning `non_snake_case`.
2. Un répertoire contenant au moins un `.rsc` (directement ou plus bas) devient un module. Son nom doit être un identifiant Rust valide, sinon **CR0012**.
3. Deux fichiers qui donnent le même nom de module dans un même répertoire (`Moteur.rsc` et `moteur.rsc`, ou `Moteur.rsc` et `moteur.rs` monté) : **CR0009**.
4. Visibilité : les modules générés sont `pub`. On peut le changer dans `Cargo.toml` :
   ```toml
   [package.metadata.crust]
   module-visibility = "pub(crate)"   # "pub" par défaut
   ```
5. **Réexport court** : chaque module de répertoire réexporte les items publics de ses fichiers `.rsc` (`pub use self::moteur::*;`). `crate::robot::Moteur` et `crate::robot::moteur::Moteur` désignent alors **le même type**. Un nom présent dans deux fichiers d'un même répertoire n'est une erreur que s'il est utilisé via le chemin court (règle Rust des globs ambigus).

## 12.2. Cohabitation avec les modules `.rs`

Les `.rs` suivent les règles Rust standard (`mod x;`, `#[path]`, `mod.rs`). Crust **lit** les déclarations `mod` des `.rs` (lexer léger, sans expansion de macros) pour connaître le chemin de module de chaque `.rs`. Il ne les modifie jamais.

- Un `.rs` déjà monté par un `mod` garde son chemin de module. Un `import` vers ce fichier produit un `use` vers ce chemin. Il n'y a **jamais de double montage**, ce qui garantit l'unicité du type (section 13.4).
- Un `.rs` qui n'est monté nulle part mais qui est **importé** depuis un `.rsc` est monté automatiquement par Crust, au chemin déduit de son emplacement (règle 12.1).
- Un `.rs` ni monté ni importé est ignoré, comme en Rust standard.
- **Répertoire mixte** : si un répertoire contenant des `.rsc` correspond déjà à un module `.rs` (par exemple `src/robot.rs` ou `src/robot/mod.rs`), ce fichier `.rs` doit contenir la ligne d'inclusion Crust (section 17.2). Sinon : **CR0010**, avec la ligne exacte à ajouter.

## 12.3. Crate avec lib et binaire

Si le paquet a une racine lib (`lib.rs` / `lib.rsc`) et une racine bin (`main.rs` / `main.rsc`) :

- les modules `.rsc` sont montés **uniquement dans la lib**;
- `main.rsc` y accède via le nom de la crate, et `import` le traduit automatiquement en `use nom_crate::…`.

Cela évite le piège Rust classique où le même fichier est compilé deux fois (une fois par cible), avec deux types incompatibles.

Les binaires supplémentaires `src/bin/*.rsc` suivent la même règle.

---

# 13. `import`

## 13.1. Syntaxe

```rust
import Moteur from "./Moteur.rsc";
import { Moteur, Etat as EtatMoteur } from "./Moteur.rsc";
import * from "./robot/Outils.rsc";
pub import Moteur from "./Moteur.rsc";      // réexport (pub use)
pub(crate) import Capteur from "./Capteur.rs";
import Moteur from "./Moteur";              // extension facultative
import Moteur from "@/robot/Moteur.rsc";    // relatif à src/
```

Grammaire :

```ebnf
ImportDecl  = OuterAttribute* Visibility? "import" ImportList "from" STRING_LITERAL ";" ;
ImportList  = IDENTIFIER
            | "*"
            | "{" ImportName ( "," ImportName )* ","? "}" ;
ImportName  = IDENTIFIER ( "as" IDENTIFIER )? ;
```

## 13.2. Résolution du chemin

1. `./` et `../` : relatif au répertoire du fichier courant.
2. `@/` : relatif à `src/` du paquet.
3. Le chemin est normalisé. Il doit rester sous `src/` (sinon **CR0013**). Les liens symboliques sont résolus avant la comparaison.
4. Sans extension : on cherche `.rsc`, puis `.rs`. Si les deux existent : **CR0008**.
5. Fichier introuvable : **CR0006**, qui pointe sur la chaîne du chemin.

Les dépendances externes s'importent avec `use` standard : `use serde::Serialize;`.

## 13.3. Traduction

`import X from "fichier"` devient `use <chemin_de_module_du_fichier>::X;`

```rust
// src/robot/Servomoteur.rsc
import Moteur from "./Moteur.rsc";
// devient
use crate::robot::moteur::Moteur;
```

- `import { A, B as C }` devient `use chemin::{A, B as C};`
- `import *` devient `use chemin::*;`
- La visibilité est reportée sur le `use`.
- Le `use` est émis **sur la même ligne** que l'`import` (section 15).
- **Symbole absent** :
  - fichier cible `.rsc` : Crust connaît ses items de premier niveau, donc **CR0007** immédiat, qui pointe sur le nom dans l'`import`;
  - fichier cible `.rs` : le nom peut venir d'une macro, donc Crust émet le `use` et laisse `rustc` signaler E0432, ramené sur le nom dans l'`import`.

## 13.4. Unicité

Un fichier = **un seul module**, monté une seule fois. Plusieurs `import` du même fichier produisent plusieurs `use` vers **le même chemin**. Il n'y a jamais de copie de définition.

Les cycles d'import sont autorisés, comme les cycles de `use` en Rust.

## 13.5. `use` reste disponible

`use crate::…`, `use super::…` et `use self::…` fonctionnent dans les `.rsc` avec les chemins de module décrits en 12.1.

---

# 14. Utilisation d'un type `.rsc` depuis un `.rs`

Un type défini dans un `.rsc` est un type Rust ordinaire :

```rust
// src/capteur.rs
use crate::robot::Moteur;           // chemin court (12.1.5)

pub fn lire(m: &mut Moteur) -> u32 { m.vitesse }
```

Un `.rs` peut créer des valeurs, prendre `&T` ou `&mut T`, recevoir ou retourner `T`, implémenter des traits (selon les règles d'orphelin Rust), appeler les méthodes et utiliser le type dans des generics.

Une autre crate dépendant d'une crate `.rsc` ne voit que des types Rust ordinaires et n'a besoin d'aucun outil Crust.

---

# 15. Disposition du code généré : préservation des lignes

C'est l'élément central qui rend **rapides et exacts** les diagnostics, le débogage et l'intégration éditeur.

## 15.1. Exigence

Dans le fichier généré, **tout code écrit par l'utilisateur et recopié** (corps de méthodes, signatures, items Rust ordinaires, contenu des `impl Trait`) se trouve **à la même ligne et à la même colonne** que dans le `.rsc`.

Conséquence : même sans remappage, une erreur `rustc` dans une méthode donne la bonne ligne et la bonne colonne. Seul le chemin du fichier diffère, et le remappage le corrige (section 18).

## 15.2. Algorithme

Pour chaque classe :

1. Les lignes d'attributs de la classe sont **conservées** : elles s'appliquent à la `struct` qui suit.
2. L'en-tête `pub class Moteur<T> over Base where … {` est remplacé, **sur le même nombre de lignes**, par la `struct` (une ligne) suivie de l'ouverture de l'`impl` inhérent : `pub struct Moteur<T> where … { … } impl<T> Moteur<T> where … {`.
3. Chaque **champ** est remplacé par des blancs (retours à la ligne conservés).
4. Les **méthodes, constantes et macros** sont recopiées telles quelles.
5. Un `impl Trait {` interne devient `} impl<…> Trait for Moteur<T> where … {` sur la même ligne : on ferme l'`impl` inhérent courant et on ouvre l'`impl` du trait. Si l'indentation fait au moins deux caractères, le `} ` y est logé, pour que `impl` garde sa colonne. Le `}` final du trait est suivi, sur la même ligne, de ` impl<T> Moteur<T> where … {`, qui rouvre un `impl` inhérent.
6. Le `}` final de la classe ferme le dernier `impl` ouvert.
7. La **`struct` générée** (champs `over`, champs déclarés) est écrite **sur une seule ligne, en tête de la ligne d'en-tête, juste avant l'`impl`** : `pub struct Moteur { … } impl Moteur {`. Les attributs de la classe restent à leur place, sur les lignes précédentes, et s'appliquent donc à la `struct`. *(Révision après les premières migrations : avant, la struct suivait le `}` final.)* Elle reste ainsi dans le même module, y compris pour une classe dans un `mod x { }` inline, sans décaler aucune ligne. Sur cette ligne, les commentaires sont retirés et les commentaires de doc deviennent des `#[doc = "…"]`. *(Implémenté en P1 : remplace le placement « en fin de fichier » de la v2 initiale, qui sortait la struct des modules inline.)*
8. Chaque `import` devient un `use` sur la même ligne.
9. Un pied de fichier `// @generated by crust-build <version> from <chemin>.rsc` est ajouté **à la fin**, jamais en tête, pour ne pas décaler les lignes.

Les colonnes ne sont décalées que sur les lignes d'en-tête et sur les lignes de fermeture des `impl` de traits, où ne figure que du code synthétisé ou une accolade.

Les en-têtes synthétisés sont complétés par des espaces, pour que ce qui les suit sur la même ligne garde sa colonne. Les fins de ligne `\r\n` sont conservées.

## 15.3. Exemple

`src/Moteur.rsc` :

```rust
 1  #[derive(Debug)]
 2  pub class Moteur {
 3      pub vitesse: u32,
 4
 5      pub fn accelerer(&mut self, delta: u32) {
 6          self.vitesse += delta;
 7      }
 8
 9      impl Default {
10          fn default() -> Self { Self { vitesse: 0 } }
11      }
12  }
```

Fichier généré :

```rust
 1  #[derive(Debug)]
 2  pub struct Moteur { pub vitesse: u32, } impl Moteur {
 3  
 4  
 5      pub fn accelerer(&mut self, delta: u32) {
 6          self.vitesse += delta;
 7      }
 8  
 9    } impl Default for Moteur {
10          fn default() -> Self { Self { vitesse: 0 } }
11      } impl Moteur {
12  }
13  // @generated by crust-build 0.1.0 from src/Moteur.rsc
```

Sortie réelle de `cargo crust expand --lines`. Le pied de page de la ligne 13 est ajouté par `crust-build`.

Ce fichier n'est **jamais montré** au développeur dans le flux normal. Il reste lisible et formatable : `cargo crust expand` produit une version rustfmt-ée en disposition naturelle (`struct` puis `impl`), pour apprendre ou déboguer.

## 15.4. Source map

En plus de la préservation des lignes, chaque fichier généré a une source map (section 18.1) : elle couvre les éléments synthétisés (la `struct` après la classe, les en-têtes d'`impl`, les `use`), dont la position diffère.

---

# 16. Architecture de l'outillage

## 16.1. Composants

```text
                ┌────────────────────────────────────────────┐
                │ crust-syntax  (lib, sans dépendance lourde)│
                │  lexer + parseur « léger » + transpileur   │
                │  + source maps                             │
                └───────┬───────────────┬────────────────┬───┘
                        │               │                │
          ┌─────────────▼──┐   ┌────────▼───────┐  ┌─────▼──────────┐
          │ crust-build    │   │ cargo-crust    │  │ crust-lsp      │
          │ (build-dep,    │   │ (CLI : build,  │  │ (proxy LSP     │
          │  appelé par    │   │  check, test,  │  │  devant        │
          │  build.rs)     │   │  remap diag,   │  │  rust-analyzer)│
          └────────────────┘   │  fmt, eject)   │  └─────┬──────────┘
                               └────────────────┘        │
                                                  ┌──────▼──────────┐
                                                  │ Extension VS    │
                                                  │ Code « Crust »  │
                                                  └─────────────────┘
```

| Crate / paquet | Rôle | Contraintes |
|---|---|---|
| `crust-syntax` | Lexer, parseur léger, transpileur, source maps, arbre de modules | Aucune dépendance hors `std` (tolérée : un lexer type `rustc_lexer`). Pas de `syn`. |
| `crust-build` | API `crust_build::run()` pour `build.rs` | Compile en moins de 3 s à froid. MSRV raisonnable (1.74). |
| `cargo-crust` | Sous-commande `cargo crust …` | Remappage des diagnostics, rendu style rustc. |
| `crust-lsp` | Binaire proxy LSP | Latence ajoutée négligeable (section 23). |
| `crust-vscode` | Extension VS Code | Dépend de l'extension rust-analyzer officielle. |

## 16.2. Parseur « léger »

Le parseur **ne parse pas les corps de fonctions**. Il ne reconnaît que :

- les items de premier niveau (pour trouver `class`, `import`, `mod`, `#[path]`, et les noms d'items pour CR0007);
- l'en-tête et les membres des classes;
- les paramètres génériques (noms, bornes, valeurs par défaut) et les clauses `where`, sous forme de suites de tokens;
- les types des champs, comme suites de tokens jusqu'à la virgule de niveau 0 (suivi de la profondeur `<>`, `()`, `[]`, `{}`).

Les corps sont des **plages de tokens équilibrées**, recopiées octet pour octet. Avantages :

- **vitesse** : objectif ≥ 100 Mo/s;
- **robustesse** : toute évolution future de la syntaxe des expressions Rust est supportée automatiquement;
- **C1 garanti par construction** : un fichier sans `class` ni `import` est recopié tel quel.

Le parseur est **tolérant aux erreurs**, ce qui est indispensable pour l'éditeur : une classe mal formée produit un diagnostic, et le reste du fichier est quand même traduit.

## 16.3. Alternatives rejetées

| Option | Pourquoi rejetée |
|---|---|
| Fork de `rustc` | Maintenance permanente (une release toutes les 6 semaines), distribution d'une toolchain spéciale, incompatibilité avec crates.io et avec l'outillage standard. Contraire à C2. |
| Driver `rustc_private` | Nightly uniquement, API instable. Contraire à C2. |
| Proc-macro (`crust::class! { }`) | Ne peut pas lire d'autres fichiers proprement, pas de spans vers des fichiers externes, pas d'`import` de fichiers, et la syntaxe dans la macro est lourde. |
| Fork de rust-analyzer | Coût de maintenance énorme. Le proxy LSP donne les mêmes fonctionnalités en réutilisant rust-analyzer officiel. |

Un fork de rustc ou de rust-analyzer ne serait reconsidéré que si un spike (section 28) montrait une impasse.

---

# 17. Intégration Cargo

## 17.1. Déclaration

`Cargo.toml` :

```toml
[build-dependencies]
crust-build = "0.1"
```

`build.rs` :

```rust
fn main() {
    crust_build::run();
}
```

Si le paquet a déjà un `build.rs`, on y ajoute simplement l'appel. `cargo crust init` fait tout cela automatiquement.

## 17.2. Racine de crate et ligne d'inclusion

Cargo exige `src/main.rs` ou `src/lib.rs`. Quand la racine est écrite en `.rsc`, le `.rs` correspondant est un **stub d'une ligne** créé par `cargo crust init` :

```rust
include!(concat!(env!("OUT_DIR"), "/crust/root_main.rs"));
```

Contenu de `root_main.rs` (sortie réelle de `crust-build`, chemins abrégés) :

```rust
#[path = "<paquet>/src/robot"] pub mod robot {                 // répertoire de .rsc
#[path = "<OUT_DIR>/crust/src/robot/Moteur.rsc"] pub mod moteur;
#[allow(unused_imports)] pub use self::moteur::*;              // chemin court crate::robot::Moteur
}
#[path = "<paquet>/src"] mod __crust_rs_capteur { #[path = "Capteur.rs"] pub mod capteur; }
pub use self::__crust_rs_capteur::capteur;                     // .rs importé, monté par Crust
include!("<OUT_DIR>/crust/src/main.rsc");                      // corps de main.rsc
```

**Révision P2 (2026-10-21).** Le module caché `__crust_root` envisagé après le spike S2 est **abandonné** : il changeait la sémantique Rust. Les champs, méthodes et items privés de la racine n'étaient plus visibles du reste de la crate, alors qu'à la racine ils le sont partout. Le corps de `main.rsc` / `lib.rsc` est donc **inclus textuellement** (`include!`), ce qui donne exactement la sémantique d'un `main.rs`.

La limite de rust-analyzer (pas de complétion dans du texte inclus, spike S2) sera traitée par `crust-lsp` (20.2) : il présente à rust-analyzer un `src/main.rs` (overlay) qui contient directement le texte généré, sans `include!`.

Règles de montage, vérifiées par des tests rustc (rustc traite un fichier chargé via `#[path]` comme un `mod.rs`) :

- **Répertoire de `.rsc`** : fichier de déclarations `$OUT_DIR/crust/dir/<rép>.rs`, monté par `#[path]` absolu. *(Révision P4 : un module inline avec `#[path]` contenant lui-même un `#[path]` absolu marche avec rustc, mais pas avec rust-analyzer, qui concatène les deux chemins.)* Ce fichier est le même que celui inclus par un répertoire mixte.
- **`.rsc`** : `#[path]` absolu vers le fichier généré.
- **`.rs` importé mais monté nulle part** : toujours par l'enveloppe `#[path = "<rép. source>"] mod __crust_rs_x { pub mod x; }` puis `pub use`. Les sous-modules de `x` sont ainsi cherchés dans `<rép>/x/`, comme en Rust.
- **`mod x;` écrit dans un `.rsc`** : le fichier généré vit dans `OUT_DIR`, donc le transpileur réécrit la déclaration en `#[path = "<rép. source>"] mod __crust_mod_x { pub mod x; } use self::__crust_mod_x::x;`, sur la même ligne. Le répertoire est celui des sous-modules du `.rsc` : `src/` pour une racine, `src/robot/Moteur/` pour `src/robot/Moteur.rsc`. Un `#[path]` éventuel reste relatif au répertoire du `.rsc`. Les `#[cfg]` et `#[macro_use]` sont reportés sur l'enveloppe.

Autres règles :

- **Attributs internes** : `include!` refuse les attributs internes. Dans `main.rsc` / `lib.rsc`, un `#![...]` de premier niveau donne l'erreur **CR0015**, qui demande de le placer dans le `.rs` racine. La doc `//!` y devient un commentaire ordinaire de même longueur.
- **Racine écrite à la main** : si la racine reste un `.rs` écrit à la main, on y ajoute cette même ligne pour monter les modules `.rsc`.
- **Répertoire mixte** (12.2) : le module `.rs` qui possède le répertoire ajoute `include!(concat!(env!("OUT_DIR"), "/crust/dir/<rép>.rs"));`, par exemple `/crust/dir/outils.rs` pour `src/outils/`.
- **Crate lib + bin** : les modules sont montés dans la lib. `root_main.rs` ne contient que l'inclusion de `main.rsc`, et ses `import` deviennent `use nom_crate::…`.

**Aucune dépendance runtime** : pas de crate `crust` à importer, rien que `include!`, `concat!` et `env!`.

## 17.3. Ce que fait `crust_build::run()`

1. Parcourt `src/`, lit les `.rsc` et les déclarations `mod` des `.rs`, puis construit l'arbre de modules (section 12).
2. Transpile chaque `.rsc` vers `$OUT_DIR/crust/src/<même chemin relatif>.rsc`. Le fichier généré **garde le nom et l'extension d'origine** pour faciliter le débogage (section 22). `rustc` accepte n'importe quelle extension via `#[path]`.
3. Génère `$OUT_DIR/crust/root_<cible>.rs` et, pour les répertoires mixtes, `$OUT_DIR/crust/dir/<rép>.rs` : déclarations de modules, réexports, inclusion de la racine (17.2).
4. **Pas de fichier de source map sur disque** (révision P2). La traduction est déterministe et très rapide (moins d'une milliseconde par fichier), donc `cargo crust` et `crust-lsp` **recalculent** la source map en retranspilant le `.rsc` avec le même arbre de modules (crate `crust-build` utilisée comme bibliothèque). Le pied de page `// @generated … from src/<chemin>.rsc` de chaque fichier généré permet de retrouver la source.
5. **N'écrit un fichier que si son contenu change**, pour préserver les mtimes et la compilation incrémentale de rustc.
6. Émet `cargo::rerun-if-changed=src`. Cargo surveille alors tout le répertoire, récursivement : un fichier modifié, ajouté ou supprimé relance le build script, et seulement dans ce cas.
7. **N'échoue jamais** sur une erreur de l'utilisateur : les erreurs Crust deviennent des `compile_error!` dans le code généré (section 19).

Les chemins absolus dans `#[path]` ne posent pas de problème : `OUT_DIR` est propre à chaque build et n'est jamais publié.

## 17.4. Sous-commande `cargo crust`

| Commande | Effet |
|---|---|
| `cargo crust new <nom>` / `init` | Crée ou convertit un paquet : `build.rs`, stub, dépendance, `.vscode/settings.json` |
| `cargo crust build`, `check`, `run`, `test`, `clippy`, `doc`, `bench` | Lance `cargo …` avec `--message-format=json` en interne, remappe les diagnostics et les affiche **au format rustc** (couleurs, extraits de code `.rsc`) |
| `cargo crust … --message-format=json` | Restitue le JSON cargo remappé, pour les outils |
| `cargo crust expand <fichier.rsc>` | Affiche le Rust équivalent, formaté |
| `cargo crust fmt` | Formate les `.rsc` (section 21) et lance `rustfmt` sur les `.rs` |
| `cargo crust eject` | Convertit tous les `.rsc` en `.rs` lisibles avec de vrais `mod`, puis retire `crust-build` |

Tous les arguments inconnus sont transmis tels quels à `cargo`.

## 17.5. Publication

- `cargo publish` fonctionne tel quel : le paquet contient les `.rsc`, `build.rs` et le stub. Chez le consommateur, `crust-build` régénère le code.
- docs.rs fonctionne, puisque le build script est standard.
- Limitation documentée : sur docs.rs, les liens « source » pointent vers le fichier généré.

## 17.6. Porte de sortie : `eject`

`cargo crust eject` garantit qu'adopter Crust n'est **jamais** un enfermement :

- chaque `.rsc` devient un `.rs` en disposition naturelle, formaté par rustfmt, commentaires conservés;
- de vrais `mod` sont générés;
- les `import` deviennent des `use`.

---

# 18. Diagnostics et positions sources

Exigence majeure : **toutes les erreurs pointent vers les `.rsc` écrits par l'utilisateur.** On ne montre jamais un fichier intermédiaire.

## 18.1. Source map

Pour chaque fichier généré, une liste triée de segments :

```text
{ gen_start, gen_len, src_start, src_len, kind }
kind = Copied        // correspondance 1:1 des octets
     | Synthesized   // toute la plage générée renvoie à une plage d'origine
```

| Élément synthétisé | Plage d'origine |
|---|---|
| `struct Moteur` en fin de fichier | le nom de la classe |
| Champ de la struct | le champ dans le `.rsc` (Copied) |
| Champ `over` | le `OverItem` |
| `impl … Moteur<T>` | l'en-tête de la classe |
| `impl Trait for Moteur` | l'en-tête `impl Trait` dans la classe |
| `use chemin::X` | l'instruction `import` (`X` renvoie à `X`, le chemin à la chaîne) |

- Format sur disque : binaire compact ou JSON (à décider au prototype). Chargement paresseux et cache en mémoire.
- Recherche : dichotomie, en O(log n).
- Conversions gérées : octets ↔ (ligne, colonne UTF-8) ↔ (ligne, colonne UTF-16) pour le LSP.

## 18.2. Remappage dans `cargo crust`

1. Pour chaque diagnostic JSON, on remappe tous les spans : primaire, secondaires, enfants, `expansion`, suggestions.
2. Une suggestion de correction dont la plage tombe dans une zone `Synthesized` est **retirée**, sauf si la plage d'origine permet une correction équivalente (par exemple ajouter un `#[derive]` sur la classe).
3. Le message est re-rendu au style rustc à partir des sources `.rsc`, avec une bibliothèque de type `annotate-snippets`, utilisée aussi par rustc.
4. Les spans qui pointent vers un fichier généré sans plage d'origine trouvée sont rattachés au début de la classe concernée. C'est un cas anormal, journalisé pour corriger le transpileur.

## 18.3. Mode dégradé (`cargo build` simple)

Sans `cargo crust`, les erreurs affichent `…/out/crust/src/Moteur.rsc:6:9`. **Le nom de fichier, la ligne et la colonne sont corrects** grâce à la section 15. Seul le préfixe du chemin est celui d'`OUT_DIR`. C'est un mode utilisable, pas seulement de secours.

## 18.4. Couverture exigée

Erreurs de type, borrow checker, lifetimes, traits, champs inexistants, collisions de noms, imports, méthodes, warnings et lints (y compris clippy), suggestions.

---

# 19. Erreurs spécifiques à Crust

## 19.1. Canal unique

Les erreurs détectées par Crust sont injectées dans le code généré sous la forme :

```rust
compile_error!("[CR0002] le champ `moteur` est déjà généré par `over Moteur`");
```

placée **à la ligne et à la colonne de l'erreur**. Conséquences :

- `cargo build`, `cargo crust` et rust-analyzer les affichent tous, sans canal spécial;
- le build script n'échoue jamais (pas de « failed to run custom build command »);
- en mode éditeur, `crust-lsp` publie aussi ces erreurs **immédiatement**, sans attendre rust-analyzer, et supprime le doublon quand il arrive.

Après une erreur, la traduction continue (récupération) pour limiter les erreurs en cascade.

## 19.2. Catalogue

| Code | Erreur | Pointe sur |
|---|---|---|
| CR0001 | Syntaxe invalide dans l'en-tête ou le corps d'une classe | le token fautif |
| CR0002 | Champ en collision avec un champ `over` | le champ, plus une note sur le `over` |
| CR0003 | Deux composants `over` avec le même nom | les deux `OverItem` |
| CR0004 | `over` sur un type non-chemin sans nom explicite | le type, avec la suggestion `(nom)` |
| CR0005 | Élément interdit dans une classe (4.6, 5.2) | l'élément, avec le déplacement proposé |
| CR0006 | Fichier d'import introuvable | la chaîne du chemin |
| CR0007 | Symbole absent du `.rsc` importé | le nom dans l'`import`, avec les noms proches proposés |
| CR0008 | Import sans extension ambigu (`.rsc` et `.rs` existent) | la chaîne du chemin |
| CR0009 | Deux fichiers donnent le même module | les deux fichiers |
| CR0010 | Répertoire mixte sans ligne d'inclusion dans le `.rs` | le `mod` du `.rs`, avec la ligne à ajouter |
| CR0011 | `.rs` monté plusieurs fois (`#[path]` multiples) puis importé | l'import, avec une note sur les montages |
| CR0012 | Nom de répertoire ou de fichier qui n'est pas un identifiant valide | le chemin |
| CR0013 | Import d'un fichier hors de `src/` | la chaîne du chemin |
| CR0014 | Édition 2015 non supportée | `Cargo.toml` |
| CR0015 | Attribut interne `#![...]` dans `main.rsc` / `lib.rsc` (inclus par `include!`) | l'attribut, avec le déplacement vers le `.rs` racine |

Chaque code a une page d'explication (`cargo crust explain CR0002`), sur le modèle de `rustc --explain`.

Exemples :

```text
error[CR0002]: le champ `moteur` est déjà généré par `over Moteur`
 --> src/Servomoteur.rsc:2:5
  |
1 | pub class Servomoteur over Moteur {
  |                            ------ génère le champ `moteur`
2 |     moteur: AutreType,
  |     ^^^^^^ déclaré ici une seconde fois
  |
  = help: nommez explicitement le composant : `over Moteur(base)`
```

```text
error[CR0006]: fichier introuvable : `./Absent.rsc`
 --> src/main.rsc:1:20
  |
1 | import Moteur from "./Absent.rsc";
  |                    ^^^^^^^^^^^^^^ aucun fichier `src/Absent.rsc`
```

---

# 20. Éditeur : `crust-lsp` et extension VS Code

## 20.1. Principe

`crust-lsp` est un **proxy LSP** placé entre VS Code et le **rust-analyzer officiel, non modifié**.

```text
VS Code + extension rust-analyzer officielle
            │  (rust-analyzer.server.path = crust-lsp)
            ▼
        crust-lsp ─── transpile en mémoire, remappe les positions
            │
            ▼
    rust-analyzer officiel (sous-processus)
```

Astuce clé : l'extension rust-analyzer officielle permet de choisir le binaire serveur (`rust-analyzer.server.path`). On y met `crust-lsp`, ce qui réutilise **gratuitement** toute son interface : runnables, boutons Run/Debug, inlay hints, « expand macro », tests, commandes, statut, téléchargement du serveur. Une seule instance de rust-analyzer tourne, donc il n'y a pas de double consommation mémoire.

## 20.2. Comportement du proxy

- **Fichiers `.rs`** : messages transmis **sans aucune transformation** (surcoût proche de zéro).
- **Fichiers `.rsc`** :
  - `didOpen` / `didChange` : transpilation **incrémentale en mémoire** (le fichier seul, en moins d'1 ms), puis envoi à rust-analyzer d'un `didOpen` / `didChange` **du fichier généré** (overlay). Si l'arbre de modules change (fichier ajouté ou supprimé, `import` modifié), la racine générée est mise à jour de la même manière.
  - Requêtes avec position (completion, hover, definition, references, rename, signatureHelp, codeAction, documentHighlight, inlayHint, codeLens, semanticTokens, selectionRange, foldingRange, documentSymbol, callHierarchy, typeHierarchy, et les extensions `rust-analyzer/*`) : la position `.rsc` est convertie en position générée, la requête est transmise, puis **toutes les positions du résultat** sont reconverties en positions `.rsc`.
  - `publishDiagnostics` de rust-analyzer (natifs et flycheck `cargo check`) sur un fichier généré : remappés et publiés sur l'URI `.rsc`.
- **Au démarrage**, `crust-lsp` ouvre en overlay (`didOpen`) **tous** les fichiers générés. C'est **obligatoire** : le VFS de rust-analyzer ne charge pas les fichiers d'extension `.rsc` depuis le disque (spike S2), mais il accepte les overlays quelle que soit l'extension (spike S3).
- **Racine `.rsc`** : rust-analyzer ne propose pas de complétion dans un texte inclus par `include!` (spike S2). `crust-lsp` présente donc à rust-analyzer un `src/main.rs` / `src/lib.rs` en overlay, qui contient directement le contenu de `root_<cible>.rs` avec le `main.rsc` transpilé à la place de l'`include!`. Ce contenu est sémantiquement identique à ce que compile Cargo. Le `.rs` racine du disque n'est jamais modifié.
- **`OUT_DIR` de rust-analyzer** : plusieurs `OUT_DIR` coexistent pour un paquet (build, check, clippy…). `crust-lsp` utilise celui de rust-analyzer, obtenu via `cargo check --message-format=json --all-targets` (message `build-script-executed`), et le réévalue si les features ou la configuration changent.
- **Deux canaux de diagnostics** à remapper :
  - les diagnostics natifs en mode *pull* (`textDocument/diagnostic`, `workspace/diagnostic`). La requête est relancée sur `-32802 ServerCancelled` avec `retriggerRequest`;
  - les diagnostics `cargo check` (flycheck) en mode *push* (`publishDiagnostics`).
- **`crust-lsp --version`** doit répondre, car l'extension rust-analyzer le vérifie au démarrage (spike S4).
- **Choix du rust-analyzer** : si celui livré avec l'extension est trop récent pour la toolchain (avertissement « outdated toolchain »), on préfère le composant `rustup`.
- **Résultats pointant vers une zone synthétisée** : convertis vers la plage d'origine (18.1). Les doublons sont fusionnés. Par exemple, renommer la classe produit des modifications sur `struct Moteur`, `impl Moteur` et `impl Default for Moteur`, qui convergent toutes vers le seul nom de la classe.

## 20.3. Fonctionnalités spécifiques

| Fonctionnalité | Comportement attendu |
|---|---|
| Complétion `servo.moteur.` / `servo.parent.` | rust-analyzer voit le champ dans la `struct` générée et propose les membres de `Moteur` |
| Goto definition sur `moteur` | Renvoie vers le `OverItem` dans l'en-tête de la classe |
| Goto definition sur le chemin d'un `import` | Ouvre le fichier cible |
| Complétion dans la chaîne d'un `import` | Fichiers `.rsc` / `.rs` du projet |
| Complétion du nom dans un `import` | Items publics du fichier cible |
| Rename d'un champ `over` **déduit** | Transforme `over Moteur` en `over Moteur(nouveau_nom)` au lieu de renommer le type |
| Rename d'une classe | Renomme la classe et toutes ses utilisations. Propose de renommer le fichier si le nom de fichier correspond. |
| Rename / déplacement d'un fichier `.rsc` | Met à jour les `import` qui pointent dessus (`workspace/willRenameFiles`) |
| Hover sur `class`, `over`, `import` | Affiche la traduction Rust correspondante |
| Coloration sémantique | `class`, `over`, `import` et `from` marqués comme mots-clés. Le reste vient de rust-analyzer. |
| Commande « Crust : afficher le Rust généré » | Vue côte à côte, en lecture seule, synchronisée (équivalent de `cargo crust expand`) |
| Code actions | Celles de rust-analyzer (« implement missing members » dans un `impl Trait` de classe, etc.), avec des modifications remappées. Ajout : « Convertir struct + impl en class » et l'inverse. |

## 20.4. Extension VS Code « Crust »

Extension légère qui :

- déclare `extensionDependencies: ["rust-lang.rust-analyzer"]`;
- associe `*.rsc` au langage `rust`, pour que l'extension rust-analyzer gère ces fichiers;
- configure (avec l'accord de l'utilisateur, au niveau du workspace) `rust-analyzer.server.path` vers `crust-lsp`, livré dans l'extension pour chaque plateforme (linux-x64/arm64, macOS, Windows);
- `crust-lsp` lance le binaire rust-analyzer trouvé selon l'ordre suivant : celui livré avec l'extension rust-analyzer, puis `rustup`, puis le `PATH`;
- ajoute une **injection de grammaire TextMate** pour `class`, `over`, `import` et `from` (coloration immédiate, avant l'arrivée des tokens sémantiques);
- fournit la configuration de débogage (section 22);
- fournit les icônes de fichiers `.rsc`.

Les fichiers `.rs` du même workspace continuent de fonctionner exactement comme avant.

Les autres éditeurs (Neovim, Helix, Zed, JetBrains via LSP) peuvent utiliser `crust-lsp` comme serveur rust-analyzer, avec la même logique. Hors périmètre V1, mais l'architecture ne doit rien faire de spécifique à VS Code côté serveur.

---

# 21. Formatage

- Style cible : celui de rustfmt. `class` est formatée comme une `struct` dont le corps contiendrait aussi des items d'`impl`.
- Implémentation : on traduit la classe en disposition naturelle (`struct` + `impl`), on lance `rustfmt`, puis on **reconstitue** la syntaxe `class` à partir du résultat. La traduction est réversible, car chaque élément généré connaît son membre d'origine.
- Les commentaires sont conservés.
- Disponible via `cargo crust fmt` et via `textDocument/formatting` dans `crust-lsp` (format on save).
- Livré en V1.1. En V1, `crust-lsp` ne renvoie aucune modification pour les `.rsc`, sans erreur.

```rust
pub class Servomoteur over Moteur(parent) {
    position: f32,

    pub fn new() -> Self {
        Self {
            parent: Moteur::new(),
            position: 0.0,
        }
    }
}
```

---

# 22. Débogage

- Les fichiers générés portent **le même nom et le même chemin relatif** que les `.rsc` (`$OUT_DIR/crust/src/robot/Moteur.rsc`), avec **les mêmes numéros de ligne** (section 15).
- Les informations DWARF pointent donc vers des lignes exactes. Il suffit de faire correspondre le préfixe `$OUT_DIR/crust/src` à `<paquet>/src`.
- L'extension VS Code injecte ce `sourceMap` dans les configurations de lancement CodeLLDB et cppvsdbg qu'elle génère, ou dans celles des runnables rust-analyzer. Les **points d'arrêt posés dans les `.rsc` fonctionnent**, ainsi que l'exécution pas à pas.
- Les chemins des backtraces de panic affichent le chemin `OUT_DIR`, avec les bons nom, ligne et colonne. `cargo crust run` et `test` les réécrivent à l'affichage.

---

# 23. Performance

## 23.1. Objectifs chiffrés

| Mesure | Objectif |
|---|---|
| Débit du transpileur | ≥ 100 Mo/s (fichier typique de 1 000 lignes : < 0,5 ms) |
| `crust_build::run()` sans changement (rerun forcé) | < 20 ms pour 500 fichiers |
| `crust_build::run()` après modification d'un fichier | < 30 ms pour 500 fichiers |
| Première compilation de `crust-build` | < 3 s (aucune dépendance lourde) |
| Surcoût de `cargo crust check` par rapport à `cargo check` | < 50 ms |
| Latence ajoutée par `crust-lsp` par requête | < 1 ms en médiane, < 5 ms au 99e centile |
| Frappe → diagnostics dans l'éditeur | Identique à rust-analyzer sur un `.rs` équivalent (± 5 %) |
| Mémoire supplémentaire de `crust-lsp` | < 50 Mo pour 500 fichiers `.rsc` |
| Binaire final | Identique à l'équivalent Rust manuel (aucun surcoût) |

## 23.2. Moyens

- Parseur léger qui ne parse pas les corps (16.2).
- Cache par fichier dans `crust-build` : hash du contenu → sortie. Seuls les fichiers modifiés sont retraités.
- Écriture d'un fichier uniquement si son contenu change (17.3.5).
- Dans `crust-lsp` : transpilation du seul fichier modifié, envoi à rust-analyzer en `didChange` **incrémental** (seules les lignes modifiées, grâce à la préservation des lignes), et source maps maintenues en mémoire.
- Aucune analyse sémantique dans Crust : tout le travail lourd est fait par rustc et rust-analyzer, déjà optimisés et incrémentaux.
- Benchmarks continus en CI (section 27.6) pour détecter toute régression.

---

# 24. Non-objectifs de la V1

Ne pas implémenter dans la V1 :

- héritage C++, classes ou méthodes virtuelles;
- surcharge de fonctions;
- dispatch dynamique supplémentaire;
- délégation automatique de méthodes;
- constructeurs automatiques;
- destructeurs propres au dialecte;
- propriétés et getters automatiques;
- conversion implicite enfant → parent;
- héritage multiple (la composition multiple de 8.5 n'en est pas);
- réflexion objet;
- garbage collector;
- changement du borrow checker;
- nouvelle ABI;
- `class` dans un corps de fonction;
- classes tuple ou unité;
- `.rsc` dans `tests/`, `examples/` et `benches/` (V1.1);
- formatage (V1.1);
- éditeurs autres que VS Code.

---

# 25. Exemples complets

## 25.1. Projet

```text
mon_robot/
    Cargo.toml          (build-dependency crust-build)
    build.rs            (crust_build::run())
    src/
        main.rs         (stub d'une ligne)
        main.rsc
        robot/
            Moteur.rsc
            Servomoteur.rsc
        Capteur.rs
```

## 25.2. `src/robot/Moteur.rsc`

```rust
#[derive(Debug, Clone)]
pub class Moteur {
    pub vitesse: u32,

    pub fn new() -> Self {
        Self { vitesse: 0 }
    }

    pub fn accelerer(&mut self, delta: u32) {
        self.vitesse += delta;
    }

    impl Default {
        fn default() -> Self {
            Self::new()
        }
    }
}
```

## 25.3. `src/robot/Servomoteur.rsc`

```rust
import Moteur from "./Moteur.rsc";

pub class Servomoteur over Moteur(parent) {
    pub position: f32,

    pub fn new() -> Self {
        Self {
            parent: Moteur::new(),
            position: 0.0,
        }
    }

    pub fn aller_a(&mut self, position: f32) {
        self.position = position;
    }
}
```

## 25.4. `src/Capteur.rs` (Rust standard)

```rust
use crate::robot::Servomoteur;

pub fn lire(servo: &Servomoteur) -> f32 {
    servo.position
}
```

## 25.5. `src/main.rsc`

```rust
import Servomoteur from "@/robot/Servomoteur";
import lire from "./Capteur.rs";

fn main() {
    let mut servo = Servomoteur::new();
    servo.parent.accelerer(1200);
    servo.aller_a(90.0);
    println!("{} {}", servo.parent.vitesse, lire(&servo));
}
```

## 25.6. Classe générique

```rust
pub class Tampon<T: Copy, const N: usize> over [T; N](donnees) {
    len: usize,

    pub fn pousser(&mut self, v: T) -> Result<(), T> {
        if self.len == N { return Err(v); }
        self.donnees[self.len] = v;
        self.len += 1;
        Ok(())
    }

    impl<'a> IntoIterator where T: 'a {
        type Item = &'a T;
        type IntoIter = std::slice::Iter<'a, T>;
        fn into_iter(self) -> Self::IntoIter { unimplemented!() }
    }
}
```

Note : cet exemple déclenche volontairement l'erreur rustc E0207 (lifetime `'a` non contrainte). Il sert de test de remappage des diagnostics.

---

# 26. Critères d'acceptation V1

| # | Test | Critère |
|---|---|---|
| T1 | Classe simple | Une classe avec champs et méthodes compile et s'exécute |
| T2 | Trait | Un `impl Trait` dans une classe compile et fonctionne |
| T3 | `over` automatique | `class B over A` génère le champ `a: A`, avec la visibilité de la classe |
| T4 | `over` nommé | `class B over A(parent)` génère le champ `parent: A` |
| T5 | Import `.rsc` | Un `.rsc` importe un autre `.rsc` |
| T6 | Imports multiples | Deux fichiers qui importent le même `.rsc` manipulent le même type (une valeur passe de l'un à l'autre) |
| T7 | Coexistence | Un `.rs` et un `.rsc` s'échangent des valeurs typées dans les deux sens |
| T8 | Erreur de type | L'erreur dans une méthode `.rsc` pointe sur la bonne ligne et la bonne colonne, avec `cargo crust` **et** en mode dégradé |
| T9 | Borrow checker | Les spans primaires et secondaires d'une erreur d'emprunt pointent dans le `.rsc` |
| T10 | Complétion | VS Code complète un champ ou une méthode d'une classe `.rsc` |
| T11 | Champ `over` | VS Code complète `servo.moteur.` et `servo.parent.` avec les membres de `Moteur` |
| T12 | Generics | La classe de 25.6, corrigée, compile |
| T13 | Sur-ensemble | Le corpus de 27.4 est traduit à l'identique |
| T14 | `cargo build` simple | Le projet 25.1 compile sans `cargo-crust` installé |
| T15 | Publication | `cargo package` réussit, et une crate `.rs` externe utilise `Moteur` |
| T16 | Erreurs CR | Chaque code CR0001–CR0014 a un test qui vérifie le message et la position |
| T17 | Goto / rename | Goto definition et rename fonctionnent de `.rs` vers `.rsc` et inversement, y compris le rename d'un `over` déduit |
| T18 | Débogage | Un point d'arrêt posé dans une méthode `.rsc` est atteint sous VS Code avec CodeLLDB |
| T19 | Performance | Les objectifs de 23.1 sont respectés sur le projet de bench |
| T20 | Eject | `cargo crust eject` sur le projet 25.1 donne un projet `.rs` qui compile, avec les mêmes tests au vert |

---

# 27. Plan de tests

## 27.1. Tests de traduction (snapshots)

`tests/transpile/*.rsc` → sortie attendue `*.rs.snap` (avec `insta`), plus la source map attendue. Cas couverts : chaque règle des sections 4, 5, 8, 13 et 15, et tous les cas limites (commentaires partout, `\r\n`, attributs sur plusieurs lignes, en-têtes sur plusieurs lignes, raw identifiers, Unicode).

## 27.2. Tests de compilation

Des mini-crates `tests/crates/*/` compilées avec `cargo build` puis exécutées (T1–T7, T12, T14), dans le style de `trybuild`.

## 27.3. Tests de diagnostics

Des crates qui contiennent des erreurs volontaires, avec annotations dans le `.rsc` :

```rust
self.vitesse = "abc"; //~ ERROR E0308
```

Le harnais lance `cargo crust check --message-format=json` et vérifie le fichier, la ligne, la colonne et le code (modèle : `tests/ui` de rustc). Il vérifie aussi le mode dégradé : même ligne et même colonne dans le fichier `OUT_DIR`.

## 27.4. Test de sur-ensemble (C1)

Un corpus d'au moins 5 000 fichiers `.rs` réels (crates populaires vendorisées et `tests/ui` de rustc, filtrés sur ceux qui se parsent) : chacun, traité comme `.rsc`, doit être recopié **octet pour octet**. À lancer en CI.

## 27.5. Tests LSP

Sessions LSP scriptées (client de test en Rust ou TypeScript) contre `crust-lsp` + rust-analyzer réel : complétion, hover, définition, références, rename, diagnostics, avec des positions dans les `.rsc`. Test de bout en bout VS Code avec `@vscode/test-electron` pour T10, T11 et T18.

## 27.6. Benchmarks

`criterion` sur le transpileur, plus un projet synthétique de 500 fichiers `.rsc` pour `crust-build` et `crust-lsp`. Tout écart de plus de 10 % sur un objectif de 23.1 fait échouer la CI.

## 27.7. Fuzzing

`cargo fuzz` sur le parseur : il ne doit jamais paniquer. Un `.rsc` valide fuzzé doit toujours produire une source map cohérente (spans croissants, bornes respectées).

---

# 28. Risques et spikes à faire en premier

**Statut au 2026-10-21 : spikes réalisés.** Voir `spikes/RAPPORT.md`.

| Spike | Résultat |
|---|---|
| S1 | ✅ rustc stable 1.92 : compile, et les erreurs donnent ligne et colonne exactes du `.rsc` |
| S2 | ⚠️ OK avec deux contraintes : overlay obligatoire des `.rsc` générés, et complétion absente dans le texte inclus par `include!`, traitée par un overlay du `.rs` racine dans `crust-lsp` (17.2, 20.2). Le module `__crust_root` d’abord envisagé a été abandonné en P2 |
| S3 | ✅ Overlays dans `OUT_DIR` : hover, complétion et diagnostics fonctionnent. Plan B `.crust/gen` inutile |
| S4 | ✅ en analyse statique (sélection par languageId `rust`, `server.path` respecté, `--version` requis). Bout en bout en P4 |
| S5 | ✅ Point d'arrêt dans un `.rsc` via le `sourceMap` de lldb |
| S6 | ✅ 500 fichiers : build sans changement 40 ms, build.rs 7 ms, surcoût d'une modification non mesurable |
| S7 | ✅ Aucun warning (seul `clippy::multiple_inherent_impl`, du groupe `restriction`, désactivé par défaut) |

Description initiale des spikes : des incertitudes techniques à lever **avant** de coder massivement. Chaque spike est un petit prototype jetable, qui doit répondre par oui ou par non.

| Spike | Question | Plan B si non |
|---|---|---|
| S1 | `include!(concat!(env!("OUT_DIR"), …))` d'un fichier qui contient des `#[path = "/absolu"] pub mod x;` et `fn main` fonctionne-t-il avec rustc stable ? | Générer `src/main.rs` complet (fichier généré dans `src/`, ajouté au gitignore) |
| S2 | rust-analyzer résout-il ce même schéma (include + `OUT_DIR` + `#[path]` vers un fichier `.rsc`) ? | Fichiers générés dans `<paquet>/.crust/gen/` (dans le workspace, gitignore) au lieu d'`OUT_DIR` |
| S3 | rust-analyzer accepte-t-il un overlay `didOpen` sur un fichier situé dans `OUT_DIR` ? | Même plan B que S2 |
| S4 | L'extension rust-analyzer fonctionne-t-elle avec `server.path` = proxy et des documents `*.rsc` associés au langage `rust` ? | Extension Crust avec son propre `LanguageClient`, qui lance `crust-lsp` + rust-analyzer |
| S5 | CodeLLDB pose-t-il des points d'arrêt dans un `.rsc` avec le `sourceMap` décrit en 22 ? | `--remap-path-prefix` via `cargo crust` |
| S6 | Coût réel de `rerun-if-changed` sur des répertoires pour un projet de 500 fichiers | Liste explicite de fichiers + détection des ajouts par `cargo crust` / `crust-lsp` |
| S7 | Un `impl X {}` vide généré déclenche-t-il un warning rustc ou clippy ? | Supprimer les `impl` vides en post-traitement, avec une colonne décalée sur la ligne concernée |

---

# 29. Feuille de route

| Phase | Contenu | Sortie | Tests |
|---|---|---|---|
| **P0** | Spikes S1–S7 | Rapport oui/non + décisions | — |
| **P1** | `crust-syntax` : lexer, parseur léger, `class`, `over`, `impl Trait`, generics, disposition préservant les lignes, source maps | Bibliothèque + `cargo crust expand` | 27.1, 27.4, 27.7, T13 |
| **P2** | Arbre de modules, `import`, cohabitation `.rs`, lib + bin, `crust-build` | Projets compilables avec `cargo build` | T1–T7, T12, T14, T15 |
| **P3** | `cargo-crust` : remappage et rendu des diagnostics, erreurs CR | Messages d'erreur `.rsc` parfaits | T8, T9, T16, 27.3 |
| **P4** | `crust-lsp` : proxy, overlays, remappage de toutes les requêtes | Utilisable dans VS Code via un réglage manuel | T10, T11, T17, 27.5 |
| **P5** | Extension VS Code : association, grammaire, configuration automatique, débogage, « afficher le Rust généré » | Extension publiable | T18 |
| **P6** | Performance + `eject` + documentation utilisateur | Release V1 | T19, T20 |
| **V1.1** | Formatage, `.rsc` dans `tests/` et `examples/`, code actions de conversion | | |

Ne pas considérer une phase comme réussie si les erreurs pointent vers du Rust généré.

## Statut

- **P0 : terminé** (2026-10-21). Voir `spikes/RAPPORT.md`.
- **P1 : terminé** (2026-10-21). Crates `crates/crust-syntax` (sans dépendance) et `crates/cargo-crust` (`cargo crust expand [--lines] [--map]`). Tests dans `crates/crust-syntax/tests/` :
  - `transpile.rs` : snapshots, invariants (nombre de lignes, segments copiés identiques, source map contiguë), compilation rustc des sorties dans les deux dispositions;
  - `diagnostics.rs` : positions exactes des erreurs rustc (types, emprunts, champs, traits) et Crust, en mode dégradé et après remappage (T8, T9);
  - `corpus.rs` : C1 sur 3 625 fichiers réels (std + registre cargo, 66,7 Mo), tous identiques (T13);
  - `robustness.rs` : aucune panique sur toutes les troncatures des fixtures;
  - `imports.rs` : résolution et erreurs CR0006, 0007, 0008, 0012, 0013.
- **Performances P1 (release)** : 241 Mo/s sur le corpus sans classe ; fichier de 34 000 lignes et 2 000 classes en 10–20 ms, démarrage du processus compris.
- **P2 : terminé** (2026-10-21). Crate `crates/crust-build` (dépend uniquement de `crust-syntax`). Arbre de modules réel :
  - lecture des `mod` des `.rs`, avec `#[path]` et modules inline;
  - montage automatique des `.rs` importés;
  - répertoires mixtes;
  - lib + bin;
  - réexports courts;
  - CR0009, CR0010, CR0011 et CR0015.

  Tests `crates/crust-build/tests/projects.rs` : quatre vrais projets Cargo compilés et exécutés avec un simple `cargo build` / `cargo run` :
  - `p1_robot` : exemple de la section 25, `.rs` importé, `mod util;` avec sous-module, détection des modifications et des nouveaux fichiers, build script non relancé sans changement;
  - `p2_workspace` : lib + bin en `.rsc`, deux fichiers qui échangent le même type (T6), crate Rust ordinaire qui utilise la lib Crust (C3, C6);
  - `p3_mixed` : projet Rust existant avec des `.rsc` ajoutés, répertoire mixte;
  - `p4_errors` : CR0009, CR0010, CR0011 et CR0015, sans erreur en cascade.

  Reste non testé : T15 (`cargo package`), qui nécessite que `crust-build` soit publié sur crates.io.
- **P3 : terminé** (2026-10-21). `cargo crust build|check|run|test|clippy|doc|bench`, `new`, `init`, `explain`, `expand`. Fonctionnement :
  - cargo est lancé avec `--message-format=json`; chaque span d'un fichier généré est ramené dans le `.rsc`, avec une source map recalculée en retranspilant le fichier (`crust_build::transpile_rsc`). Si le fichier généré ne correspond plus, seul le chemin est corrigé, ce qui suffit grâce aux lignes préservées;
  - le message est réaffiché au format rustc avec annotate-snippets 0.11;
  - les messages qui ne concernent aucun fichier généré sont transmis tels quels, avec le texte de rustc;
  - les suggestions qui visent du code synthétisé sont retirées;
  - la sortie du programme (`run`, `test`) passe en flux continu, invites sans fin de ligne comprises;
  - `--message-format=json` de l'utilisateur renvoie le JSON remappé (utilisable par rust-analyzer en P4).

  Tests `crates/cargo-crust/tests/cli.rs` : erreurs (type, champ dans la struct générée) au bon endroit et sans chemin généré visible, JSON remappé, `run`/`test`, `new` + `init`, `explain`. Écarts de rendu connus, liés à annotate-snippets 0.11 :
  - en couleur, le soulignement principal d'un warning est rouge au lieu de jaune;
  - une suggestion est affichée en ligne (`help: … : \`_x\``) et non sous forme de diff.
- **P4 : terminé** (2026-10-21). Crate `crates/crust-lsp` (binaire `crust-lsp`, installé dans `~/.cargo/bin`).
  - **Fonctionnement** :
    - recherche de rust-analyzer : `CRUST_RA_PATH`, puis `rustup`, puis les extensions VS Code;
    - au démarrage : `cargo metadata` et `cargo check --all-targets`, arrêté dès que les build scripts ont donné l'`OUT_DIR` de rust-analyzer;
    - overlays de tous les `.rsc` générés et des fichiers de déclarations;
    - overlay du `.rs` racine avec `main.rsc` inséré directement;
    - conversion des positions dans les requêtes, les réponses (y compris `WorkspaceEdit`, `LocationLink`, éléments `resolve`), les diagnostics *push* et *pull*, et les tokens sémantiques (mots-clés `class`, `over`, `import`, `from` ajoutés);
    - les modifications de texte qui tomberaient dans du code généré sont retirées, et les doublons de renommage fusionnés.
  - **Test** `crates/crust-lsp/tests/lsp.rs`, contre le vrai rust-analyzer :
    - complétion sur `self.parent.` (champ `over`, T11) et dans `main.rsc` (T10);
    - survol;
    - définition `.rsc` → `.rsc` et `.rs` → `.rsc` (T17);
    - références jusque dans la racine;
    - diagnostic natif à la bonne position;
    - renommage d'un champ (3 + 2 modifications, sans doublon);
    - coloration de `class`.
- **P5 : terminé** (2026-10-21). Extension `editors/vscode` (JavaScript sans compilation, `crust-0.1.1.vsix`, installée sur cette machine). Elle fournit :
  - l'association `.rsc` → langage `rust` (pas de `files.associations` global);
  - une grammaire d'injection pour `class`, `over`, `import` et `from`, vérifiée avec vscode-textmate et la grammaire Rust de VS Code : rien n'est coloré à tort dans les chaînes, les commentaires ou les identifiants;
  - dans un projet Crust (`crust-build` en dépendance de build), la proposition, avec l'accord de l'utilisateur, de régler `rust-analyzer.server.path` sur `crust-lsp` au niveau du workspace;
  - la commande « afficher le Rust généré », en document en lecture seule actualisé à l'enregistrement;
  - un fournisseur de configuration de débogage `lldb` qui ajoute le `sourceMap` `<OUT_DIR>/crust/src` → `src`, grâce au fichier `OUT_DIR/crust/source_dir` écrit par `crust-build`.

  Exemple prêt à ouvrir : `exemples/robot/`.
- **P6 et V1.1 : terminés** (2026-10-21).
  - **`cargo crust eject`** (17.6, T20) :
    - conversion en `.rs` lisibles formatés par rustfmt, vrais `mod`, attributs internes des racines replacés en tête, `crust-build` retiré;
    - refuse un dépôt git non committé (`--force`), `--dry-run`;
    - testé sur p1, p2 (lib + bin + `src/bin` + crate cliente) et p3 (répertoire mixte), qui compilent et se comportent à l'identique avec cargo seul.
  - **`src/bin/<nom>.rsc`** : racine `root_bin_<nom>.rs`, incluse par `src/bin/<nom>.rs` (créé par `cargo crust init`). Ses `import` vers la lib deviennent `use nom_crate::…`.
  - **Formatage** (21), `crust_syntax::fmt` : un seul appel à rustfmt par fichier.
    - rustfmt formate le fichier, chaque classe et chaque `import` y étant remplacé par un marqueur, ainsi que chaque membre placé dans un `struct` / `impl` temporaire;
    - Crust écrit les en-têtes `class … over …` et `impl Trait` au style rustfmt : `where` sur plusieurs lignes, type des composants `over` formaté;
    - les commentaires sont conservés, y compris en fin de ligne;
    - les lignes vides de l'original sont gardées (au plus une);
    - les attributs internes `#![...]` sont recopiés tels quels (rustfmt les supprime d'un `impl`);
    - un fichier qui contient une erreur Crust est refusé;
    - accessible par `cargo crust fmt [--check]` et par `textDocument/formatting` dans `crust-lsp`;
    - tests : stabilité (formater deux fois ne change rien), sens préservé (Rust généré identique, au formatage près, sur tous les fichiers de test), rendu attendu.
  - **Performances** (`crates/crust-build/tests/perf.rs`, projet de 500 `.rsc`, release) :
    - transpileur : 241 Mo/s;
    - premier build, `crust-build` compris : 1,9 s;
    - build sans changement : 12 ms;
    - build script seul : 10 ms. `cargo crust init` ajoute `opt-level = 3` pour `crust-syntax` et `crust-build` seulement ; sans ce réglage, 28 ms;
    - modification d'un `.rsc` : environ +30 ms par rapport à la modification d'un `.rs` (285 ms);
    - `cargo crust check` : aucun surcoût mesurable;
    - latence ajoutée par `crust-lsp` : 0,04 ms (`.rs`) et 0,07 ms (`.rsc`) en médiane, 3 ms au 99e centile.
- **Retours de migration (2026-10-21)**, sur RustUI et le Simulateur électronique, environ 85 `.rsc`, sorties identiques. Corrections :
  - `[package.metadata.crust] private = [...]` : dossiers ou fichiers `pub(crate)` (remplace l'option `module-visibility` de 12.1.4);
  - `#![cfg]` de tête reporté sur le `mod` et le `pub use` générés;
  - `allow(clippy::module_inception)` pour `banc/Banc.rsc`;
  - `include_str!` / `include_bytes!` / `include!` relatifs réécrits vers le `.rsc`;
  - `import { A, B } from "./dossier/"`;
  - **révision de 15.2.7** : la `struct` est écrite sur la ligne de l'en-tête de la classe, avant l'`impl`, et les attributs de la classe restent sur leurs lignes. Même avec `cargo build` seul, « struct X defined here » et les erreurs de `#[derive]` tombent sur les bonnes lignes;
  - extension VS Code : `breakpointMode: "file"` par défaut (plusieurs copies compilées d'un même paquet), lettre de lecteur normalisée sous Windows.
- **`cargo crust migrate`** : conversion automatique d'un projet Rust (`crust_syntax::migrate`, cas sans ambiguïté), avec instantané de tous les fichiers touchés et restauration si `cargo check` échoue. Testé sur `p6_avant_migration` : sortie identique, tests au vert.
- **Non pris en charge pour l'instant** :
  - `.rsc` dans `tests/`, `examples/`, `benches/`;
  - `mod x;` à l'intérieur d'un module inline d'un `.rsc` (chemin approximé).

---

# 30. Philosophie du projet

Le projet reste conservateur.

Une bonne fonctionnalité `.rsc` peut se décrire ainsi :

> « Cette syntaxe est simplement une écriture plus pratique de constructions Rust déjà valides. »

Éviter autant que possible d'ajouter une nouvelle sémantique. Rester compréhensible par un développeur Rust expérimenté.

```rust
pub class Servomoteur over Moteur(parent) {
    position: f32,
}
```

doit toujours pouvoir être lu mentalement comme :

```rust
pub struct Servomoteur {
    pub parent: Moteur,
    position: f32,
}
```

Critère de décision pour toute nouvelle fonctionnalité :

1. Elle se traduit localement et de façon déterministe en Rust standard.
2. Elle ne casse aucun fichier `.rs` valide (C1).
3. Elle ne dégrade ni les diagnostics, ni l'éditeur, ni la vitesse.
4. Elle supprime une douleur réelle et fréquente du développement Rust.

---

# 31. Première tâche proposée à Codex

**Ne pas commencer par analyser ou modifier `rustc`.** L'architecture retenue n'y touche pas.

1. ~~Réaliser les spikes S1 à S7~~ : **fait** (`spikes/RAPPORT.md`, prototypes dans `spikes/`). Lire le rapport : il impose deux contraintes de conception (sections 17.2 et 20.2).
2. Proposer la structure du dépôt :
   ```text
   crust/
       crates/crust-syntax/
       crates/crust-build/
       crates/cargo-crust/
       crates/crust-lsp/
       editors/vscode/
       tests/{transpile,crates,diagnostics,corpus,lsp}/
       Specs/
   ```
3. Implémenter la **phase P1** :
   - lexer (dérivé de `rustc_lexer` ou équivalent), parseur léger tolérant aux erreurs;
   - traduction `class` / `over` / `impl Trait` / generics avec la disposition de la section 15;
   - source maps;
   - `cargo crust expand`;
   - tests 27.1 et 27.4.
4. Avant la phase P2, produire :
   - la spécification exacte du format de source map;
   - l'algorithme de construction de l'arbre de modules (section 12) en pseudo-code;
   - la liste des cas limites découverts, à reporter dans ce cahier des charges.

Priorité absolue :

> conserver Rust standard intact et ajouter uniquement une couche syntaxique `.rsc`, rapide et parfaitement intégrée à l'éditeur.

---

# Annexe A — Évolutions envisagées après la V1

À évaluer avec le critère de la section 30. Aucune n'est engagée.

| Idée | Traduction | Douleur supprimée |
|---|---|---|
| Méthodes et `impl Trait` dans un `enum` d'un `.rsc` | `enum` + `impl` | Même dispersion que pour les structs |
| Délégation **explicite** : `use self.moteur::{accelerer, freiner};` dans une classe | Méthodes de transfert `#[inline]` générées | Écriture répétitive de méthodes de transfert |
| Classes tuple : `class Metres(pub f64) { … }` | struct tuple + `impl` | Newtypes |
| `.rsc` dans `tests/`, `examples/`, `benches/` | Stubs générés | Cohérence du projet |
| Support Neovim, Helix, Zed | Même `crust-lsp` | Autres éditeurs |
