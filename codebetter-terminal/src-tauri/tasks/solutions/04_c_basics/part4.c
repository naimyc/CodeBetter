#include <stdio.h>

int fahrenheit(int celsius) {
    return celsius * 9 / 5 + 32;
}

int main(void) {
    printf("=== CB Thermo ===\n");
    int von, bis;
    scanf("%d %d", &von, &bis);
    for (int c = von; c <= bis; c += 10) {
        printf("%d C = %d F\n", c, fahrenheit(c));
    }
    return 0;
}
