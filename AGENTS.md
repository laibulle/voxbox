# AGENTS.md

This repository is Greybound: a circuit-informed guitar amp and pedal DSP project.
Treat this file as the first instruction layer for Codex work in this repo.

## Project Memory

The source of truth for project knowledge is `knowledge/`.

- Read `knowledge/index.mdx`, `knowledge/progress/state.mdx`, and the relevant
  `knowledge/architecture/` page before changing architecture or DSP behavior.
- Read the relevant model page under `knowledge/models/` before changing an amp
  or pedal.
- Read `knowledge/circuits/` before introducing or changing reusable physical
  circuit cells.
- `docs/` is the Fumadocs/Next UI layer. Authored engineering knowledge belongs
  in `knowledge/`, not in `docs/`.
- Keep model-specific reverse-engineering notes in `knowledge/models/`.
- Keep reusable component, solver, validation, and circuit-cell notes in
  `knowledge/circuits/`.
- Do not commit third-party schematic scans, service PDFs, or current-production
  circuit artwork. Store project-owned topology notes, values, assumptions, and
  source links instead.

## Architecture Rules

- Rigs live in `rigs/*.json5` and describe musical topology only: amp, pedals,
  order, bypass, and controls.
- Runtime choices stay outside rig files: audio devices, WAV paths, monitor
  settings, sample rate, period size, render duration, and output paths.
- The signal chain must support arbitrary ordered devices in pre-amp, FX-loop,
  and post-amp sections.
- Avoid hardcoding a specific pedal, amp, or rig into chain APIs.
- Devices keep private DSP state private.
- Shared electrical behavior moves through explicit connection state: voltage,
  source impedance, load impedance, coupling mode, DC offset, headroom, and
  diagnostic warnings.
- Bypassed true-bypass slots should preserve the direct electrical/audio path
  unless their configured bypass mode says otherwise.
- Direct amp behavior should remain bit-for-bit or measurably equivalent when a
  rig has no active devices.

## Working Standard

Before changing DSP behavior:

1. Identify the target behavior and the current reference source.
2. Read the relevant knowledge pages and current code.
3. Make the smallest implementation change that tests the hypothesis.
4. Run focused Rust checks.
5. Render the affected simulation to WAV with monitor logging.
6. Compare against the previous candidate, baseline, SPICE, NAM, or measured
   reference when available.
7. Update `knowledge/` with the decision, evidence, and remaining assumptions.

A good change explains:

- what behavior changed,
- why that behavior is more correct or more useful,
- what evidence changed,
- which metrics improved,
- which metrics regressed or stayed ambiguous,
- what assumptions remain.

## Simulation Quality Evaluation

Do not judge an amp, pedal, or circuit simulation by listening impressions alone.
Every iteration should produce repeatable evidence.

### 1. Define The Comparison Target

Choose the strongest available reference for the specific change:

- measured hardware capture,
- SPICE export for a circuit cell,
- NAM render for an external amp/pedal reference,
- previous Greybound baseline render,
- deterministic Rust fixture or analytic expectation.

If there is no external reference, compare against the previous Greybound render
and document that the result is regression-oriented, not proof of realism.

### 2. Keep The Test Conditions Fixed

For iteration-to-iteration comparisons, keep these constant unless the change is
explicitly about one of them:

- input WAV or generated stimulus,
- rig file,
- sample rate,
- render duration,
- IR/cab setting,
- input/output gain,
- controls and bypass states,
- segment marker file,
- reference WAV and metadata.

If any condition changes, call it out in the final report because metrics may no
longer be directly comparable.

### 3. Always Check Runtime Health

Use monitor logs and WAV output, not a live-only run, for automated evaluation.

Preferred monitor renders:

```bash
target/release/greybound-cli --rig rigs/nox30-driven.json5 --input-wav "lab/references/tone3000-inputs/Brit - Guitar.wav" --output-wav target/greybound-nox30-monitor.wav --render-seconds 10 --sample-rate 48000 --period-size 16 --ir lab/references/tone3000-irs/celestion.wav --monitor
target/release/greybound-cli --rig rigs/muffin-nox30.json5 --input-wav "lab/references/tone3000-inputs/Brit - Guitar.wav" --output-wav target/greybound-fuzz-nox30-monitor.wav --render-seconds 10 --sample-rate 48000 --period-size 16 --ir lab/references/tone3000-irs/celestion.wav --monitor
target/release/greybound-cli --rig rigs/minotaur-nox30.json5 --input-wav "lab/references/tone3000-inputs/Brit - Guitar.wav" --output-wav target/greybound-overdrive-nox30-monitor.wav --render-seconds 10 --sample-rate 48000 --period-size 16 --ir lab/references/tone3000-irs/celestion.wav --monitor
```

