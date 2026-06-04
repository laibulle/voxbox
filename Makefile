build:
	cargo build --release

standalone: build
	  target/release/voxbox-standalone --device 'Scarlett 18i8 USB' --input-channel 1

devices: build
	target/release/voxbox-standalone --list-devices
