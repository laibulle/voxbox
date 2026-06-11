from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from scipy.io import wavfile


@dataclass(frozen=True)
class StimulusFile:
    wav_path: Path
    markers_path: Path


def generate_stimuli(output_dir: Path, sample_rate_hz: int = 44_100) -> list[StimulusFile]:
    output_dir.mkdir(parents=True, exist_ok=True)
    generated = [
        _write_sine_level_sweep(output_dir, sample_rate_hz),
        _write_frequency_response_sweep(output_dir, sample_rate_hz),
        _write_nonlinear_transfer_probe(output_dir, sample_rate_hz),
        _write_two_tone(output_dir, sample_rate_hz),
        _write_aliasing_sweep(output_dir, sample_rate_hz),
        _write_sag_bursts(output_dir, sample_rate_hz),
        _write_pluck_attacks(output_dir, sample_rate_hz),
        _write_reverb_diagnostics(output_dir, sample_rate_hz),
    ]
    return generated


def _write_sine_level_sweep(output_dir: Path, sample_rate_hz: int) -> StimulusFile:
    levels_db = [-36, -24, -18, -12, -6]
    frequencies_hz = [100.0, 1_000.0, 5_000.0]
    segment_seconds = 1.0
    gap_seconds = 0.08
    chunks: list[np.ndarray] = []
    segments = []
    cursor = 0.0
    for frequency_hz in frequencies_hz:
        for level_db in levels_db:
            start = cursor
            tone = _fade(_sine(frequency_hz, level_db, segment_seconds, sample_rate_hz), sample_rate_hz)
            chunks.append(tone)
            cursor += segment_seconds
            segments.append(
                {
                    "name": f"sine_{int(frequency_hz)}hz_{level_db}dbfs",
                    "kind": "harmonic",
                    "start_s": round(start + 0.15, 6),
                    "end_s": round(cursor - 0.10, 6),
                    "fundamental_hz": frequency_hz,
                }
            )
            chunks.append(np.zeros(int(round(gap_seconds * sample_rate_hz)), dtype=np.float32))
            cursor += gap_seconds
    wav_path = output_dir / "sine-level-sweep.wav"
    markers_path = output_dir / "sine-level-sweep.markers.json"
    return _write_pair(wav_path, markers_path, np.concatenate(chunks), sample_rate_hz, segments)


def _write_frequency_response_sweep(output_dir: Path, sample_rate_hz: int) -> StimulusFile:
    chunks: list[np.ndarray] = []
    segments = []
    cursor = 0.0
    for name, start_hz, end_hz, seconds in [
        ("transfer_low_mid_sweep", 40.0, 2_000.0, 3.0),
        ("transfer_presence_air_sweep", 2_000.0, 18_000.0, 3.0),
    ]:
        start = cursor
        chunks.append(_fade(_chirp(start_hz, end_hz, -18.0, seconds, sample_rate_hz), sample_rate_hz))
        cursor += seconds
        segments.append(
            {
                "name": name,
                "kind": "general",
                "start_s": round(start + 0.15, 6),
                "end_s": round(cursor - 0.10, 6),
                "notes": "Frequency-response and phase/group-delay probe.",
            }
        )
        chunks.append(np.zeros(int(round(0.10 * sample_rate_hz)), dtype=np.float32))
        cursor += 0.10
    wav_path = output_dir / "frequency-response-sweep.wav"
    markers_path = output_dir / "frequency-response-sweep.markers.json"
    return _write_pair(wav_path, markers_path, np.concatenate(chunks), sample_rate_hz, segments)


