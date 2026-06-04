# Real AC30 functional-test fixtures

This test requires a paired recording made from exactly the same source signal:

- `input-di.wav`: mono DI sent to both VoxBox and the amplifier.
- `reference-ac30.wav`: mono output recorded from the real AC30.

Both files must use the same sample rate and start from the same exported
timeline. The test compensates residual latency, polarity, and constant gain.

## Capture protocol

1. Record or obtain a clean, non-clipping guitar DI.
2. Send it through a reamp box into the AC30 Top Boost input.
3. Record either:
   - the load-box output without cabinet simulation; run with
     `VOXBOX_AC30_REFERENCE_KIND=amp`, or
   - the cabinet microphone; the default test includes VoxBox's speaker IR.
4. Disable gates, compression, reverb, normalization, fades, and other effects.
5. Note the AC30 Volume, Bass, Treble, and Cut positions.
6. Export both mono WAV files with identical start/end positions.

Do not use an unrelated public AC30 demo. Without its exact DI source, waveform
and dynamic comparisons are invalid.

## Run

```sh
cargo test --test ac30_reference -- --ignored --nocapture
```

Configure the test with environment variables:

```sh
VOXBOX_AC30_VOLUME=6 \
VOXBOX_AC30_BASS=5 \
VOXBOX_AC30_TREBLE=6 \
VOXBOX_AC30_CUT=3.5 \
VOXBOX_AC30_INPUT_DB=0 \
cargo test --test ac30_reference -- --ignored --nocapture
```

Paths can be overridden with `VOXBOX_AC30_DI` and
`VOXBOX_AC30_REFERENCE`. Thresholds can be overridden with:

- `VOXBOX_AC30_MIN_CORRELATION`
- `VOXBOX_AC30_MAX_NMSE_DB`
- `VOXBOX_AC30_MAX_ENVELOPE_ERROR_DB`
- `VOXBOX_AC30_MAX_BAND_ERROR_DB`
