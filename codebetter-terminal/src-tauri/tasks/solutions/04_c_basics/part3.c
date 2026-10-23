#include <stdio.h>

int main(void) {
    printf("=== CB Thermo ===\n");
    int celsius;
    scanf("%d", &celsius);
    int fahrenheit = celsius * 9 / 5 + 32;
    printf("Celsius: %d\n", celsius);
    printf("Fahrenheit: %d\n", fahrenheit);
    if (celsius <= 0) {
        printf("Achtung: Frost!\n");
    } else if (celsius >= 30) {
        printf("Achtung: Hitze!\n");
    }
    return 0;
}
