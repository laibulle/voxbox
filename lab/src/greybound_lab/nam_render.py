from __future__ import annotations

import json
import platform
import shlex
import subprocess
from pathlib import Path

from greybound_lab.render import git_revision, relative_or_absolute


def render_nam(
    *,
    repo_root: Path,
    model: Path,
    input_wav: Path,
    output_wav: Path,
    metadata: Path,
    renderer_command: str,
    render_seconds: float,
    sample_rate_hz: int,
    ir_wav: Path | None = None,
    dry_run: bool = False,
) -> list[str]:
    if not renderer_command.strip():
        raise ValueError("renderer_command is required")

    output_wav.parent.mkdir(parents=True, exist_ok=True)
    metadata.parent.mkdir(parents=True, exist_ok=True)

    command = expand_renderer_command(
        renderer_command,
        model=model,
        input_wav=input_wav,
        output_wav=output_wav,
        metadata=metadata,
        sample_rate_hz=sample_rate_hz,
        render_seconds=render_seconds,
        ir_wav=ir_wav,
    )
    if not dry_run:
        subprocess.run(command, cwd=repo_root, check=True)
        if not output_wav.exists():
            raise FileNotFoundError(f"NAM renderer completed but did not write {output_wav}")

    metadata_payload = {
        "schema_version": 1,
        "project_revision": git_revision(repo_root),
        "candidate": {
            "kind": "nam-render",
            "path": relative_or_absolute(output_wav, repo_root),
            "command": " ".join(shlex.quote(part) for part in command),
        },
        "audio": {
            "sample_rate_hz": sample_rate_hz,
            "duration_seconds": render_seconds,
            "input_wav": relative_or_absolute(input_wav, repo_root),
            "ir_enabled": ir_wav is not None,
            "ir_path": relative_or_absolute(ir_wav, repo_root) if ir_wav else None,
        },
        "controls": {
            "model": relative_or_absolute(model, repo_root),
        },
        "environment": {
            "os": platform.platform(),
            "toolchain": "greybound-lab render-nam external renderer",
        },
    }
    metadata.write_text(json.dumps(metadata_payload, indent=2) + "\n", encoding="utf-8")
    return command


def expand_renderer_command(
    renderer_command: str,
    *,
    model: Path,
    input_wav: Path,
    output_wav: Path,
    metadata: Path,
    sample_rate_hz: int,
    render_seconds: float,
    ir_wav: Path | None,
) -> list[str]:
    placeholders = {
        "model": str(model),
        "input_wav": str(input_wav),
        "output_wav": str(output_wav),
        "metadata": str(metadata),
        "sample_rate": str(sample_rate_hz),
        "render_seconds": f"{render_seconds:g}",
        "ir_wav": str(ir_wav) if ir_wav else "",
    }
    try:
        formatted = renderer_command.format(**placeholders)
    except KeyError as exc:
        raise ValueError(f"unknown renderer command placeholder: {exc}") from exc
    command = shlex.split(formatted)
    if not command:
        raise ValueError("renderer_command expanded to an empty command")
    return command
