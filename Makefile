#DEVICE ?= Scarlett 18i8 USB
#DEVICE ?= Écouteurs externes
#DEVICE ?= Haut-parleurs MacBook Air
#DEVICE ?= AirPods Pro de Guillaume
DEVICE ?= WH-1000XM5
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
KLON_NAM_MODEL ?= lab/references/nam/J. Rockett _The Jeff_ Archer/Klon Gain 5.nam
KLON_INPUT_DB ?= -36
KLON_OUTPUT_DB ?= -12
KLON_OUTPUT_WAV ?= lab/reports/klon-minotaur/klon-gain5.wav
KLON_METADATA ?= lab/reports/klon-minotaur/klon-gain5.run.json
MINOTAUR_PEDAL_RIG ?= rigs/minotaur-pedal-only.json5
MINOTAUR_INPUT_DB ?= $(KLON_INPUT_DB)
MINOTAUR_OUTPUT_DB ?= $(KLON_OUTPUT_DB)
MINOTAUR_OUTPUT_WAV ?= lab/reports/klon-minotaur/minotaur-pedal-only.wav
MINOTAUR_METADATA ?= lab/reports/klon-minotaur/minotaur-pedal-only.run.json
MINOTAUR_KLON_REPORT ?= lab/reports/klon-minotaur/minotaur-vs-klon-gain5.md
MINOTAUR_KLON_SWEEP_DIR ?= lab/reports/klon-minotaur/sweep-gain5
MINOTAUR_KLON_SWEEP_REPORT ?= lab/reports/klon-minotaur/minotaur-vs-klon-gain5-sweep.md
MINOTAUR_KLON_SWEEP_METADATA ?= lab/reports/klon-minotaur/minotaur-vs-klon-gain5-sweep.run.json
MINOTAUR_KLON_TRIAGE_REPORT ?= lab/reports/klon-minotaur/minotaur-klon-triage.md
MINOTAUR_KLON_TRIAGE_METADATA ?= lab/reports/klon-minotaur/minotaur-klon-triage.run.json
MINOTAUR_SWEEP_GAIN ?= 0.25,0.42,0.60,0.78
MINOTAUR_SWEEP_TREBLE ?= 0.40,0.55,0.70
MINOTAUR_SWEEP_OUTPUT ?= 0.42,0.58,0.74
KLON_SPICE_DIR ?= lab/references/spice/klon-centaur-jatinchowdhury18
KLON_SPICE_ASC ?= $(KLON_SPICE_DIR)/GainStage2_zzz.asc
KLON_SPICE_RUN_ASC ?= $(KLON_SPICE_DIR)/GainStage2_zzz.greybound.asc
KLON_SPICE_SOURCE_URL ?= https://raw.githubusercontent.com/jatinchowdhury18/KlonCentaur/master/GainStageTraining/SPICE/GainStage2_zzz.asc
KLON_SPICE_LICENSE_URL ?= https://raw.githubusercontent.com/jatinchowdhury18/KlonCentaur/master/LICENSE
KLON_SPICE_README ?= $(KLON_SPICE_DIR)/README.md
LTSPICE_BIN ?= /Applications/LTspice.app/Contents/MacOS/LTspice
SPICE_FIXTURE ?= common-cathode-12ax7
SPICE_OUTPUT_DIR ?= lab/references/spice
SPICE_DATASET_DIR ?= lab/datasets/spice
KLON_SPICE_DATASET_DIR ?= lab/datasets/spice/klon-centaur
KLON_NEURAL_TARGET ?= tone_ac_v
KLON_NEURAL_OUTPUT_DIR ?= lab/models/klon-tone-mlp-current
KLON_NEURAL_DATASET_MANIFEST ?= $(KLON_SPICE_DATASET_DIR)/klon-centaur.dataset.json
KLON_NEURAL_HISTORY_SAMPLES ?= 4
KLON_NEURAL_HIDDEN_SIZE ?= 48
NEURAL_CELL ?= common-cathode-12ax7-mlp
NEURAL_DATASET_MANIFEST ?= lab/datasets/spice/common-cathode-12ax7.dataset.json
NEURAL_OUTPUT_DIR ?= lab/models/common-cathode-12ax7-mlp-current
NEURAL_EPOCHS ?= 1200
NEURAL_HIDDEN_SIZE ?= 32
NEURAL_LEARNING_RATE ?= 0.0005
NEURAL_STRIDE ?= 8
NEURAL_HISTORY_SAMPLES ?= 1
NEURAL_DESCRIPTOR ?= $(NEURAL_OUTPUT_DIR)/model.greybound.json
NEURAL_VECTORS ?= $(NEURAL_OUTPUT_DIR)/equivalence-vectors.json
NEURAL_EVAL_REPORT ?= $(NEURAL_OUTPUT_DIR)/spice-evaluation.md
NEURAL_EVAL_SPLIT ?= all
ANALYTIC_EVAL_REPORT ?= lab/reports/common-cathode-analytic-spice-evaluation.md
ANALYTIC_STRIDE ?= $(NEURAL_STRIDE)
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
GRAYBOX_CELL ?= common-cathode-12ax7-state
GRAYBOX_OUTPUT_DIR ?= lab/models/common-cathode-12ax7-graybox-state-current
GRAYBOX_CONFIG ?= accepted
INTEGRATED_GRAYBOX_CONFIG ?= accepted-live
GRAYBOX_LOCAL_CONFIG ?= $(GRAYBOX_OUTPUT_DIR)/common-cathode-graybox-state.json
GRAYBOX_EVAL_REPORT ?= $(GRAYBOX_OUTPUT_DIR)/rust-evaluation.md
GRAYBOX_EPOCHS ?= 220
GRAYBOX_LEARNING_RATE ?= 0.008
GRAYBOX_STRIDE ?= 16
GRAYBOX_MAX_TRAIN_SAMPLES ?= 2048
INTEGRATED_GRAYBOX_DIR ?= lab/reports/integrated-graybox-first-stage-anchor-current
INTEGRATED_GRAYBOX_REPORT ?= lab/reports/integrated-graybox-first-stage-anchor-current.md
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

