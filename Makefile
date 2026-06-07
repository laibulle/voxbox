#DEVICE ?= Scarlett 18i8 USB
#DEVICE ?= Écouteurs externes
DEVICE ?= Haut-parleurs MacBook Air
#DEVICE ?= AirPods Pro de Guillaume
#DEVICE ?= WH-1000XM5
INPUT_CHANNEL ?= 1
OUTPUT_CHANNELS ?= 1,2
SAMPLE_RATE ?= 48000
FILE_SAMPLE_RATE ?= 48000
PERIOD_SIZE ?= 16
INPUT_DB ?= 0
OUTPUT_DB ?= -12
TEST_INPUT_WAV ?= lab/references/tone3000-inputs/Brit - Guitar.wav
TEST_OUTPUT_WAV ?= target/greybound-nox30-monitor.wav
RENDER_SECONDS ?= 20
RIG ?=
INPUT ?= live
OUTPUT ?= device
IR ?= 0
IR_WAV ?= lab/references/tone3000-irs/celestion.wav
MONITOR ?= 0
EXTRA_ARGS ?=
TONE3000_INPUTS_DIR ?= lab/references/tone3000-inputs
TONE3000_IRS_DIR ?= lab/references/tone3000-irs
NAM_PACK_DIR ?= lab/references/nam/AC30HWH
NAM_PACK_MANIFEST ?= lab/references/nam/manifests/ac30hwh-6580.json
NAM_TONE_URL ?= https://www.tone3000.com/tones/ac30hwh-6580
NAM_MODEL ?= lab/references/nam/AC30HWH/TopBoost-Gain5.nam
NAM_RENDERER ?= uv run --python 3.11 --with neural-amp-modeler==0.13.0 --with scipy python lab/scripts/nam_a2_render.py --model {model} --input {input_wav} --output {output_wav} --sample-rate {sample_rate} --seconds {render_seconds} --input-db {input_db} --output-db {output_db}
NAM_INPUT_WAV ?= lab/references/tone3000-inputs/Brit - Guitar.wav
NAM_OUTPUT_WAV ?= lab/references/nam/renders/ac30hwh-6580-topboost-gain5.wav
NAM_METADATA ?= lab/references/nam/renders/ac30hwh-6580-topboost-gain5.run.json
NAM_SAMPLE_RATE ?= 48000
NAM_RENDER_SECONDS ?= 20
NAM_INPUT_DB ?= -70
NAM_OUTPUT_DB ?= -12
SPICE_FIXTURE ?= common-cathode-12ax7
SPICE_DATASET_DIR ?= lab/datasets/spice
NEURAL_CELL ?= common-cathode-12ax7-mlp
NEURAL_DATASET_MANIFEST ?= lab/datasets/spice/common-cathode-12ax7.dataset.json
NEURAL_OUTPUT_DIR ?= lab/models/common-cathode-12ax7-mlp-current
NEURAL_EPOCHS ?= 1200
NEURAL_HIDDEN_SIZE ?= 32
NEURAL_LEARNING_RATE ?= 0.0005
NEURAL_STRIDE ?= 8
NEURAL_DESCRIPTOR ?= $(NEURAL_OUTPUT_DIR)/model.greybound.json
NEURAL_VECTORS ?= $(NEURAL_OUTPUT_DIR)/equivalence-vectors.json
NEURAL_EVAL_REPORT ?= $(NEURAL_OUTPUT_DIR)/spice-evaluation.md
NEURAL_EVAL_SPLIT ?= all
ANALYTIC_EVAL_REPORT ?= lab/reports/common-cathode-analytic-spice-evaluation.md
INTEGRATED_NEURAL_DIR ?= lab/reports/integrated-neural-first-stage-anchor-current
INTEGRATED_NEURAL_REPORT ?= lab/reports/integrated-neural-first-stage-anchor-current.md
INTEGRATED_NEURAL_RIG ?= rigs/nox30-nam-anchor.json5
INTEGRATED_NEURAL_IR ?= 0
INTEGRATED_NEURAL_REFERENCE_WAV ?= lab/reports/nam-diagnostics-ac30hwh-topboost-gain5-brit-noir.wav
INTEGRATED_NEURAL_SEGMENTS ?= lab/segments/guitar-chords.markers.json
NEURAL_BLEND_DIR ?= lab/reports/neural-blend-first-stage-anchor-current
NEURAL_BLEND_REPORT ?= lab/reports/neural-blend-first-stage-anchor-current.md
NEURAL_BLEND_METADATA ?= lab/reports/neural-blend-first-stage-anchor-current.run.json
NEURAL_BLEND_ALPHAS ?= 0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1
WASM_OUT_DIR ?= web/lib/greybound-wasm
OVERWRITE ?= 0
VERCEL ?= npx vercel
VERCEL_ARGS ?= --prod
VERCEL_BUILD_ARGS ?= $(VERCEL_ARGS)
VERCEL_DEPLOY_ARGS ?= --prebuilt $(VERCEL_ARGS)
WEB_VERCEL_BUILD_ARGS ?= $(VERCEL_BUILD_ARGS)
DOCS_VERCEL_BUILD_ARGS ?= $(VERCEL_BUILD_ARGS)
WEB_VERCEL_DEPLOY_ARGS ?= $(VERCEL_DEPLOY_ARGS)
DOCS_VERCEL_DEPLOY_ARGS ?= $(VERCEL_DEPLOY_ARGS)
CLI := target/release/greybound-cli
DESKTOP :=target/release/greybound-desktop

