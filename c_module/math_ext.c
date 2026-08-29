#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

// 1 Number arg -> 1 Number return
double zera_sqrt(double n) {
    return sqrt(n);
}

// 2 Number args -> 1 Number return
double zera_hypot(double a, double b) {
    return sqrt(a*a + b*b);
}

// 1 String arg, 1 Number arg -> 1 String return
const char* zera_repeat(const char* str, double times_d) {
    int times = (int)times_d;
    int len = strlen(str);

    char* result = malloc(len * times + 1);
    result[0] = '\0';

    for(int i = 0; i < times; i++) {
        strcat(result, str);
    }

    return result;
}

// 1 String arg, 2 Number args -> 1 String return
const char* zera_substring(const char* str, double start_d, double end_d) {
    int start = (int)start_d;
    int end = (int)end_d;
    int len = strlen(str);

    if (start < 0) start = 0;
    if (end > len) end = len;
    if (start >= end) return "";

    char* result = malloc(end - start + 1);
    memcpy(result, str + start, end - start);
    result[end - start] = '\0';

    return result;
}