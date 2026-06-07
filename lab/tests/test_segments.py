from __future__ import annotations

import json
from pathlib import Path

from greybound_lab.segments import load_segments


def test_loads_segments(tmp_path: Path) -> None:
    path = tmp_path / "segments.json"
    path.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "segments": [
                    {
                        "name": "attack",
                        "kind": "attack",
                        "start_s": 0.1,
                        "end_s": 0.2,
                        "fundamental_hz": 440.0,
                        "notes": "test",
                    }
                ],
            }
        ),
        encoding="utf-8",
    )

    segments = load_segments(path)

    assert len(segments) == 1
    assert segments[0].name == "attack"
    assert segments[0].kind == "attack"
    assert segments[0].duration_s == 0.1
    assert segments[0].fundamental_hz == 440.0
