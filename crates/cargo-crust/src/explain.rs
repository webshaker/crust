//! `cargo crust explain CRxxxx` (section 19.2).

pub fn text(code: &str) -> Option<&'static str> {
    let code = code.to_ascii_uppercase();
    Some(match code.as_str() {
        "CR0001" => {
            "Syntaxe invalide dans une classe ou un import.

Une classe contient des champs `nom: Type,` (la virgule est obligatoire, même
avant une méthode), des fonctions, des constantes et des `impl Trait { }` :

    pub class Moteur<T> over Base(nom) where T: Clone {
        pub vitesse: u32,
        pub fn new() -> Self { ... }
        impl Default { fn default() -> Self { ... } }
    }

Un import s'écrit `import Nom from \"./Fichier.rsc\";` ou
`import { A, B as C } from \"@/chemin\";`."
        }
        "CR0002" => {
            "Un champ porte le même nom qu'un champ généré par `over`.

    pub class Servo over Moteur {   // génère `moteur: Moteur`
        moteur: u8,                 // erreur
    }

Nommez explicitement le composant (`over Moteur(base)`) ou renommez le champ."
        }
        "CR0003" => {
            "Deux composants `over` génèrent le même nom de champ.

    pub class Robot over Moteur, Moteur { }        // erreur : deux fois `moteur`
    pub class Robot over Moteur(gauche), Moteur(droite) { }   // correct"
        }
        "CR0004" => {
            "Le nom du champ `over` ne peut être déduit que d'un type chemin (`Moteur`,
`crate::io::Uart<T>`). Pour une référence, un tableau, un tuple…, donnez un nom :

    pub class Vue over &'a Config(config) { }"
        }
        "CR0005" => {
            "Élément interdit dans le corps d'une classe : `struct`, `enum`, `trait`,
`mod`, `use`, `import`, classe imbriquée, type associé inhérent (`type T = ...;`),
ou `impl Trait for AutreType`.

Déclarez-les au niveau du module, à côté de la classe. Dans une classe,
`impl Trait { }` s'applique toujours à la classe elle-même."
        }
        "CR0006" => {
            "Fichier d'import introuvable, ou chemin mal formé.

Le chemin commence par `./` ou `../` (relatif au fichier courant) ou par `@/`
(relatif à `src/`). Sans extension, Crust cherche `.rsc` puis `.rs`."
        }
        "CR0007" => {
            "Le nom importé n'est pas défini au premier niveau du fichier `.rsc` visé.
Vérifiez l'orthographe (une suggestion est proposée quand un nom proche existe)."
        }
        "CR0008" => {
            "Import sans extension ambigu : `X.rsc` et `X.rs` existent tous les deux.
Précisez l'extension dans le chemin."
        }
        "CR0009" => {
            "Deux fichiers donnent le même module : `Moteur.rsc` et `moteur.rsc`, ou
`Moteur.rsc` et un `moteur.rs` déjà déclaré par `mod`, ou un fichier et un
répertoire de même nom. Renommez l'un d'eux ; le second n'est pas monté."
        }
        "CR0010" => {
            "Un répertoire contient des `.rsc` mais son module est défini par un fichier
`.rs` (`src/outils.rs` ou `src/outils/mod.rs`). Ajoutez dans ce fichier :

    include!(concat!(env!(\"OUT_DIR\"), \"/crust/dir/outils.rs\"));"
        }
        "CR0011" => {
            "Le fichier `.rs` importé est déclaré par plusieurs `mod` : Crust ne sait pas
quel module désigner. Un fichier ne doit être monté qu'une fois ; ailleurs,
utilisez `use` vers ce module."
        }
        "CR0012" => {
            "Un nom de répertoire ou de fichier ne donne pas un identifiant Rust valide
(tiret, chiffre initial, mot-clé…). Renommez-le : `mon-dossier` → `mon_dossier`."
        }
        "CR0013" => "Un import ne peut viser qu'un fichier situé sous `src/` dans le même paquet.",
        "CR0014" => "L'édition 2015 n'est pas prise en charge : utilisez `edition = \"2021\"` ou plus.",
        "CR0015" => {
            "`main.rsc` et `lib.rsc` sont insérés par `include!`, qui refuse les attributs
internes. Placez les `#![...]` dans `src/main.rs` / `src/lib.rs`, au-dessus de
la ligne `include!(concat!(env!(\"OUT_DIR\"), \"/crust/root_main.rs\"));`."
        }
        _ => return None,
    })
}
