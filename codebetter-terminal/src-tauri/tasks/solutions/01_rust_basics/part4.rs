fn main() {
    println!("=== CB Shop ===");
    let artikel = "Apfel";
    let preis = 2;
    let menge = 3;
    let mut summe = 0;
    summe += gesamtpreis(preis, menge);
    println!("Artikel: {artikel}");
    println!("Preis: {preis} Euro");
    println!("Menge: {menge}");
    println!("Summe: {summe} Euro");
}

fn gesamtpreis(preis: u32, menge: u32) -> u32 {
    preis * menge
}
