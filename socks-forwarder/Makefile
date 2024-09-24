default: build

run:
	RUST_LOG=debug cargo run

setup:
	# D-Bus development headers are required for building with the bluer crate.
	sudo apt install libdbus-1-dev
	# Fakeroot is needed to call dpkg commands without sudo.
	sudo apt install fakeroot
	# Rust, cargo and bluez (>= 0.5.60) must also be installed.

build:
	cargo build --release

dpkg:
	# Remove existing binary files.
	rm -rf target
	# Create tarball.
	dpkg-source -b .
	# Build package with no signatures.
	dpkg-buildpackage --no-sign -b