Monitor evidence to inspect:

- input RMS and peak range,
- output RMS and peak range,
- near-clip and hard-clip counts,
- xrun counts,
- output gain trend,
- suspicious silence, runaway levels, denormals, or static telemetry,
- component telemetry when present: rail sag, currents, cathode voltages,
  screen current, attack current, and transformer flux.

Any xrun is severe. Output clipping is severe unless the task is explicitly to
shape distortion and the gain staging proves the clipping is intentional.

### 4. Compare WAVs With Diagnostics

When a reference WAV exists, use the lab comparison workflow:

```bash
uv --project lab run greybound-lab compare-wav \
  --candidate lab/renders/nox30-driven.wav \
  --reference lab/references/nox30-reference.wav \
  --metadata lab/renders/nox30-driven.run.json \
  --segments lab/segments/guitar-chords.markers.json \
  --report lab/reports/nox30-driven-vs-reference.md
```

Metric families are diagnostic, not a single score:

- latency and gain alignment: makes the comparison fair,
- RMS, peak, and crest factor: gain staging and dynamic shape,
- null residual: strict regression/error signal after alignment and gain,
- band residual: where the error lives musically,
- log-spectral distance: EQ, harmonic balance, cab/IR color, tonal drift,
- envelope error: compression, sustain, decay, tremolo/modulation depth,
- attack diagnostics: pick feel, rise time, overshoot, transient sharpness,
- harmonic diagnostics: THD and H2-H5 balance on stable tones,
- high-band diagnostics: aliasing triage for nonlinear stages,
- sag diagnostics: drop and recovery on burst windows,
- intermodulation: chord smear and nonlinear harshness.

An improvement is credible only when the relevant metric family moves in the
expected direction without creating a worse runtime-health problem.

### 5. Use The Right Stimulus

Choose stimuli based on the behavior being changed:

- guitar DI render for musical integration,
- sine sweeps for tone stacks and frequency response,
- stable sines for harmonic balance and THD,
- two-tone tests for intermodulation,
- transient pick/burst material for attack and envelope behavior,
- low-frequency bursts for coupling caps, blocking distortion, sag, and bias
  recovery,
- high-frequency generated stimuli for aliasing checks.

Do not over-trust a metric computed from the wrong stimulus. For example, high
band residual on arbitrary guitar DI is only a triage signal, not an aliasing
proof.

### 6. New Pedal Integration Workflow

When adding a new pedal, treat the work as a circuit-research and validation
loop, not as a direct DSP coding task.

1. Define the target pedal and reference policy.
   - Identify exact model, revision, control set, bypass behavior, and expected
     operating voltage.
   - Prefer measured hardware captures when available; otherwise use SPICE,
     NAM, previous Greybound renders, or deterministic fixtures as the stated
     comparison target.
   - Document whether the work is real greybox circuit modeling or a temporary
     audio-shaped approximation.

2. Research the schematic and source confidence.
   - Search for schematic, trace, service notes, community layouts, BOMs,
     known component substitutions, and revision differences.
   - Do not commit third-party schematic scans, copyrighted PDFs, or current
     production artwork. Store project-owned topology notes, values,
     confidence levels, assumptions, and source links in `knowledge/models/`.
   - Resolve ambiguous parts explicitly: potentiometer tapers, diode types,
     op-amp/transistor part numbers, supply filtering, coupling caps, clipping
     topology, tone network, buffers, bypass mode, and input/output impedance.

3. Create or recover a SPICE reference.
   - Prefer an existing credible SPICE netlist only when its source and
     component values can be audited.
   - Otherwise build a minimal project-owned SPICE fixture for the important
     stages first: input/buffer, clipping cell, tone network, output driver,
     and any supply or bias behavior that materially affects audio.
   - Keep third-party netlists out of the repo unless their license is clearly
     compatible. Store derived project-owned fixtures under the lab/reference
     workflow and document provenance.
   - Run DC operating point, AC sweep, transient, level sweep, and focused
     harmonic/intermodulation checks before porting behavior to Rust.

