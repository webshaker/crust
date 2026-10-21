//@ no-compile
compile_error!("[CR0006] impossible de résoudre `./Moteur.rsc` dans ce contexte");
compile_error!("[CR0006] impossible de résoudre `@/robot/Truc` dans ce contexte");
compile_error!("[CR0006] impossible de résoudre `../x.rs` dans ce contexte");

impl Ok {             } pub struct Ok { x: u8, }
