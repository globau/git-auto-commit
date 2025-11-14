rs-files := $(wildcard src/*.rs)

.PHONY: build
build: target/release/git-auto-commit

target/release/git-auto-commit: $(rs-files) Cargo.toml Makefile
	RUSTFLAGS="--remap-path-prefix=$$HOME=~" cargo build --release

.PHONY: format
format:
	cargo fmt --verbose

.PHONY: test
test:
	cargo fmt -- --check
	cargo clippy --all-targets --all-features -- -D warnings -W clippy::pedantic
	cargo test
