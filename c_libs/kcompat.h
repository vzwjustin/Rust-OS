#ifndef RUSTOS_KCOMPAT_H
#define RUSTOS_KCOMPAT_H

/* Kernel compatibility layer for C compression libraries.
 *
 * Provides minimal C standard library functions (malloc/free/memset/etc.)
 * backed by the RustOS kernel allocator so that libzstd, libbz2, and
 * liblzma can be compiled as freestanding C and linked into the kernel.
 */

#include <stddef.h>
#include <stdint.h>

/* --- Memory allocation (backed by kernel linked_list_allocator) --- */
void *kmalloc(size_t size);
void  kfree(void *ptr);
void *krealloc(void *ptr, size_t size);
void *kcalloc(size_t nmemb, size_t size);

/* Standard aliases used by C libraries */
#define malloc  kmalloc
#define free    kfree
#define realloc krealloc
#define calloc  kcalloc

/* --- String functions (freestanding implementations) --- */
void *kmemset(void *dst, int c, size_t n);
void *kmemcpy(void *dst, const void *src, size_t n);
void *kmemmove(void *dst, const void *src, size_t n);
int   kmemcmp(const void *a, const void *b, size_t n);

#define memset  kmemset
#define memcpy  kmemcpy
#define memmove kmemmove
#define memcmp  kmemcmp

/* --- Misc stubs --- */
void kabort(void) __attribute__((noreturn));
#define abort kabort

/* Disable assert in C libraries */
#define NDEBUG

#endif /* RUSTOS_KCOMPAT_H */
