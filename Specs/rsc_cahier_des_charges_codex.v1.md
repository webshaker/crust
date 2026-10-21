# Cahier des charges — Extension syntaxique Rust `.rsc`

## 1. Objectif général

Créer une extension syntaxique de Rust permettant d'écrire certains types de manière plus lisible, sans modifier les règles sémantiques fondamentales de Rust.

Le projet ne cherche **pas** à transformer Rust en langage orienté objet.

L'objectif est uniquement d'ajouter du sucre syntaxique autour de constructions Rust existantes, notamment :

- regrouper dans une même déclaration :
  - les champs d'une `struct`,
  - les méthodes `impl`,
  - les implémentations de `trait`;
- simplifier l'organisation des fichiers;
- permettre un fichier par "classe";
- simplifier les imports entre fichiers;
- fournir une syntaxe légère de composition automatique avec `over`;
- conserver une compatibilité maximale avec Rust standard;
- permettre à des fichiers `.rsc` et `.rs` de cohabiter dans le même projet et dans la même crate.

Les fichiers `.rs` doivent conserver **exactement** la syntaxe Rust standard.

Les fichiers `.rsc` doivent accepter la syntaxe Rust habituelle plus les extensions définies ici.

---

# 2. Principe fondamental

Toute construction `.rsc` doit avoir une traduction claire et déterministe vers du Rust standard.

Le compilateur final doit continuer à utiliser les règles Rust existantes pour :

- le typage;
- le borrow checker;
- les lifetimes;
- les generics;
- les traits;
- la visibilité;
- la génération de code;
- les optimisations;
- l'ABI Rust.

Le projet ne doit pas introduire un nouveau modèle objet.

---

# 3. Extension de fichier

Les fichiers utilisant la nouvelle syntaxe utilisent l'extension :

```text
.rsc
```

Exemples :

```text
src/
    main.rs
    Moteur.rsc
    Servomoteur.rsc
    Capteur.rs
```

Les `.rs` restent du Rust standard.

Les `.rsc` utilisent le parser étendu.

Les deux types de fichiers doivent pouvoir cohabiter et se référencer mutuellement.

---

# 4. Syntaxe `class`

## 4.1. Exemple minimal

Syntaxe `.rsc` :

```rust
pub class Moteur {
    pub vitesse: u32,

    pub fn new() -> Self {
        Self {
            vitesse: 0,
        }
    }

    pub fn accelerer(&mut self, delta: u32) {
        self.vitesse += delta;
    }
}
```

Traduction conceptuelle en Rust standard :

```rust
pub struct Moteur {
    pub vitesse: u32,
}

impl Moteur {
    pub fn new() -> Self {
        Self {
            vitesse: 0,
        }
    }

    pub fn accelerer(&mut self, delta: u32) {
        self.vitesse += delta;
    }
}
```

`class` ne doit donc pas introduire de nouvelle sémantique.

Il s'agit uniquement d'une syntaxe regroupant la `struct` et ses blocs `impl`.

---

# 5. Traits dans une `class`

Une `class` doit pouvoir contenir directement des implémentations de traits.

Exemple :

```rust
pub class Moteur {
    pub vitesse: u32,

    pub fn new() -> Self {
        Self { vitesse: 0 }
    }

    impl Default {
        fn default() -> Self {
            Self::new()
        }
    }
}
```

Traduction :

```rust
pub struct Moteur {
    pub vitesse: u32,
}

impl Moteur {
    pub fn new() -> Self {
        Self { vitesse: 0 }
    }
}

impl Default for Moteur {
    fn default() -> Self {
        Self::new()
    }
}
```

La syntaxe :

```rust
impl TraitName {
    ...
}
```

à l'intérieur d'une `class` signifie donc :

```rust
impl TraitName for NomDeLaClasse {
    ...
}
```

---

# 6. Pas d'héritage objet

Le projet ne doit pas introduire l'héritage de classes au sens C++/Java.

En particulier, il ne faut pas ajouter automatiquement :

- de vtable;
- de méthodes virtuelles;
- de conversion implicite classe fille -> classe parent;
- de surcharge dynamique;
- de polymorphisme objet supplémentaire;
- de RTTI supplémentaire;
- de nouveau système de dispatch.

Rust conserve ses mécanismes habituels :

- composition;
- traits;
- generics;
- `dyn Trait`.

---

# 7. Syntaxe `over`

`over` représente une **composition automatique**, et non un héritage objet.

## 7.1. Nom automatique

Syntaxe :

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

Le nom du champ est déduit du nom du type en `snake_case`.

Exemples :

```text
Moteur          -> moteur
MoteurBrushless -> moteur_brushless
RobotArm        -> robot_arm
```

