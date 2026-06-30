#ifndef RUSTOS_ACPICA_SHIM_H
#define RUSTOS_ACPICA_SHIM_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

int rustos_acpica_available(void);
uint32_t rustos_acpica_initialize(uint64_t rsdp_physical);
uint32_t rustos_acpica_evaluate_integer(const char *path, const char *method, uint64_t *out_value);

#ifdef __cplusplus
}
#endif

#endif /* RUSTOS_ACPICA_SHIM_H */
