from __future__ import annotations

import json
import platform
import subprocess
from pathlib import Path

DEFAULT_IR_WAV = Path("lab/references/tone3000-irs/celestion.wav")


def render_rig(
    *,
    repo_root: Path,
    binary: Path,
    rig: Path,
    rig_text: str | None = None,
    input_wav: Path,
    output_wav: Path,
    metadata: Path,
    render_seconds: float,
    sample_rate_hz: int,
    period_size: int,
    input_gain_db: float,
    output_gain_db: float,
    ir_enabled: bool,
    ir_wav: Path | None = None,
    monitor_enabled: bool = False,
    monitor_log: Path | None = None,
    neural_cell: tuple[str, Path] | None = None,
    neural_cell_mode: str = "shadow",
) -> None:
    output_wav.parent.mkdir(parents=True, exist_ok=True)
    metadata.parent.mkdir(parents=True, exist_ok=True)

    command = [
        str(binary),
        "--rig",
        str(rig),
        "--input-wav",
        str(input_wav),
        "--output-wav",
        str(output_wav),
        "--render-seconds",
        _format_number(render_seconds),
        "--sample-rate",
        str(sample_rate_hz),
        "--period-size",
        str(period_size),
        "--input-db",
        _format_number(input_gain_db),
        "--output-db",
        _format_number(output_gain_db),
    ]
    resolved_ir_wav = ir_wav or DEFAULT_IR_WAV
    if ir_enabled:
        command.extend(["--ir", str(resolved_ir_wav)])
    if monitor_enabled:
        command.append("--monitor")
        if monitor_log is not None:
            command.extend(["--monitor-log", str(monitor_log)])
    if neural_cell is not None:
        component, descriptor = neural_cell
        command.extend(["--neural-cell", f"{component}={descriptor}"])
        command.extend(["--neural-cell-mode", neural_cell_mode])

    subprocess.run(command, cwd=repo_root, check=True, input=rig_text, text=rig_text is not None)

    metadata_payload = {
        "schema_version": 1,
        "project_revision": git_revision(repo_root),
        "candidate": {
            "kind": "greybound-render",
            "path": relative_or_absolute(output_wav, repo_root),
            "rig": relative_or_absolute(rig, repo_root),
            "rig_source": "stdin" if rig_text is not None else "file",
            "command": " ".join(command),
        },
        "audio": {
            "sample_rate_hz": sample_rate_hz,
            "duration_seconds": render_seconds,
            "input_wav": relative_or_absolute(input_wav, repo_root),
            "input_gain_db": input_gain_db,
            "output_gain_db": output_gain_db,
            "ir_enabled": ir_enabled,
            "ir_wav": relative_or_absolute(resolved_ir_wav, repo_root) if ir_enabled else None,
            "monitor_enabled": monitor_enabled,
            "monitor_log": relative_or_absolute(monitor_log, repo_root) if monitor_log else None,
            "neural_cell": {
                "component": neural_cell[0],
                "descriptor": relative_or_absolute(neural_cell[1], repo_root),
                "mode": neural_cell_mode,
            }
            if neural_cell
            else None,
        },
        "environment": {
            "os": platform.platform(),
            "toolchain": "greybound-lab render-rig",
        },
    }
    metadata.write_text(json.dumps(metadata_payload, indent=2) + "\n", encoding="utf-8")


def git_revision(repo_root: Path) -> str:
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=repo_root,
            check=True,
            capture_output=True,
            text=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError):
        return "unknown"
    revision = result.stdout.strip()
    return revision if revision else "unknown"


def relative_or_absolute(path: Path, root: Path) -> str:
    if not path.is_absolute():
        return str(path)
    path = path.resolve()
    try:
        return str(path.relative_to(root.resolve()))
    except ValueError:
        return str(path)


def _format_number(value: float) -> str:
    return f"{value:g}"
