# NAM References

This directory is for local Neural Amp Modeler reference renders and metadata.

Do not commit downloaded NAM models, downloaded tone packs, or rendered WAVs
unless their redistribution license is explicit and compatible with the project.
Commit only source-safe metadata and experiment notes.

Preferred reference policy:

1. Use **NAM A2** only.
2. Use an **Amp Head** NAM capture when possible.
3. Render it with the same dry DI used for Greybound.
4. Do not add an IR to the NAM render.
5. Compare it against a Greybound render with cab/IR disabled.

This repository treats the current AC30HWH NAM pack as an amp-head reference
without IR. Speaker and cabinet IR tests are useful, but they are a separate
comparison axis from the NAM amp-core match.

Fallback policy:

1. Use a **Full Rig / Combo** NAM capture only when no suitable amp-head capture
   is available.
2. Do not add an extra Greybound IR to the NAM side.
3. Treat all cab/mic differences as part of the reference mismatch.

Suggested first search target:

- Provider: TONE3000
- Candidate: https://www.tone3000.com/tones/ac30hwh-6580
- Category: VOX AC30
- Gear filter: Amp Head
- Platform: NAM
- Architecture: A2
- Tone family: clean or edge-of-breakup AC30/Top Boost

The `AC30HWH-6580` page exposes a useful capture grid in model names: Normal
Bright, Top Boost, and Hot Mode variants at gain positions 3, 5, 7, or Full,
with optional Top Cut. Treat that as semi-structured capture semantics, not as a
complete knob schema.

After manually downloading the pack, inspect it with:

```sh
make lab-inspect-nam-pack
```

This writes `manifests/ac30hwh-6580.json`, which is source-safe to commit. The
manifest records the 22 model files, local paths, NAM architecture, sample rate,
training metadata, parsed capture semantics, and the four priority models for
the first comparison pass. The `.nam` files themselves remain ignored by git.
