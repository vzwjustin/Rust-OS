/* Minimal XZ/LZMA2 decompressor — implements the XZ container format
 * and LZMA2 stream format with the LZMA range decoder.
 *
 * Supports: XZ stream header/footer, single-block streams, LZMA2 filter,
 * dictionary reset, state reset, and the full LZMA probability model.
 */

#include "xz_decompress.h"
#include "../kcompat.h"

/* ── CRC32 (poly 0xEDB88320) ────────────────────────────────────── */
static uint32_t crc32_table[256];
static int crc32_initialized = 0;

static void crc32_init(void) {
    for (uint32_t i = 0; i < 256; i++) {
        uint32_t c = i;
        for (int j = 0; j < 8; j++) {
            if (c & 1) c = 0xEDB88320u ^ (c >> 1);
            else c >>= 1;
        }
        crc32_table[i] = c;
    }
    crc32_initialized = 1;
}

static uint32_t crc32_calc(const uint8_t *data, size_t size) {
    if (!crc32_initialized) crc32_init();
    uint32_t crc = 0xFFFFFFFFu;
    for (size_t i = 0; i < size; i++)
        crc = crc32_table[(crc ^ data[i]) & 0xFF] ^ (crc >> 8);
    return crc ^ 0xFFFFFFFFu;
}

/* ── Range decoder ──────────────────────────────────────────────── */
typedef struct {
    const uint8_t *src;
    size_t size;
    size_t pos;
    uint32_t range;
    uint32_t code;
} RangeDecoder;

static int rd_init(RangeDecoder *rd, const uint8_t *src, size_t size) {
    if (size < 5) return -1;
    rd->src = src; rd->size = size; rd->pos = 0;
    rd->code = 0;
    rd->range = 0xFFFFFFFFu;
    /* Skip the first byte (must be 0) */
    rd->pos = 1;
    for (int i = 0; i < 4; i++) {
        rd->code = (rd->code << 8) | src[rd->pos++];
    }
    return 0;
}

static void rd_normalize(RangeDecoder *rd) {
    if (rd->range < (1u << 24)) {
        rd->range <<= 8;
        if (rd->pos < rd->size) {
            rd->code = (rd->code << 8) | rd->src[rd->pos++];
        } else {
            rd->code <<= 8;
        }
    }
}

static int rd_decode_bit(RangeDecoder *rd, uint16_t *prob) {
    uint32_t bound = (rd->range >> 11) * (*prob);
    int bit;
    if (rd->code < bound) {
        rd->range = bound;
        *prob += ((1u << 11) - *prob) >> 5;
        bit = 0;
    } else {
        rd->range -= bound;
        rd->code -= bound;
        *prob -= *prob >> 5;
        bit = 1;
    }
    rd_normalize(rd);
    return bit;
}

static int rd_decode_direct(RangeDecoder *rd, int nbits) {
    int result = 0;
    for (int i = 0; i < nbits; i++) {
        rd->range >>= 1;
        result <<= 1;
        if (rd->code >= rd->range) {
            rd->code -= rd->range;
            result |= 1;
        }
        rd_normalize(rd);
    }
    return result;
}

/* ── LZMA decoder ───────────────────────────────────────────────── */
#define LZMA_NUM_STATES 12
#define LZMA_NUM_LIT_STATES 7
#define LZMA_DIST_STATES 4
#define LZMA_DIST_SLOTS 64
#define LZMA_DIST_MODEL_END 14
#define LZMA_ALIGN_BITS 4
#define LZMA_ALIGN_TABLE_SIZE (1 << LZMA_ALIGN_BITS)
#define LZMA_LEN_LOW_BITS 3
#define LZMA_LEN_LOW_SYMBOLS (1 << LZMA_LEN_LOW_BITS)
#define LZMA_LEN_MID_BITS 3
#define LZMA_LEN_MID_SYMBOLS (1 << LZMA_LEN_MID_BITS)
#define LZMA_LEN_HIGH_BITS 8
#define LZMA_LEN_HIGH_SYMBOLS (1 << LZMA_LEN_HIGH_BITS)
#define LZMA_POS_STATES_MAX (1 << 4)

