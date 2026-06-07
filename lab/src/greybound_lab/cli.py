from __future__ import annotations

import argparse
from pathlib import Path

from greybound_lab.audio import read_wav_mono
from greybound_lab.metrics import compare_signals
from greybound_lab.report import write_markdown_report
from greybound_lab.render import render_rig
from greybound_lab.segments import load_segments


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

    args = parser.parse_args()
    if args.command == "compare-wav":
        run_compare_wav(args)
    elif args.command == "render-rig":
        run_render_rig(args)


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


if __name__ == "__main__":
    main()
