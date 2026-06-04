build:
	cargo build --release

standalone: build
	target/release/voxbox-standalone --device 'Scarlett 18i8 USB' --input-channel 1

standalone-with-ir: build
	target/release/voxbox-standalone --device 'Scarlett 18i8 USB' --input-channel 1 --ir

devices: build
	target/release/voxbox-standalone --list-devices
