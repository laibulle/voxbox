from __future__ import annotations

import json
from pathlib import Path

from greybound_lab.nam_render import expand_renderer_command, render_nam


def test_expand_renderer_command_uses_placeholders() -> None:
    command = expand_renderer_command(
        "nam-a2 --model {model} --in {input_wav} --out {output_wav} --sr {sample_rate}",
        model=Path("model.nam"),
        input_wav=Path("in.wav"),
        output_wav=Path("out.wav"),
        metadata=Path("run.json"),
        sample_rate_hz=48000,
        render_seconds=2.5,
        ir_wav=None,
    )

    assert command == ["nam-a2", "--model", "model.nam", "--in", "in.wav", "--out", "out.wav", "--sr", "48000"]


def test_render_nam_writes_metadata(tmp_path: Path) -> None:
    script = tmp_path / "fake_renderer.py"
    model = tmp_path / "model.nam"
    input_wav = tmp_path / "input.wav"
    output_wav = tmp_path / "output.wav"
    metadata = tmp_path / "run.json"
    model.write_text("{}", encoding="utf-8")
    input_wav.write_bytes(b"RIFF")
    script.write_text(
        "from pathlib import Path\n"
        "import sys\n"
        "Path(sys.argv[sys.argv.index('--output') + 1]).write_bytes(b'RIFF')\n",
        encoding="utf-8",
    )

    command = render_nam(
        repo_root=tmp_path,
        model=model,
        input_wav=input_wav,
        output_wav=output_wav,
        metadata=metadata,
        renderer_command=f"python {script} --model {{model}} --input {{input_wav}} --output {{output_wav}}",
        render_seconds=1.0,
        sample_rate_hz=48000,
        ir_wav=None,
    )

    assert command[0] == "python"
    assert output_wav.exists()
    payload = json.loads(metadata.read_text(encoding="utf-8"))
    assert payload["candidate"]["kind"] == "nam-render"
    assert payload["audio"]["sample_rate_hz"] == 48000
    assert payload["controls"]["model"].endswith("model.nam")
