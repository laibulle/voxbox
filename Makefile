DEVICE ?= Scarlett 18i8 USB
INPUT_CHANNEL ?= 1
OUTPUT_CHANNELS ?= 1,2
SAMPLE_RATE ?= 48000
PERIOD_SIZE ?= 16
VOLUME ?= 5.5
BASS ?= 5.0
TREBLE ?= 6.0
CUT ?= 3.5
INPUT_DB ?= 0
OUTPUT_DB ?= 0

build:
	cargo build --release


CLI := target/release/voxbox-cli
DESKTOP :=target/release/voxbox-desktop

standalone: build
	$(CLI) --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		--volume $(VOLUME) --bass $(BASS) --treble $(TREBLE) --cut $(CUT) \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB)


standalone-with-ir: build
	$(CLI) --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		--volume $(VOLUME) --bass $(BASS) --treble $(TREBLE) --cut $(CUT) \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB) --ir

standalone-with-ir-clean: VOLUME=3.2
standalone-with-ir-clean: BASS=6.5
standalone-with-ir-clean: TREBLE=5.2
standalone-with-ir-clean: CUT=4.0
standalone-with-ir-clean: standalone-with-ir

standalone-with-ir-edge: VOLUME=5.8
standalone-with-ir-edge: BASS=6.0
standalone-with-ir-edge: TREBLE=5.4
standalone-with-ir-edge: CUT=4.3
standalone-with-ir-edge: standalone-with-ir

standalone-with-ir-crunch: VOLUME=8.0
standalone-with-ir-crunch: BASS=5.6
standalone-with-ir-crunch: TREBLE=5.6
standalone-with-ir-crunch: CUT=4.7
standalone-with-ir-crunch: standalone-with-ir

standalone-with-ir-driven: VOLUME=10.0
standalone-with-ir-driven: BASS=5.0
standalone-with-ir-driven: TREBLE=5.8
standalone-with-ir-driven: CUT=5.2
standalone-with-ir-driven: standalone-with-ir

standalone-dumble: build
	$(CLI) --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		--preset dumble \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB)

standalone-dumble-ir: build
	$(CLI) --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		--preset dumble --ir \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB)

standalone-dumble-clean: build
	$(CLI) --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		--preset dumble-clean \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB)

standalone-dumble-crunch: build
	$(CLI) --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		--preset dumble-crunch --ir \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB)

standalone-dumble-driven: build
	$(CLI) --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		--preset dumble-driven --ir \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB)

devices: build
	$(CLI) --list-devices

compare-ac30:
	cargo test --test ac30_reference -- --ignored --nocapture

desktop:
	cargo run -p desktop

desktop-release:
	cargo run -p desktop --release

run-desktop: desktop-release
	$(DESKTOP)

.PHONY: standalone-dumble standalone-dumble-ir
