//@ no-compile
#[derive(Default)]
pub struct Moteur;

pub struct Collision { pub moteur: Moteur, }

impl Collision {
compile_error!("[CR0002] le champ `moteur` est déjà généré par `over Moteur`\n= help: nommez explicitement le composant : `over Moteur(autre_nom)`");
}


pub struct Doublon { pub moteur: Moteur, }

impl Doublon {
compile_error!("[CR0003] deux composants `over` génèrent le champ `moteur`\n= help: donnez un nom explicite à l'un d'eux : `over Type(autre_nom)`");
}


pub struct NonChemin { }

impl NonChemin {
compile_error!("[CR0004] ce type n'est pas un chemin : le nom du champ ne peut pas être déduit\n= help: nommez le champ explicitement : `over &'static Moteur(nom)`");
}


pub struct Interdits { 
x: u8, }

impl Interdits {
compile_error!("[CR0005] `struct` interdit dans le corps d'une classe\n= help: déclarez-le au niveau du module, à côté de la classe");
compile_error!("[CR0005] un type associé inhérent interdit dans le corps d'une classe\n= help: les types associés ne sont permis que dans un `impl Trait { }`");
compile_error!("[CR0005] `use` interdit dans le corps d'une classe\n= help: placez les `use` au niveau du fichier");
compile_error!("[CR0005] `impl Trait for AutreType` interdit dans une classe\n= help: dans une classe, `impl Trait { }` s'applique à la classe elle-même ; écrivez cet `impl` hors de la classe");
    fn ok(&self) {}
}


pub struct SansVirgule { 
b: u8, }

impl SansVirgule {
compile_error!("[CR0001] `,` attendue après le champ\n= help: séparez les champs par des virgules");
}


compile_error!("[CR0001] paramètres génériques non fermés : `>` attendu");

fn suite_valide() {}