lab-render-klon-nam:
	@test -n "$(NAM_RENDERER)" || (echo "NAM_RENDERER is required. It must accept placeholders: {model}, {input_wav}, {output_wav}, {sample_rate}, {render_seconds}, {ir_wav}." >&2; exit 2)
	uv --project lab run greybound-lab render-nam \
		--model "$(KLON_NAM_MODEL)" \
		--input-wav "$(NAM_INPUT_WAV)" \
		--output-wav "$(KLON_OUTPUT_WAV)" \
		--metadata "$(KLON_METADATA)" \
		--renderer-command "$(NAM_RENDERER)" \
		--sample-rate "$(NAM_SAMPLE_RATE)" \
		--render-seconds "$(NAM_RENDER_SECONDS)" \
		--input-db "$(KLON_INPUT_DB)" \
		--output-db "$(KLON_OUTPUT_DB)"

lab-render-minotaur-pedal: build
	uv --project lab run greybound-lab render-rig \
		--rig "$(MINOTAUR_PEDAL_RIG)" \
		--input-wav "$(NAM_INPUT_WAV)" \
		--output-wav "$(MINOTAUR_OUTPUT_WAV)" \
		--metadata "$(MINOTAUR_METADATA)" \
		--binary "$(CLI)" \
		--render-seconds "$(NAM_RENDER_SECONDS)" \
		--sample-rate "$(NAM_SAMPLE_RATE)" \
		--period-size "$(PERIOD_SIZE)" \
		--input-db "$(MINOTAUR_INPUT_DB)" \
		--output-db "$(MINOTAUR_OUTPUT_DB)" \
		--disable-neural-cell

lab-compare-minotaur-klon:
	uv --project lab run greybound-lab compare-wav \
		--candidate "$(MINOTAUR_OUTPUT_WAV)" \
		--reference "$(KLON_OUTPUT_WAV)" \
		--report "$(MINOTAUR_KLON_REPORT)" \
		--metadata "$(MINOTAUR_METADATA)"

lab-sweep-minotaur-klon: lab-render-klon-nam build
	uv --project lab run greybound-lab sweep-rig-vs-reference \
		--rig "$(MINOTAUR_PEDAL_RIG)" \
		--input-wav "$(NAM_INPUT_WAV)" \
		--reference-wav "$(KLON_OUTPUT_WAV)" \
		--binary "$(CLI)" \
		--output-dir "$(MINOTAUR_KLON_SWEEP_DIR)" \
		--report "$(MINOTAUR_KLON_SWEEP_REPORT)" \
		--metadata "$(MINOTAUR_KLON_SWEEP_METADATA)" \
		--render-seconds "$(NAM_RENDER_SECONDS)" \
		--sample-rate "$(NAM_SAMPLE_RATE)" \
		--period-size "$(PERIOD_SIZE)" \
		--input-db "$(MINOTAUR_INPUT_DB)" \
		--output-db "$(MINOTAUR_OUTPUT_DB)" \
		--sweep gain="$(MINOTAUR_SWEEP_GAIN)" \
		--sweep treble="$(MINOTAUR_SWEEP_TREBLE)" \
		--sweep output="$(MINOTAUR_SWEEP_OUTPUT)"

