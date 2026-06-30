# ACPICA vendor integration

RustOS does not vendor ACPICA in this tree yet. The build script enables the
`rustos_acpica` cfg only when a real ACPICA source checkout is present under:

```text
c_libs/acpica/source/include/acpi.h
c_libs/acpica/source/components/
```

The wrapper contract lives in `rustos_acpica_shim.c` and
`include/rustos_acpica_shim.h`. It is intentionally not compiled without the
real upstream ACPICA sources; missing ACPICA must stay visible to Rust as an
unavailable capability rather than a successful fake AML interpreter.

When vendoring ACPICA, provide the required OS services implementation for the
RustOS kernel environment alongside this directory and wire it into
`build.rs`. Do not replace this with dummy `AcpiOs*` functions.
