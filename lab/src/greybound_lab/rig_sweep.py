from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path

from greybound_lab.audio import read_wav_mono
from greybound_lab.metrics import ComparisonMetrics, compare_signals
from greybound_lab.render import git_revision, render_rig
from greybound_lab.segments import SegmentSpec


@dataclass(frozen=True)
class SweepPoint:
    value: float
    rig_path: Path
    output_wav: Path
    metadata_path: Path
    metrics: ComparisonMetrics


def run_amp_control_sweep(
    *,
    repo_root: Path,
    binary: Path,
    rig: Path,
    control: str,
    values: list[float],
    input_wav: Path,
    reference_wav: Path,
    output_dir: Path,
    report: Path,
    metadata: Path,
    render_seconds: float,
    sample_rate_hz: int,
    period_size: int,
    input_gain_db: float,
    output_gain_db: float,
    segments: list[SegmentSpec] | None = None,
    max_lag_ms: float = 100.0,
) -> list[SweepPoint]:
    if not values:
        raise ValueError("sweep needs at least one value")
    for value in values:
        if not 0.0 <= value <= 1.0:
            raise ValueError(f"sweep value {value:g} is outside normalized 0.0..1.0 range")

    output_dir.mkdir(parents=True, exist_ok=True)
    generated_rig_dir = output_dir / "generated-rigs"
    render_dir = output_dir / "renders"
    metadata_dir = output_dir / "metadata"
    generated_rig_dir.mkdir(parents=True, exist_ok=True)
    render_dir.mkdir(parents=True, exist_ok=True)
    metadata_dir.mkdir(parents=True, exist_ok=True)

    base_rig_text = rig.read_text(encoding="utf-8")
    reference = read_wav_mono(reference_wav)
    points: list[SweepPoint] = []

    for index, value in enumerate(values):
        label = f"{control}-{value:.3f}".replace(".", "p").replace("_", "-")
        generated_rig_text = replace_amp_control(base_rig_text, control, value, sweep_name(label))
        generated_rig_path = generated_rig_dir / f"{index:02d}-{label}.json5"
        output_wav = render_dir / f"{index:02d}-{label}.wav"
        run_metadata = metadata_dir / f"{index:02d}-{label}.run.json"
        generated_rig_path.write_text(generated_rig_text, encoding="utf-8")

        render_rig(
            repo_root=repo_root,
            binary=binary,
            rig=Path("-"),
            rig_text=generated_rig_text,
            input_wav=input_wav,
            output_wav=output_wav,
            metadata=run_metadata,
            render_seconds=render_seconds,
            sample_rate_hz=sample_rate_hz,
            period_size=period_size,
            input_gain_db=input_gain_db,
            output_gain_db=output_gain_db,
            ir_enabled=False,
        )

        candidate = read_wav_mono(output_wav)
        if candidate.sample_rate != reference.sample_rate:
            raise ValueError(
                f"sample-rate mismatch for {output_wav}: "
                f"candidate={candidate.sample_rate} Hz reference={reference.sample_rate} Hz"
            )
        metrics = compare_signals(
            candidate.samples,
            reference.samples,
            candidate.sample_rate,
            max_lag_ms=max_lag_ms,
            segments=segments,
        )
        points.append(
            SweepPoint(
                value=value,
                rig_path=generated_rig_path,
                output_wav=output_wav,
                metadata_path=run_metadata,
                metrics=metrics,
            )
        )

    write_sweep_report(
        report,
        rig=rig,
        control=control,
        input_wav=input_wav,
        reference_wav=reference_wav,
        points=points,
    )
    write_sweep_metadata(
        metadata,
        repo_root=repo_root,
        rig=rig,
        control=control,
        input_wav=input_wav,
        reference_wav=reference_wav,
        output_dir=output_dir,
        render_seconds=render_seconds,
        sample_rate_hz=sample_rate_hz,
        period_size=period_size,
        input_gain_db=input_gain_db,
        output_gain_db=output_gain_db,
        points=points,
    )
    return points


