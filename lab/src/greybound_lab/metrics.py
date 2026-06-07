from __future__ import annotations

from dataclasses import dataclass

import numpy as np
from scipy import signal

from greybound_lab.segments import SegmentSpec


EPSILON = 1.0e-12


@dataclass(frozen=True)
class SignalStats:
    rms_dbfs: float
    peak_dbfs: float
    crest_db: float


@dataclass(frozen=True)
class ComparisonMetrics:
    sample_rate_hz: int
    candidate_samples: int
    reference_samples: int
    compared_samples: int
    latency_samples: int
    latency_ms: float
    gain_db: float
    candidate: SignalStats
    reference: SignalStats
    aligned_candidate: SignalStats
    aligned_reference: SignalStats
    null_rms_dbfs: float
    null_relative_db: float
    log_spectral_distance_db: float
    envelope_error_db: float
    segments: tuple[SegmentComparisonMetrics, ...] = ()


@dataclass(frozen=True)
class SegmentComparisonMetrics:
    name: str
    kind: str
    start_s: float
    end_s: float
    samples: int
    local_gain_db: float
    null_relative_db: float
    log_spectral_distance_db: float
    envelope_error_db: float
    attack: AttackMetrics | None = None
    harmonics: HarmonicMetrics | None = None
    aliasing: AliasingMetrics | None = None
    sag: SagMetrics | None = None


@dataclass(frozen=True)
class AttackMetrics:
    candidate_peak_time_ms: float
    reference_peak_time_ms: float
    peak_time_delta_ms: float
    candidate_rise_time_ms: float
    reference_rise_time_ms: float
    rise_time_delta_ms: float
    overshoot_delta_db: float


@dataclass(frozen=True)
class HarmonicMetrics:
    fundamental_hz: float
    candidate_thd_db: float
    reference_thd_db: float
    thd_delta_db: float
    h2_delta_db: float | None
    h3_delta_db: float | None
    h4_delta_db: float | None
    h5_delta_db: float | None


@dataclass(frozen=True)
class AliasingMetrics:
    candidate_high_band_dbfs: float
    reference_high_band_dbfs: float
    high_band_delta_db: float
    residual_high_band_dbfs: float


@dataclass(frozen=True)
class SagMetrics:
    candidate_drop_db: float
    reference_drop_db: float
    drop_delta_db: float
    candidate_recovery_db: float
    reference_recovery_db: float
    recovery_delta_db: float


def compare_signals(
    candidate: np.ndarray,
    reference: np.ndarray,
    sample_rate_hz: int,
    max_lag_ms: float = 100.0,
    segments: list[SegmentSpec] | None = None,
) -> ComparisonMetrics:
    candidate = np.asarray(candidate, dtype=np.float64)
    reference = np.asarray(reference, dtype=np.float64)
    latency_samples = estimate_latency(candidate, reference, sample_rate_hz, max_lag_ms)
    aligned_candidate, aligned_reference = align_by_latency(candidate, reference, latency_samples)
    gain = optimal_gain(aligned_candidate, aligned_reference)
    corrected_candidate = aligned_candidate * gain
    residual = corrected_candidate - aligned_reference

    candidate_stats = signal_stats(candidate)
    reference_stats = signal_stats(reference)
    corrected_stats = signal_stats(corrected_candidate)
    aligned_reference_stats = signal_stats(aligned_reference)
    residual_rms = rms(residual)
    reference_rms = rms(aligned_reference)

    return ComparisonMetrics(
        sample_rate_hz=sample_rate_hz,
        candidate_samples=int(candidate.shape[0]),
        reference_samples=int(reference.shape[0]),
        compared_samples=int(aligned_reference.shape[0]),
        latency_samples=int(latency_samples),
        latency_ms=1000.0 * latency_samples / sample_rate_hz,
        gain_db=linear_to_db(gain),
        candidate=candidate_stats,
        reference=reference_stats,
        aligned_candidate=corrected_stats,
        aligned_reference=aligned_reference_stats,
        null_rms_dbfs=linear_to_db(residual_rms),
        null_relative_db=linear_to_db(residual_rms / max(reference_rms, EPSILON)),
        log_spectral_distance_db=log_spectral_distance(corrected_candidate, aligned_reference, sample_rate_hz),
        envelope_error_db=envelope_error(corrected_candidate, aligned_reference),
        segments=tuple(
            compare_segment(corrected_candidate, aligned_reference, sample_rate_hz, segment)
            for segment in (segments or [])
        ),
    )


