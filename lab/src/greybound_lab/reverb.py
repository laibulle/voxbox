from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from pathlib import Path

import numpy as np

from greybound_lab.audio import AudioBuffer


@dataclass(frozen=True)
class ReverbWindowMetrics:
    rms_dbfs: float
    energy_db: float
    centroid_hz: float


@dataclass(frozen=True)
class ReverbMetrics:
    sample_rate_hz: int
    duration_seconds: float
    dry_rms_dbfs: float
    wet_render_rms_dbfs: float
    wet_component_rms_dbfs: float
    wet_to_dry_db: float
    dry_peak_dbfs: float
    wet_render_peak_dbfs: float
    wet_component_peak_dbfs: float
    near_clip_count: int
    hard_clip_count: int
    dc_offset_dbfs: float
    onset_seconds: float
    early: ReverbWindowMetrics
    mid: ReverbWindowMetrics
    late: ReverbWindowMetrics
    decay_slope_db_per_s: float
    late_to_early_db: float
    verdict: str


def evaluate_reverb(dry: AudioBuffer, wet_render: AudioBuffer) -> ReverbMetrics:
    if dry.sample_rate != wet_render.sample_rate:
        raise ValueError(f"sample-rate mismatch: dry={dry.sample_rate} Hz wet={wet_render.sample_rate} Hz")

    sample_count = min(dry.samples.shape[0], wet_render.samples.shape[0])
    dry_samples = dry.samples[:sample_count].astype(np.float64)
    wet_samples = wet_render.samples[:sample_count].astype(np.float64)
    wet_component = wet_samples - dry_samples
    sample_rate = dry.sample_rate
    onset = _find_onset(dry_samples, sample_rate)

    early = _window_metrics(wet_component, sample_rate, onset, 0.0, 0.10)
    mid = _window_metrics(wet_component, sample_rate, onset, 0.10, 0.80)
    late = _window_metrics(wet_component, sample_rate, onset, 0.80, 3.00)
    decay_slope = _decay_slope_db_per_s(wet_component, sample_rate, onset + int(round(0.10 * sample_rate)))
    dry_rms = _rms_dbfs(dry_samples)
    wet_component_rms = _rms_dbfs(wet_component)
    near_clip_count = int(np.count_nonzero(np.abs(wet_samples) >= 0.98))
    hard_clip_count = int(np.count_nonzero(np.abs(wet_samples) >= 1.0))
    dc = float(abs(np.mean(wet_samples)))
    dc_offset_dbfs = _dbfs(dc)
    late_to_early = late.energy_db - early.energy_db
    verdict = _verdict(
        wet_to_dry_db=wet_component_rms - dry_rms,
        hard_clip_count=hard_clip_count,
        near_clip_count=near_clip_count,
        dc_offset_dbfs=dc_offset_dbfs,
        decay_slope_db_per_s=decay_slope,
        late_to_early_db=late_to_early,
    )

    return ReverbMetrics(
        sample_rate_hz=sample_rate,
        duration_seconds=float(sample_count / sample_rate),
        dry_rms_dbfs=dry_rms,
        wet_render_rms_dbfs=_rms_dbfs(wet_samples),
        wet_component_rms_dbfs=wet_component_rms,
        wet_to_dry_db=wet_component_rms - dry_rms,
        dry_peak_dbfs=_dbfs(float(np.max(np.abs(dry_samples))) if sample_count else 0.0),
        wet_render_peak_dbfs=_dbfs(float(np.max(np.abs(wet_samples))) if sample_count else 0.0),
        wet_component_peak_dbfs=_dbfs(float(np.max(np.abs(wet_component))) if sample_count else 0.0),
        near_clip_count=near_clip_count,
        hard_clip_count=hard_clip_count,
        dc_offset_dbfs=dc_offset_dbfs,
        onset_seconds=float(onset / sample_rate),
        early=early,
        mid=mid,
        late=late,
        decay_slope_db_per_s=decay_slope,
        late_to_early_db=late_to_early,
        verdict=verdict,
    )


