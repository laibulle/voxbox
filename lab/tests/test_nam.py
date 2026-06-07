from __future__ import annotations

import json
from pathlib import Path

from greybound_lab.nam import inspect_nam_pack, parse_ac30hwh_model_name, write_nam_pack_manifest


def test_parse_ac30hwh_model_name_topboost() -> None:
    semantics = parse_ac30hwh_model_name("TopBoost-Gain5-TopCut")

    assert semantics["channel"] == "TopBoost"
    assert semantics["gain_position"] == 5
    assert semantics["top_cut_enabled"] is True
    assert semantics["top_cut_position"] == 6
    assert semantics["treble_position"] == 5
    assert semantics["bass_position"] == 5


def test_parse_ac30hwh_model_name_normal_bright() -> None:
    semantics = parse_ac30hwh_model_name("Normal-Bright-GainFull")

    assert semantics["channel"] == "Normal-Bright"
    assert semantics["gain_position"] == 10
    assert semantics["bright_switch"] is True
    assert semantics["top_cut_enabled"] is False


def test_write_nam_pack_manifest(tmp_path: Path) -> None:
    pack = tmp_path / "AC30HWH"
    pack.mkdir()
    (pack / "TopBoost-Gain5.nam").write_text(
        json.dumps(
            {
                "version": "0.7.0",
                "architecture": "SlimmableContainer",
                "sample_rate": 48000.0,
                "metadata": {
                    "name": "TopBoost-Gain5",
                    "modeled_by": "bjeffhind",
                    "gear_make": "Vox AC30HWH",
                    "gear_model": "Vox AC30HWH",
                    "loudness": -10.0,
                    "gain": 0.7,
                    "training": {
                        "validation_esr": 0.004,
                        "data": {"latency": {"calibration": {"recommended": 15}}},
                    },
                },
            }
        ),
        encoding="utf-8",
    )

    output = tmp_path / "manifest.json"
    manifest = write_nam_pack_manifest(pack, output, tone_url="https://www.tone3000.com/tones/ac30hwh-6580")

    assert output.exists()
    assert manifest["reference_id"] == "ac30hwh-6580"
    assert manifest["source"]["creator"] == "bjeffhind"
    assert manifest["model_policy"]["includes_cab"] is False
    assert manifest["model_policy"]["ir_policy"] == "amp-head-no-ir"
    assert manifest["models"][0]["priority"] is True
    assert manifest["models"][0]["metadata_name"] == "TopBoost-Gain5"
    assert manifest["models"][0]["recommended_latency_samples"] == 15


def test_inspect_nam_pack_rejects_empty_directory(tmp_path: Path) -> None:
    try:
        inspect_nam_pack(tmp_path, tone_url="https://example.test")
    except ValueError as exc:
        assert "no .nam files" in str(exc)
    else:
        raise AssertionError("expected empty pack to fail")
