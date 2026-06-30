/* Minimal bzip2 decompressor — implements the bzip2 format.
 *
 * Supports: stream header, block header, Huffman decoding (2-6 tables),
 * BWT inverse, MTF inverse, RLE inverse.
 */

#include "bzip2_decompress.h"
#include "../kcompat.h"

/* ── Bit reader (big-endian, MSB-first) ─────────────────────────── */
typedef struct {
    const uint8_t *data;
    size_t size;
    size_t byte_pos;
    int bit_pos;
    uint32_t bits;
    int nbits;
} BitReaderBE;

static void brbe_init(BitReaderBE *br, const uint8_t *data, size_t size) {
    br->data = data; br->size = size; br->byte_pos = 0; br->bit_pos = 0;
    br->bits = 0; br->nbits = 0;
}

static void brbe_refill(BitReaderBE *br) {
    while (br->nbits <= 24 && br->byte_pos < br->size) {
        br->bits = (br->bits << 8) | br->data[br->byte_pos++];
        br->nbits += 8;
    }
}

static uint32_t brbe_read(BitReaderBE *br, int nbits) {
    while (br->nbits < nbits && br->byte_pos < br->size) {
        br->bits = (br->bits << 8) | br->data[br->byte_pos++];
        br->nbits += 8;
    }
    if (br->nbits < nbits) {
        br->bits <<= (nbits - br->nbits);
        br->nbits = nbits;
    }
    br->nbits -= nbits;
    uint32_t result = (br->bits >> br->nbits) & ((1u << nbits) - 1);
    return result;
}

static int brbe_get_bit(BitReaderBE *br) {
    if (br->nbits == 0) brbe_refill(br);
    if (br->nbits == 0) return 0;
    br->nbits--;
    return (int)((br->bits >> br->nbits) & 1);
}

/* ── Huffman decoder (bzip2 style) ──────────────────────────────── */
#define BZ_MAX_ALPHA 258
#define BZ_MAX_CODELEN 23

typedef struct {
    int len[BZ_MAX_ALPHA];
    int min_len;
    int max_len;
    /* Canonical code base per length */
    uint32_t base[BZ_MAX_CODELEN + 2];
    uint32_t limit[BZ_MAX_CODELEN + 2];
    uint16_t perm[BZ_MAX_ALPHA];
    int nperm;
} BzHuffman;

static void bz_huffman_build(BzHuffman *h, const int *lengths, int alpha_size) {
    for (int i = 0; i < alpha_size; i++) h->len[i] = lengths[i];

    /* Find min/max lengths */
    h->min_len = 32; h->max_len = 0;
    for (int i = 0; i < alpha_size; i++) {
        if (lengths[i] > 0) {
            if (lengths[i] < h->min_len) h->min_len = lengths[i];
            if (lengths[i] > h->max_len) h->max_len = lengths[i];
        }
    }

    /* Build canonical codes */
    int pp[BZ_MAX_ALPHA];
    for (int i = h->min_len; i <= h->max_len; i++) {
        for (int j = 0; j < alpha_size; j++) {
            if (h->len[j] == i) { pp[h->nperm] = j; h->nperm++; }
        }
    }
    for (int i = 0; i < h->nperm; i++) h->perm[i] = (uint16_t)pp[i];

    /* Compute base and limit arrays */
    uint32_t vec = 0;
    for (int i = h->min_len; i <= h->max_len; i++) {
        h->base[i] = vec;
        int n = 0;
        for (int j = 0; j < alpha_size; j++) if (h->len[j] == i) n++;
        vec += (uint32_t)n << (32 - i);
        h->limit[i] = vec - 1;
    }
    for (int i = h->min_len + 1; i <= h->max_len; i++) {
        h->base[i] = (h->base[i - 1] + h->limit[i - 1] - h->base[i - 1] + 1) << 1;
    }
}

static int bz_huffman_decode(BitReaderBE *br, const BzHuffman *h) {
    uint32_t vec = 0;
    for (int i = h->min_len; i <= h->max_len; i++) {
        vec = (vec << 1) | (uint32_t)brbe_get_bit(br);
        if (vec <= h->limit[i]) {
            uint32_t idx = h->base[i] + (vec >> (32 - i)) - (h->base[i] >> (32 - i));
            /* Actually, simpler approach: */
            int code = (int)vec >> (32 - i);
            int base_val = (int)(h->base[i] >> (32 - i));
            int offset = code - base_val;
            if (offset >= 0 && offset < h->nperm) {
                /* Find the symbol at this offset for this length */
                int count = 0;
                for (int j = 0; j < h->nperm; j++) {
                    if (h->len[h->perm[j]] == i) {
                        if (count == offset) return h->perm[j];
                        count++;
                    }
                }
            }
        }
    }
    return -1;
}

