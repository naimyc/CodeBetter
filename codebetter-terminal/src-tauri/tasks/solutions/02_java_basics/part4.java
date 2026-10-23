import java.util.Scanner;

public class Main {
    public static void main(String[] args) {
        System.out.println("=== CB Kino ===");
        String film = "Matrix";
        System.out.println("Film: " + film);
        Scanner scanner = new Scanner(System.in);
        int anzahl = scanner.nextInt();
        int summe = 0;
        for (int i = 1; i <= anzahl; i++) {
            int preis = preisFuer(scanner.nextInt());
            summe += preis;
            System.out.println("Ticket " + i + ": " + preis + " Euro");
        }
        System.out.println("Summe: " + summe + " Euro");
    }

    static int preisFuer(int alter) {
        if (alter < 12) {
            return 5;
        } else {
            return 9;
        }
    }
}
