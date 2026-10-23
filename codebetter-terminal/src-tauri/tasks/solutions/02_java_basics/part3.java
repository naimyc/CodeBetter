import java.util.Scanner;

public class Main {
    public static void main(String[] args) {
        System.out.println("=== CB Kino ===");
        String film = "Matrix";
        Scanner scanner = new Scanner(System.in);
        int alter = scanner.nextInt();
        int preis;
        if (alter < 12) {
            preis = 5;
        } else {
            preis = 9;
        }
        System.out.println("Film: " + film);
        System.out.println("Alter: " + alter);
        System.out.println("Preis: " + preis + " Euro");
    }
}
