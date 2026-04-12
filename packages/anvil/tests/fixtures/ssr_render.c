#include <stdint.h>

extern unsigned char __heap_base;

static int32_t OFFSET = 0;

static const char PREFIX[] = "<section data-component=\"hello\"><h1>Hello, ";
static const char SUFFIX[] = "</h1></section>";

__attribute__((export_name("alloc")))
int32_t alloc(int32_t len) {
    if (OFFSET == 0) {
        OFFSET = (int32_t)(uintptr_t)&__heap_base;
    }
    int32_t ptr = OFFSET;
    OFFSET += len;
    return ptr;
}

static int32_t find_name_start(uint8_t *src, int32_t len) {
    for (int32_t i = 0; i + 7 < len; i++) {
        if (src[i] == '"' &&
            src[i + 1] == 'n' &&
            src[i + 2] == 'a' &&
            src[i + 3] == 'm' &&
            src[i + 4] == 'e' &&
            src[i + 5] == '"' &&
            src[i + 6] == ':' &&
            src[i + 7] == '"') {
            return i + 8;
        }
    }
    return -1;
}

__attribute__((export_name("render")))
int64_t render(int32_t ptr, int32_t len) {
    uint8_t *src = (uint8_t *)(uintptr_t)ptr;
    int32_t name_start = find_name_start(src, len);
    int32_t name_len = 0;

    if (name_start >= 0) {
        while (name_start + name_len < len && src[name_start + name_len] != '"') {
            name_len += 1;
        }
    }

    int32_t prefix_len = (int32_t)(sizeof(PREFIX) - 1);
    int32_t suffix_len = (int32_t)(sizeof(SUFFIX) - 1);
    int32_t out_len = prefix_len + name_len + suffix_len;
    int32_t out_ptr = alloc(out_len);
    uint8_t *dst = (uint8_t *)(uintptr_t)out_ptr;

    for (int32_t i = 0; i < prefix_len; i++) {
        dst[i] = (uint8_t)PREFIX[i];
    }
    for (int32_t i = 0; i < name_len; i++) {
        dst[prefix_len + i] = src[name_start + i];
    }
    for (int32_t i = 0; i < suffix_len; i++) {
        dst[prefix_len + name_len + i] = (uint8_t)SUFFIX[i];
    }

    return ((int64_t)(uint32_t)out_ptr << 32) | (uint32_t)out_len;
}
