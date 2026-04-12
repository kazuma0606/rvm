#include <stdint.h>

__attribute__((export_name("add")))
int32_t add(int32_t a, int32_t b) {
    return a + b;
}
