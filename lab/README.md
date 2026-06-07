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

It defines the minimum useful analysis loop: render, align, normalize, measure,
and report.

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

`datasets/`

: Generated or imported training datasets. Keep large data out of git unless it
  is tiny, source-safe, and necessary for tests.

`references/`

: External references such as NAM renders, measured pedal captures, or SPICE
  exports. Treat this as local working data unless redistribution rights are
  explicit.

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
- short engineering notes for the next model decision.

This gives us a useful baseline before NAM, SPICE, or training choices become
expensive.
