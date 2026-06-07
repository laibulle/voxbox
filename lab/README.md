# Greybound Lab

`lab/` is the offline R&D workspace for Greybound. It is separate from the
real-time engine on purpose: experiments may use slower tools, generated WAV
files, SPICE renders, NAM references, plots, and training artifacts. The runtime
crates should only consume artifacts after they have been reviewed and frozen.

## Setup

The lab is a Python scientific workspace managed with `uv`. From the repository
root:

```sh
uv --project lab sync --dev
uv --project lab run pytest
```

Run the first comparison tool with:

```sh
uv --project lab run greybound-lab compare-wav \
  --candidate lab/renders/nox30-driven.wav \
  --reference lab/references/nox30-reference.wav \
  --segments lab/segments/guitar-chords.markers.json \
  --report lab/reports/nox30-driven-vs-reference.md
```

From inside `lab/`, use `uv run ...` and drop the leading `lab/` path
components.

Render a Greybound rig into the lab with reproducible metadata:

```sh
uv --project lab run greybound-lab render-rig \
  --rig rigs/nox30-driven.json5 \
  --input-wav samples/teenager-electric-guitar-smooth-chords-dry_94bpm_G_major.wav \
  --output-wav lab/renders/nox30-driven.wav \
  --metadata lab/renders/nox30-driven.run.json \
  --render-seconds 10 \
  --sample-rate 44100 \
  --period-size 16 \
  --output-db -18 \
  --ir
```

Generate standard lab stimuli for focused metrics:

```sh
uv --project lab run greybound-lab generate-stimuli \
  --output-dir lab/stimuli \
  --sample-rate 44100
```

Run the first SPICE cell reference:

```sh
uv --project lab run greybound-lab spice-run \
  --fixture common-cathode-12ax7 \
  --output-dir lab/references/spice
```

Download public TONE3000 DI input WAV files for local NAM and Greybound
integration tests:

```sh
uv --project lab run greybound-lab download-tone3000-inputs \
  --output-dir lab/references/tone3000-inputs
```

Download public TONE3000 IR WAV files for local cab/reference tests:

```sh
uv --project lab run greybound-lab download-tone3000-irs \
  --output-dir lab/references/tone3000-irs
```

NAM references are imported manually for now. See:

- [004 NAM Reference Comparison](experiments/004-nam-reference-comparison.md)
- [references/nam/README.md](references/nam/README.md)

Inspect a manually downloaded NAM pack and write a source-safe manifest:

```sh
uv --project lab run greybound-lab inspect-nam-pack \
  --pack-dir lab/references/nam/AC30HWH \
  --manifest lab/references/nam/manifests/ac30hwh-6580.json \
  --tone-url https://www.tone3000.com/tones/ac30hwh-6580
```

Render a NAM model once an external NAM A2 renderer is installed:

```sh
uv --project lab run greybound-lab render-nam \
  --model lab/references/nam/AC30HWH/TopBoost-Gain5.nam \
  --input-wav samples/teenager-electric-guitar-smooth-chords-dry_94bpm_G_major.wav \
  --output-wav lab/references/nam/renders/ac30hwh-6580-topboost-gain5.wav \
  --metadata lab/references/nam/renders/ac30hwh-6580-topboost-gain5.run.json \
  --renderer-command "nam-a2-render --model {model} --input {input_wav} --output {output_wav} --sample-rate {sample_rate}" \
  --sample-rate 48000 \
  --render-seconds 20
```

The renderer command is intentionally configurable. The lab does not vendor NAM
inference code yet; it only standardizes metadata, input/output paths, and the
render contract.

## Start Here

The first R&D target is not training. It is measurement.

Before replacing circuit cells with fitted micro-models, we need a repeatable
way to compare:

- Greybound rig renders,
- reference WAV files from NAM or real captures,
- SPICE-generated cell outputs,
- previous Greybound model versions.

The first experiment is:

- [001 Chain Reference Analysis](experiments/001-chain-reference-analysis.md)
- [002 Nox30 Stimulus Batch](experiments/002-nox30-stimulus-batch.md)
- [003 Common-Cathode SPICE Reference](experiments/003-common-cathode-spice-reference.md)
- [004 NAM Reference Comparison](experiments/004-nam-reference-comparison.md)

These define the minimum useful analysis loop and the first controlled-stimulus
comparison between Greybound rigs, then bridge into the first cell-level SPICE
reference.

## Directory Layout

`experiments/`

: Human-readable experiment plans. These are committed and should explain the
  purpose, protocol, inputs, expected outputs, and decision criteria.

`schemas/`

: JSON schemas for lab metadata. These are committed so generated datasets and
  reports have stable structure.

`segments/`

: Committed marker files that define named regions for local diagnostics:
  attacks, sustains, sag windows, high-band checks, and future harmonic tests.

`stimuli/`

: Generated synthetic WAV stimuli and marker files for harmonic, intermodulation,
  aliasing, sag, and attack analysis. Ignored by git by default.

`datasets/`

: Generated or imported training datasets. Keep large data out of git unless it
  is tiny, source-safe, and necessary for tests.

`references/`

: External references such as NAM renders, measured pedal captures, or SPICE
  exports. Treat this as local working data unless redistribution rights are
  explicit. Public TONE3000 input WAV files can be downloaded into
  `references/tone3000-inputs/`; the WAVs, generated manifest, and generated
  README remain ignored by git. Public TONE3000 IR WAV files can be downloaded
  into `references/tone3000-irs/` with the same local-only rule.

`renders/`

: Greybound offline WAV renders.

`reports/`

: Generated metric reports, plots, and comparison summaries.

## Lab Rules

- Keep raw third-party captures and generated audio out of git by default.
- Every report should point to a metadata file that describes its inputs.
- Every accepted result should be reproducible from committed code and declared
  local assets.
- Do not require Python, SPICE, or neural tooling in the live Rust runtime.
- Promote only reviewed artifacts into the runtime crates.

## First Implementation Boundary

The first lab tool consumes WAV pairs and produces a Markdown report with:

- sample rate and channel validation,
- gain and latency alignment,
- RMS, peak, and crest factor,
- STFT or log-spectrum distance,
- transient envelope error,
- null residual after alignment,
- optional segment-level diagnostics with `--segments`,
- attack, harmonic, high-band/aliasing, and sag metrics for typed segments,
- band residual metrics for each segment,
- intermodulation metrics for generated two-tone segments,
- short engineering notes for the next model decision.

This gives us a useful baseline before NAM, SPICE, or training choices become
expensive.