def replace_amp_control(rig_text: str, control: str, value: float, name: str) -> str:
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", control):
        raise ValueError(f"unsupported amp control name: {control}")
    control_pattern = re.compile(rf"(^\s*{re.escape(control)}\s*:\s*)([-+]?\d+(?:\.\d+)?)(\s*,)", re.MULTILINE)
    rig_text, control_count = control_pattern.subn(rf"\g<1>{value:.6f}\3", rig_text, count=1)
    if control_count != 1:
        raise ValueError(f"could not find amp.controls.{control} in rig")

    name_pattern = re.compile(r"(^\s*name\s*:\s*)(['\"])(.*?)(\2)(\s*,)", re.MULTILINE)
    rig_text, name_count = name_pattern.subn(rf"\g<1>'{name}'\5", rig_text, count=1)
    if name_count == 0:
        rig_text = rig_text.replace("{", "{\n  name: '" + name + "',", 1)
    return rig_text


def sweep_name(label: str) -> str:
    return f"sweep-{label}"


def write_sweep_report(
    path: Path,
    *,
    rig: Path,
    control: str,
    input_wav: Path,
    reference_wav: Path,
    points: list[SweepPoint],
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    ranked = sorted(points, key=lambda point: point.metrics.log_spectral_distance_db)
    best = ranked[0]
    lines = [
        "# Rig Sweep vs NAM Reference",
        "",
        "## Protocol",
        "",
        f"- Base rig: `{rig}`",
        f"- Swept control: `amp.controls.{control}`",
        f"- Input DI: `{input_wav}`",
        f"- Reference WAV: `{reference_wav}`",
        "- IR policy: `amp-head-no-ir`; Greybound is rendered without `--ir`.",
        "",
        "## Best Point",
        "",
        f"- Value: `{best.value:.3f}`",
        f"- Log-spectral distance: `{best.metrics.log_spectral_distance_db:.2f} dB`",
        f"- Null residual relative: `{best.metrics.null_relative_db:.2f} dB`",
        f"- Gain correction: `{best.metrics.gain_db:.2f} dB`",
        f"- WAV: `{best.output_wav}`",
        "",
        "## Sweep Table",
        "",
        "| Value | Gain corr dB | Null rel dB | Log-spectral dB | Envelope dB | Candidate RMS | Candidate peak | WAV |",
        "| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |",
    ]
    for point in points:
        metrics = point.metrics
        lines.append(
            f"| {point.value:.3f} | {metrics.gain_db:.2f} | {metrics.null_relative_db:.2f} | "
            f"{metrics.log_spectral_distance_db:.2f} | {metrics.envelope_error_db:.2f} | "
            f"{metrics.candidate.rms_dbfs:.2f} | {metrics.candidate.peak_dbfs:.2f} | `{point.output_wav}` |"
        )
    lines.extend(
        [
            "",
            "## Interpretation",
            "",
            "This report ranks by log-spectral distance first because this sweep is meant to find a coarse amp-control anchor.",
            "Use the null residual, envelope error, and segment diagnostics before deciding that one point is musically superior.",
        ]
    )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_sweep_metadata(
    path: Path,
    *,
    repo_root: Path,
    rig: Path,
    control: str,
    input_wav: Path,
    reference_wav: Path,
    output_dir: Path,
    render_seconds: float,
    sample_rate_hz: int,
    period_size: int,
    input_gain_db: float,
    output_gain_db: float,
    points: list[SweepPoint],
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema_version": 1,
        "project_revision": git_revision(repo_root),
        "kind": "rig-sweep-vs-nam-reference",
        "protocol": {
            "ir_policy": "amp-head-no-ir",
            "greybound_ir_enabled": False,
        },
        "inputs": {
            "base_rig": str(rig),
            "control": f"amp.controls.{control}",
            "input_wav": str(input_wav),
            "reference_wav": str(reference_wav),
            "output_dir": str(output_dir),
            "render_seconds": render_seconds,
            "sample_rate_hz": sample_rate_hz,
            "period_size": period_size,
            "input_gain_db": input_gain_db,
            "output_gain_db": output_gain_db,
        },
        "points": [
            {
                "value": point.value,
                "generated_rig": str(point.rig_path),
                "output_wav": str(point.output_wav),
                "metadata": str(point.metadata_path),
                "gain_db": point.metrics.gain_db,
                "null_relative_db": point.metrics.null_relative_db,
                "log_spectral_distance_db": point.metrics.log_spectral_distance_db,
                "envelope_error_db": point.metrics.envelope_error_db,
                "candidate_rms_dbfs": point.metrics.candidate.rms_dbfs,
                "candidate_peak_dbfs": point.metrics.candidate.peak_dbfs,
            }
            for point in points
        ],
    }
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
