#include <stdio.h>

int main(void) {
    printf("=== CB Thermo ===\n");
    int celsius = 20;
    int fahrenheit = celsius * 9 / 5 + 32;
    printf("Celsius: %d\n", celsius);
    printf("Fahrenheit: %d\n", fahrenheit);
    return 0;
}
