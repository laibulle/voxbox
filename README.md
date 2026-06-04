# VoxBox

Rust real-time graybox model of a JMI AC30/6 with the OS/010 Top Boost unit,
implemented as a CLAP/VST3/standalone plugin with
[NIH-plug](https://github.com/robbert-vdh/nih-plug) and
[`rill-core-wdf`](https://docs.rs/rill-core-wdf).

This is not a component-exact circuit simulation. It follows the archived JMI
schematic topology using WDF RC networks, filters, and behavioral nonlinear
stages:

- bright-capped Top Boost volume and two ECC83 gain stages
- interactive Top Boost bass/treble network
- long-tail-pair phase inverter and post-PI Cut control
- hot cathode-biased push-pull EL84 quartet with bias shift and GZ34-like sag
- output-transformer bandwidth followed by the optional speaker IR

Original schematic images, service references, and the extracted circuit map
are in [`schematic/`](schematic/).

The topology and major time constants now follow those references, but the
triodes, EL84 banks, phase inverter, transformer, and supply remain compact
behavioral models. The nonlinear stages are not yet oversampled.

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

`make standalone` runs that Scarlett configuration at 48 kHz. Override the
configuration with Make variables:

```sh
make standalone SAMPLE_RATE=44100 PERIOD_SIZE=128
make standalone-with-ir SAMPLE_RATE=96000
```

Available variables are `DEVICE`, `INPUT_CHANNEL`, `OUTPUT_CHANNELS`,
`SAMPLE_RATE`, and `PERIOD_SIZE`. The CLAP/VST3 plugin always uses the sample
rate selected by its host.

If CoreAudio rejects a configuration, use an interface-supported sample rate
such as `44100`, `48000`, or `96000` and try period sizes such as `128`, `256`,
or `512`. `44000` is not a standard Scarlett sample rate. Headphones are
strongly recommended while testing.

The speaker IR is optional and disabled by default. Enable the embedded,
sample-rate-matched 200 ms Celestion Vintage 30 IR with:

```sh
make standalone-with-ir
# or add --ir to voxbox-standalone
```

The CLAP/VST3 plugin exposes the same feature as the default-off `Speaker IR`
parameter. It reports a fixed 256-sample latency so switching the IR on does not
change timing; when the IR is off, convolution is skipped and only the matching
dry delay runs.

## Standalone controls and presets

The standalone controls use the real amp's `0-10` knob scale:

```sh
target/release/voxbox-standalone --device 'Scarlett 18i8 USB' --ir \
  --input-db -12 --volume 6.5 --bass 4.5 --treble 6.5 --cut 4.0 \
  --output-db -12
```

`Volume`, `Bass`, `Treble`, and `Cut` correspond to the AC30 Top Boost controls.
`Input DB` calibrates the audio interface level before the modeled input jack.
The default `-12 dB` prevents a hot Scarlett preamp signal from making every
setting sound cranked. `Output DB` is a safety trim after the modeled amp
because the original AC30 has no master volume.

Set the Scarlett to instrument mode and adjust its hardware gain so normal hard
playing peaks around `-18` to `-12 dBFS`. Then adjust `INPUT_DB` if needed.

Ready-made IR presets:

```sh
make standalone-with-ir-clean
make standalone-with-ir-edge
make standalone-with-ir-crunch
make standalone-with-ir-driven
# "dimed" is an alias for the driven preset
make standalone-with-ir-dimed
```

All preset values remain overridable, for example:

```sh
make standalone-with-ir-crunch TREBLE=5.5 CUT=5 OUTPUT_DB=-16
```

## Real-time and portability notes

- `amp::VoxAmp` is a reusable DSP core independent of the plugin and standalone
  wrappers. A future CPAL, embedded, or other device adapter can call it
  directly.
- The amp sample-processing path uses concrete types and static dispatch. The
  optional IR uses preplanned FFT trait objects once per 256-sample block.
- Neither path allocates, locks, or performs I/O in the audio callback.
- `Vec<VoxAmp>` and plugin parameter state are allocated during initialization,
  outside the audio callback.
- The nonlinear model still has a computational cost, including `tanh()` and a
  cutoff-coefficient `exp()` per sample. Benchmark the target device before
  treating it as hard real-time.
- The CPAL standalone adapter bridges CoreAudio's input and output callbacks
  with a lock-free ring buffer. Use the same interface for input and output to
  keep both streams on the same hardware clock.

## Controls

- **Top Boost Volume**: Top Boost channel volume and drive
- **Bass**: Top Boost bass control
- **Treble**: Top Boost treble control
- **Cut**: global high-frequency damping across the phase-inverter outputs
- **Output Trim**: safety output level; not present on the original amp
