from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class SegmentSpec:
    name: str
    start_s: float
    end_s: float
    kind: str = "general"
    fundamental_hz: float | None = None
    notes: str | None = None

    @property
    def duration_s(self) -> float:
        return self.end_s - self.start_s


def load_segments(path: Path) -> list[SegmentSpec]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    segments = payload.get("segments")
    if not isinstance(segments, list):
        raise ValueError(f"{path} must contain a 'segments' list")
    return [_parse_segment(path, segment) for segment in segments]


def _parse_segment(path: Path, payload: object) -> SegmentSpec:
    if not isinstance(payload, dict):
        raise ValueError(f"{path} contains a non-object segment")
    name = payload.get("name")
    start_s = payload.get("start_s")
    end_s = payload.get("end_s")
    if not isinstance(name, str) or not name:
        raise ValueError(f"{path} contains a segment without a valid name")
    if not isinstance(start_s, int | float) or not isinstance(end_s, int | float):
        raise ValueError(f"{name} must define numeric start_s and end_s")
    if end_s <= start_s:
        raise ValueError(f"{name} must have end_s greater than start_s")
    kind = payload.get("kind", "general")
    if not isinstance(kind, str) or not kind:
        raise ValueError(f"{name} has an invalid kind")
    fundamental_hz = payload.get("fundamental_hz")
    if fundamental_hz is not None and not isinstance(fundamental_hz, int | float):
        raise ValueError(f"{name} has an invalid fundamental_hz")
    notes = payload.get("notes")
    if notes is not None and not isinstance(notes, str):
        raise ValueError(f"{name} has invalid notes")
    return SegmentSpec(
        name=name,
        start_s=float(start_s),
        end_s=float(end_s),
        kind=kind,
        fundamental_hz=float(fundamental_hz) if fundamental_hz is not None else None,
        notes=notes,
    )