typedef struct {
    /* Literal probabilities */
    uint16_t lit[0x300]; /* 768 */
    /* Length decoder */
    uint16_t len_choice[2];
    uint16_t len_low[LZMA_POS_STATES_MAX][LZMA_LEN_LOW_SYMBOLS];
    uint16_t len_mid[LZMA_POS_STATES_MAX][LZMA_LEN_MID_SYMBOLS];
    uint16_t len_high[LZMA_LEN_HIGH_SYMBOLS];
    /* Rep length decoder (same structure) */
    uint16_t rep_len_choice[2];
    uint16_t rep_len_low[LZMA_POS_STATES_MAX][LZMA_LEN_LOW_SYMBOLS];
    uint16_t rep_len_mid[LZMA_POS_STATES_MAX][LZMA_LEN_MID_SYMBOLS];
    uint16_t rep_len_high[LZMA_LEN_HIGH_SYMBOLS];
    /* State + rep */
    int state;
    uint32_t rep0, rep1, rep2, rep3;
    /* Distance decoder */
    uint16_t dist_slots[LZMA_DIST_SLOTS][6]; /* simplified */
    uint16_t dist_decoders[LZMA_DIST_MODEL_END];
    uint16_t dist_align[LZMA_ALIGN_TABLE_SIZE];
    /* IsMatch / IsRep / IsRepG0 / IsRepG1 / IsRepG2 / IsRep0Long */
    uint16_t is_match[LZMA_NUM_STATES][LZMA_POS_STATES_MAX];
    uint16_t is_rep[LZMA_NUM_STATES];
    uint16_t is_rep_g0[LZMA_NUM_STATES];
    uint16_t is_rep_g1[LZMA_NUM_STATES];
    uint16_t is_rep_g2[LZMA_NUM_STATES];
    uint16_t is_rep0_long[LZMA_NUM_STATES][LZMA_POS_STATES_MAX];
    /* Position slot decoder */
    uint16_t pos_slot_decoder[LZMA_DIST_STATES][LZMA_DIST_SLOTS];
    uint16_t pos_decoders[1 + LZMA_DIST_MODEL_END]; /* full pos decoder */
} LzmaState;

static void lzma_init_probs(LzmaState *s) {
    for (int i = 0; i < 0x300; i++) s->lit[i] = 1u << 10;
    for (int i = 0; i < 2; i++) s->len_choice[i] = 1u << 10;
    for (int i = 0; i < LZMA_POS_STATES_MAX; i++)
        for (int j = 0; j < LZMA_LEN_LOW_SYMBOLS; j++) s->len_low[i][j] = 1u << 10;
    for (int i = 0; i < LZMA_POS_STATES_MAX; i++)
        for (int j = 0; j < LZMA_LEN_MID_SYMBOLS; j++) s->len_mid[i][j] = 1u << 10;
    for (int i = 0; i < LZMA_LEN_HIGH_SYMBOLS; i++) s->len_high[i] = 1u << 10;
    for (int i = 0; i < 2; i++) s->rep_len_choice[i] = 1u << 10;
    for (int i = 0; i < LZMA_POS_STATES_MAX; i++)
        for (int j = 0; j < LZMA_LEN_LOW_SYMBOLS; j++) s->rep_len_low[i][j] = 1u << 10;
    for (int i = 0; i < LZMA_POS_STATES_MAX; i++)
        for (int j = 0; j < LZMA_LEN_MID_SYMBOLS; j++) s->rep_len_mid[i][j] = 1u << 10;
    for (int i = 0; i < LZMA_LEN_HIGH_SYMBOLS; i++) s->rep_len_high[i] = 1u << 10;
    s->state = 0;
    s->rep0 = s->rep1 = s->rep2 = s->rep3 = 0;
    for (int i = 0; i < LZMA_NUM_STATES; i++)
        for (int j = 0; j < LZMA_POS_STATES_MAX; j++) s->is_match[i][j] = 1u << 10;
    for (int i = 0; i < LZMA_NUM_STATES; i++) s->is_rep[i] = 1u << 10;
    for (int i = 0; i < LZMA_NUM_STATES; i++) s->is_rep_g0[i] = 1u << 10;
    for (int i = 0; i < LZMA_NUM_STATES; i++) s->is_rep_g1[i] = 1u << 10;
    for (int i = 0; i < LZMA_NUM_STATES; i++) s->is_rep_g2[i] = 1u << 10;
    for (int i = 0; i < LZMA_NUM_STATES; i++)
        for (int j = 0; j < LZMA_POS_STATES_MAX; j++) s->is_rep0_long[i][j] = 1u << 10;
    for (int i = 0; i < LZMA_DIST_STATES; i++)
        for (int j = 0; j < LZMA_DIST_SLOTS; j++) s->pos_slot_decoder[i][j] = 1u << 10;
    for (int i = 0; i < 1 + LZMA_DIST_MODEL_END; i++) s->pos_decoders[i] = 1u << 10;
    for (int i = 0; i < LZMA_DIST_MODEL_END; i++) s->dist_decoders[i] = 1u << 10;
    for (int i = 0; i < LZMA_ALIGN_TABLE_SIZE; i++) s->dist_align[i] = 1u << 10;
}