def compare_segment(
    corrected_candidate: np.ndarray,
    aligned_reference: np.ndarray,
    sample_rate_hz: int,
    segment: SegmentSpec,
) -> SegmentComparisonMetrics:
    start = max(0, int(round(segment.start_s * sample_rate_hz)))
    end = min(corrected_candidate.shape[0], aligned_reference.shape[0], int(round(segment.end_s * sample_rate_hz)))
    if end <= start:
        raise ValueError(f"segment {segment.name} is outside the compared audio range")
    candidate = corrected_candidate[start:end]
    reference = aligned_reference[start:end]
    local_gain = optimal_gain(candidate, reference)
    locally_corrected = candidate * local_gain
    residual = locally_corrected - reference
    reference_rms = rms(reference)

    kind = segment.kind.lower()
    return SegmentComparisonMetrics(
        name=segment.name,
        kind=segment.kind,
        start_s=segment.start_s,
        end_s=segment.end_s,
        samples=int(reference.shape[0]),
        local_gain_db=linear_to_db(local_gain),
        null_relative_db=linear_to_db(rms(residual) / max(reference_rms, EPSILON)),
        log_spectral_distance_db=log_spectral_distance(locally_corrected, reference, sample_rate_hz),
        envelope_error_db=envelope_error(locally_corrected, reference),
        attack=attack_metrics(locally_corrected, reference, sample_rate_hz) if kind == "attack" else None,
        harmonics=harmonic_metrics(
            locally_corrected,
            reference,
            sample_rate_hz,
            segment.fundamental_hz,
        )
        if kind == "harmonic"
        else None,
        aliasing=aliasing_metrics(locally_corrected, reference, sample_rate_hz) if kind == "aliasing" else None,
        sag=sag_metrics(locally_corrected, reference, sample_rate_hz) if kind == "sag" else None,
    )


def estimate_latency(
    candidate: np.ndarray,
    reference: np.ndarray,
    sample_rate_hz: int,
    max_lag_ms: float,
) -> int:
    max_lag = int(round(sample_rate_hz * max_lag_ms / 1000.0))
    candidate_window = _analysis_window(candidate, sample_rate_hz)
    reference_window = _analysis_window(reference, sample_rate_hz)
    length = min(candidate_window.shape[0], reference_window.shape[0])
    if length < 8:
        raise ValueError("signals are too short to estimate latency")
    candidate_window = candidate_window[:length] - np.mean(candidate_window[:length])
    reference_window = reference_window[:length] - np.mean(reference_window[:length])
    correlation = signal.correlate(candidate_window, reference_window, mode="full", method="fft")
    lags = signal.correlation_lags(candidate_window.shape[0], reference_window.shape[0], mode="full")
    mask = np.abs(lags) <= max_lag
    if not np.any(mask):
        raise ValueError("no correlation lags available")
    return int(lags[mask][np.argmax(np.abs(correlation[mask]))])


def align_by_latency(
    candidate: np.ndarray,
    reference: np.ndarray,
    latency_samples: int,
) -> tuple[np.ndarray, np.ndarray]:
    if latency_samples >= 0:
        candidate_start = latency_samples
        reference_start = 0
    else:
        candidate_start = 0
        reference_start = -latency_samples
    length = min(candidate.shape[0] - candidate_start, reference.shape[0] - reference_start)
    if length <= 0:
        raise ValueError("latency alignment produced no overlapping samples")
    return (
        candidate[candidate_start : candidate_start + length],
        reference[reference_start : reference_start + length],
    )


