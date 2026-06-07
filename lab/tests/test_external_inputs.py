from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

from greybound_lab import external_inputs


class FakeResponse:
    def __init__(self, data: bytes) -> None:
        self.data = data

    def __enter__(self) -> "FakeResponse":
        return self

    def __exit__(self, *args: object) -> None:
        return None

    def read(self, size: int = -1) -> bytes:
        if size == -1:
            data = self.data
            self.data = b""
            return data
        data = self.data[:size]
        self.data = self.data[size:]
        return data


def test_download_tone3000_inputs_writes_files_and_manifest(
    tmp_path: Path,
    monkeypatch,
) -> None:
    listing = [
        {
            "name": "Clean - Guitar.wav",
            "type": "file",
            "size": 4,
            "sha": "abc123",
            "html_url": "https://example.test/blob/Clean.wav",
            "download_url": "https://example.test/raw/Clean.wav",
        },
        {
            "name": "README.md",
            "type": "file",
            "size": 123,
            "download_url": "https://example.test/raw/README.md",
        },
    ]

    def fake_urlopen(request, timeout: float):
        url = request.full_url
        if url == external_inputs.TONE3000_INPUTS_API_URL:
            return FakeResponse(json.dumps(listing).encode("utf-8"))
        if url == "https://example.test/raw/Clean.wav":
            return FakeResponse(b"RIFF")
        raise AssertionError(f"unexpected URL {url}")

    monkeypatch.setattr(external_inputs, "urlopen", fake_urlopen)

    downloaded = external_inputs.download_tone3000_inputs(tmp_path)

    assert [item.name for item in downloaded] == ["Clean - Guitar.wav"]
    assert downloaded[0].downloaded is True
    assert (tmp_path / "Clean - Guitar.wav").read_bytes() == b"RIFF"

    manifest = json.loads((tmp_path / "manifest.json").read_text(encoding="utf-8"))
    assert manifest["schema_version"] == 1
    assert manifest["files"][0]["github_sha"] == "abc123"
    assert "local Greybound R&D" in manifest["license_notes"]
    assert "Clean - Guitar.wav" in (tmp_path / "README.md").read_text(encoding="utf-8")


def test_download_tone3000_inputs_skips_existing_files(tmp_path: Path, monkeypatch) -> None:
    existing = tmp_path / "Clean - Guitar.wav"
    existing.write_bytes(b"OLD")
    listing = [
        {
            "name": "Clean - Guitar.wav",
            "type": "file",
            "size": 4,
            "sha": "abc123",
            "download_url": "https://example.test/raw/Clean.wav",
        }
    ]
    calls = SimpleNamespace(downloads=0)

    def fake_urlopen(request, timeout: float):
        url = request.full_url
        if url == external_inputs.TONE3000_INPUTS_API_URL:
            return FakeResponse(json.dumps(listing).encode("utf-8"))
        calls.downloads += 1
        return FakeResponse(b"RIFF")

    monkeypatch.setattr(external_inputs, "urlopen", fake_urlopen)

    downloaded = external_inputs.download_tone3000_inputs(tmp_path)

    assert downloaded[0].downloaded is False
    assert calls.downloads == 0
    assert existing.read_bytes() == b"OLD"


def test_download_tone3000_irs_uses_ir_source(tmp_path: Path, monkeypatch) -> None:
    listing = [
        {
            "name": "celestion.wav",
            "type": "file",
            "size": 4,
            "sha": "def456",
            "html_url": "https://example.test/blob/celestion.wav",
            "download_url": "https://example.test/raw/celestion.wav",
        }
    ]

    def fake_urlopen(request, timeout: float):
        url = request.full_url
        if url == external_inputs.TONE3000_IRS_API_URL:
            return FakeResponse(json.dumps(listing).encode("utf-8"))
        if url == "https://example.test/raw/celestion.wav":
            return FakeResponse(b"RIFF")
        raise AssertionError(f"unexpected URL {url}")

    monkeypatch.setattr(external_inputs, "urlopen", fake_urlopen)

    downloaded = external_inputs.download_tone3000_irs(tmp_path)

    assert [item.name for item in downloaded] == ["celestion.wav"]
    assert (tmp_path / "celestion.wav").read_bytes() == b"RIFF"
    manifest = json.loads((tmp_path / "manifest.json").read_text(encoding="utf-8"))
    assert manifest["provider"] == "TONE3000 neural-amp-modeler-wasm impulse responses"
    assert "IR rights" in manifest["license_notes"]
