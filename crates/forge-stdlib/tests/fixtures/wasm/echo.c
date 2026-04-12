#include <stdint.h>

extern unsigned char __heap_base;

static int32_t OFFSET = 0;

__attribute__((export_name("alloc")))
int32_t alloc(int32_t len) {
    if (OFFSET == 0) {
        OFFSET = (int32_t)(uintptr_t)&__heap_base;
    }
    int32_t ptr = OFFSET;
    OFFSET += len;
    return ptr;
}

__attribute__((export_name("echo")))
int64_t echo(int32_t ptr, int32_t len) {
    uint8_t *src = (uint8_t *)(uintptr_t)ptr;
    int32_t out_ptr = alloc(len);
    uint8_t *dst = (uint8_t *)(uintptr_t)out_ptr;

    for (int32_t i = 0; i < len; i++) {
        dst[i] = src[i];
    }

    return ((int64_t)(uint32_t)out_ptr << 32) | (uint32_t)len;
}