IR_FLAG = $(if $(filter 0 false no off,$(IR)),,$(if $(filter 1 true yes on,$(IR)),--ir "$(IR_WAV)",--ir "$(IR)"))
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


standalone-with-ir: IR=1
standalone-with-ir: build
	@$(REQUIRE_RIG)
	$(CLI) --device '$(DEVICE)' \
		--input-channel $(INPUT_CHANNEL) --output-channels $(OUTPUT_CHANNELS) \
		--sample-rate $(SAMPLE_RATE) --period-size $(PERIOD_SIZE) \
		$(RIG_FLAG) \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB) $(IR_FLAG)

standalone-run: build
	@$(REQUIRE_RIG)
	@sample_rate="$(SAMPLE_RATE)"; \
	case "$(INPUT):$(OUTPUT)" in \
		live:device) set -- --device "$(DEVICE)" --input-channel "$(INPUT_CHANNEL)" --output-channels "$(OUTPUT_CHANNELS)" ;; \
		file:device) sample_rate="$(FILE_SAMPLE_RATE)"; set -- --input-wav "$(TEST_INPUT_WAV)" --output-device "$(DEVICE)" --output-channels "$(OUTPUT_CHANNELS)" ;; \
		file:null) sample_rate="$(FILE_SAMPLE_RATE)"; set -- --input-wav "$(TEST_INPUT_WAV)" --null-output ;; \
		file:wav) sample_rate="$(FILE_SAMPLE_RATE)"; set -- --input-wav "$(TEST_INPUT_WAV)" --output-wav "$(TEST_OUTPUT_WAV)" --render-seconds "$(RENDER_SECONDS)" ;; \
		*) echo "Unsupported INPUT=$(INPUT) OUTPUT=$(OUTPUT). Use INPUT=live OUTPUT=device, or INPUT=file OUTPUT=device|null|wav." >&2; exit 2 ;; \
	esac; \
	set -x; \
	$(CLI) "$$@" \
		--sample-rate "$$sample_rate" --period-size $(PERIOD_SIZE) \
		$(RIG_FLAG) $(IR_FLAG) $(MONITOR_FLAG) \
		--input-db $(INPUT_DB) --output-db $(OUTPUT_DB) \
		$(EXTRA_ARGS)

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

