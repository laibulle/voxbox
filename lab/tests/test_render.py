from __future__ import annotations

import json
from pathlib import Path

from greybound_lab import render as render_module


def test_render_rig_writes_metadata(monkeypatch, tmp_path: Path) -> None:
    calls = []

    def fake_run(command, cwd=None, check=False, capture_output=False, text=False, **kwargs):
        calls.append(
            {
                "command": command,
                "cwd": cwd,
                "check": check,
                "capture_output": capture_output,
                "text": text,
            }
        )
        if command[:2] == ["git", "rev-parse"]:
            return FakeCompletedProcess(stdout="abc123\n")
        return FakeCompletedProcess(stdout="")

    monkeypatch.setattr(render_module.subprocess, "run", fake_run)
    repo_root = tmp_path
    output_wav = repo_root / "lab/renders/test.wav"
    metadata = repo_root / "lab/renders/test.run.json"

    render_module.render_rig(
        repo_root=repo_root,
        binary=Path("target/release/greybound-cli"),
        rig=Path("rigs/nox30-driven.json5"),
        input_wav=Path("samples/input.wav"),
        output_wav=output_wav,
        metadata=metadata,
        render_seconds=3.0,
        sample_rate_hz=44_100,
        period_size=16,
        input_gain_db=0.0,
        output_gain_db=-18.0,
        ir_enabled=True,
        monitor_enabled=True,
        monitor_log=repo_root / "lab/renders/test.monitor.log",
        neural_cell=("nox30.first_stage", Path("lab/models/cell/model.greybound.json")),
        neural_cell_mode="replace",
    )

    assert calls[0]["command"][0] == "target/release/greybound-cli"
    assert "--ir" in calls[0]["command"]
    ir_index = calls[0]["command"].index("--ir")
    assert calls[0]["command"][ir_index + 1] == "lab/references/tone3000-irs/celestion.wav"
    assert "--monitor" in calls[0]["command"]
    assert "--monitor-log" in calls[0]["command"]
    assert "--neural-cell" in calls[0]["command"]
    assert "--neural-cell-mode" in calls[0]["command"]
    payload = json.loads(metadata.read_text(encoding="utf-8"))
    assert payload["project_revision"] == "abc123"
    assert payload["candidate"]["kind"] == "greybound-render"
    assert payload["candidate"]["path"] == "lab/renders/test.wav"
    assert payload["candidate"]["rig"] == "rigs/nox30-driven.json5"
    assert payload["audio"]["input_wav"] == "samples/input.wav"
    assert payload["audio"]["output_gain_db"] == -18.0
    assert payload["audio"]["ir_enabled"] is True
    assert payload["audio"]["ir_wav"] == "lab/references/tone3000-irs/celestion.wav"
    assert payload["audio"]["monitor_enabled"] is True
    assert payload["audio"]["neural_cell"]["component"] == "nox30.first_stage"
    assert payload["audio"]["neural_cell"]["mode"] == "replace"


def test_render_rig_can_pipe_rig_text_to_stdin(monkeypatch, tmp_path: Path) -> None:
    calls = []

    def fake_run(command, cwd=None, check=False, capture_output=False, text=False, input=None, **kwargs):
        calls.append(
            {
                "command": command,
                "cwd": cwd,
                "check": check,
                "capture_output": capture_output,
                "text": text,
                "input": input,
            }
        )
        if command[:2] == ["git", "rev-parse"]:
            return FakeCompletedProcess(stdout="abc123\n")
        return FakeCompletedProcess(stdout="")

    monkeypatch.setattr(render_module.subprocess, "run", fake_run)
    repo_root = tmp_path
    metadata = repo_root / "lab/renders/stdin.run.json"

    render_module.render_rig(
        repo_root=repo_root,
        binary=Path("target/release/greybound-cli"),
        rig=Path("-"),
        rig_text="{ name: 'generated', amp: { model: 'nox30', controls: { drive: 0.5 } } }",
        input_wav=Path("samples/input.wav"),
        output_wav=repo_root / "lab/renders/stdin.wav",
        metadata=metadata,
        render_seconds=1.0,
        sample_rate_hz=48_000,
        period_size=16,
        input_gain_db=0.0,
        output_gain_db=-12.0,
        ir_enabled=False,
    )

    render_call = calls[0]
    assert render_call["command"][:3] == ["target/release/greybound-cli", "--rig", "-"]
    assert render_call["input"] is not None
    assert render_call["text"] is True
    payload = json.loads(metadata.read_text(encoding="utf-8"))
    assert payload["candidate"]["rig"] == "-"
    assert payload["candidate"]["rig_source"] == "stdin"


class FakeCompletedProcess:
    def __init__(self, stdout: str):
        self.stdout = stdout
