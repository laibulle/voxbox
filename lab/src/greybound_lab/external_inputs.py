from __future__ import annotations

import json
import shutil
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.request import Request, urlopen

TONE3000_INPUTS_API_URL = (
    "https://api.github.com/repos/tone-3000/neural-amp-modeler-wasm/"
    "contents/ui/public/inputs?ref=main"
)
TONE3000_INPUTS_SOURCE_URL = (
    "https://github.com/tone-3000/neural-amp-modeler-wasm/tree/main/ui/public/inputs"
)
TONE3000_IRS_API_URL = (
    "https://api.github.com/repos/tone-3000/neural-amp-modeler-wasm/"
    "contents/ui/public/irs?ref=main"
)
TONE3000_IRS_SOURCE_URL = (
    "https://github.com/tone-3000/neural-amp-modeler-wasm/tree/main/ui/public/irs"
)


@dataclass(frozen=True)
class DownloadedInput:
    name: str
    local_path: Path
    size_bytes: int
    sha: str
    source_url: str
    download_url: str
    downloaded: bool


def download_tone3000_inputs(
    output_dir: Path,
    *,
    overwrite: bool = False,
    timeout_s: float = 60.0,
) -> list[DownloadedInput]:
    return _download_tone3000_wavs(
        output_dir,
        api_url=TONE3000_INPUTS_API_URL,
        source_url=TONE3000_INPUTS_SOURCE_URL,
        provider="TONE3000 neural-amp-modeler-wasm input audio",
        readme_title="TONE3000 Input Audio",
        expected_format="20-30 seconds, 48 kHz, 24-bit, mono WAV",
        rights_note=(
            "Downloaded for local Greybound R&D only. Do not redistribute audio files "
            "from this directory unless the sample rights are explicit."
        ),
        overwrite=overwrite,
        timeout_s=timeout_s,
    )


def download_tone3000_irs(
    output_dir: Path,
    *,
    overwrite: bool = False,
    timeout_s: float = 60.0,
) -> list[DownloadedInput]:
    return _download_tone3000_wavs(
        output_dir,
        api_url=TONE3000_IRS_API_URL,
        source_url=TONE3000_IRS_SOURCE_URL,
        provider="TONE3000 neural-amp-modeler-wasm impulse responses",
        readme_title="TONE3000 Impulse Responses",
        expected_format="48 kHz, 24-bit, mono or stereo WAV",
        rights_note=(
            "Downloaded for local Greybound R&D only. Do not redistribute IR files "
            "from this directory unless the IR rights are explicit."
        ),
        overwrite=overwrite,
        timeout_s=timeout_s,
    )


def _download_tone3000_wavs(
    output_dir: Path,
    *,
    api_url: str,
    source_url: str,
    provider: str,
    readme_title: str,
    expected_format: str,
    rights_note: str,
    overwrite: bool,
    timeout_s: float,
) -> list[DownloadedInput]:
    output_dir.mkdir(parents=True, exist_ok=True)
    remote_items = _fetch_json(api_url, timeout_s=timeout_s)
    if not isinstance(remote_items, list):
        raise ValueError("TONE3000 API did not return a directory listing")

    wav_items = [
        item
        for item in remote_items
        if isinstance(item, dict)
        and item.get("type") == "file"
        and str(item.get("name", "")).lower().endswith(".wav")
        and item.get("download_url")
    ]
    if not wav_items:
        raise ValueError("TONE3000 API did not expose any WAV files")

    downloaded: list[DownloadedInput] = []
    for item in sorted(wav_items, key=lambda value: str(value["name"]).lower()):
        name = str(item["name"])
        local_path = output_dir / name
        did_download = overwrite or not local_path.exists()
        if did_download:
            _download_file(str(item["download_url"]), local_path, timeout_s=timeout_s)
        downloaded.append(
            DownloadedInput(
                name=name,
                local_path=local_path,
                size_bytes=int(item.get("size") or local_path.stat().st_size),
                sha=str(item.get("sha", "")),
                source_url=str(item.get("html_url", source_url)),
                download_url=str(item["download_url"]),
                downloaded=did_download,
            )
        )

    _write_manifest(output_dir, downloaded, provider=provider, source_url=source_url, rights_note=rights_note)
    _write_readme(
        output_dir,
        downloaded,
        title=readme_title,
        source_url=source_url,
        expected_format=expected_format,
        rights_note=rights_note,
    )
    return downloaded


def _fetch_json(url: str, *, timeout_s: float) -> Any:
    request = Request(url, headers={"Accept": "application/vnd.github+json"})
    with urlopen(request, timeout=timeout_s) as response:
        return json.loads(response.read().decode("utf-8"))


def _download_file(url: str, destination: Path, *, timeout_s: float) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = Request(url, headers={"Accept": "application/octet-stream"})
    tmp_path = destination.with_suffix(destination.suffix + ".tmp")
    with urlopen(request, timeout=timeout_s) as response, tmp_path.open("wb") as handle:
        shutil.copyfileobj(response, handle)
    tmp_path.replace(destination)


def _write_manifest(
    output_dir: Path,
    files: list[DownloadedInput],
    *,
    provider: str,
    source_url: str,
    rights_note: str,
) -> None:
    manifest = {
        "$schema": "../../schemas/external-inputs-manifest.schema.json",
        "schema_version": 1,
        "provider": provider,
        "source_url": source_url,
        "retrieved_at": datetime.now(timezone.utc).isoformat(),
        "license_notes": rights_note,
        "files": [
            {
                "name": item.name,
                "path": item.local_path.name,
                "size_bytes": item.size_bytes,
                "github_sha": item.sha,
                "source_url": item.source_url,
                "download_url": item.download_url,
            }
            for item in files
        ],
    }
    (output_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def _write_readme(
    output_dir: Path,
    files: list[DownloadedInput],
    *,
    title: str,
    source_url: str,
    expected_format: str,
    rights_note: str,
) -> None:
    lines = [
        f"# {title}",
        "",
        "Generated by `greybound-lab`.",
        "",
        "These WAV files are public demonstration assets from TONE3000's",
        "`neural-amp-modeler-wasm` repository.",
        "",
        rights_note,
        "",
        f"Source: {source_url}",
        "",
        f"Expected upstream format: {expected_format}.",
        "",
        "## Files",
        "",
    ]
    lines.extend(f"- `{item.name}` ({item.size_bytes} bytes)" for item in files)
    lines.append("")
    (output_dir / "README.md").write_text("\n".join(lines), encoding="utf-8")
