from __future__ import annotations

from pathlib import Path

from greybound_lab.metrics import ComparisonMetrics, SegmentComparisonMetrics


def write_markdown_report(
    path: Path,
    candidate_path: Path,
    reference_path: Path,
    metrics: ComparisonMetrics,
    metadata_path: Path | None = None,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    metadata_line = str(metadata_path) if metadata_path else "not provided"
    path.write_text(
        _render_markdown(candidate_path, reference_path, metrics, metadata_line),
        encoding="utf-8",
    )


def _render_markdown(
    candidate_path: Path,
    reference_path: Path,
    metrics: ComparisonMetrics,
    metadata_line: str,
) -> str:
    return f"""# WAV Comparison Report

## Inputs

- Candidate: `{candidate_path}`
- Reference: `{reference_path}`
- Metadata: `{metadata_line}`
- Sample rate: {metrics.sample_rate_hz} Hz
- Candidate samples: {metrics.candidate_samples}
- Reference samples: {metrics.reference_samples}
- Compared samples: {metrics.compared_samples}

## Alignment

- Estimated candidate latency: {metrics.latency_samples} samples ({metrics.latency_ms:.3f} ms)
- Candidate gain correction: {metrics.gain_db:.3f} dB

## Levels

| Signal | RMS dBFS | Peak dBFS | Crest dB |
| --- | ---: | ---: | ---: |
| Candidate input | {metrics.candidate.rms_dbfs:.2f} | {metrics.candidate.peak_dbfs:.2f} | {metrics.candidate.crest_db:.2f} |
| Reference input | {metrics.reference.rms_dbfs:.2f} | {metrics.reference.peak_dbfs:.2f} | {metrics.reference.crest_db:.2f} |
| Candidate aligned | {metrics.aligned_candidate.rms_dbfs:.2f} | {metrics.aligned_candidate.peak_dbfs:.2f} | {metrics.aligned_candidate.crest_db:.2f} |
| Reference aligned | {metrics.aligned_reference.rms_dbfs:.2f} | {metrics.aligned_reference.peak_dbfs:.2f} | {metrics.aligned_reference.crest_db:.2f} |

## Error Metrics

- Null residual RMS: {metrics.null_rms_dbfs:.2f} dBFS
- Null residual relative to reference: {metrics.null_relative_db:.2f} dB
- Log-spectral distance: {metrics.log_spectral_distance_db:.2f} dB
- Envelope error: {metrics.envelope_error_db:.2f} dB

{_render_segments(metrics.segments)}

## Engineering Notes

Use this report as a directional diagnostic, not as a single quality score. A
good next analysis pass should inspect whether the residual is dominated by
level, latency, spectral tilt, transient behavior, or nonlinear dynamics.
"""


def _render_segments(segments: tuple[SegmentComparisonMetrics, ...]) -> str:
    if not segments:
        return ""
    lines = [
        "## Segment Metrics",
        "",
        "| Segment | Kind | Time s | Local gain dB | Null rel dB | Log-spectral dB | Envelope dB |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: |",
    ]
    for segment in segments:
        lines.append(
            f"| {segment.name} | {segment.kind} | {segment.start_s:.3f}-{segment.end_s:.3f} | "
            f"{segment.local_gain_db:.2f} | {segment.null_relative_db:.2f} | "
            f"{segment.log_spectral_distance_db:.2f} | {segment.envelope_error_db:.2f} |"
        )
    lines.append("")
    specialized = _render_specialized_segments(segments)
    if specialized:
        lines.extend(specialized)
    return "\n".join(lines)


def _render_specialized_segments(segments: tuple[SegmentComparisonMetrics, ...]) -> list[str]:
    lines: list[str] = []
    attack_segments = [segment for segment in segments if segment.attack is not None]
    if attack_segments:
        lines.extend(
            [
                "### Attack Diagnostics",
                "",
                "| Segment | Peak delta ms | Rise delta ms | Overshoot delta dB |",
                "| --- | ---: | ---: | ---: |",
            ]
        )
        for segment in attack_segments:
            assert segment.attack is not None
            lines.append(
                f"| {segment.name} | {segment.attack.peak_time_delta_ms:.2f} | "
                f"{segment.attack.rise_time_delta_ms:.2f} | {segment.attack.overshoot_delta_db:.2f} |"
            )
        lines.append("")

    harmonic_segments = [segment for segment in segments if segment.harmonics is not None]
    if harmonic_segments:
        lines.extend(
            [
                "### Harmonic Diagnostics",
                "",
                "| Segment | F0 Hz | THD cand dB | THD ref dB | THD delta dB | H2 delta | H3 delta | H4 delta | H5 delta |",
                "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
            ]
        )
        for segment in harmonic_segments:
            assert segment.harmonics is not None
            harmonics = segment.harmonics
            lines.append(
                f"| {segment.name} | {harmonics.fundamental_hz:.1f} | {harmonics.candidate_thd_db:.2f} | "
                f"{harmonics.reference_thd_db:.2f} | {harmonics.thd_delta_db:.2f} | "
                f"{_optional_db(harmonics.h2_delta_db)} | {_optional_db(harmonics.h3_delta_db)} | "
                f"{_optional_db(harmonics.h4_delta_db)} | {_optional_db(harmonics.h5_delta_db)} |"
            )
        lines.append("")

    aliasing_segments = [segment for segment in segments if segment.aliasing is not None]
    if aliasing_segments:
        lines.extend(
            [
                "### High-Band / Aliasing Diagnostics",
                "",
                "| Segment | Candidate >18k dBFS | Reference >18k dBFS | Delta dB | Residual >18k dBFS |",
                "| --- | ---: | ---: | ---: | ---: |",
            ]
        )
        for segment in aliasing_segments:
            assert segment.aliasing is not None
            aliasing = segment.aliasing
            lines.append(
                f"| {segment.name} | {aliasing.candidate_high_band_dbfs:.2f} | "
                f"{aliasing.reference_high_band_dbfs:.2f} | {aliasing.high_band_delta_db:.2f} | "
                f"{aliasing.residual_high_band_dbfs:.2f} |"
            )
        lines.append("")

    sag_segments = [segment for segment in segments if segment.sag is not None]
    if sag_segments:
        lines.extend(
            [
                "### Sag Diagnostics",
                "",
                "| Segment | Drop cand dB | Drop ref dB | Drop delta dB | Recovery cand dB | Recovery ref dB | Recovery delta dB |",
                "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
            ]
        )
        for segment in sag_segments:
            assert segment.sag is not None
            sag = segment.sag
            lines.append(
                f"| {segment.name} | {sag.candidate_drop_db:.2f} | {sag.reference_drop_db:.2f} | "
                f"{sag.drop_delta_db:.2f} | {sag.candidate_recovery_db:.2f} | "
                f"{sag.reference_recovery_db:.2f} | {sag.recovery_delta_db:.2f} |"
            )
        lines.append("")
    return lines


def _optional_db(value: float | None) -> str:
    return "n/a" if value is None else f"{value:.2f}"
