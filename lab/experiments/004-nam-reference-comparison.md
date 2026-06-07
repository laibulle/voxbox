# 004 NAM Reference Comparison

Status: planned

## Purpose

Use NAM as an integration oracle for the complete Greybound signal path.

The goal is not to copy NAM internally. The goal is to compare Greybound against
a high-realism capture at fixed settings, then use our metrics to decide which
subsystems need work.

## Reference Choice

Preferred reference:

- TONE3000 `VOX AC30` category,
- gear type: `Amp Head`,
- platform: `NAM`,
- architecture: `A2` only,
- clean or edge-of-breakup / Top Boost style capture,
- rendered with the same DI and the same cabinet IR as the Greybound comparison.

Fallback reference:

- TONE3000 `VOX AC30` `Full Rig / Combo`,
- rendered without an extra IR,
- compared against Greybound with cab/IR disabled,
- marked as cab/mic-confounded.

TONE3000 documents the important distinction: `Amp Head` captures need a
separate IR, while `Full Rig / Combo` captures already include the speaker cab.
For Greybound R&D, the downloaded AC30HWH amp-head NAM pack is used as an
amp-core reference without adding IR. Cabinet IR matching is tested separately.

## Initial Candidate Search

Public TONE3000 pages confirm:

- there is a `VOX AC30` category,
- the category exposes `Full Rig`, `Amp Head`, `Pedal`, `Outboard`, and `IR`
  filters,
- TONE3000 has many VOX-family NAM captures,
- some public pages mention VOX AC30 full-rig captures and VOX-style amp-head
  examples.

First manual search:

```text
https://www.tone3000.com/categories/makes/VOX%2BAC30
```

Search/filter criteria:

- gear: `Amp Head`,
- platform: `NAM`,
- architecture: `A2`,
- tags or title: `AC30`, `Top Boost`, `clean`, `edge`, `breakup`,
- avoid captures that include boost pedals unless explicitly needed.

Selected first candidate:

```text
https://www.tone3000.com/tones/ac30hwh-6580
```

Observed public metadata:

- title: `AC30HWH`,
- gear: `Amp Head Capture`,
- platform: `NAM`,
- make/model: `Vox AC30HWH`,
- license: `T3K`,
- author notes: retubed with JJ tubes, master volume bypassed,
- channels/settings represented: Normal channel with bright switch on, Top Boost
  with treble and bass at noon, Hot mode, with and without Top Cut at 6/10,
- page exposes 22 model variants.

Useful model-name semantics from the page:

- `Normal-Bright-Gain3`,
- `Normal-Bright-Gain5`,
- `Normal-Bright-Gain7`,
- `Normal-Bright-GainFull`,
- `TopBoost-Gain3`,
- `TopBoost-Gain5`,
- `TopBoost-Gain7`,
- `TopBoost-GainFull`,
- `HotMode-Gain5`,
- `HotMode-Gain7`,
- `HotMode-GainFull`,
- optional `TopCut` suffix.

These names give us a useful semi-structured capture grid, but they are not a
complete machine-readable knob schema. Treat them as parsed capture semantics:
channel/mode, approximate gain position, top-cut state, and fixed author notes.
Do not infer unlisted control values beyond what the title or description says.

If the best exact AC30 result is full-rig only, keep it as a fallback but do not
use it as the first diagnostic reference for amp-stage tuning.

## Render Protocol

Use the existing dry guitar sample first:

```text
lab/references/tone3000-inputs/Brit - Guitar.wav
```

Required render settings:

- sample rate: `44100 Hz`,
- no normalization after rendering,
- no limiter,
- record/render enough duration to cover the complete sample,
- export mono WAV if possible,
- document host/plugin/tool,
- document NAM tone URL and creator,
- document NAM architecture version and use A2 only,
- document whether the capture includes cab.

For amp-head NAM:

- load NAM amp-head capture,
- do not add IR after NAM,
- render Greybound with cab/IR disabled,
- render to `lab/references/nam/<reference-id>.wav`,
- metadata `ir_policy`: `amp-head-no-ir`.