---

# 8. Nom explicite pour `over`

Il doit être possible de choisir explicitement le nom du champ généré.

Syntaxe :

```rust
pub class Servomoteur over Moteur(parent) {
    position: f32,
}
```

Traduction :

```rust
pub struct Servomoteur {
    pub parent: Moteur,
    position: f32,
}
```

Autre exemple :

```rust
pub class Axe over Moteur(motorisation) {
    position: f32,
}
```

Traduction :

```rust
pub struct Axe {
    pub motorisation: Moteur,
    position: f32,
}
```

Règle :

```text
over Type
```

équivaut à :

```text
over Type(nom_deduit_en_snake_case)
```

Le champ généré par `over` est public.

Si le nom généré ou explicite entre en collision avec un autre champ, le compilateur doit produire une erreur claire pointant sur le `.rsc` d'origine.

---

# 9. Accès au composant créé par `over`

Exemple :

```rust
let mut s = Servomoteur::new();

s.moteur.vitesse = 1200;
s.moteur.accelerer(100);
```

Avec un nom explicite :

```rust
pub class Servomoteur over Moteur(parent) {
    position: f32,
}
```

Utilisation :

```rust
s.parent.vitesse = 1200;
s.parent.accelerer(100);
```

Aucune délégation implicite ne doit être ajoutée dans la première version.

Ainsi :

```rust
s.accelerer();
```

ne doit pas automatiquement appeler :

```rust
s.moteur.accelerer();
```

Le programmeur doit écrire explicitement l'accès au composant.

---

# 10. Constructeurs

Le projet ne crée pas de constructeur spécial.

Les fonctions associées Rust restent utilisées.

Exemple :

```rust
pub class Servomoteur over Moteur {
    position: f32,

    pub fn new() -> Self {
        Self {
            moteur: Moteur::new(),
            position: 0.0,
        }
    }
}
```

Le champ généré par `over` doit être initialisé normalement.

La V1 ne doit pas essayer de générer automatiquement le constructeur parent.

---

# 11. Organisation des fichiers

Objectif principal :

> permettre naturellement un fichier par classe.

Exemple :

```text
src/
    main.rsc
    robot/
        Robot.rsc
        Moteur.rsc
        Servomoteur.rsc
```

Il ne doit pas être nécessaire de créer des fichiers `mod.rs` uniquement pour déclarer l'arborescence.

---

# 12. Import simplifié

La syntaxe cible est :

```rust
import Moteur from "./robot/Moteur.rsc";
```

Exemple :

```rust
import Moteur from "./Moteur.rsc";
import Servomoteur from "./Servomoteur.rsc";
```

Cette instruction doit :

1. charger le fichier cible dans la crate;
2. garantir qu'un fichier donné ne correspond qu'à un seul module/type logique;
3. rendre le symbole demandé visible dans le fichier courant.

Plusieurs fichiers important le même fichier `.rsc` doivent manipuler exactement le même type Rust.

Il ne faut jamais générer plusieurs définitions indépendantes du même type.

---

# 13. Imports de fichiers `.rs`

Un fichier `.rsc` doit également pouvoir importer du Rust standard.

Exemple cible :

```rust
import Capteur from "./Capteur.rs";
```

Le système doit permettre à un `.rsc` d'utiliser :

- `struct`;
- `enum`;
- `trait`;
- fonctions;
- constantes;
- types;
- modules;

définis dans des fichiers `.rs`.

Les constructions Rust standard telles que `use crate::...` doivent également rester disponibles dans les `.rsc`.

---

# 14. Utilisation d'un type `.rsc` depuis un fichier `.rs`

Un type défini dans un `.rsc` doit apparaître au reste de la crate comme un type Rust normal.

Conceptuellement :

```rust
// Moteur.rsc
pub class Moteur {
    pub vitesse: u32,
}
```

doit exposer le même type qu'aurait exposé :

```rust
pub struct Moteur {
    pub vitesse: u32,
}
```

Un fichier `.rs` doit donc pouvoir :

- créer une valeur;
- prendre `&Moteur`;
- prendre `&mut Moteur`;
- recevoir ou retourner `Moteur`;
- implémenter des traits autorisés par les règles Rust;
- utiliser ses méthodes;
- l'utiliser dans des generics.

La frontière `.rs` / `.rsc` doit être transparente après parsing/desugaring.

---

# 15. Compatibilité Rust

Le but n'est pas de forker le langage Rust sur le plan sémantique.

Il faut essayer de conserver au maximum :

- la compatibilité avec `rustc`;
- le format des crates;
- le modèle de types;
- MIR;
- LLVM/codegen;
- borrow checking;
- traits;
- macros Rust lorsque possible;
- Cargo lorsque possible.