def _write_nonlinear_transfer_probe(output_dir: Path, sample_rate_hz: int) -> StimulusFile:
    chunks: list[np.ndarray] = []
    segments = []
    cursor = 0.0
    for level_db in [-36.0, -24.0, -12.0, -6.0]:
        start = cursor
        chunks.append(_fade(_sine(1_000.0, level_db, 0.75, sample_rate_hz), sample_rate_hz))
        cursor += 0.75
        segments.append(
            {
                "name": f"transfer_1khz_{int(level_db)}dbfs",
                "kind": "harmonic",
                "start_s": round(start + 0.10, 6),
                "end_s": round(cursor - 0.08, 6),
                "fundamental_hz": 1_000.0,
                "notes": "Level-dependent nonlinear transfer probe.",
            }
        )
        chunks.append(np.zeros(int(round(0.08 * sample_rate_hz)), dtype=np.float32))
        cursor += 0.08
    start = cursor
    ramp = _amplitude_ramp_sine(1_000.0, -42.0, -3.0, 3.0, sample_rate_hz)
    chunks.append(_fade(ramp, sample_rate_hz))
    cursor += 3.0
    segments.append(
        {
            "name": "transfer_1khz_amplitude_ramp",
            "kind": "general",
            "start_s": round(start + 0.10, 6),
            "end_s": round(cursor - 0.10, 6),
            "notes": "Continuous input-level ramp for nonlinear transfer curve diagnostics.",
        }
    )
    wav_path = output_dir / "nonlinear-transfer-probe.wav"
    markers_path = output_dir / "nonlinear-transfer-probe.markers.json"
    return _write_pair(wav_path, markers_path, np.concatenate(chunks), sample_rate_hz, segments)


def _write_two_tone(output_dir: Path, sample_rate_hz: int) -> StimulusFile:
    specs = [
        ("two_tone_440_550", 440.0, 550.0, -18.0),
        ("two_tone_997_1499", 997.0, 1_499.0, -18.0),
        ("two_tone_hot_997_1499", 997.0, 1_499.0, -9.0),
    ]
    chunks: list[np.ndarray] = []
    segments = []
    cursor = 0.0
    for name, first_hz, second_hz, level_db in specs:
        start = cursor
        duration = 1.4
        time = _time(duration, sample_rate_hz)
        amplitude = _db_to_linear(level_db) / 2.0
        tone = amplitude * (np.sin(2.0 * np.pi * first_hz * time) + np.sin(2.0 * np.pi * second_hz * time))
        chunks.append(_fade(tone.astype(np.float32), sample_rate_hz))
        cursor += duration
        segments.append(
            {
                "name": name,
                "kind": "imd",
                "start_s": round(start + 0.15, 6),
                "end_s": round(cursor - 0.10, 6),
                "first_hz": first_hz,
                "second_hz": second_hz,
                "notes": "Two-tone intermodulation stimulus.",
            }
        )
        chunks.append(np.zeros(int(round(0.1 * sample_rate_hz)), dtype=np.float32))
        cursor += 0.1
    wav_path = output_dir / "two-tone-imd.wav"
    markers_path = output_dir / "two-tone-imd.markers.json"
    return _write_pair(wav_path, markers_path, np.concatenate(chunks), sample_rate_hz, segments)


def _write_aliasing_sweep(output_dir: Path, sample_rate_hz: int) -> StimulusFile:
    chunks: list[np.ndarray] = []
    segments = []
    cursor = 0.0
    high_sines = [(7_000.0, -9.0), (10_000.0, -9.0), (14_000.0, -12.0)]
    for frequency_hz, level_db in high_sines:
        start = cursor
        duration = 1.0
        chunks.append(_fade(_sine(frequency_hz, level_db, duration, sample_rate_hz), sample_rate_hz))
        cursor += duration
        segments.append(
            {
                "name": f"aliasing_{int(frequency_hz)}hz_{int(level_db)}dbfs",
                "kind": "aliasing",
                "start_s": round(start + 0.10, 6),
                "end_s": round(cursor - 0.05, 6),
                "notes": "High-frequency sine for nonlinear aliasing stress.",
            }
        )
        chunks.append(np.zeros(int(round(0.08 * sample_rate_hz)), dtype=np.float32))
        cursor += 0.08
    start = cursor
    duration = 3.0
    sweep = _fade(_chirp(2_000.0, 18_000.0, -12.0, duration, sample_rate_hz), sample_rate_hz)
    chunks.append(sweep)
    cursor += duration
    segments.append(
        {
            "name": "aliasing_2k_18k_sweep",
            "kind": "aliasing",
            "start_s": round(start + 0.20, 6),
            "end_s": round(cursor - 0.10, 6),
            "notes": "Rising sweep to expose folded residuals after nonlinear processing.",
        }
    )
    wav_path = output_dir / "aliasing-stress.wav"
    markers_path = output_dir / "aliasing-stress.markers.json"
    return _write_pair(wav_path, markers_path, np.concatenate(chunks), sample_rate_hz, segments)