lab-benchmark-minotaur-klon: lab-render-klon-nam lab-render-minotaur-pedal lab-compare-minotaur-klon lab-sweep-minotaur-klon

lab-spice-klon:
	uv --project lab run greybound-lab spice-run \
		--fixture klon-centaur \
		--output-dir "$(SPICE_OUTPUT_DIR)"

lab-triage-minotaur-klon: lab-spice-klon lab-benchmark-minotaur-klon
	uv --project lab run greybound-lab minotaur-klon-triage \
		--spice-data "$(SPICE_OUTPUT_DIR)/klon-centaur.dat" \
		--candidate-wav "$(MINOTAUR_OUTPUT_WAV)" \
		--reference-wav "$(KLON_OUTPUT_WAV)" \
		--report "$(MINOTAUR_KLON_TRIAGE_REPORT)" \
		--metadata "$(MINOTAUR_KLON_TRIAGE_METADATA)" \
		--sweep-report "$(MINOTAUR_KLON_SWEEP_REPORT)"

lab-fetch-klon-spice:
	mkdir -p "$(KLON_SPICE_DIR)"
	test -f "$(KLON_SPICE_ASC)" || curl -L "$(KLON_SPICE_SOURCE_URL)" -o "$(KLON_SPICE_ASC)"
	test -f "$(KLON_SPICE_DIR)/LICENSE" || curl -L "$(KLON_SPICE_LICENSE_URL)" -o "$(KLON_SPICE_DIR)/LICENSE"
	printf '%s\n' \
		'# Klon Centaur LTspice Reference' \
		'' \
		'Source: https://github.com/jatinchowdhury18/KlonCentaur' \
		'Fetched file: GainStageTraining/SPICE/GainStage2_zzz.asc' \
		'License: BSD-3-Clause, copied to LICENSE in this directory.' \
		'' \
		'This is an LTspice `.asc` reference for the Klon gain stage used by the upstream project training scripts. It is not yet a Greybound ngspice dataset fixture.' \
		> "$(KLON_SPICE_README)"

lab-check-ltspice:
	@test -x "$(LTSPICE_BIN)" || (echo "LTspice not found at LTSPICE_BIN=$(LTSPICE_BIN). Install LTspice or run with LTSPICE_BIN=/path/to/LTspice." >&2; exit 2)

lab-prepare-klon-spice-ltspice: lab-fetch-klon-spice
	cp "$(KLON_SPICE_ASC)" "$(KLON_SPICE_RUN_ASC)"
	perl -0pi -e 's/^FLAG 0 0 Vout\n//m' "$(KLON_SPICE_RUN_ASC)"
	printf '%s\n' 'TEXT -928 552 Left 2 !.tran 0.2\n.param fr=100 N=10 G=1.0 RVaTop=100000 RVaBot=1 RVbTop=100000 RVbBot=1\n.probe v(Vi) v(Vout)' >> "$(KLON_SPICE_RUN_ASC)"

lab-run-klon-spice-ltspice: lab-prepare-klon-spice-ltspice lab-check-ltspice
	cd "$(KLON_SPICE_DIR)" && "$(abspath $(LTSPICE_BIN))" -b "$(notdir $(KLON_SPICE_RUN_ASC))"

lab-spice-run:
	uv --project lab run greybound-lab spice-run \
		--fixture "$(SPICE_FIXTURE)" \
		--output-dir "$(SPICE_OUTPUT_DIR)"

lab-spice-dataset:
	uv --project lab run greybound-lab spice-dataset \
		--fixture "$(SPICE_FIXTURE)" \
		--output-dir "$(SPICE_DATASET_DIR)"

lab-spice-klon-dataset:
	uv --project lab run greybound-lab spice-dataset \
		--fixture klon-centaur \
		--output-dir "$(KLON_SPICE_DATASET_DIR)"

