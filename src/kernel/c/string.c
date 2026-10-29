/*
 * Freestanding memory primitives for the Hollowstar kernel.
 * Rust's core/compiler-builtins may emit calls to these symbols.
 */

typedef unsigned long size_t;

void *memcpy(void *dest, const void *src, size_t n)
{
    unsigned char *d = (unsigned char *)dest;
    const unsigned char *s = (const unsigned char *)src;
    size_t i;
    for (i = 0; i < n; i++)
        d[i] = s[i];
    return dest;
}

void *memmove(void *dest, const void *src, size_t n)
{
    unsigned char *d = (unsigned char *)dest;
    const unsigned char *s = (const unsigned char *)src;
    size_t i;
    if (d < s) {
        for (i = 0; i < n; i++)
            d[i] = s[i];
    } else if (d > s) {
        for (i = n; i > 0; i--)
            d[i - 1] = s[i - 1];
    }
    return dest;
}

void *memset(void *dest, int value, size_t n)
{
    unsigned char *d = (unsigned char *)dest;
    size_t i;
    for (i = 0; i < n; i++)
        d[i] = (unsigned char)value;
    return dest;
}

int memcmp(const void *a, const void *b, size_t n)
{
    const unsigned char *x = (const unsigned char *)a;
    const unsigned char *y = (const unsigned char *)b;
    size_t i;
    for (i = 0; i < n; i++) {
        if (x[i] != y[i])
            return (int)x[i] - (int)y[i];
    }
    return 0;
}

size_t strlen(const char *s)
{
    size_t n = 0;
    while (s[n])
        n++;
    return n;
}