def _write_sag_bursts(output_dir: Path, sample_rate_hz: int) -> StimulusFile:
    chunks: list[np.ndarray] = []
    segments = []
    cursor = 0.0
    for index, level_db in enumerate([-9.0, -6.0, -6.0]):
        start = cursor
        burst = _sine(110.0, level_db, 0.20, sample_rate_hz)
        sustain = _sine(110.0, level_db - 3.0, 0.45, sample_rate_hz)
        recovery = np.zeros(int(round(0.45 * sample_rate_hz)), dtype=np.float32)
        chunks.extend([_fade(burst, sample_rate_hz), sustain, recovery])
        cursor += 1.10
        segments.append(
            {
                "name": f"sag_burst_{index + 1}",
                "kind": "sag",
                "start_s": round(start, 6),
                "end_s": round(cursor - 0.15, 6),
                "notes": "Burst plus recovery window for supply/compression dynamics.",
            }
        )
    wav_path = output_dir / "sag-bursts.wav"
    markers_path = output_dir / "sag-bursts.markers.json"
    return _write_pair(wav_path, markers_path, np.concatenate(chunks), sample_rate_hz, segments)


def _write_pluck_attacks(output_dir: Path, sample_rate_hz: int) -> StimulusFile:
    chunks: list[np.ndarray] = []
    segments = []
    cursor = 0.0
    for index, frequency_hz in enumerate([110.0, 220.0, 440.0, 880.0]):
        start = cursor
        chunks.append(_pluck(frequency_hz, -12.0, 0.75, sample_rate_hz))
        cursor += 0.75
        segments.append(
            {
                "name": f"pluck_{index + 1}_{int(frequency_hz)}hz",
                "kind": "attack",
                "start_s": round(start, 6),
                "end_s": round(start + 0.18, 6),
                "notes": "Synthetic picked transient for attack timing and overshoot.",
            }
        )
        chunks.append(np.zeros(int(round(0.12 * sample_rate_hz)), dtype=np.float32))
        cursor += 0.12
    wav_path = output_dir / "pluck-attacks.wav"
    markers_path = output_dir / "pluck-attacks.markers.json"
    return _write_pair(wav_path, markers_path, np.concatenate(chunks), sample_rate_hz, segments)


def _write_reverb_diagnostics(output_dir: Path, sample_rate_hz: int) -> StimulusFile:
    chunks: list[np.ndarray] = []
    segments = []
    cursor = 0.0

    for index, level_db in enumerate([-18.0, -12.0, -6.0]):
        start = cursor
        impulse = np.zeros(int(round(3.0 * sample_rate_hz)), dtype=np.float32)
        impulse[0] = _db_to_linear(level_db)
        chunks.append(impulse)
        cursor += 3.0
        segments.append(
            {
                "name": f"reverb_impulse_{index + 1}_{int(level_db)}dbfs",
                "kind": "reverb",
                "start_s": round(start, 6),
                "end_s": round(cursor, 6),
                "notes": "Impulse plus silence for early/mid/late reverb tail diagnostics.",
            }
        )

    for index, frequency_hz in enumerate([110.0, 220.0, 440.0]):
        start = cursor
        pluck = np.zeros(int(round(3.0 * sample_rate_hz)), dtype=np.float32)
        source = _pluck(frequency_hz, -12.0, 0.35, sample_rate_hz)
        pluck[: source.shape[0]] = source
        chunks.append(pluck)
        cursor += 3.0
        segments.append(
            {
                "name": f"reverb_pluck_{index + 1}_{int(frequency_hz)}hz",
                "kind": "reverb",
                "start_s": round(start, 6),
                "end_s": round(cursor, 6),
                "notes": "Pluck plus silence for spring splash and decay diagnostics.",
            }
        )

    wav_path = output_dir / "reverb-diagnostics.wav"
    markers_path = output_dir / "reverb-diagnostics.markers.json"
    return _write_pair(wav_path, markers_path, np.concatenate(chunks), sample_rate_hz, segments)


