#ifndef CARVE_FFI_H
#define CARVE_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum carve_status {
    CARVE_STATUS_OK = 0,
    CARVE_STATUS_INVALID_POINTER = 1,
    CARVE_STATUS_INVALID_UTF8 = 2,
    CARVE_STATUS_PANIC = 3
};

int32_t carve_to_html(
    const uint8_t *input,
    size_t input_len,
    uint8_t **out_data,
    size_t *out_len
);

void carve_html_free(uint8_t *data, size_t len);

#ifdef __cplusplus
}
#endif

#endif