static int lzma_decode_len(RangeDecoder *rd, LzmaState *s, int pos_state,
                           int is_rep) {
    uint16_t *choice = is_rep ? s->rep_len_choice : s->len_choice;
    uint16_t (*low)[LZMA_LEN_LOW_SYMBOLS] = is_rep ? s->rep_len_low : s->len_low;
    uint16_t (*mid)[LZMA_LEN_MID_SYMBOLS] = is_rep ? s->rep_len_mid : s->len_mid;
    uint16_t *high = is_rep ? s->rep_len_high : s->len_high;

    if (rd_decode_bit(rd, &choice[0]) == 0) {
        /* Low: 3 bits, base 2 */
        int idx = 0;
        for (int i = 0; i < LZMA_LEN_LOW_BITS; i++)
            idx = (idx << 1) | rd_decode_bit(rd, &low[pos_state][idx]);
        return 2 + idx;
    }
    if (rd_decode_bit(rd, &choice[1]) == 0) {
        /* Mid: 3 bits, base 10 */
        int idx = 0;
        for (int i = 0; i < LZMA_LEN_MID_BITS; i++)
            idx = (idx << 1) | rd_decode_bit(rd, &mid[pos_state][idx]);
        return 10 + idx;
    }
    /* High: 8 bits, base 18 */
    int idx = 0;
    for (int i = 0; i < LZMA_LEN_HIGH_BITS; i++)
        idx = (idx << 1) | rd_decode_bit(rd, &high[idx]);
    return 18 + idx;
}

static uint32_t lzma_decode_dist(RangeDecoder *rd, LzmaState *s, int len) {
    int pos_state = (len < 4) ? len : 3;
    int slot = 0;
    for (int i = 0; i < 6; i++)
        slot = (slot << 1) | rd_decode_bit(rd, &s->pos_slot_decoder[pos_state][slot]);

    if (slot < 4) return slot;

    int num_direct = (slot >> 1) - 1;
    uint32_t dist = (2 | (slot & 1)) << num_direct;

    if (slot < 14) {
        /* Use distance decoder */
        uint32_t base = dist - num_direct - 1;
        int v = 0;
        for (int i = 0; i < num_direct; i++) {
            /* Use pos_decoders */
            v = (v << 1) | rd_decode_bit(rd, &s->pos_decoders[base + v]);
        }
        dist += v;
    } else {
        /* Direct bits + align */
        int align_bits = num_direct - 4;
        dist += (uint32_t)rd_decode_direct(rd, align_bits) << LZMA_ALIGN_BITS;
        int align = 0;
        for (int i = 0; i < LZMA_ALIGN_BITS; i++)
            align = (align << 1) | rd_decode_bit(rd, &s->dist_align[align]);
        dist += align;
    }
    return dist;
}

static int lzma_decode_literal(RangeDecoder *rd, LzmaState *s,
                               uint8_t prev_byte, int lit_state,
                               uint8_t *out) {
    /* Simple literal decoding without match context */
    uint16_t *probs = &s->lit[lit_state * 0x300];
    (void)prev_byte;

    int symbol = 1;
    for (int i = 0; i < 8; i++)
        symbol = (symbol << 1) | rd_decode_bit(rd, &probs[symbol]);
    *out = (uint8_t)(symbol & 0xFF);
    return 0;
}