lab-train-neural-cell:
	uv --project lab run --with torch greybound-lab train-neural-cell \
		--cell "$(NEURAL_CELL)" \
		--dataset-manifest "$(NEURAL_DATASET_MANIFEST)" \
		--output-dir "$(NEURAL_OUTPUT_DIR)" \
		--epochs "$(NEURAL_EPOCHS)" \
		--hidden-size "$(NEURAL_HIDDEN_SIZE)" \
		--learning-rate "$(NEURAL_LEARNING_RATE)" \
		--stride "$(NEURAL_STRIDE)" \
		--history-samples "$(NEURAL_HISTORY_SAMPLES)"

lab-train-klon-neural-cell:
	uv --project lab run --with torch greybound-lab train-neural-cell \
		--cell klon-drive-clip-tone-mlp \
		--dataset-manifest "$(KLON_NEURAL_DATASET_MANIFEST)" \
		--output-dir "$(KLON_NEURAL_OUTPUT_DIR)" \
		--target "$(KLON_NEURAL_TARGET)" \
		--epochs "$(NEURAL_EPOCHS)" \
		--hidden-size "$(KLON_NEURAL_HIDDEN_SIZE)" \
		--learning-rate "$(NEURAL_LEARNING_RATE)" \
		--stride "$(NEURAL_STRIDE)" \
		--history-samples "$(KLON_NEURAL_HISTORY_SAMPLES)"

lab-fit-graybox-cell:
	uv --project lab run --with torch greybound-lab fit-graybox-cell \
		--cell "$(GRAYBOX_CELL)" \
		--dataset-manifest "$(NEURAL_DATASET_MANIFEST)" \
		--output-dir "$(GRAYBOX_OUTPUT_DIR)" \
		--epochs "$(GRAYBOX_EPOCHS)" \
		--learning-rate "$(GRAYBOX_LEARNING_RATE)" \
		--stride "$(GRAYBOX_STRIDE)" \
		--max-train-samples-per-stimulus "$(GRAYBOX_MAX_TRAIN_SAMPLES)"

lab-evaluate-graybox-cell-rust:
	cargo run -p greybound --example common_cathode_graybox_eval -- \
		--manifest "$(NEURAL_DATASET_MANIFEST)" \
		--config "$(GRAYBOX_CONFIG)" \
		--report "$(GRAYBOX_EVAL_REPORT)" \
		--stride "$(GRAYBOX_STRIDE)" \
		--split "$(NEURAL_EVAL_SPLIT)"

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

lab-evaluate-integrated-graybox-cell: build
	uv --project lab run greybound-lab evaluate-integrated-neural-cell \
		--graybox-config "$(INTEGRATED_GRAYBOX_CONFIG)" \
		--component "nox30.first_stage" \
		--rig "$(INTEGRATED_NEURAL_RIG)" \
		--input-wav "$(TEST_INPUT_WAV)" \
		--binary "$(CLI)" \
		--output-dir "$(INTEGRATED_GRAYBOX_DIR)" \
		--report "$(INTEGRATED_GRAYBOX_REPORT)" \
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
		--stride "$(ANALYTIC_STRIDE)" \
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

.PHONY: standalone standalone-with-ir standalone-run standalone-run-wave standalone-run-wavetofile devices desktop desktop-release run-desktop lab-download-tone3000-inputs lab-download-tone3000-irs lab-inspect-nam-pack lab-render-nam lab-render-klon-nam lab-render-minotaur-pedal lab-compare-minotaur-klon lab-sweep-minotaur-klon lab-benchmark-minotaur-klon lab-spice-klon lab-triage-minotaur-klon lab-fetch-klon-spice lab-check-ltspice lab-run-klon-spice-ltspice lab-spice-run lab-spice-dataset lab-spice-klon-dataset lab-train-neural-cell lab-train-klon-neural-cell lab-fit-graybox-cell lab-evaluate-graybox-cell-rust lab-export-neural-cell-vectors lab-check-neural-cell-rust lab-evaluate-neural-cell lab-shadow-nox30-first-stage lab-evaluate-integrated-neural-cell lab-evaluate-integrated-graybox-cell lab-sweep-neural-blend lab-evaluate-analytic-common-cathode wasm-build web-build docs-build site-build web-vercel-build docs-vercel-build vercel-build web-deploy docs-deploy vercel-deploy