def write_reverb_report(path: Path, *, dry_path: Path, wet_path: Path, metrics: ReverbMetrics) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "# Reverb Evaluation",
        "",
        f"- Dry/reference WAV: `{dry_path}`",
        f"- Wet render WAV: `{wet_path}`",
        f"- Verdict: `{metrics.verdict}`",
        f"- Sample rate: `{metrics.sample_rate_hz} Hz`",
        f"- Duration: `{metrics.duration_seconds:.2f} s`",
        f"- Detected onset: `{metrics.onset_seconds:.4f} s`",
        "",
        "## Levels",
        "",
        "| Metric | Value |",
        "| --- | ---: |",
        f"| Dry RMS | {metrics.dry_rms_dbfs:.2f} dBFS |",
        f"| Wet render RMS | {metrics.wet_render_rms_dbfs:.2f} dBFS |",
        f"| Reverb component RMS | {metrics.wet_component_rms_dbfs:.2f} dBFS |",
        f"| Reverb to dry | {metrics.wet_to_dry_db:.2f} dB |",
        f"| Dry peak | {metrics.dry_peak_dbfs:.2f} dBFS |",
        f"| Wet render peak | {metrics.wet_render_peak_dbfs:.2f} dBFS |",
        f"| Reverb component peak | {metrics.wet_component_peak_dbfs:.2f} dBFS |",
        f"| DC offset | {metrics.dc_offset_dbfs:.2f} dBFS |",
        f"| Near clips | {metrics.near_clip_count} samples |",
        f"| Hard clips | {metrics.hard_clip_count} samples |",
        "",
        "## Tail Windows",
        "",
        "| Window | RMS | Energy | Centroid |",
        "| --- | ---: | ---: | ---: |",
        _format_window("Early 0-100 ms", metrics.early),
        _format_window("Mid 100-800 ms", metrics.mid),
        _format_window("Late 800-3000 ms", metrics.late),
        "",
        "## Decay",
        "",
        f"- Decay slope: `{metrics.decay_slope_db_per_s:.2f} dB/s`",
        f"- Late-to-early energy: `{metrics.late_to_early_db:.2f} dB`",
        "",
        "## Interpretation",
        "",
        "This report evaluates the reverb as a designed effect rather than a clone. "
        "The wet component is computed as `wet_render - dry_reference`, so fixed render conditions are required.",
        "",
    ]
    path.write_text("\n".join(lines), encoding="utf-8")


def write_reverb_json(path: Path, metrics: ReverbMetrics) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(asdict(metrics), indent=2) + "\n", encoding="utf-8")


def _format_window(label: str, metrics: ReverbWindowMetrics) -> str:
    return f"| {label} | {metrics.rms_dbfs:.2f} dBFS | {metrics.energy_db:.2f} dB | {metrics.centroid_hz:.1f} Hz |"


def _find_onset(samples: np.ndarray, sample_rate: int) -> int:
    if samples.size == 0:
        return 0
    threshold = max(float(np.max(np.abs(samples))) * 0.05, 10.0 ** (-70.0 / 20.0))
    candidates = np.flatnonzero(np.abs(samples) >= threshold)
    return int(candidates[0]) if candidates.size else 0


def _window_metrics(samples: np.ndarray, sample_rate: int, onset: int, start_s: float, end_s: float) -> ReverbWindowMetrics:
    start = min(samples.shape[0], onset + int(round(start_s * sample_rate)))
    end = min(samples.shape[0], onset + int(round(end_s * sample_rate)))
    window = samples[start:end]
    if window.size == 0:
        return ReverbWindowMetrics(rms_dbfs=-300.0, energy_db=-300.0, centroid_hz=0.0)
    energy = float(np.sum(window * window))
    return ReverbWindowMetrics(
        rms_dbfs=_rms_dbfs(window),
        energy_db=float(10.0 * np.log10(energy + 1.0e-30)),
        centroid_hz=_centroid_hz(window, sample_rate),
    )


def _decay_slope_db_per_s(samples: np.ndarray, sample_rate: int, start: int) -> float:
    frame = max(1, int(round(0.050 * sample_rate)))
    hop = frame
    centers = []
    levels = []
    for index in range(start, samples.shape[0] - frame + 1, hop):
        chunk = samples[index : index + frame]
        rms = float(np.sqrt(np.mean(chunk * chunk) + 1.0e-30))
        if rms <= 10.0 ** (-120.0 / 20.0):
            continue
        centers.append((index + frame * 0.5) / sample_rate)
        levels.append(20.0 * np.log10(rms + 1.0e-30))
    if len(centers) < 3:
        return 0.0
    slope, _ = np.polyfit(np.asarray(centers), np.asarray(levels), 1)
    return float(slope)


def _centroid_hz(samples: np.ndarray, sample_rate: int) -> float:
    if samples.size < 16:
        return 0.0
    window = samples * np.hanning(samples.size)
    spectrum = np.abs(np.fft.rfft(window))
    frequencies = np.fft.rfftfreq(samples.size, 1.0 / sample_rate)
    return float(np.sum(frequencies * spectrum) / (np.sum(spectrum) + 1.0e-30))


def _rms_dbfs(samples: np.ndarray) -> float:
    if samples.size == 0:
        return -300.0
    return _dbfs(float(np.sqrt(np.mean(samples * samples) + 1.0e-30)))


def _dbfs(value: float) -> float:
    return float(20.0 * np.log10(max(abs(value), 1.0e-30)))


def _verdict(
    *,
    wet_to_dry_db: float,
    hard_clip_count: int,
    near_clip_count: int,
    dc_offset_dbfs: float,
    decay_slope_db_per_s: float,
    late_to_early_db: float,
) -> str:
    if hard_clip_count > 0 or near_clip_count > 0 or dc_offset_dbfs > -60.0:
        return "severe"
    if wet_to_dry_db < -55.0:
        return "warning"
    if decay_slope_db_per_s >= -0.5 and late_to_early_db > -6.0:
        return "warning"
    return "pass"