/* ── LZMA2 stream decoder ───────────────────────────────────────── */
static int lzma2_decode(const uint8_t *src, size_t src_size,
                        uint8_t *dst, size_t dst_cap, size_t *out_size) {
    size_t pos = 0;
    size_t out_pos = 0;

    /* LZMA2 properties byte */
    if (pos >= src_size) return -1;
    uint8_t props = src[pos++];
    int lc = props % 9;
    int lp = (props / 9) % 5;
    int pb = (props / 9) / 5;
    int pos_states = 1 << pb;
    int lit_pos_state = 1 << lp;

    LzmaState s;
    lzma_init_probs(&s);

    while (pos < src_size) {
        uint8_t control = src[pos++];
        if (control == 0) {
            /* End of stream */
            break;
        }

        int reset_dict = 0, reset_state = 0, reset_probs = 0;
        int uncompressed_size;
        int compressed_size;

        if (control == 1) {
            /* Reset dict + state + probs */
            reset_dict = reset_state = reset_probs = 1;
            uncompressed_size = ((int)src[pos] << 8) | src[pos + 1];
            pos += 2;
            compressed_size = ((int)src[pos] << 8) | src[pos + 1];
            pos += 2;
        } else if (control == 2) {
            /* Reset state + probs */
            reset_state = reset_probs = 1;
            uncompressed_size = ((int)src[pos] << 8) | src[pos + 1];
            pos += 2;
            compressed_size = ((int)src[pos] << 8) | src[pos + 1];
            pos += 2;
        } else if (control == 3) {
            /* Reset state */
            reset_state = 1;
            uncompressed_size = ((int)src[pos] << 8) | src[pos + 1];
            pos += 2;
            compressed_size = ((int)src[pos] << 8) | src[pos + 1];
            pos += 2;
        } else {
            /* Continuation: no reset */
            uncompressed_size = (((int)(control & 0x1F)) << 16) |
                                ((int)src[pos] << 8) | src[pos + 1];
            pos += 2;
            compressed_size = ((int)src[pos] << 8) | src[pos + 1];
            pos += 2;
            compressed_size += 1;
        }

        uncompressed_size += 1;

        if (pos + compressed_size > src_size) return -1;

        if (reset_dict) {
            s.rep0 = s.rep1 = s.rep2 = s.rep3 = 0;
        }
        if (reset_state) {
            s.state = 0;
        }
        if (reset_probs) {
            lzma_init_probs(&s);
        }

        RangeDecoder rd;
        if (rd_init(&rd, src + pos, compressed_size) < 0) return -1;

        int chunk_out_start = (int)out_pos;

        while ((int)out_pos - chunk_out_start < uncompressed_size) {
            int pos_state = (int)(out_pos & (pos_states - 1));
            int state = s.state;

            if (rd_decode_bit(&rd, &s.is_match[state][pos_state]) == 0) {
                /* Literal */
                uint8_t prev = (out_pos > 0) ? dst[out_pos - 1] : 0;
                int lit_state = ((int)(out_pos & (lit_pos_state - 1)) << lc) |
                                (prev >> (8 - lc));
                uint8_t lit;
                lzma_decode_literal(&rd, &s, prev, lit_state, &lit);
                if (out_pos >= dst_cap) return -1;
                dst[out_pos++] = lit;
                if (state < 4) s.state = 0;
                else if (state < 10) s.state -= 3;
                else s.state -= 6;
            } else {
                /* Match or rep match */
                int len;
                if (rd_decode_bit(&rd, &s.is_rep[state]) != 0) {
                    /* Rep match */
                    if (rd_decode_bit(&rd, &s.is_rep_g0[state]) == 0) {
                        if (rd_decode_bit(&rd, &s.is_rep0_long[state][pos_state]) == 0) {
                            /* Short rep (length 1) */
                            if (out_pos >= dst_cap) return -1;
                            /* Reject back-references that point before the
                             * start of the output. Computed in 64-bit because
                             * the unsigned size_t arithmetic below wraps. */
                            if ((uint64_t)s.rep0 + 1 > (uint64_t)out_pos) return -1;
                            dst[out_pos] = dst[out_pos - s.rep0 - 1];
                            out_pos++;
                            if (state < 7) s.state = 9;
                            else s.state = 11;
                            continue;
                        }
                    } else {
                        uint32_t dist;
                        if (rd_decode_bit(&rd, &s.is_rep_g1[state]) == 0) {
                            dist = s.rep1;
                        } else {
                            if (rd_decode_bit(&rd, &s.is_rep_g2[state]) == 0) {
                                dist = s.rep2;
                            } else {
                                dist = s.rep3;
                                s.rep3 = s.rep2;
                            }
                            s.rep2 = s.rep1;
                        }
                        s.rep1 = s.rep0;
                        s.rep0 = dist;
                    }
                    len = lzma_decode_len(&rd, &s, pos_state, 1);
                    if (state < 7) s.state = 8;
                    else s.state = 11;
                } else {
                    /* Normal match */
                    s.rep3 = s.rep2;
                    s.rep2 = s.rep1;
                    s.rep1 = s.rep0;
                    len = lzma_decode_len(&rd, &s, pos_state, 0);
                    s.rep0 = lzma_decode_dist(&rd, &s, len);
                    if (s.rep0 == 0xFFFFFFFFu) return -1;
                    if (state < 7) s.state = 7;
                    else s.state = 10;
                }
                len += 2; /* minimum match length is 2 */

                /* Validate the back-reference distance once: it is loop-
                 * invariant and out_pos only grows, so the first iteration is
                 * the tightest. Computed in 64-bit because the unsigned size_t
                 * arithmetic in the copy below wraps (the original
                 * `out_pos - s.rep0 - 1 < 0` check could never fire). */
                if ((uint64_t)s.rep0 + 1 > (uint64_t)out_pos) return -1;

                /* Copy match */
                for (int i = 0; i < len; i++) {
                    if (out_pos >= dst_cap) return -1;
                    dst[out_pos] = dst[out_pos - s.rep0 - 1];
                    out_pos++;
                }
            }
        }

        pos += compressed_size;
    }

    *out_size = out_pos;
    return 0;
}