/* ── BWT inverse ────────────────────────────────────────────────── */
static void bwt_inverse(const uint8_t *bwt_data, int bwt_size, int orig_ptr,
                        uint8_t *out) {
    /* Build the T vector (character counts) */
    int count[256];
    kmemset(count, 0, sizeof(count));
    for (int i = 0; i < bwt_size; i++) count[bwt_data[i]]++;

    /* Build cumulative counts (C table) */
    int c[257];
    c[0] = 0;
    for (int i = 0; i < 256; i++) c[i + 1] = c[i] + count[i];

    /* Build the next vector */
    int *next = (int *)kmalloc(sizeof(int) * bwt_size);
    if (!next) return;

    int *bucket = (int *)kmalloc(sizeof(int) * 256);
    if (!bucket) { kfree(next); return; }
    kmemset(bucket, 0, sizeof(int) * 256);

    for (int i = 0; i < bwt_size; i++) {
        int ch = bwt_data[i];
        next[c[ch] + bucket[ch]] = i;
        bucket[ch]++;
    }

    /* Reconstruct the original string */
    int p = orig_ptr;
    for (int i = 0; i < bwt_size; i++) {
        p = next[p];
        out[i] = bwt_data[p];
    }

    kfree(bucket);
    kfree(next);
}

/* ── MTF inverse ────────────────────────────────────────────────── */
static void mtf_inverse(const uint8_t *mtf_data, int mtf_size,
                        uint8_t *out, int n_groups,
                        const uint8_t *alpha_map) {
    /* The MTF list is seeded with the actual in-use byte values (sorted
     * ascending) — bzip2's "seqToUnseq" table — not the raw alphabet indices.
     * Seeding with 0..n-1 only happens to be correct when the in-use set is
     * exactly {0,1,...,n-1}, so it silently corrupts most real payloads. */
    uint8_t order[256];
    for (int i = 0; i < n_groups; i++) order[i] = alpha_map[i];

    int out_pos = 0;
    int run_len = 0;
    int run_char = -1;

    for (int i = 0; i < mtf_size; i++) {
        int idx = mtf_data[i];

        if (idx <= 1) {
            /* RUNA/RUNB encoding */
            if (run_len == 0) run_len = 1;
            if (idx == 1) run_len = (run_len << 1) + 1;
            else run_len = run_len << 1;
            continue;
        }

        /* Output any pending run */
        if (run_len > 0) {
            if (run_char < 0) run_char = order[0];
            for (int j = 0; j < run_len; j++) out[out_pos++] = (uint8_t)run_char;
            run_len = 0;
        }

        /* MTF decode: idx-1 is the position in the order array */
        int pos = idx - 1;
        uint8_t ch = order[pos];
        out[out_pos++] = ch;

        /* Move to front */
        for (int j = pos; j > 0; j--) order[j] = order[j - 1];
        order[0] = ch;
        run_char = ch;
    }

    /* Output final run */
    if (run_len > 0) {
        if (run_char < 0) run_char = order[0];
        for (int j = 0; j < run_len; j++) out[out_pos++] = (uint8_t)run_char;
    }
}

/* ── RLE inverse (bzip2's final RLE stage) ──────────────────────── */
static int rle_inverse(const uint8_t *src, int src_size,
                       uint8_t *dst, size_t dst_cap) {
    int out = 0;
    int i = 0;
    while (i < src_size) {
        if (out >= (int)dst_cap) return -1;
        dst[out++] = src[i];
        if (src[i] == 0) {
            /* RLE: next byte is count-1 */
            i++;
            if (i >= src_size) break;
            int count = src[i];
            for (int j = 0; j < count; j++) {
                if (out >= (int)dst_cap) return -1;
                dst[out++] = 0;
            }
        }
        i++;
    }
    return out;
}

