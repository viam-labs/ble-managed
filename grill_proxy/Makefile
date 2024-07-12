default: run

run:
	export RUST_LOG=debug
	cargo run

setup:
	# D-Bus development headers are required for building with the bluer crate.
	sudo apt install libdbus-1-dev

build:
	cargo build
