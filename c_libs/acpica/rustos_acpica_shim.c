#include "rustos_acpica_shim.h"

#include "acpi.h"

int rustos_acpica_available(void) {
    return 1;
}

uint32_t rustos_acpica_initialize(uint64_t rsdp_physical) {
    ACPI_STATUS status;

    (void)rsdp_physical;

    status = AcpiInitializeSubsystem();
    if (ACPI_FAILURE(status)) {
        return (uint32_t)status;
    }

    status = AcpiInitializeTables(NULL, 16, FALSE);
    if (ACPI_FAILURE(status)) {
        return (uint32_t)status;
    }

    status = AcpiLoadTables();
    if (ACPI_FAILURE(status)) {
        return (uint32_t)status;
    }

    status = AcpiEnableSubsystem(ACPI_FULL_INITIALIZATION);
    if (ACPI_FAILURE(status)) {
        return (uint32_t)status;
    }

    status = AcpiInitializeObjects(ACPI_FULL_INITIALIZATION);
    return (uint32_t)status;
}

uint32_t rustos_acpica_evaluate_integer(const char *path, const char *method, uint64_t *out_value) {
    ACPI_HANDLE handle = NULL;
    ACPI_OBJECT result;
    ACPI_BUFFER buffer = { sizeof(result), &result };
    ACPI_STATUS status;

    if (path == NULL || method == NULL || out_value == NULL) {
        return (uint32_t)AE_BAD_PARAMETER;
    }

    status = AcpiGetHandle(NULL, (ACPI_STRING)path, &handle);
    if (ACPI_FAILURE(status)) {
        return (uint32_t)status;
    }

    status = AcpiEvaluateObject(handle, (ACPI_STRING)method, NULL, &buffer);
    if (ACPI_FAILURE(status)) {
        return (uint32_t)status;
    }

    if (result.Type != ACPI_TYPE_INTEGER) {
        return (uint32_t)AE_TYPE;
    }

    *out_value = (uint64_t)result.Integer.Value;
    return (uint32_t)AE_OK;
}
