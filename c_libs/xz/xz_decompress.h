#ifndef XZ_DECOMPRESS_H
#define XZ_DECOMPRESS_H

#include <stddef.h>
#include <stdint.h>

int xz_decompress(const uint8_t *src, size_t src_size,
                  uint8_t *dst, size_t dst_capacity,
                  size_t *out_size);

#endif
