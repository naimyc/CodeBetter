use std::io;

fn main() {
    println!("=== CB Shop ===");
    let mut artikel = String::new();
    io::stdin().read_line(&mut artikel).unwrap();
    let artikel = artikel.trim();
    let preis = preis_von(artikel);

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

fn preis_von(artikel: &str) -> u32 {
    match artikel {
        "Apfel" => 2,
        "Brot" => 3,
        "Milch" => 1,
        _ => 0,
    }
}