lab-inspect-nam-pack:
	uv --project lab run greybound-lab inspect-nam-pack \
		--pack-dir "$(NAM_PACK_DIR)" \
		--manifest "$(NAM_PACK_MANIFEST)" \
		--tone-url "$(NAM_TONE_URL)"

lab-render-nam:
	@test -n "$(NAM_RENDERER)" || (echo "NAM_RENDERER is required. It must accept placeholders: {model}, {input_wav}, {output_wav}, {sample_rate}, {render_seconds}, {ir_wav}." >&2; exit 2)
	uv --project lab run greybound-lab render-nam \
		--model "$(NAM_MODEL)" \
		--input-wav "$(NAM_INPUT_WAV)" \
		--output-wav "$(NAM_OUTPUT_WAV)" \
		--metadata "$(NAM_METADATA)" \
		--renderer-command "$(NAM_RENDERER)" \
		--sample-rate "$(NAM_SAMPLE_RATE)" \
		--render-seconds "$(NAM_RENDER_SECONDS)" \
		--input-db "$(NAM_INPUT_DB)" \
		--output-db "$(NAM_OUTPUT_DB)"

lab-spice-dataset:
	uv --project lab run greybound-lab spice-dataset \
		--fixture "$(SPICE_FIXTURE)" \
		--output-dir "$(SPICE_DATASET_DIR)"

lab-train-neural-cell:
	uv --project lab run --with torch greybound-lab train-neural-cell \
		--cell "$(NEURAL_CELL)" \
		--dataset-manifest "$(NEURAL_DATASET_MANIFEST)" \
		--output-dir "$(NEURAL_OUTPUT_DIR)" \
		--epochs "$(NEURAL_EPOCHS)" \
		--hidden-size "$(NEURAL_HIDDEN_SIZE)" \
		--learning-rate "$(NEURAL_LEARNING_RATE)" \
		--stride "$(NEURAL_STRIDE)"

lab-export-neural-cell-vectors:
	uv --project lab run greybound-lab export-neural-cell-vectors \
		--descriptor "$(NEURAL_DESCRIPTOR)" \
		--output "$(NEURAL_VECTORS)"

lab-check-neural-cell-rust:
	GREYBOUND_NEURAL_CELL_DESCRIPTOR="$(abspath $(NEURAL_DESCRIPTOR))" \
	GREYBOUND_NEURAL_CELL_VECTORS="$(abspath $(NEURAL_VECTORS))" \
	cargo test -p greybound generated_neural_cell_vectors_match_rust_loader

lab-evaluate-neural-cell:
	uv --project lab run greybound-lab evaluate-neural-cell \
		--descriptor "$(NEURAL_DESCRIPTOR)" \
		--dataset-manifest "$(NEURAL_DATASET_MANIFEST)" \
		--report "$(NEURAL_EVAL_REPORT)" \
		--stride "$(NEURAL_STRIDE)" \
		--split "$(NEURAL_EVAL_SPLIT)"

lab-shadow-nox30-first-stage: RIG = rigs/nox30-driven.json5
lab-shadow-nox30-first-stage: INPUT=file
lab-shadow-nox30-first-stage: OUTPUT=wav
lab-shadow-nox30-first-stage: MONITOR=1
lab-shadow-nox30-first-stage: IR=1
lab-shadow-nox30-first-stage: EXTRA_ARGS=--neural-cell "nox30.first_stage=$(NEURAL_DESCRIPTOR)" --neural-cell-mode shadow
lab-shadow-nox30-first-stage:
	$(MAKE) standalone-run RIG="$(RIG)" INPUT="$(INPUT)" OUTPUT="$(OUTPUT)" MONITOR="$(MONITOR)" IR="$(IR)" EXTRA_ARGS='$(EXTRA_ARGS)'

