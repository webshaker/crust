fn main() {
    println!("{} {} {}", p5_retours::texte(), p5_retours::mesure(), p5_retours::brut());
    #[cfg(feature = "capture")]
    println!("{}", p5_retours::essai::capturer());
}
