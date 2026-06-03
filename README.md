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

## Controls

- **Gain**: preamp and power-stage drive
- **Tone**: upper-mid/treble emphasis into the power stage
- **Cut**: global high-frequency damping after the power stage
- **Master**: output level

