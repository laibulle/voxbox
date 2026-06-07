from __future__ import annotations

import json
import platform
import subprocess
from pathlib import Path


def render_rig(
    *,
    repo_root: Path,
    binary: Path,
    rig: Path,
    input_wav: Path,
    output_wav: Path,
    metadata: Path,
    render_seconds: float,
    sample_rate_hz: int,
    period_size: int,
    input_gain_db: float,
    output_gain_db: float,
    ir_enabled: bool,
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
    if ir_enabled:
        command.append("--ir")

    subprocess.run(command, cwd=repo_root, check=True)

    metadata_payload = {
        "schema_version": 1,
        "project_revision": git_revision(repo_root),
        "candidate": {
            "kind": "greybound-render",
            "path": relative_or_absolute(output_wav, repo_root),
            "rig": relative_or_absolute(rig, repo_root),
            "command": " ".join(command),
        },
        "audio": {
            "sample_rate_hz": sample_rate_hz,
            "duration_seconds": render_seconds,
            "input_wav": relative_or_absolute(input_wav, repo_root),
            "input_gain_db": input_gain_db,
            "output_gain_db": output_gain_db,
            "ir_enabled": ir_enabled,
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
