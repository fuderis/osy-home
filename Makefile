SHELL := /bin/bash
.SHELLFLAGS := -euo pipefail -c

INSTALL_DIR ?= /usr/local/bin

.PHONY: all build install clean

all: build install

# 1. Verification of requirements
check:
	@printf "\n\033[1mChecking requirements\033[0m\n"
	@command -v cargo >/dev/null 2>&1 || { echo "  ✗ cargo is not installed" >&2; exit 1; }
	@command -v jq >/dev/null 2>&1    || { echo "  ✗ jq is not installed" >&2; exit 1; }
	@[[ -f Cargo.toml ]]             || { echo "  ✗ Cargo.toml not found" >&2; exit 1; }
	@printf "  \033[32m✓\033[0m Environment ready\n"

# 2. Building with Cargo
build: check
	@printf "\n\033[1mBuilding binaries\033[0m\n"
	cargo build --release
	@printf "  \033[32m✓\033[0m Compilation completed\n"

# 3. Installing binaries in the system
install: build
	@printf "\n\033[1mInstalling binaries\033[0m\n"
	@if [[ -w "$(INSTALL_DIR)" ]]; then \
		INSTALL_CMD="install"; \
	else \
		echo "  ℹ No write access to $(INSTALL_DIR), using sudo"; \
		INSTALL_CMD="sudo install"; \
	fi; \
	METADATA="$$(cargo metadata --format-version=1 --no-deps)"; \
	BINARIES=$$(jq -r '.packages[].targets[] | select(.kind | any(. == "bin")) | .name' <<< "$$METADATA"); \
	for bin in $$BINARIES; do \
		src="target/release/$$bin"; \
		if [[ -f "$$src" ]]; then \
			$$INSTALL_CMD -m 755 "$$src" "$(INSTALL_DIR)/$$bin"; \
			printf "  \033[32m✓\033[0m Installed $$bin -> $(INSTALL_DIR)/$$bin\n"; \
		else \
			echo "  ✗ Binary missing: $$src" >&2; \
			exit 1; \
		fi; \
	done
	@printf "\n───────────────────────────────────────────────────────\n"
	@printf "\033[1;32mCompleted\033[0m Installation finished\n"
