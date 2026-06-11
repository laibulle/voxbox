from __future__ import annotations

from pathlib import Path

import numpy as np

from greybound_lab.audio import AudioBuffer
from greybound_lab.reverb import evaluate_reverb, write_reverb_json, write_reverb_report


def test_evaluate_reverb_reports_tail_and_clean_health(tmp_path: Path) -> None:
    sample_rate = 48_000
    dry = np.zeros(sample_rate * 3, dtype=np.float32)
    dry[0] = 0.25
    time = np.arange(dry.shape[0], dtype=np.float64) / sample_rate
    tail = np.zeros_like(time)
    start = int(round(0.080 * sample_rate))
    tail[start:] = (
        0.02
        * np.exp(-(time[start:] - time[start]) * 1.6)
        * np.sin(2.0 * np.pi * 480.0 * (time[start:] - time[start]))
    )
    wet = (dry + tail).astype(np.float32)

    metrics = evaluate_reverb(
        AudioBuffer(Path("dry.wav"), sample_rate, dry),
        AudioBuffer(Path("wet.wav"), sample_rate, wet),
    )

    assert metrics.verdict == "pass"
    assert metrics.wet_component_rms_dbfs < metrics.wet_component_peak_dbfs
    assert metrics.wet_to_dry_db > 0.0
    assert metrics.early.rms_dbfs > metrics.late.rms_dbfs
    assert metrics.decay_slope_db_per_s < -1.0
    assert metrics.near_clip_count == 0
    assert metrics.hard_clip_count == 0

    report = tmp_path / "reverb.md"
    data = tmp_path / "reverb.json"
    write_reverb_report(report, dry_path=Path("dry.wav"), wet_path=Path("wet.wav"), metrics=metrics)
    write_reverb_json(data, metrics)
    assert "Reverb Evaluation" in report.read_text()
    assert '"verdict": "pass"' in data.read_text()


def test_evaluate_reverb_flags_clipping() -> None:
    sample_rate = 48_000
    dry = np.zeros(sample_rate, dtype=np.float32)
    wet = np.ones(sample_rate, dtype=np.float32)

    metrics = evaluate_reverb(
        AudioBuffer(Path("dry.wav"), sample_rate, dry),
        AudioBuffer(Path("wet.wav"), sample_rate, wet),
    )

    assert metrics.verdict == "severe"
    assert metrics.hard_clip_count == sample_rate
