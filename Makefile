DEVICE ?= Scarlett 18i8 USB
#DEVICE ?= Écouteurs externes
#DEVICE ?= Haut-parleurs MacBook Air
#DEVICE ?= AirPods Pro de Guillaume
#DEVICE ?= WH-1000XM5
INPUT_CHANNEL ?= 1
OUTPUT_CHANNELS ?= 1,2
SAMPLE_RATE ?= 48000
PERIOD_SIZE ?= 16
INPUT_DB ?= 0
OUTPUT_DB ?= -12
TEST_INPUT_WAV ?= samples/teenager-electric-guitar-smooth-chords-dry_94bpm_G_major.wav
TEST_OUTPUT_WAV ?= target/greybound-nox30-monitor.wav
RENDER_SECONDS ?= 20
RIG ?=
INPUT ?= live
OUTPUT ?= device
IR ?= 0
MONITOR ?= 0
TONE3000_INPUTS_DIR ?= lab/references/tone3000-inputs
TONE3000_IRS_DIR ?= lab/references/tone3000-irs
OVERWRITE ?= 0
CLI := target/release/greybound-cli
DESKTOP :=target/release/greybound-desktop

IR_FLAG = $(if $(filter 1 true yes on,$(IR)),--ir,)
MONITOR_FLAG = $(if $(filter 1 true yes on,$(MONITOR)),--monitor,)
OVERWRITE_FLAG = $(if $(filter 1 true yes on,$(OVERWRITE)),--overwrite,)
RIG_FLAG = $(if $(strip $(RIG)),--rig "$(RIG)",)
REQUIRE_RIG = $(if $(strip $(RIG)),true,echo "RIG is required, for example: make standalone-run RIG=rigs/nox30-driven.json5" >&2; exit 2)

build:
	cargo build --release

standalone: build
	@$(REQUIRE_RIG)
	$(CLI) --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		$(RIG_FLAG) \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB)


standalone-with-ir: build
	@$(REQUIRE_RIG)
	$(CLI) --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		$(RIG_FLAG) \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB) --ir

standalone-run: build
	@$(REQUIRE_RIG)
	@case "$(INPUT):$(OUTPUT)" in \
		live:device) set -- --device "$(DEVICE)" --input-channel "$(INPUT_CHANNEL)" --output-channels "$(OUTPUT_CHANNELS)" ;; \
		file:device) set -- --input-wav "$(TEST_INPUT_WAV)" --output-device "$(DEVICE)" --output-channels "$(OUTPUT_CHANNELS)" ;; \
		file:null) set -- --input-wav "$(TEST_INPUT_WAV)" --null-output ;; \
		file:wav) set -- --input-wav "$(TEST_INPUT_WAV)" --output-wav "$(TEST_OUTPUT_WAV)" --render-seconds "$(RENDER_SECONDS)" ;; \
		*) echo "Unsupported INPUT=$(INPUT) OUTPUT=$(OUTPUT). Use INPUT=live OUTPUT=device, or INPUT=file OUTPUT=device|null|wav." >&2; exit 2 ;; \
	esac; \
	set -x; \
	$(CLI) "$$@" \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		$(RIG_FLAG) $(IR_FLAG) $(MONITOR_FLAG) \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB)

standalone-run-wave: INPUT=file
standalone-run-wave: OUTPUT=device
standalone-run-wave: standalone-run

standalone-run-wavetofile: INPUT=file
standalone-run-wavetofile: OUTPUT=wav
standalone-run-wavetofile: standalone-run

devices: build
	$(CLI) --list-devices

desktop:
	cargo run -p greybound-desktop

desktop-release:
	cargo run -p greybound-desktop --release

run-desktop: desktop-release
	$(DESKTOP)

lab-download-tone3000-inputs:
	uv --project lab run greybound-lab download-tone3000-inputs \
		--output-dir "$(TONE3000_INPUTS_DIR)" $(OVERWRITE_FLAG)

lab-download-tone3000-irs:
	uv --project lab run greybound-lab download-tone3000-irs \
		--output-dir "$(TONE3000_IRS_DIR)" $(OVERWRITE_FLAG)

.PHONY: standalone standalone-with-ir standalone-run standalone-run-wave standalone-run-wavetofile devices desktop desktop-release run-desktop lab-download-tone3000-inputs lab-download-tone3000-irs
