# VoxBox

Rust proof-of-concept graybox model of a Vox-style cathode-biased British combo,
implemented as a CLAP/VST3/standalone plugin with
[NIH-plug](https://github.com/robbert-vdh/nih-plug) and
[`rill-core-wdf`](https://docs.rs/rill-core-wdf).

This is not a component-accurate AC30 model. It combines WDF RC networks with
behavioral nonlinear stages to capture the useful macro behavior:

- bright input and cathode-bypass response
- asymmetric preamp saturation
- upper-mid "chime" and global power-amp cut
- soft cathode-biased power-stage compression
- simple combo-speaker rolloff

## Build

```sh
cargo test
cargo build --release
```

The release build produces the plugin library and a `voxbox-standalone` binary.
For convenient plugin bundles, install NIH-plug's `cargo xtask` bundler or add
the standard NIH-plug `xtask` crate later.

## Real-time use on macOS

The standalone binary opens the audio interface's native multichannel streams,
processes one selected guitar input, and sends the result to selected outputs.

List device names:

```sh
target/release/voxbox-standalone --list-devices
```

Then run with a name from the list. Channel numbers are one-based:

```sh
target/release/voxbox-standalone \
  --device 'Scarlett 18i8 USB' \
  --input-channel 1 \
  --output-channels 1,2 \
  --sample-rate 48000 \
  --period-size 256
```

`make standalone` runs that Scarlett configuration. If CoreAudio rejects it,
use the interface's configured sample rate (`44100`, `48000`, or `96000`) and
try period sizes such as `128`, `256`, or `512`. `44000` is not a standard
Scarlett sample rate. Headphones are strongly recommended while testing.

## Real-time and portability notes

- `amp::VoxAmp` is a reusable DSP core independent of the plugin and standalone
  wrappers. A future CPAL, embedded, or other device adapter can call it
  directly.
- The sample-processing path uses concrete types and static dispatch. It does
  not allocate, lock, perform I/O, or use trait objects.
- `Vec<VoxAmp>` and plugin parameter state are allocated during initialization,
  outside the audio callback.
- The nonlinear model still has a computational cost, including `tanh()` and a
  cutoff-coefficient `exp()` per sample. Benchmark the target device before
  treating it as hard real-time.
- The CPAL standalone adapter bridges CoreAudio's input and output callbacks
  with a lock-free ring buffer. Use the same interface for input and output to
  keep both streams on the same hardware clock.

## Controls

- **Gain**: preamp and power-stage drive
- **Tone**: upper-mid/treble emphasis into the power stage
- **Cut**: global high-frequency damping after the power stage
- **Master**: output level
