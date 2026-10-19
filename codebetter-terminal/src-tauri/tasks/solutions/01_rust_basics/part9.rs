use std::io;

struct Posten {
    name: String,
    preis: u32,
    menge: u32,
}

fn main() {
    println!("=== CB Shop ===");
    let mut posten: Vec<Posten> = Vec::new();
    for zeile in io::stdin().lines() {
        let zeile = zeile.unwrap();
        if zeile.trim().is_empty() {
            continue;
        }
        let mut teile = zeile.split_whitespace();
        let artikel = teile.next().unwrap();
        let menge: u32 = teile.next().unwrap().parse().unwrap();
        posten.push(Posten { name: artikel.to_string(), preis: preis_von(artikel), menge });
    }

    let mut summe = 0;
    for p in &posten {
        let betrag = gesamtpreis(p.preis, p.menge);
        println!("{} x{} = {} Euro", p.name, p.menge, betrag);
        summe += betrag;
    }
    println!("Posten: {}", posten.len());
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