Une architecture souhaitable est :

```text
.rsc
  |
  v
Parser RSC
  |
  v
AST/HIR compatible Rust
  |
  v
pipeline rustc habituel
```

Le plus tôt possible, les nouvelles constructions doivent être abaissées vers des constructions Rust ordinaires.

---

# 16. Diagnostics et positions sources

C'est une exigence majeure.

Toutes les erreurs doivent pointer vers les fichiers `.rsc` réellement écrits par l'utilisateur.

Exemple :

```rust
pub class Moteur {
    vitesse: u32,

    pub fn test(&mut self) {
        self.vitesse = "abc";
    }
}
```

L'erreur de type doit pointer vers :

```rust
"abc"
```

dans `Moteur.rsc`.

Le système ne doit pas montrer au développeur un fichier Rust intermédiaire généré.

Il faut conserver les informations de source/spans pendant les transformations.

Cela concerne notamment :

- erreurs de type;
- borrow checker;
- lifetime errors;
- erreurs de trait;
- champs inexistants;
- collisions de noms;
- erreurs d'import;
- erreurs dans les méthodes;
- suggestions de correction.

Les diagnostics issus du Rust transformé doivent pouvoir être reliés aux positions exactes dans le `.rsc`.

---

# 17. Erreurs spécifiques à `.rsc`

Exemples de diagnostics nécessaires.

## Collision de champ

```rust
pub class Servomoteur over Moteur {
    moteur: AutreType,
}
```

Doit produire une erreur expliquant que `over Moteur` génère déjà le champ `moteur`.

## Fichier absent

```rust
import Moteur from "./Absent.rsc";
```

Doit pointer sur le chemin de l'import.

## Symbole absent

```rust
import Moteur from "./Capteur.rsc";
```

si `Capteur.rsc` ne définit/exporte pas `Moteur`.

L'erreur doit pointer sur `Moteur` dans l'instruction `import`.

---

# 18. rust-analyzer

`rust-analyzer` est open source.

Le projet devra probablement maintenir une version adaptée afin de comprendre les fichiers `.rsc`.

Fonctionnalités attendues :

- parsing `.rsc`;
- coloration syntaxique;
- diagnostics;
- autocomplétion;
- goto definition;
- find references;
- rename symbol;
- hover;
- type information;
- complétion des champs;
- complétion des méthodes;
- connaissance du champ généré par `over`;
- connaissance des imports `.rsc`;
- coexistence avec les `.rs`.

Exemple :

```rust
s.moteur.
```

doit proposer les champs et méthodes disponibles sur `Moteur`.

Avec :

```rust
pub class Servomoteur over Moteur(parent) {
}
```

l'analyseur doit comprendre :

```rust
s.parent.
```

comme un accès à un `Moteur`.

---

# 19. Extension VS Code

Il faudra fournir une extension VS Code dédiée ou une adaptation de l'extension rust-analyzer.

Elle doit reconnaître :

```text
*.rsc
```

comme fichiers du nouveau dialecte.

Elle doit fournir au minimum :

- syntax highlighting;
- lancement du language server;
- erreurs en temps réel;
- complétion;
- navigation;
- renommage;
- formatage à terme.

Les fichiers `.rs` doivent continuer à être supportés normalement dans le même workspace.

---

# 20. Formatage

À terme, prévoir un équivalent de `rustfmt` ou une extension de `rustfmt`.

Exemple de style cible :

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

Le style général doit rester aussi proche que possible de `rustfmt`.

---

# 21. Non-objectifs de la V1

Ne pas implémenter dans la V1 :

- héritage C++;
- classes virtuelles;
- méthodes virtuelles automatiques;
- surcharge de fonctions;
- dispatch dynamique supplémentaire;
- délégation automatique de méthodes;
- constructeurs automatiques complexes;
- destructeurs spécifiques au dialecte;
- propriétés/getters automatiques;
- conversion implicite enfant -> parent;
- multiple inheritance;
- réflexion objet;
- garbage collector;
- changement du borrow checker;
- nouvelle ABI.

---

# 22. Exemple complet

## `Moteur.rsc`

```rust
pub class Moteur {
    pub vitesse: u32,

    pub fn new() -> Self {
        Self {
            vitesse: 0,
        }
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

## `Servomoteur.rsc`

```rust
import Moteur from "./Moteur.rsc";

pub class Servomoteur over Moteur {
    pub position: f32,

    pub fn new() -> Self {
        Self {
            moteur: Moteur::new(),
            position: 0.0,
        }
    }

