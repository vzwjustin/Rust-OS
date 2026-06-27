# glib-native workspace (host std tests; kernel uses repo-root build scripts)

HOST_TARGET ?= $(shell rustc -vV | sed -n 's/^host: //p')
TARGET_DIR := $(shell cargo metadata --format-version=1 --no-deps 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['target_directory'])" 2>/dev/null || echo $(CURDIR)/target)
STATICLIB := $(TARGET_DIR)/$(HOST_TARGET)/release/libglib_native.a
HEADER := $(CURDIR)/glib-native/include/glib_native.h

.PHONY: help test check staticlib header c-ffi-smoke

help:
	@echo "glib-native workspace"
	@echo "  make test        - run host unit tests (glib-native)"
	@echo "  make check       - cargo check glib-native on host"
	@echo "  make staticlib   - build release static library (libglib_native.a)"
	@echo "  make header      - regenerate include/glib_native.h via build.rs/cbindgen"
	@echo "  make c-ffi-smoke - link and run examples/c_ffi_smoke.c against staticlib"

test:
	@./scripts/test.sh

check:
	@./scripts/check.sh

staticlib:
	@./scripts/build-static.sh

header:
	@HOST_TARGET="$(HOST_TARGET)" cargo build -p glib-native --target "$(HOST_TARGET)" --features c-abi 2>/dev/null || true
	@test -f "$(HEADER)" || (echo "error: $(HEADER) not generated" && exit 1)
	@echo "[glib-native] header: $(HEADER)"

c-ffi-smoke: staticlib
	@./scripts/c-ffi-smoke.sh
