from __future__ import annotations

import numpy as np

from greybound_lab.metrics import align_by_latency, compare_signals, estimate_latency, sag_metrics
from greybound_lab.segments import SegmentSpec


def test_estimates_positive_candidate_latency() -> None:
    sample_rate = 48_000
    reference = _sine(sample_rate, 0.5)
    candidate = np.concatenate([np.zeros(240), reference])

    latency = estimate_latency(candidate, reference, sample_rate, max_lag_ms=20)

    assert latency == 240


def test_alignment_and_gain_reduce_residual() -> None:
    sample_rate = 48_000
    reference = _sine(sample_rate, 0.5)
    candidate = np.concatenate([np.zeros(120), reference * 0.5])

    metrics = compare_signals(candidate, reference, sample_rate, max_lag_ms=20)

    assert metrics.latency_samples == 120
    assert abs(metrics.gain_db - 6.0206) < 0.02
    assert metrics.null_relative_db < -100.0


def test_aligns_negative_candidate_latency() -> None:
    reference = np.concatenate([np.zeros(10), np.arange(5, dtype=np.float64)])
    candidate = np.arange(5, dtype=np.float64)

    aligned_candidate, aligned_reference = align_by_latency(candidate, reference, -10)

    np.testing.assert_array_equal(aligned_candidate, aligned_reference)


def test_segment_attack_metrics_are_reported() -> None:
    sample_rate = 48_000
    reference = np.concatenate([np.linspace(0.0, 1.0, 240), np.ones(2_000)])
    candidate = np.concatenate([np.linspace(0.0, 1.0, 480), np.ones(1_760)])

    metrics = compare_signals(
        candidate,
        reference,
        sample_rate,
        segments=[SegmentSpec(name="attack", start_s=0.0, end_s=0.04, kind="attack")],
    )

    assert len(metrics.segments) == 1
    assert metrics.segments[0].attack is not None
    assert metrics.segments[0].attack.rise_time_delta_ms > 0.0


def test_segment_harmonic_metrics_are_reported() -> None:
    sample_rate = 48_000
    time = np.arange(sample_rate, dtype=np.float64) / sample_rate
    reference = np.sin(2.0 * np.pi * 1_000.0 * time)
    candidate = reference + 0.1 * np.sin(2.0 * np.pi * 2_000.0 * time)

    metrics = compare_signals(
        candidate,
        reference,
        sample_rate,
        segments=[
            SegmentSpec(
                name="harmonic",
                start_s=0.1,
                end_s=0.8,
                kind="harmonic",
                fundamental_hz=1_000.0,
            )
        ],
    )

    harmonics = metrics.segments[0].harmonics
    assert harmonics is not None
    assert harmonics.fundamental_hz == 1_000.0
    assert harmonics.h2_delta_db is not None
    assert harmonics.h2_delta_db > 20.0


def test_segment_aliasing_and_sag_metrics_are_reported() -> None:
    sample_rate = 48_000
    time = np.arange(sample_rate, dtype=np.float64) / sample_rate
    reference = np.sin(2.0 * np.pi * 500.0 * time)
    candidate = reference + 0.01 * np.sin(2.0 * np.pi * 20_000.0 * time)

    aliasing = compare_signals(
        candidate,
        reference,
        sample_rate,
        segments=[SegmentSpec(name="aliasing", start_s=0.0, end_s=1.0, kind="aliasing")],
    ).segments[0].aliasing

    assert aliasing is not None
    assert aliasing.candidate_high_band_dbfs > aliasing.reference_high_band_dbfs

    burst = np.concatenate([np.ones(2_400), np.ones(9_600) * 0.5, np.ones(2_400) * 0.8])
    sag = sag_metrics(burst, np.ones_like(burst), sample_rate)

    assert sag.candidate_drop_db < sag.reference_drop_db


def _sine(sample_rate: int, seconds: float) -> np.ndarray:
    time = np.arange(int(sample_rate * seconds), dtype=np.float64) / sample_rate
    return np.sin(2.0 * np.pi * 997.0 * time)
