#include <stdlib.h>
#include <string.h>

typedef struct {
    int width;
    int height;
    int* pixels;
} Buffer;

// 2 Numbers -> Pointer _ptr suffix
void* zera_create_buffer_ptr(double w, double h) {
    Buffer* b = (Buffer*)malloc(sizeof(Buffer));
    b->width = (int)w;
    b->height = (int)h;
    b->pixels = (int*)malloc(sizeof(int) * b->width * b->height);

    for (int i = 0; i < b->width * b->height; i++) {
        b->pixels[i] = 0;
    }

    return (void*)b;
}

// Pointer, 2 Numbers -> Number
double zera_get_pixel(void* ptr, double x, double y) {
    Buffer* b = (Buffer*)ptr;
    int ix = (int)x;
    int iy = (int)y;

    if (ix >= 0 && ix < b->width && iy >= 0 && iy < b->height) {
        return (double)b->pixels[iy * b->width + ix];
    }
    return -1.0;
}

// Pointer, 3 Numbers -> Null (Set pixel value)
void zera_set_pixel(void* ptr, double x, double y, double color) {
    Buffer* b = (Buffer*)ptr;
    int ix = (int)x;
    int iy = (int)y;

    if (ix >= 0 && ix < b->width && iy >= 0 && iy < b->height) {
        b->pixels[iy * b->width + ix] = (int)color;
    }
}

// Pointer -> Null (Free the memory)
void zera_free_buffer(void* ptr) {
    Buffer* b = (Buffer*)ptr;
    if (b) {
        free(b->pixels);
        free(b);
    }
}