def optimal_gain(candidate: np.ndarray, reference: np.ndarray) -> float:
    denominator = float(np.dot(candidate, candidate))
    if denominator <= EPSILON:
        return 1.0
    return float(np.dot(reference, candidate) / denominator)


def signal_stats(samples: np.ndarray) -> SignalStats:
    sample_rms = rms(samples)
    sample_peak = float(np.max(np.abs(samples))) if samples.size else 0.0
    return SignalStats(
        rms_dbfs=linear_to_db(sample_rms),
        peak_dbfs=linear_to_db(sample_peak),
        crest_db=linear_to_db(sample_peak / max(sample_rms, EPSILON)),
    )


def log_spectral_distance(candidate: np.ndarray, reference: np.ndarray, sample_rate_hz: int) -> float:
    if candidate.shape[0] < 32 or reference.shape[0] < 32:
        return 0.0
    nperseg = min(4096, max(256, _largest_power_of_two(candidate.shape[0] // 8)))
    _, _, candidate_stft = signal.stft(candidate, fs=sample_rate_hz, nperseg=nperseg, noverlap=nperseg // 2)
    _, _, reference_stft = signal.stft(reference, fs=sample_rate_hz, nperseg=nperseg, noverlap=nperseg // 2)
    candidate_db = 20.0 * np.log10(np.abs(candidate_stft) + EPSILON)
    reference_db = 20.0 * np.log10(np.abs(reference_stft) + EPSILON)
    return float(np.sqrt(np.mean(np.square(candidate_db - reference_db))))


def envelope_error(candidate: np.ndarray, reference: np.ndarray) -> float:
    if candidate.shape[0] < 4 or reference.shape[0] < 4:
        return 0.0
    candidate_env = np.abs(signal.hilbert(candidate))
    reference_env = np.abs(signal.hilbert(reference))
    error = rms(candidate_env - reference_env)
    return linear_to_db(error / max(rms(reference_env), EPSILON))


def rms(samples: np.ndarray) -> float:
    if samples.size == 0:
        return 0.0
    return float(np.sqrt(np.mean(np.square(samples))))


def linear_to_db(value: float) -> float:
    return 20.0 * float(np.log10(max(abs(value), EPSILON)))


def attack_metrics(candidate: np.ndarray, reference: np.ndarray, sample_rate_hz: int) -> AttackMetrics:
    candidate_env = _smooth_envelope(candidate, sample_rate_hz)
    reference_env = _smooth_envelope(reference, sample_rate_hz)
    candidate_peak_time = _peak_time_ms(candidate_env, sample_rate_hz)
    reference_peak_time = _peak_time_ms(reference_env, sample_rate_hz)
    candidate_rise_time = _rise_time_ms(candidate_env, sample_rate_hz)
    reference_rise_time = _rise_time_ms(reference_env, sample_rate_hz)
    return AttackMetrics(
        candidate_peak_time_ms=candidate_peak_time,
        reference_peak_time_ms=reference_peak_time,
        peak_time_delta_ms=candidate_peak_time - reference_peak_time,
        candidate_rise_time_ms=candidate_rise_time,
        reference_rise_time_ms=reference_rise_time,
        rise_time_delta_ms=candidate_rise_time - reference_rise_time,
        overshoot_delta_db=_overshoot_db(candidate_env) - _overshoot_db(reference_env),
    )


def harmonic_metrics(
    candidate: np.ndarray,
    reference: np.ndarray,
    sample_rate_hz: int,
    fundamental_hz: float | None,
) -> HarmonicMetrics:
    f0 = fundamental_hz or _estimate_fundamental(reference, sample_rate_hz)
    candidate_harmonics = _harmonic_levels(candidate, sample_rate_hz, f0)
    reference_harmonics = _harmonic_levels(reference, sample_rate_hz, f0)
    candidate_thd = _thd_db(candidate_harmonics)
    reference_thd = _thd_db(reference_harmonics)
    return HarmonicMetrics(
        fundamental_hz=f0,
        candidate_thd_db=candidate_thd,
        reference_thd_db=reference_thd,
        thd_delta_db=candidate_thd - reference_thd,
        h2_delta_db=_harmonic_delta(candidate_harmonics, reference_harmonics, 2),
        h3_delta_db=_harmonic_delta(candidate_harmonics, reference_harmonics, 3),
        h4_delta_db=_harmonic_delta(candidate_harmonics, reference_harmonics, 4),
        h5_delta_db=_harmonic_delta(candidate_harmonics, reference_harmonics, 5),
    )


def aliasing_metrics(candidate: np.ndarray, reference: np.ndarray, sample_rate_hz: int) -> AliasingMetrics:
    residual = candidate - reference
    candidate_high = _band_rms(candidate, sample_rate_hz, 18_000.0, sample_rate_hz / 2.0)
    reference_high = _band_rms(reference, sample_rate_hz, 18_000.0, sample_rate_hz / 2.0)
    residual_high = _band_rms(residual, sample_rate_hz, 18_000.0, sample_rate_hz / 2.0)
    return AliasingMetrics(
        candidate_high_band_dbfs=linear_to_db(candidate_high),
        reference_high_band_dbfs=linear_to_db(reference_high),
        high_band_delta_db=linear_to_db(candidate_high / max(reference_high, EPSILON)),
        residual_high_band_dbfs=linear_to_db(residual_high),
    )


def sag_metrics(candidate: np.ndarray, reference: np.ndarray, sample_rate_hz: int) -> SagMetrics:
    candidate_drop, candidate_recovery = _sag_shape(candidate, sample_rate_hz)
    reference_drop, reference_recovery = _sag_shape(reference, sample_rate_hz)
    return SagMetrics(
        candidate_drop_db=candidate_drop,
        reference_drop_db=reference_drop,
        drop_delta_db=candidate_drop - reference_drop,
        candidate_recovery_db=candidate_recovery,
        reference_recovery_db=reference_recovery,
        recovery_delta_db=candidate_recovery - reference_recovery,
    )


def _analysis_window(samples: np.ndarray, sample_rate_hz: int) -> np.ndarray:
    max_samples = min(samples.shape[0], sample_rate_hz * 20)
    return samples[:max_samples]


def _largest_power_of_two(value: int) -> int:
    if value <= 1:
        return 1
    return 1 << (value.bit_length() - 1)


def _smooth_envelope(samples: np.ndarray, sample_rate_hz: int) -> np.ndarray:
    envelope = np.abs(signal.hilbert(samples))
    window = max(1, int(round(sample_rate_hz * 0.001)))
    if window <= 1:
        return envelope
    kernel = np.ones(window) / window
    return np.convolve(envelope, kernel, mode="same")


def _peak_time_ms(envelope: np.ndarray, sample_rate_hz: int) -> float:
    if envelope.size == 0:
        return 0.0
    return 1000.0 * int(np.argmax(envelope)) / sample_rate_hz


def _rise_time_ms(envelope: np.ndarray, sample_rate_hz: int) -> float:
    if envelope.size == 0:
        return 0.0
    peak = float(np.max(envelope))
    if peak <= EPSILON:
        return 0.0
    low = 0.1 * peak
    high = 0.9 * peak
    low_indices = np.flatnonzero(envelope >= low)
    high_indices = np.flatnonzero(envelope >= high)
    if low_indices.size == 0 or high_indices.size == 0:
        return 0.0
    return 1000.0 * max(0, int(high_indices[0]) - int(low_indices[0])) / sample_rate_hz


def _overshoot_db(envelope: np.ndarray) -> float:
    if envelope.size == 0:
        return 0.0
    peak = float(np.max(envelope))
    tail_start = max(0, int(envelope.size * 0.5))
    steady = float(np.median(envelope[tail_start:])) if tail_start < envelope.size else peak
    return linear_to_db(peak / max(steady, EPSILON))


def _estimate_fundamental(samples: np.ndarray, sample_rate_hz: int) -> float:
    spectrum = np.abs(np.fft.rfft(_windowed(samples)))
    freqs = np.fft.rfftfreq(samples.shape[0], 1.0 / sample_rate_hz)
    mask = (freqs >= 40.0) & (freqs <= min(5_000.0, sample_rate_hz / 2.0))
    if not np.any(mask):
        return 440.0
    return float(freqs[mask][np.argmax(spectrum[mask])])


def _harmonic_levels(samples: np.ndarray, sample_rate_hz: int, fundamental_hz: float) -> dict[int, float]:
    windowed = _windowed(samples)
    spectrum = np.abs(np.fft.rfft(windowed))
    freqs = np.fft.rfftfreq(samples.shape[0], 1.0 / sample_rate_hz)
    levels: dict[int, float] = {}
    for harmonic in range(1, 6):
        frequency = fundamental_hz * harmonic
        if frequency >= sample_rate_hz / 2.0:
            continue
        index = int(np.argmin(np.abs(freqs - frequency)))
        left = max(0, index - 1)
        right = min(spectrum.shape[0], index + 2)
        levels[harmonic] = float(np.sqrt(np.sum(np.square(spectrum[left:right]))))
    return levels


def _thd_db(levels: dict[int, float]) -> float:
    fundamental = levels.get(1, 0.0)
    harmonic_power = sum(level * level for harmonic, level in levels.items() if harmonic > 1)
    return linear_to_db(np.sqrt(harmonic_power) / max(fundamental, EPSILON))


def _harmonic_delta(candidate: dict[int, float], reference: dict[int, float], harmonic: int) -> float | None:
    if harmonic not in candidate or harmonic not in reference:
        return None
    candidate_ratio = candidate[harmonic] / max(candidate.get(1, 0.0), EPSILON)
    reference_ratio = reference[harmonic] / max(reference.get(1, 0.0), EPSILON)
    return linear_to_db(candidate_ratio / max(reference_ratio, EPSILON))


def _band_rms(samples: np.ndarray, sample_rate_hz: int, low_hz: float, high_hz: float) -> float:
    if samples.size < 8:
        return 0.0
    spectrum = np.fft.rfft(_windowed(samples))
    freqs = np.fft.rfftfreq(samples.shape[0], 1.0 / sample_rate_hz)
    mask = (freqs >= low_hz) & (freqs <= high_hz)
    if not np.any(mask):
        return 0.0
    return float(np.sqrt(np.mean(np.square(np.abs(spectrum[mask])))) / max(samples.shape[0] / 2.0, 1.0))


def _sag_shape(samples: np.ndarray, sample_rate_hz: int) -> tuple[float, float]:
    early = _window_rms(samples, 0, int(round(0.050 * sample_rate_hz)))
    middle_start = int(round(0.200 * sample_rate_hz))
    middle = _window_rms(samples, middle_start, middle_start + int(round(0.100 * sample_rate_hz)))
    late_start = max(0, samples.shape[0] - int(round(0.150 * sample_rate_hz)))
    late = _window_rms(samples, late_start, samples.shape[0])
    drop = linear_to_db(middle / max(early, EPSILON))
    recovery = linear_to_db(late / max(middle, EPSILON))
    return drop, recovery


def _window_rms(samples: np.ndarray, start: int, end: int) -> float:
    start = max(0, min(start, samples.shape[0]))
    end = max(start, min(end, samples.shape[0]))
    return rms(samples[start:end])


def _windowed(samples: np.ndarray) -> np.ndarray:
    if samples.size == 0:
        return samples
    return samples * signal.windows.hann(samples.shape[0], sym=False)