def _write_pair(
    wav_path: Path,
    markers_path: Path,
    samples: np.ndarray,
    sample_rate_hz: int,
    segments: list[dict],
) -> StimulusFile:
    samples = np.clip(samples, -0.999, 0.999).astype(np.float32)
    wavfile.write(wav_path, sample_rate_hz, samples)
    markers = {
        "schema_version": 1,
        "source": str(wav_path),
        "notes": "Generated by greybound-lab generate-stimuli.",
        "segments": segments,
    }
    markers_path.write_text(json.dumps(markers, indent=2) + "\n", encoding="utf-8")
    return StimulusFile(wav_path=wav_path, markers_path=markers_path)


def _time(seconds: float, sample_rate_hz: int) -> np.ndarray:
    return np.arange(int(round(seconds * sample_rate_hz)), dtype=np.float64) / sample_rate_hz


def _sine(frequency_hz: float, level_dbfs: float, seconds: float, sample_rate_hz: int) -> np.ndarray:
    return (_db_to_linear(level_dbfs) * np.sin(2.0 * np.pi * frequency_hz * _time(seconds, sample_rate_hz))).astype(
        np.float32
    )


def _chirp(start_hz: float, end_hz: float, level_dbfs: float, seconds: float, sample_rate_hz: int) -> np.ndarray:
    time = _time(seconds, sample_rate_hz)
    sweep = np.sin(2.0 * np.pi * (start_hz * time + 0.5 * (end_hz - start_hz) * time * time / seconds))
    return (_db_to_linear(level_dbfs) * sweep).astype(np.float32)


def _amplitude_ramp_sine(
    frequency_hz: float,
    start_dbfs: float,
    end_dbfs: float,
    seconds: float,
    sample_rate_hz: int,
) -> np.ndarray:
    time = _time(seconds, sample_rate_hz)
    envelope_db = np.linspace(start_dbfs, end_dbfs, time.shape[0])
    envelope = np.power(10.0, envelope_db / 20.0)
    return (envelope * np.sin(2.0 * np.pi * frequency_hz * time)).astype(np.float32)


def _pluck(frequency_hz: float, level_dbfs: float, seconds: float, sample_rate_hz: int) -> np.ndarray:
    time = _time(seconds, sample_rate_hz)
    carrier = np.sin(2.0 * np.pi * frequency_hz * time)
    transient = np.exp(-time * 90.0) * np.sin(2.0 * np.pi * 4_000.0 * time)
    body = np.exp(-time * 5.0) * carrier
    return (_db_to_linear(level_dbfs) * (body + 0.35 * transient)).astype(np.float32)


def _fade(samples: np.ndarray, sample_rate_hz: int) -> np.ndarray:
    length = min(samples.shape[0] // 2, int(round(0.010 * sample_rate_hz)))
    if length <= 1:
        return samples
    fade_in = np.linspace(0.0, 1.0, length, dtype=np.float32)
    fade_out = np.linspace(1.0, 0.0, length, dtype=np.float32)
    faded = samples.copy()
    faded[:length] *= fade_in
    faded[-length:] *= fade_out
    return faded


def _db_to_linear(db: float) -> float:
    return float(10.0 ** (db / 20.0))