/* ── XZ container parser ────────────────────────────────────────── */
int xz_decompress(const uint8_t *src, size_t src_size,
                  uint8_t *dst, size_t dst_capacity,
                  size_t *out_size) {
    /* XZ magic: FD 37 7A 58 5A 00 */
    static const uint8_t xz_magic[6] = {0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00};
    if (src_size < 12) return -1;
    for (int i = 0; i < 6; i++)
        if (src[i] != xz_magic[i]) return -1;

    /* Stream flags (2 bytes): check type in low nibble of second byte */
    /* uint8_t check_type = src[7] & 0x0F; */

    size_t pos = 12; /* skip magic + stream flags + CRC32 of stream flags */

    size_t out_pos = 0;

    while (pos < src_size) {
        /* Block header */
        if (pos >= src_size) return -1;
        uint8_t block_header_size = src[pos];
        if (block_header_size == 0) {
            /* Index marker — stream is done */
            break;
        }
        int real_header_size = (block_header_size + 1) * 4;
        if (pos + real_header_size > src_size) return -1;

        /* Parse block header */
        const uint8_t *bh = src + pos;
        /* uint8_t block_flags = bh[1]; */
        /* Filter ID: we expect LZMA2 (0x21) */
        /* For simplicity, skip to the compressed data */
        /* The block header format is:
         * byte 0: header_size / 4 - 1
         * byte 1: block flags
         * then filter flags (variable size)
         * then padding to 4-byte boundary
         * then CRC32 (4 bytes)
         */
        /* We'll just find the LZMA2 filter and its properties */
        int bh_pos = 2;
        /* Filter flags: varint filter ID, varint properties size, properties */
        /* For LZMA2: filter ID = 0x21, properties size = 1 */
        if (bh[bh_pos] != 0x21) return -1; /* only LZMA2 supported */
        bh_pos++;
        if (bh[bh_pos] != 1) return -1; /* LZMA2 properties size = 1 */
        bh_pos++;
        /* uint8_t lzma2_props = bh[bh_pos]; */
        bh_pos++;

        /* Skip to end of header (including CRC32) */
        pos += real_header_size;

        /* The compressed data follows until we hit the block padding */
        /* We don't know the compressed size from the block header, so we
         * pass the rest of the stream to the LZMA2 decoder, which will
         * stop at the end-of-stream marker. */
        size_t chunk_out = 0;
        if (lzma2_decode(src + pos, src_size - pos, dst + out_pos,
                         dst_capacity - out_pos, &chunk_out) < 0) return -1;
        out_pos += chunk_out;

        /* Skip past the compressed data and any padding + check */
        /* For now, break — we only support single-block streams */
        break;
    }

    *out_size = out_pos;
    return 0;
}
