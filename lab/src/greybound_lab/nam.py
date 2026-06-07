from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

PRIORITY_MODELS = {
    "TopBoost-Gain3",
    "TopBoost-Gain5",
    "TopBoost-Gain7",
    "TopBoost-Gain5-TopCut",
}


@dataclass(frozen=True)
class NamModelSummary:
    name: str
    metadata_name: str | None
    path: Path
    size_bytes: int
    version: str
    architecture: str
    sample_rate_hz: int
    metadata: dict[str, Any]
    semantics: dict[str, Any]
    priority: bool


def inspect_nam_pack(pack_dir: Path, *, tone_url: str) -> dict[str, Any]:
    models = [inspect_nam_model(path) for path in sorted(pack_dir.glob("*.nam"))]
    if not models:
        raise ValueError(f"no .nam files found in {pack_dir}")

    gear_make = _first_metadata_value(models, "gear_make")
    gear_model = _first_metadata_value(models, "gear_model")
    modeled_by = _first_metadata_value(models, "modeled_by")

    return {
        "$schema": "../../schemas/nam-pack-manifest.schema.json",
        "schema_version": 1,
        "reference_id": "ac30hwh-6580",
        "source": {
            "provider": "TONE3000",
            "url": tone_url,
            "tone_id": "6580",
            "creator": modeled_by,
            "license": "T3K",
            "license_notes": (
                "TONE3000 T3K: local use and published outputs are allowed; "
                "do not republish or redistribute the data files without author permission."
            ),
        },
        "model_policy": {
            "format": "nam-a2",
            "architecture_version": "2",
            "gear_type": "amp-head",
            "includes_cab": False,
            "ir_policy": "amp-head-plus-greybound-ir",
        },
        "gear": {
            "make": gear_make,
            "model": gear_model,
            "notes": (
                "Public page notes: retubed with JJ tubes, master volume bypassed; "
                "Normal channel bright switch on; Top Boost treble and bass at noon; "
                "Hot Mode; optional Top Cut at 6/10."
            ),
        },
        "priority_models": sorted(PRIORITY_MODELS),
        "models": [_summary_to_manifest_item(model, pack_dir) for model in models],
    }


def inspect_nam_model(path: Path) -> NamModelSummary:
    data = json.loads(path.read_text(encoding="utf-8"))
    metadata = data.get("metadata", {})
    if not isinstance(metadata, dict):
        metadata = {}
    metadata_name = str(metadata["name"]) if metadata.get("name") else None
    name = path.stem
    sample_rate = int(float(data.get("sample_rate", 0)))
    return NamModelSummary(
        name=name,
        metadata_name=metadata_name,
        path=path,
        size_bytes=path.stat().st_size,
        version=str(data.get("version", "")),
        architecture=str(data.get("architecture", "")),
        sample_rate_hz=sample_rate,
        metadata=metadata,
        semantics=parse_ac30hwh_model_name(name),
        priority=name in PRIORITY_MODELS,
    )


def parse_ac30hwh_model_name(name: str) -> dict[str, Any]:
    match = re.fullmatch(
        r"(?P<channel>TopBoost|Normal-Bright|HotMode)-Gain(?P<gain>3|5|7|Full)(?P<topcut>-TopCut)?",
        name,
    )
    if not match:
        return {"raw_name": name}

    channel = match.group("channel")
    gain_label = match.group("gain")
    top_cut = match.group("topcut") is not None
    semantics: dict[str, Any] = {
        "channel": channel,
        "gain_label": gain_label,
        "gain_position": 10 if gain_label == "Full" else int(gain_label),
        "top_cut_enabled": top_cut,
        "top_cut_position": 6 if top_cut else None,
    }
    if channel == "TopBoost":
        semantics["treble_position"] = 5
        semantics["bass_position"] = 5
    if channel == "Normal-Bright":
        semantics["bright_switch"] = True
    if channel == "HotMode":
        semantics["hot_mode"] = True
    return semantics


def write_nam_pack_manifest(pack_dir: Path, manifest_path: Path, *, tone_url: str) -> dict[str, Any]:
    manifest = inspect_nam_pack(pack_dir, tone_url=tone_url)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return manifest


def _summary_to_manifest_item(model: NamModelSummary, pack_dir: Path) -> dict[str, Any]:
    training = model.metadata.get("training", {})
    if not isinstance(training, dict):
        training = {}
    latency = (
        training.get("data", {}).get("latency", {}).get("calibration", {})
        if isinstance(training.get("data"), dict)
        else {}
    )
    return {
        "name": model.name,
        "metadata_name": model.metadata_name,
        "path": str(model.path.relative_to(pack_dir.parent)),
        "size_bytes": model.size_bytes,
        "version": model.version,
        "architecture": model.architecture,
        "sample_rate_hz": model.sample_rate_hz,
        "loudness": model.metadata.get("loudness"),
        "gain": model.metadata.get("gain"),
        "validation_esr": training.get("validation_esr"),
        "recommended_latency_samples": latency.get("recommended") if isinstance(latency, dict) else None,
        "semantics": model.semantics,
        "priority": model.priority,
    }


def _first_metadata_value(models: list[NamModelSummary], key: str) -> Any:
    for model in models:
        value = model.metadata.get(key)
        if value:
            return value
    return None