/* ── Main decompressor ──────────────────────────────────────────── */
int bzip2_decompress(const uint8_t *src, size_t src_size,
                     uint8_t *dst, size_t dst_capacity,
                     size_t *out_size) {
    if (src_size < 4) return -1;

    /* Stream header: "BZ" + version + block_size */
    if (src[0] != 'B' || src[1] != 'Z') return -1;
    if (src[2] != 'h') return -1; /* version 'h' */
    int block_size_100k = src[3] - '0';
    if (block_size_100k < 1 || block_size_100k > 9) return -1;

    BitReaderBE br;
    brbe_init(&br, src + 4, src_size - 4);

    size_t out_pos = 0;

    while (1) {
        /* Check for end-of-stream magic or block magic */
        uint32_t magic = brbe_read(&br, 24);
        if (magic == 0x314159) {
            /* Block magic (lower 24 bits of 0x314159265359) */
            /* Read upper 24 bits */
            uint32_t magic2 = brbe_read(&br, 24);
            if (magic2 != 0x265359) return -1;

            /* Block CRC (32 bits, ignored) */
            brbe_read(&br, 32);

            /* Randomised flag */
            int randomised = brbe_get_bit(&br);

            /* origPtr (24 bits) */
            int orig_ptr = (int)brbe_read(&br, 24);

            /* Mapping table */
            int n_in_use = 0;
            int in_use[256];
            kmemset(in_use, 0, sizeof(in_use));
            for (int i = 0; i < 256; i += 8) {
                int byte = brbe_get_bit(&br);
                if (byte) {
                    for (int j = 0; j < 8; j++) {
                        if (brbe_get_bit(&br)) {
                            in_use[i + j] = 1;
                            n_in_use++;
                        }
                    }
                }
            }

            /* Build alphabet */
            uint8_t alpha_map[256];
            int alpha_size = n_in_use + 2; /* +2 for RUNA/RUNB */
            int ai = 0;
            for (int i = 0; i < 256; i++) {
                if (in_use[i]) alpha_map[ai++] = (uint8_t)i;
            }

            /* Number of Huffman tables (2-6) */
            int n_tables = (int)brbe_read(&br, 3);
            if (n_tables < 2 || n_tables > 6) return -1;

            /* Number of selectors */
            int n_selectors = (int)brbe_read(&br, 15);
            if (n_selectors < 1) return -1;

            /* Selectors (MTF-coded) */
            int *selectors = (int *)kmalloc(sizeof(int) * n_selectors);
            if (!selectors) return -1;
            for (int i = 0; i < n_selectors; i++) {
                int j = 0;
                while (brbe_get_bit(&br) && j < n_tables) j++;
                if (j >= n_tables) { kfree(selectors); return -1; }
                selectors[i] = j;
            }

            /* Huffman tables */
            BzHuffman tables[6];
            for (int t = 0; t < n_tables; t++) {
                int lengths[BZ_MAX_ALPHA];
                int curr_len = (int)brbe_read(&br, 5);
                for (int a = 0; a < alpha_size; a++) {
                    while (1) {
                        if (curr_len < 1 || curr_len > 20) {
                            kfree(selectors); return -1;
                        }
                        if (!brbe_get_bit(&br)) break;
                        if (brbe_get_bit(&br)) curr_len--;
                        else curr_len++;
                    }
                    lengths[a] = curr_len;
                }
                tables[t].nperm = 0;
                bz_huffman_build(&tables[t], lengths, alpha_size);
            }

            /* Decode block data */
            int max_block = block_size_100k * 100000;
            uint8_t *mtf_buf = (uint8_t *)kmalloc(max_block + 20);
            if (!mtf_buf) { kfree(selectors); return -1; }

            int mtf_pos = 0;
            int sel_idx = 0;
            int eob = alpha_size - 1; /* End of block symbol */

            while (mtf_pos < max_block) {
                if (sel_idx >= n_selectors) {
                    kfree(mtf_buf); kfree(selectors); return -1;
                }
                int t = selectors[sel_idx++];
                int sym = bz_huffman_decode(&br, &tables[t]);
                if (sym < 0 || sym >= alpha_size) {
                    kfree(mtf_buf); kfree(selectors); return -1;
                }
                if (sym == eob) break;
                mtf_buf[mtf_pos++] = (uint8_t)sym;
            }

            kfree(selectors);

            /* MTF inverse → BWT data */
            uint8_t *bwt_buf = (uint8_t *)kmalloc(max_block + 20);
            if (!bwt_buf) { kfree(mtf_buf); return -1; }

            /* A malformed block can encode an empty alphabet. mtf_inverse would
             * then leave `order` uninitialized and read order[0], so reject it. */
            if (n_in_use == 0) { kfree(bwt_buf); kfree(mtf_buf); return -1; }
            mtf_inverse(mtf_buf, mtf_pos, bwt_buf, n_in_use, alpha_map);
            kfree(mtf_buf);

            /* BWT inverse */
            uint8_t *raw_buf = (uint8_t *)kmalloc(max_block + 20);
            if (!raw_buf) { kfree(bwt_buf); return -1; }

            bwt_inverse(bwt_buf, mtf_pos, orig_ptr, raw_buf);
            kfree(bwt_buf);

            /* RLE inverse */
            uint8_t *rle_buf = (uint8_t *)kmalloc(max_block * 2 + 20);
            if (!rle_buf) { kfree(raw_buf); return -1; }

            int rle_size = rle_inverse(raw_buf, mtf_pos, rle_buf, dst_capacity - out_pos);
            kfree(raw_buf);

            if (rle_size < 0) { kfree(rle_buf); return -1; }

            if (out_pos + rle_size > dst_capacity) {
                kfree(rle_buf); return -1;
            }
            kmemcpy(dst + out_pos, rle_buf, rle_size);
            out_pos += rle_size;
            kfree(rle_buf);

        } else if (magic == 0x177245) {
            /* End-of-stream magic (lower 24 bits of 0x177245385090) */
            uint32_t magic2 = brbe_read(&br, 24);
            if (magic2 != 0x385090) return -1;
            /* CRC (32 bits, ignored) */
            brbe_read(&br, 32);
            break;
        } else {
            return -1;
        }
    }

    *out_size = out_pos;
    return 0;
}
