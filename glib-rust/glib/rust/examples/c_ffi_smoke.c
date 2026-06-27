/*
 * Minimal C smoke test for glib-native static library (Phase 13).
 *
 * SPDX-License-Identifier: LGPL-2.1-or-later
 */

#include <stdio.h>
#include <string.h>

#include "glib_native.h"

int main(void) {
    g_type_init();

    gpointer mem = g_malloc(32);
    if (mem == NULL) {
        fprintf(stderr, "g_malloc failed\n");
        return 1;
    }
    memset(mem, 0xAB, 32);
    g_free(mem);

    printf("c_ffi_smoke: g_type_init, g_malloc, g_free OK\n");
    return 0;
}