4. Map the circuit into Greybound explicitly.
   - Define every electrical boundary: voltage meaning, source impedance, load
     impedance, coupling mode, DC offset behavior, headroom, and bypass path.
   - Reuse existing circuit cells from `knowledge/circuits/` and shared code
     when the topology matches. Add a reusable cell only when the voltage/current
     meaning and validation fixture are clear.
   - Keep private pedal DSP state private. The rig should describe only musical
     topology, pedal order, bypass state, and controls.
   - Add the pedal to the maintained integration rigs only when it is useful for
     evaluation; current maintained Nox rigs are `rigs/grey-nox.json5` for
     Klon/Minotaur plus Nox and `rigs/all-nox.json5` for the full board.

5. Implement incrementally.
   - Start with the smallest stage that can be compared against SPICE or a
     deterministic fixture.
   - Add controls with documented tapers and ranges, not arbitrary normalized
     mappings.
   - Preserve direct amp behavior when the pedal is absent or bypassed.
   - Add focused Rust tests for parsing, bypass behavior, impedance boundaries,
     control response, and at least one measurable signal response.

6. Evaluate continuously.
   - After each meaningful circuit or DSP change, render WAV output with monitor
     logging and inspect xrun, clip, RMS, peak, silence, runaway, denormal, and
     telemetry warnings.
   - Compare candidate WAVs against SPICE, measured captures, NAM renders, or
     the previous Greybound baseline with fixed test conditions.
   - Use the relevant metric families: alignment/gain, null residual,
     spectral balance, weighted log-spectral distance, envelope, transient,
     harmonics, intermodulation, aliasing, phase/group delay, decay, level
     response, and nonlinear transfer shape.
   - Use dedicated stimuli for the behavior being changed: sine/AC sweeps for
     tone networks, stable sines for THD, two-tone for IMD, bursts for recovery
     and coupling behavior, high-frequency stress for aliasing, and guitar DI
     for integration.

7. Promote only with evidence.
   - A pedal candidate is not accepted because it sounds plausible in one rig.
     It needs documented source confidence, circuit mapping, runtime-health
     evidence, metric deltas, known regressions, and remaining assumptions.
   - If metrics disagree, keep the candidate only when the tradeoff is explicit
     and tied to the target behavior. Do not claim realism from a composite
     score alone.
   - Update `knowledge/models/`, relevant `knowledge/circuits/`, lab reports,
     and any rig documentation in the same work cycle as the code change.

### 7. Circuit-Level Acceptance Gates

For component or circuit-cell work, require more than final audio similarity:

- sourced component values with confidence levels,
- explicit mapping from capacitors and inductors to model state,
- exposed voltage/current meaning for each important port or state,
- documented source/load impedance assumptions,
- measured, documented, or deliberately assumed control tapers,
- DC operating point check,
- AC sweep check,
- transient check at realistic amplitudes,
- Rust fixture comparing operating points, RMS, harmonics, or fixed-frequency
  responses where practical.

If a proposed DSP block cannot say what voltage or current it represents, treat
it as a graybox approximation and validate it as such.

### 8. Iteration Report Format

For each simulation-quality iteration, report:

- baseline artifact and candidate artifact paths,
- exact render and comparison commands,
- changed files,
- target behavior,
- monitor verdict: clean, warning, or severe,
- key metric deltas,
- audible or musical interpretation if useful,
- regressions and unresolved assumptions,
- next most useful experiment.

Avoid broad claims like "more realistic" without evidence. Prefer concrete
claims like "presence-band residual dropped while crest factor stayed within the
baseline range and monitor clipping remained zero."

## Common Commands

Rust checks:

```bash
cargo test -p greybound
cargo test -p greybound-cli
cargo test --workspace
cargo fmt --check
```

Build release CLI before render-quality work:

```bash
cargo build --release
```

Documentation checks from `docs/` when knowledge/docs behavior changes:

```bash
npm run build
npm run typecheck
```

## Documentation Duty

When code behavior changes, update the project memory in the same work cycle.
Document:

- current behavior,
- validation evidence,
- known limitations,
- source/reference confidence,
- next experiments.

If implementation and docs disagree, treat the implementation plus fresh
evidence as provisional truth, then update `knowledge/` so future work does not
repeat stale assumptions.
