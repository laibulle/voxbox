DEVICE ?= Scarlett 18i8 USB
INPUT_CHANNEL ?= 1
OUTPUT_CHANNELS ?= 1,2
SAMPLE_RATE ?= 48000
PERIOD_SIZE ?= 32
VOLUME ?= 5.5
BASS ?= 5.0
TREBLE ?= 6.0
CUT ?= 3.5
INPUT_DB ?= 0
OUTPUT_DB ?= -18

build:
	cargo build --release

standalone: build
	target/release/voxbox-standalone --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		--volume $(VOLUME) --bass $(BASS) --treble $(TREBLE) --cut $(CUT) \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB)

standalone-with-ir: build
	target/release/voxbox-standalone --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		--volume $(VOLUME) --bass $(BASS) --treble $(TREBLE) --cut $(CUT) \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB) --ir

standalone-with-ir-clean: VOLUME=3.2
standalone-with-ir-clean: BASS=6.5
standalone-with-ir-clean: TREBLE=5.2
standalone-with-ir-clean: CUT=4.0
standalone-with-ir-clean: OUTPUT_DB=-14
standalone-with-ir-clean: standalone-with-ir

standalone-with-ir-edge: VOLUME=5.8
standalone-with-ir-edge: BASS=6.0
standalone-with-ir-edge: TREBLE=5.4
standalone-with-ir-edge: CUT=4.3
standalone-with-ir-edge: OUTPUT_DB=-15
standalone-with-ir-edge: standalone-with-ir

standalone-with-ir-crunch: VOLUME=8.0
standalone-with-ir-crunch: BASS=5.6
standalone-with-ir-crunch: TREBLE=5.6
standalone-with-ir-crunch: CUT=4.7
standalone-with-ir-crunch: OUTPUT_DB=-16
standalone-with-ir-crunch: standalone-with-ir

standalone-with-ir-driven: VOLUME=10.0
standalone-with-ir-driven: BASS=5.0
standalone-with-ir-driven: TREBLE=5.8
standalone-with-ir-driven: CUT=5.2
standalone-with-ir-driven: OUTPUT_DB=-17
standalone-with-ir-driven: standalone-with-ir

gui: build
	target/release/voxbox-standalone

devices: build
	target/release/voxbox-standalone --list-devices

compare-ac30:
	cargo test --test ac30_reference -- --ignored --nocapture
