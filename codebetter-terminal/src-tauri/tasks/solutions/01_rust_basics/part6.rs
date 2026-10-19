use std::io;

fn main() {
    println!("=== CB Shop ===");
    let artikel = "Apfel";
    let preis = 2;
    let mut eingabe = String::new();
    io::stdin().read_line(&mut eingabe).unwrap();
    let menge: u32 = eingabe.trim().parse().unwrap();
    let mut summe = 0;
    summe += gesamtpreis(preis, menge);
    println!("Artikel: {artikel}");
    println!("Preis: {preis} Euro");
    println!("Menge: {menge}");
    println!("Summe: {summe} Euro");

    let rabatt = if summe >= 10 { 1 } else { 0 };
    if rabatt > 0 {
        println!("Rabatt: {rabatt} Euro");
    } else {
        println!("Kein Rabatt");
    }
    println!("Zu zahlen: {} Euro", summe - rabatt);
}

fn gesamtpreis(preis: u32, menge: u32) -> u32 {
    preis * menge
}