    pub fn aller_a(&mut self, position: f32) {
        self.position = position;
    }
}
```

Utilisation :

```rust
let mut servo = Servomoteur::new();

servo.moteur.accelerer(1200);
servo.position = 90.0;
```

---

# 23. Exemple avec nom explicite du composant

```rust
import Moteur from "./Moteur.rsc";

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

Utilisation :

```rust
let mut servo = Servomoteur::new();

servo.parent.vitesse = 1000;
servo.parent.accelerer(200);
```

---

# 24. Critères d'acceptation V1

La V1 est considérée fonctionnelle si les cas suivants fonctionnent.

## Test 1 — classe simple

Un fichier `.rsc` contenant une `class` avec champs et méthodes compile.

## Test 2 — trait

Une implémentation de trait déclarée dans une `class` compile et fonctionne.

## Test 3 — `over` automatique

```rust
class B over A
```

génère un champ public `a: A`.

## Test 4 — `over` nommé

```rust
class B over A(parent)
```

génère un champ public `parent: A`.

## Test 5 — import `.rsc`

Un `.rsc` peut importer un autre `.rsc`.

## Test 6 — imports multiples

Deux fichiers important le même `.rsc` utilisent exactement la même définition/type.

## Test 7 — coexistence `.rs` / `.rsc`

Une crate contient des `.rs` et `.rsc` pouvant échanger des valeurs typées.

## Test 8 — erreur de type

Une erreur située dans une méthode `.rsc` pointe vers la ligne correcte dans le `.rsc`.

## Test 9 — borrow checker

Une erreur d'emprunt créée dans un `.rsc` pointe vers les bonnes positions dans le `.rsc`.

## Test 10 — rust-analyzer

VS Code propose la complétion d'un champ/méthode sur une classe `.rsc`.

## Test 11 — champ `over`

VS Code comprend :

```rust
servo.moteur.
```

ou :

```rust
servo.parent.
```

et propose les membres du type composant.

---

# 25. Architecture de développement conseillée

Ne pas commencer par modifier tout rustc.

Procéder progressivement.

## Phase 1 — grammaire et prototype

Créer un prototype minimal capable de parser :

```rust
class
over
import ... from ...
```

et de produire du Rust standard.

Valider d'abord la syntaxe et les règles de traduction.

## Phase 2 — identité des modules/types

Garantir que l'import d'un même fichier depuis plusieurs endroits conserve une définition unique.

## Phase 3 — diagnostics

Mettre en place la conservation/mapping précis des positions sources.

Ne pas considérer un prototype comme réussi si les erreurs pointent uniquement vers du Rust généré.

## Phase 4 — intégration rustc

Intégrer la syntaxe suffisamment tôt dans le frontend pour réutiliser le maximum de la pipeline Rust existante.

## Phase 5 — rust-analyzer

Adapter le parser et le modèle sémantique de rust-analyzer.

## Phase 6 — VS Code

Adapter l'extension pour `.rsc`.

---

# 26. Philosophie du projet

Le projet doit rester conservateur.

Une bonne fonctionnalité `.rsc` est une fonctionnalité qui peut être décrite ainsi :

> "Cette syntaxe est simplement une écriture plus pratique de constructions Rust déjà valides."

Éviter autant que possible d'ajouter une nouvelle sémantique.

Le projet doit rester compréhensible par un développeur Rust expérimenté.

Exemple :

```rust
pub class Servomoteur over Moteur(parent) {
    position: f32,
}
```

doit toujours pouvoir être mentalement lu comme :

```rust
pub struct Servomoteur {
    pub parent: Moteur,
    position: f32,
}
```

---

# 27. Première tâche proposée à Codex

Commencer par analyser les sources actuelles de `rustc` et `rust-analyzer`, puis proposer une architecture minimale permettant :

1. de reconnaître les fichiers `.rsc`;
2. de parser une déclaration `class`;
3. de traduire `class` vers `struct + impl`;
4. de prendre en charge :
   ```rust
   over Type
   ```
   et :
   ```rust
   over Type(nom)
   ```
5. de conserver les spans des champs et méthodes;
6. de compiler une crate contenant à la fois `.rs` et `.rsc`;
7. de ne modifier aucune règle sémantique Rust non nécessaire.

Avant de coder massivement, produire :

- la liste exacte des crates/modules de rustc à modifier;
- la liste exacte des composants rust-analyzer à modifier;
- une proposition pour le chargement des `.rsc`;
- une proposition pour la résolution de `import ... from ...`;
- la stratégie de conservation des diagnostics/spans;
- un petit plan de tests.

Priorité absolue :

> conserver Rust standard intact et ajouter uniquement une couche syntaxique `.rsc`.
