PYTHON ?= python3
DIST_DIR ?= dist
NIRS4ALL_METHODS_ROOT ?= $(if $(wildcard nirs4all-methods),$(abspath nirs4all-methods),$(abspath ../nirs4all-methods))
NIRS4ALL_METHODS_R_PATH ?= $(NIRS4ALL_METHODS_ROOT)/bindings/r/n4m
NIRS4ALL_METHODS_LIB_DIR ?= $(NIRS4ALL_METHODS_ROOT)/build/dev-release/cpp/src
NIRS4ALL_METHODS_GENERATED_DIR ?= $(NIRS4ALL_METHODS_ROOT)/build/dev-release/generated
NIRS4ALL_METHODS_MATLAB_PATH ?= $(NIRS4ALL_METHODS_ROOT)/bindings/matlab
R_PARITY_LIB ?= $(abspath .r-parity-lib)

.PHONY: test test-v1-surfaces test-rust test-rust-parity test-python test-python-parity test-wasm test-r test-r-if-available test-r-fixtures test-r-parity test-matlab-parity check-r build build-python build-npm build-r build-matlab package-rust clean

test: test-rust test-python test-wasm

test-v1-surfaces: test-python test-wasm test-r-if-available

test-rust:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

test-rust-parity:
	NIRS4ALL_LITE_REQUIRE_METHODS_PARITY=1 cargo test -p nirs4all rust_binding_execution_matches_full_python_nirs4all_oracle -- --nocapture

test-python:
	PYTHONPATH=bindings/python/src $(PYTHON) -m unittest discover -s bindings/python/tests

test-python-parity:
	PYTHONPATH=bindings/python/src$(if $(NIRS4ALL_METHODS_PYTHONPATH),:$(NIRS4ALL_METHODS_PYTHONPATH)) NIRS4ALL_LITE_REQUIRE_METHODS_PARITY=1 $(PYTHON) -m unittest bindings/python/tests/test_execution_parity.py -v

test-wasm:
	npm ci --prefix bindings/wasm
	npm test --prefix bindings/wasm

test-r:
	R CMD check --no-manual bindings/r

test-r-if-available:
	@if command -v R >/dev/null 2>&1; then \
		$(MAKE) test-r; \
	else \
		printf '%s\n' "Skipping R CMD check: R is not installed"; \
	fi

test-r-fixtures:
	diff -ru tests/parity/fixtures bindings/r/inst/extdata

test-r-parity: test-r-fixtures
	mkdir -p $(R_PARITY_LIB)
	if [ -d "$(NIRS4ALL_METHODS_R_PATH)" ]; then \
		PLS4ALL_LIB_DIR="$(NIRS4ALL_METHODS_LIB_DIR)" \
		PLS4ALL_GENERATED_DIR="$(NIRS4ALL_METHODS_GENERATED_DIR)" \
		LD_LIBRARY_PATH="$(NIRS4ALL_METHODS_LIB_DIR):$${LD_LIBRARY_PATH}" \
		R_LIBS_USER="$(R_PARITY_LIB):$${R_LIBS_USER}" \
		R CMD INSTALL --library="$(R_PARITY_LIB)" --no-multiarch --no-staged-install "$(NIRS4ALL_METHODS_R_PATH)"; \
	fi
	R_LIBS_USER="$(R_PARITY_LIB):$${R_LIBS_USER}" R CMD INSTALL --library="$(R_PARITY_LIB)" bindings/r
	NIRS4ALL_LITE_PARITY_ORACLE=$(abspath tests/parity/expected/portable_python_oracle.json) \
	NIRS4ALL_LITE_PARITY_FIXTURES=$(abspath bindings/r/inst/extdata) \
	NIRS4ALL_LITE_REQUIRE_METHODS_PARITY=1 \
	R_LIBS_USER="$(R_PARITY_LIB):$${R_LIBS_USER}" \
	Rscript bindings/r/tests/parity.R

test-matlab-parity:
	NIRS4ALL_LITE_PARITY_ORACLE=$(abspath tests/parity/expected/portable_python_oracle.json) \
	NIRS4ALL_LITE_PARITY_FIXTURES=$(abspath tests/parity/fixtures) \
	NIRS4ALL_METHODS_MATLAB_PATH=$(NIRS4ALL_METHODS_MATLAB_PATH) \
	NIRS4ALL_LITE_REQUIRE_METHODS_PARITY=1 \
	octave --quiet --eval "addpath('bindings/matlab/tests'); parity"

check-r: build-r
	R CMD check --no-manual $(DIST_DIR)/r/nirs4all_*.tar.gz

build: build-python build-npm build-r build-matlab package-rust

build-python:
	$(PYTHON) -m build bindings/python --outdir $(abspath $(DIST_DIR)/python)

build-npm:
	mkdir -p $(DIST_DIR)/npm
	npm pack ./bindings/wasm --pack-destination $(DIST_DIR)/npm

build-r:
	mkdir -p $(DIST_DIR)/r
	cd $(DIST_DIR)/r && R CMD build ../../bindings/r

build-matlab:
	scripts/build-matlab-package.sh $(DIST_DIR)/matlab

package-rust:
	cargo package -p nirs4all

clean:
	rm -rf $(DIST_DIR)
