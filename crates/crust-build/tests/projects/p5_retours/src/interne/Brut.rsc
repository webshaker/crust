/// Publique dans un dossier privé : clippy (`not_unsafe_ptr_arg_deref`, refusé
/// par défaut) ne doit pas la considérer comme exportée.
pub fn lire(p: *const u8) -> u8 {
    unsafe { *p }
}
