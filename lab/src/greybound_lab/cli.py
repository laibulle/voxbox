from __future__ import annotations

import argparse
from pathlib import Path

from greybound_lab.audio import read_wav_mono
from greybound_lab.external_inputs import download_tone3000_inputs, download_tone3000_irs
from greybound_lab.metrics import compare_signals
from greybound_lab.report import write_markdown_report
from greybound_lab.render import render_rig
from greybound_lab.segments import load_segments
from greybound_lab.spice import run_spice_fixture
from greybound_lab.stimuli import generate_stimuli


def main() -> None:
    parser = argparse.ArgumentParser(prog="greybound-lab")
    subparsers = parser.add_subparsers(dest="command", required=True)

    compare = subparsers.add_parser("compare-wav", help="Compare a candidate WAV against a reference WAV.")
    compare.add_argument("--candidate", required=True, type=Path)
    compare.add_argument("--reference", required=True, type=Path)
    compare.add_argument("--report", required=True, type=Path)
    compare.add_argument("--metadata", type=Path)
    compare.add_argument("--segments", type=Path)
    compare.add_argument("--max-lag-ms", type=float, default=100.0)

    render = subparsers.add_parser("render-rig", help="Render a Greybound rig to WAV and write lab metadata.")
    render.add_argument("--rig", required=True, type=Path)
    render.add_argument("--input-wav", required=True, type=Path)
    render.add_argument("--output-wav", required=True, type=Path)
    render.add_argument("--metadata", required=True, type=Path)
    render.add_argument("--binary", type=Path, default=Path("target/release/greybound-cli"))
    render.add_argument("--render-seconds", type=float, default=10.0)
    render.add_argument("--sample-rate", type=int, default=44_100)
    render.add_argument("--period-size", type=int, default=16)
    render.add_argument("--input-db", type=float, default=0.0)
    render.add_argument("--output-db", type=float, default=-18.0)
    render.add_argument("--ir", action="store_true")

    stimuli = subparsers.add_parser("generate-stimuli", help="Generate standard lab WAV stimuli and marker files.")
    stimuli.add_argument("--output-dir", type=Path, default=Path("lab/stimuli"))
    stimuli.add_argument("--sample-rate", type=int, default=44_100)

    spice = subparsers.add_parser("spice-run", help="Run a supported SPICE fixture and write a lab report.")
    spice.add_argument("--fixture", required=True, choices=["common-cathode-12ax7"])
    spice.add_argument("--output-dir", type=Path, default=Path("lab/references/spice"))

    inputs = subparsers.add_parser(
        "download-tone3000-inputs",
        help="Download public TONE3000 DI input WAV files for local NAM/Greybound tests.",
    )
    inputs.add_argument("--output-dir", type=Path, default=Path("lab/references/tone3000-inputs"))
    inputs.add_argument("--overwrite", action="store_true")

    irs = subparsers.add_parser(
        "download-tone3000-irs",
        help="Download public TONE3000 IR WAV files for local NAM/Greybound tests.",
    )
    irs.add_argument("--output-dir", type=Path, default=Path("lab/references/tone3000-irs"))
    irs.add_argument("--overwrite", action="store_true")

    args = parser.parse_args()
    if args.command == "compare-wav":
        run_compare_wav(args)
    elif args.command == "render-rig":
        run_render_rig(args)
    elif args.command == "generate-stimuli":
        run_generate_stimuli(args)
    elif args.command == "spice-run":
        run_spice(args)
    elif args.command == "download-tone3000-inputs":
        run_download_tone3000_inputs(args)
    elif args.command == "download-tone3000-irs":
        run_download_tone3000_irs(args)


def run_compare_wav(args: argparse.Namespace) -> None:
    candidate = read_wav_mono(args.candidate)
    reference = read_wav_mono(args.reference)
    if candidate.sample_rate != reference.sample_rate:
        raise SystemExit(
            f"sample-rate mismatch: candidate={candidate.sample_rate} Hz, "
            f"reference={reference.sample_rate} Hz"
        )
    metrics = compare_signals(
        candidate.samples,
        reference.samples,
        candidate.sample_rate,
        max_lag_ms=args.max_lag_ms,
        segments=load_segments(args.segments) if args.segments else None,
    )
    write_markdown_report(
        args.report,
        candidate.path,
        reference.path,
        metrics,
        metadata_path=args.metadata,
    )
    print(f"wrote {args.report}")


def run_render_rig(args: argparse.Namespace) -> None:
    repo_root = Path.cwd()
    render_rig(
        repo_root=repo_root,
        binary=args.binary,
        rig=args.rig,
        input_wav=args.input_wav,
        output_wav=args.output_wav,
        metadata=args.metadata,
        render_seconds=args.render_seconds,
        sample_rate_hz=args.sample_rate,
        period_size=args.period_size,
        input_gain_db=args.input_db,
        output_gain_db=args.output_db,
        ir_enabled=args.ir,
    )
    print(f"wrote {args.output_wav}")
    print(f"wrote {args.metadata}")


def run_generate_stimuli(args: argparse.Namespace) -> None:
    generated = generate_stimuli(args.output_dir, sample_rate_hz=args.sample_rate)
    for item in generated:
        print(f"wrote {item.wav_path}")
        print(f"wrote {item.markers_path}")


def run_spice(args: argparse.Namespace) -> None:
    data_path, report_path = run_spice_fixture(args.fixture, args.output_dir, repo_root=Path.cwd())
    print(f"wrote {data_path}")
    print(f"wrote {report_path}")


def run_download_tone3000_inputs(args: argparse.Namespace) -> None:
    downloaded = download_tone3000_inputs(args.output_dir, overwrite=args.overwrite)
    for item in downloaded:
        action = "downloaded" if item.downloaded else "kept"
        print(f"{action} {item.local_path}")
    print(f"wrote {args.output_dir / 'manifest.json'}")
    print(f"wrote {args.output_dir / 'README.md'}")


def run_download_tone3000_irs(args: argparse.Namespace) -> None:
    downloaded = download_tone3000_irs(args.output_dir, overwrite=args.overwrite)
    for item in downloaded:
        action = "downloaded" if item.downloaded else "kept"
        print(f"{action} {item.local_path}")
    print(f"wrote {args.output_dir / 'manifest.json'}")
    print(f"wrote {args.output_dir / 'README.md'}")


if __name__ == "__main__":
    main()