lab-evaluate-integrated-neural-cell: build
	uv --project lab run greybound-lab evaluate-integrated-neural-cell \
		--descriptor "$(NEURAL_DESCRIPTOR)" \
		--component "nox30.first_stage" \
		--rig "$(INTEGRATED_NEURAL_RIG)" \
		--input-wav "$(TEST_INPUT_WAV)" \
		--binary "$(CLI)" \
		--output-dir "$(INTEGRATED_NEURAL_DIR)" \
		--report "$(INTEGRATED_NEURAL_REPORT)" \
		--render-seconds "$(RENDER_SECONDS)" \
		--sample-rate "$(FILE_SAMPLE_RATE)" \
		--period-size "$(PERIOD_SIZE)" \
		--input-db "$(INPUT_DB)" \
		--output-db "$(OUTPUT_DB)" \
		$(if $(filter 1 true yes on,$(INTEGRATED_NEURAL_IR)),--ir --ir-wav "$(IR_WAV)",) \
		$(if $(strip $(INTEGRATED_NEURAL_REFERENCE_WAV)),--reference-wav "$(INTEGRATED_NEURAL_REFERENCE_WAV)",) \
		$(if $(strip $(INTEGRATED_NEURAL_SEGMENTS)),--segments "$(INTEGRATED_NEURAL_SEGMENTS)",)

lab-sweep-neural-blend:
	uv --project lab run greybound-lab sweep-neural-blend \
		--analytic-wav "$(INTEGRATED_NEURAL_DIR)/analytic.wav" \
		--replace-wav "$(INTEGRATED_NEURAL_DIR)/replace.wav" \
		--reference-wav "$(INTEGRATED_NEURAL_REFERENCE_WAV)" \
		--output-dir "$(NEURAL_BLEND_DIR)" \
		--report "$(NEURAL_BLEND_REPORT)" \
		--metadata "$(NEURAL_BLEND_METADATA)" \
		--alphas "$(NEURAL_BLEND_ALPHAS)" \
		$(if $(strip $(INTEGRATED_NEURAL_SEGMENTS)),--segments "$(INTEGRATED_NEURAL_SEGMENTS)",)

lab-evaluate-analytic-common-cathode:
	cargo run -p greybound --example common_cathode_dataset_eval -- \
		--manifest "$(NEURAL_DATASET_MANIFEST)" \
		--report "$(ANALYTIC_EVAL_REPORT)" \
		--stride "$(NEURAL_STRIDE)" \
		--split "$(NEURAL_EVAL_SPLIT)"

wasm-build:
	wasm-pack build wasm --target web --out-dir "../$(WASM_OUT_DIR)" --out-name greybound_wasm

web-build: wasm-build
	npm --prefix web run build:next

docs-build:
	npm --prefix docs run build

site-build: web-build docs-build

web-vercel-build:
	cd web && $(VERCEL) build $(WEB_VERCEL_BUILD_ARGS)

docs-vercel-build:
	cd docs && $(VERCEL) build $(DOCS_VERCEL_BUILD_ARGS)

vercel-build: web-vercel-build docs-vercel-build

web-deploy: web-vercel-build
	cd web && $(VERCEL) deploy $(WEB_VERCEL_DEPLOY_ARGS)

docs-deploy: docs-vercel-build
	cd docs && $(VERCEL) deploy $(DOCS_VERCEL_DEPLOY_ARGS)

vercel-deploy: web-deploy docs-deploy

.PHONY: standalone standalone-with-ir standalone-run standalone-run-wave standalone-run-wavetofile devices desktop desktop-release run-desktop lab-download-tone3000-inputs lab-download-tone3000-irs lab-inspect-nam-pack lab-render-nam lab-spice-dataset lab-train-neural-cell lab-export-neural-cell-vectors lab-check-neural-cell-rust lab-evaluate-neural-cell lab-shadow-nox30-first-stage lab-evaluate-integrated-neural-cell lab-sweep-neural-blend lab-evaluate-analytic-common-cathode wasm-build web-build docs-build site-build web-vercel-build docs-vercel-build vercel-build web-deploy docs-deploy vercel-deploy
