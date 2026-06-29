#ifndef ZSTD_DECOMPRESS_H
#define ZSTD_DECOMPRESS_H

#include <stddef.h>
#include <stdint.h>

/* Returns 0 on success, negative on error. */
int zstd_decompress(const uint8_t *src, size_t src_size,
                    uint8_t *dst, size_t dst_capacity,
                    size_t *out_size);

#endif
