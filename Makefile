PYTHON ?= python3
DIST_DIR ?= dist

.PHONY: test test-rust test-python test-wasm test-r check-r build build-python build-npm build-r build-matlab package-rust clean

test: test-rust test-python test-wasm

test-rust:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace

test-python:
	PYTHONPATH=bindings/python/src $(PYTHON) -m unittest discover -s bindings/python/tests

test-wasm:
	npm test --prefix bindings/wasm

test-r:
	R CMD check --no-manual bindings/r

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
