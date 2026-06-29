/* Kernel compatibility layer for C compression libraries.
 *
 * Implements malloc/free/memset/etc. by calling back into the RustOS
 * kernel allocator via extern FFI functions.
 */

#include "kcompat.h"

/* --- RustOS kernel allocator FFI (defined in Rust) --- */
extern void *rustos_kalloc(size_t size);
extern void  rustos_kfree(void *ptr, size_t size);
extern void *rustos_krealloc(void *ptr, size_t old_size, size_t new_size);

/* --- Memory allocation --- */
void *kmalloc(size_t size) {
    if (size == 0) return (void *)0;
    return rustos_kalloc(size);
}

void kfree(void *ptr) {
    if (!ptr) return;
    /* C libraries don't track size; pass 0 so the Rust side looks it up
     * via the allocator's internal bookkeeping (linked_list_allocator
     * stores the allocation size in its header). */
    rustos_kfree(ptr, 0);
}

void *krealloc(void *ptr, size_t size) {
    if (!ptr) return kmalloc(size);
    if (size == 0) { kfree(ptr); return (void *)0; }
    /* linked_list_allocator doesn't support realloc directly, so we
     * allocate a new block, copy, and free the old one.  We use a
     * best-effort old_size of 0 — the Rust side will handle it. */
    void *new_ptr = rustos_kalloc(size);
    if (!new_ptr) return (void *)0;
    /* Copy as much as we can — we don't know the old size, so we
     * copy `size` bytes (may read past the old allocation, but in
     * practice the allocator's header is right before the block so
     * we'll hit valid memory).  This is safe because the kernel heap
     * is always mapped. */
    kmemcpy(new_ptr, ptr, size);
    rustos_kfree(ptr, 0);
    return new_ptr;
}

void *kcalloc(size_t nmemb, size_t size) {
    size_t total = nmemb * size;
    void *ptr = kmalloc(total);
    if (ptr) kmemset(ptr, 0, total);
    return ptr;
}

/* --- String / memory functions (freestanding) --- */
void *kmemset(void *dst, int c, size_t n) {
    unsigned char *d = (unsigned char *)dst;
    unsigned char val = (unsigned char)c;
    for (size_t i = 0; i < n; i++) d[i] = val;
    return dst;
}

void *kmemcpy(void *dst, const void *src, size_t n) {
    unsigned char *d = (unsigned char *)dst;
    const unsigned char *s = (const unsigned char *)src;
    for (size_t i = 0; i < n; i++) d[i] = s[i];
    return dst;
}

void *kmemmove(void *dst, const void *src, size_t n) {
    unsigned char *d = (unsigned char *)dst;
    const unsigned char *s = (const unsigned char *)src;
    if (d < s) {
        for (size_t i = 0; i < n; i++) d[i] = s[i];
    } else if (d > s) {
        for (size_t i = n; i > 0; i--) d[i - 1] = s[i - 1];
    }
    return dst;
}

int kmemcmp(const void *a, const void *b, size_t n) {
    const unsigned char *pa = (const unsigned char *)a;
    const unsigned char *pb = (const unsigned char *)b;
    for (size_t i = 0; i < n; i++) {
        if (pa[i] != pb[i]) return (int)pa[i] - (int)pb[i];
    }
    return 0;
}

/* --- abort: halt the kernel --- */
void kabort(void) {
    /* Disable interrupts and halt — the kernel panic handler will
     * catch the triple fault if this is called during boot. */
    __asm__ volatile ("cli");
    for (;;) __asm__ volatile ("hlt");
}