For full-rig NAM:

- load NAM full-rig capture,
- do not add Greybound IR,
- render to `lab/references/nam/<reference-id>.wav`,
- metadata `ir_policy`: `full-rig-no-extra-ir`.

## Metadata

Create a metadata file matching:

```text
lab/schemas/nam-reference.schema.json
```

Suggested path:

```text
lab/references/nam/<reference-id>.json
```

Do not commit downloaded model files or rendered WAVs unless redistribution
rights are explicit.

The manually downloaded `AC30HWH-6580` pack is summarized by:

```text
lab/references/nam/manifests/ac30hwh-6580.json
```

Regenerate that source-safe manifest with:

```sh
make lab-inspect-nam-pack
```

The manifest deliberately stores model metadata, parsed capture semantics, local
paths, and priorities, but not model weights.

## NAM Rendering

The lab has a renderer wrapper, not an embedded NAM engine. Use it once an
external NAM A2 renderer is available:

```sh
make lab-render-nam \
  NAM_MODEL=lab/references/nam/AC30HWH/TopBoost-Gain5.nam \
  NAM_OUTPUT_WAV=lab/references/nam/renders/ac30hwh-6580-topboost-gain5.wav \
  NAM_METADATA=lab/references/nam/renders/ac30hwh-6580-topboost-gain5.run.json \
  NAM_INPUT_DB=-70 \
  NAM_OUTPUT_DB=-12
```

The wrapper expands these placeholders:

- `{model}`
- `{input_wav}`
- `{output_wav}`
- `{metadata}`
- `{sample_rate}`
- `{render_seconds}`
- `{ir_wav}`

The default renderer uses `uv run --python 3.11 --with neural-amp-modeler==0.13.0`
and `lab/scripts/nam_a2_render.py`. The adapter selects the highest-quality
submodel from the A2 `SlimmableContainer` and renders it through the official
NAM WaveNet loader.

Current provisional gain staging:

- `NAM_INPUT_DB=-70`
- `NAM_OUTPUT_DB=-12`

This keeps the exported float WAV in a sane range for the current dry guitar
sample. It is a calibration setting, not a final statement about NAM's physical
input reference.

## Comparison Command

Render Greybound:

```sh
uv --project lab run greybound-lab render-rig \
  --rig rigs/nox30-driven.json5 \
  --input-wav "lab/references/tone3000-inputs/Brit - Guitar.wav" \
  --output-wav lab/renders/nox30-driven-for-nam.wav \
  --metadata lab/renders/nox30-driven-for-nam.run.json \
  --render-seconds 20 \
  --sample-rate 48000 \
  --period-size 16 \
  --output-db -18 \
  --ir lab/references/tone3000-irs/celestion.wav
```

Compare:

```sh
uv --project lab run greybound-lab compare-wav \
  --candidate lab/renders/nox30-driven-for-nam.wav \
  --reference lab/references/nam/<reference-id>.wav \
  --metadata lab/renders/nox30-driven-for-nam.run.json \
  --segments lab/segments/guitar-chords.markers.json \
  --report lab/reports/nox30-driven-vs-nam-<reference-id>.md \
  --max-lag-ms 200
```

## Decision Criteria

The NAM comparison is useful if it identifies one or more dominant gaps:

- gain staging / compression,
- harmonic or IMD mismatch,
- attack overshoot mismatch,
- sag/recovery mismatch,
- stable band residual pointing to tone stack or cab/IR,
- high-band mismatch pointing to anti-aliasing or nonlinear top-end behavior.

The comparison is not useful if:

- the reference is full-rig and cab/mic differences dominate every metric,
- the input gain is unknown or clipped,
- post-processing, limiter, normalization, or room/reverb is present,
- the NAM reference is a stylistic preset rather than a close AC30-like amp
  capture.

## Open Question

If no high-quality AC30 amp-head NAM is available, decide whether to:

- use a close VOX-family amp-head capture such as AC15 Top Boost,
- use an AC30 full-rig only for broad end-to-end sanity,
- or capture/train our own reference later.
