from __future__ import annotations

import argparse
from pathlib import Path

from greybound_lab.audio import read_wav_mono
from greybound_lab.external_inputs import download_tone3000_inputs, download_tone3000_irs
from greybound_lab.integrated_neural import evaluate_integrated_neural_cell
from greybound_lab.metrics import compare_signals
from greybound_lab.nam import write_nam_pack_manifest
from greybound_lab.nam_render import render_nam
from greybound_lab.neural_cell import evaluate_neural_cell_against_spice, export_neural_cell_vectors
from greybound_lab.neural_cell import train_common_cathode_mlp
from greybound_lab.report import write_markdown_report
from greybound_lab.render import DEFAULT_IR_WAV, render_rig
from greybound_lab.rig_sweep import run_amp_control_sweep
from greybound_lab.segments import load_segments
from greybound_lab.spice import run_spice_fixture, write_spice_dataset
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
    render.add_argument("--ir-wav", type=Path, default=DEFAULT_IR_WAV)

    stimuli = subparsers.add_parser("generate-stimuli", help="Generate standard lab WAV stimuli and marker files.")
    stimuli.add_argument("--output-dir", type=Path, default=Path("lab/stimuli"))
    stimuli.add_argument("--sample-rate", type=int, default=44_100)

    spice = subparsers.add_parser("spice-run", help="Run a supported SPICE fixture and write a lab report.")
    spice.add_argument("--fixture", required=True, choices=["common-cathode-12ax7"])
    spice.add_argument("--output-dir", type=Path, default=Path("lab/references/spice"))

    spice_dataset = subparsers.add_parser(
        "spice-dataset",
        help="Run a supported SPICE fixture and write a dataset artifact plus manifest.",
    )
    spice_dataset.add_argument("--fixture", required=True, choices=["common-cathode-12ax7"])
    spice_dataset.add_argument("--output-dir", type=Path, default=Path("lab/datasets/spice"))

    train_cell = subparsers.add_parser(
        "train-neural-cell",
        help="Train an experimental neural-cell model from a SPICE dataset manifest.",
    )
    train_cell.add_argument("--cell", required=True, choices=["common-cathode-12ax7-mlp"])
    train_cell.add_argument("--dataset-manifest", required=True, type=Path)
    train_cell.add_argument("--output-dir", type=Path, default=Path("lab/models/common-cathode-12ax7-mlp-v1"))
    train_cell.add_argument("--epochs", type=int, default=300)
    train_cell.add_argument("--hidden-size", type=int, default=16)
    train_cell.add_argument("--learning-rate", type=float, default=1.0e-3)
    train_cell.add_argument("--stride", type=int, default=16)
    train_cell.add_argument("--seed", type=int, default=59)

    export_vectors = subparsers.add_parser(
        "export-neural-cell-vectors",
        help="Export Python reference vectors for a Greybound neural-cell artifact.",
    )
    export_vectors.add_argument("--descriptor", required=True, type=Path)
    export_vectors.add_argument("--output", required=True, type=Path)
    export_vectors.add_argument("--input-v", type=float, action="append")

    evaluate_cell = subparsers.add_parser(
        "evaluate-neural-cell",
        help="Evaluate a Greybound neural-cell artifact against a SPICE dataset manifest.",
    )
    evaluate_cell.add_argument("--descriptor", required=True, type=Path)
    evaluate_cell.add_argument("--dataset-manifest", required=True, type=Path)
    evaluate_cell.add_argument("--report", required=True, type=Path)
    evaluate_cell.add_argument("--stride", type=int, default=16)
    evaluate_cell.add_argument("--split", choices=["all", "train", "validation", "test"], default="all")

    integrated_cell = subparsers.add_parser(
        "evaluate-integrated-neural-cell",
        help="Render analytic/shadow/replace Nox30 runs and compare the integrated neural counterpart.",
    )
    integrated_cell.add_argument("--descriptor", required=True, type=Path)
    integrated_cell.add_argument("--component", default="nox30.first_stage")
    integrated_cell.add_argument("--rig", type=Path, default=Path("rigs/nox30-driven.json5"))
    integrated_cell.add_argument("--input-wav", type=Path, default=Path("lab/references/tone3000-inputs/Brit - Guitar.wav"))
    integrated_cell.add_argument("--binary", type=Path, default=Path("target/release/greybound-cli"))
    integrated_cell.add_argument("--output-dir", type=Path, default=Path("lab/reports/integrated-neural-first-stage"))
    integrated_cell.add_argument("--report", type=Path, default=Path("lab/reports/integrated-neural-first-stage.md"))
    integrated_cell.add_argument("--render-seconds", type=float, default=20.0)
    integrated_cell.add_argument("--sample-rate", type=int, default=48_000)
    integrated_cell.add_argument("--period-size", type=int, default=16)
    integrated_cell.add_argument("--input-db", type=float, default=0.0)
    integrated_cell.add_argument("--output-db", type=float, default=-12.0)
    integrated_cell.add_argument("--ir", action="store_true")
    integrated_cell.add_argument("--ir-wav", type=Path, default=DEFAULT_IR_WAV)
    integrated_cell.add_argument("--segments", type=Path)

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

    nam = subparsers.add_parser(
        "inspect-nam-pack",
        help="Inspect local NAM files and write a source-safe pack manifest.",
    )
    nam.add_argument("--pack-dir", required=True, type=Path)
    nam.add_argument("--manifest", required=True, type=Path)
    nam.add_argument("--tone-url", required=True)

    nam_render = subparsers.add_parser(
        "render-nam",
        help="Render a local NAM model through an external NAM A2 renderer command.",
    )
    nam_render.add_argument("--model", required=True, type=Path)
    nam_render.add_argument("--input-wav", required=True, type=Path)
    nam_render.add_argument("--output-wav", required=True, type=Path)
    nam_render.add_argument("--metadata", required=True, type=Path)
    nam_render.add_argument("--renderer-command", required=True)
    nam_render.add_argument("--render-seconds", type=float, default=20.0)
    nam_render.add_argument("--sample-rate", type=int, default=48_000)
    nam_render.add_argument("--input-db", type=float, default=0.0)
    nam_render.add_argument("--output-db", type=float, default=0.0)
    nam_render.add_argument("--ir-wav", type=Path)
    nam_render.add_argument("--dry-run", action="store_true")

    rig_sweep = subparsers.add_parser(
        "sweep-rig-vs-reference",
        help="Sweep a normalized amp control and compare generated Greybound renders against a NAM reference WAV.",
    )
    rig_sweep.add_argument("--rig", required=True, type=Path)
    rig_sweep.add_argument("--control", default="drive")
    rig_sweep.add_argument("--values", required=True, help="Comma-separated normalized values, for example 0.3,0.5,0.7")
    rig_sweep.add_argument("--input-wav", required=True, type=Path)
    rig_sweep.add_argument("--reference-wav", required=True, type=Path)
    rig_sweep.add_argument("--binary", type=Path, default=Path("target/release/greybound-cli"))
    rig_sweep.add_argument("--output-dir", type=Path, default=Path("lab/reports/rig-sweep"))
    rig_sweep.add_argument("--report", type=Path, default=Path("lab/reports/rig-sweep.md"))
    rig_sweep.add_argument("--metadata", type=Path, default=Path("lab/reports/rig-sweep.run.json"))
    rig_sweep.add_argument("--render-seconds", type=float, default=10.0)
    rig_sweep.add_argument("--sample-rate", type=int, default=48_000)
    rig_sweep.add_argument("--period-size", type=int, default=16)
    rig_sweep.add_argument("--input-db", type=float, default=0.0)
    rig_sweep.add_argument("--output-db", type=float, default=-12.0)
    rig_sweep.add_argument("--segments", type=Path)
    rig_sweep.add_argument("--max-lag-ms", type=float, default=100.0)

    args = parser.parse_args()
    if args.command == "compare-wav":
        run_compare_wav(args)
    elif args.command == "render-rig":
        run_render_rig(args)
    elif args.command == "generate-stimuli":
        run_generate_stimuli(args)
    elif args.command == "spice-run":
        run_spice(args)
    elif args.command == "spice-dataset":
        run_spice_dataset(args)
    elif args.command == "train-neural-cell":
        run_train_neural_cell(args)
    elif args.command == "export-neural-cell-vectors":
        run_export_neural_cell_vectors(args)
    elif args.command == "evaluate-neural-cell":
        run_evaluate_neural_cell(args)
    elif args.command == "evaluate-integrated-neural-cell":
        run_evaluate_integrated_neural_cell(args)
    elif args.command == "download-tone3000-inputs":
        run_download_tone3000_inputs(args)
    elif args.command == "download-tone3000-irs":
        run_download_tone3000_irs(args)
    elif args.command == "inspect-nam-pack":
        run_inspect_nam_pack(args)
    elif args.command == "render-nam":
        run_render_nam(args)
    elif args.command == "sweep-rig-vs-reference":
        run_sweep_rig_vs_reference(args)


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
        ir_wav=args.ir_wav,
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


def run_spice_dataset(args: argparse.Namespace) -> None:
    dataset_path, manifest_path = write_spice_dataset(args.fixture, args.output_dir, repo_root=Path.cwd())
    print(f"wrote {dataset_path}")
    print(f"wrote {manifest_path}")


def run_train_neural_cell(args: argparse.Namespace) -> None:
    if args.cell != "common-cathode-12ax7-mlp":
        raise SystemExit(f"unsupported neural cell {args.cell}")
    descriptor_path, weights_path, report_path = train_common_cathode_mlp(
        manifest_path=args.dataset_manifest,
        output_dir=args.output_dir,
        repo_root=Path.cwd(),
        epochs=args.epochs,
        hidden_size=args.hidden_size,
        learning_rate=args.learning_rate,
        stride=args.stride,
        seed=args.seed,
    )
    print(f"wrote {descriptor_path}")
    print(f"wrote {weights_path}")
    print(f"wrote {report_path}")


def run_export_neural_cell_vectors(args: argparse.Namespace) -> None:
    output_path = export_neural_cell_vectors(
        descriptor_path=args.descriptor,
        output_path=args.output,
        input_values=args.input_v,
    )
    print(f"wrote {output_path}")


def run_evaluate_neural_cell(args: argparse.Namespace) -> None:
    report_path = evaluate_neural_cell_against_spice(
        descriptor_path=args.descriptor,
        dataset_manifest_path=args.dataset_manifest,
        report_path=args.report,
        stride=args.stride,
        split=args.split,
    )
    print(f"wrote {report_path}")


def run_evaluate_integrated_neural_cell(args: argparse.Namespace) -> None:
    result = evaluate_integrated_neural_cell(
        repo_root=Path.cwd(),
        binary=args.binary,
        rig=args.rig,
        input_wav=args.input_wav,
        descriptor=args.descriptor,
        output_dir=args.output_dir,
        report=args.report,
        component=args.component,
        render_seconds=args.render_seconds,
        sample_rate_hz=args.sample_rate,
        period_size=args.period_size,
        input_gain_db=args.input_db,
        output_gain_db=args.output_db,
        ir_enabled=args.ir,
        ir_wav=args.ir_wav,
        segments=args.segments,
    )
    print(f"wrote {args.report}")
    print(f"replace residual {result.replace_vs_analytic.null_relative_db:.2f} dB relative")


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


def run_inspect_nam_pack(args: argparse.Namespace) -> None:
    manifest = write_nam_pack_manifest(args.pack_dir, args.manifest, tone_url=args.tone_url)
    print(f"wrote {args.manifest}")
    print(f"models {len(manifest['models'])}")
    print("priority " + ", ".join(manifest["priority_models"]))


def run_render_nam(args: argparse.Namespace) -> None:
    command = render_nam(
        repo_root=Path.cwd(),
        model=args.model,
        input_wav=args.input_wav,
        output_wav=args.output_wav,
        metadata=args.metadata,
        renderer_command=args.renderer_command,
        render_seconds=args.render_seconds,
        sample_rate_hz=args.sample_rate,
        input_gain_db=args.input_db,
        output_gain_db=args.output_db,
        ir_wav=args.ir_wav,
        dry_run=args.dry_run,
    )
    print("command " + " ".join(command))
    print(f"wrote {args.metadata}")
    if not args.dry_run:
        print(f"wrote {args.output_wav}")


def run_sweep_rig_vs_reference(args: argparse.Namespace) -> None:
    values = parse_float_csv(args.values)
    points = run_amp_control_sweep(
        repo_root=Path.cwd(),
        binary=args.binary,
        rig=args.rig,
        control=args.control,
        values=values,
        input_wav=args.input_wav,
        reference_wav=args.reference_wav,
        output_dir=args.output_dir,
        report=args.report,
        metadata=args.metadata,
        render_seconds=args.render_seconds,
        sample_rate_hz=args.sample_rate,
        period_size=args.period_size,
        input_gain_db=args.input_db,
        output_gain_db=args.output_db,
        segments=load_segments(args.segments) if args.segments else None,
        max_lag_ms=args.max_lag_ms,
    )
    best = min(points, key=lambda point: point.metrics.log_spectral_distance_db)
    print(f"wrote {args.report}")
    print(f"wrote {args.metadata}")
    print(
        "best "
        f"{args.control}={best.value:.3f} "
        f"lsd={best.metrics.log_spectral_distance_db:.2f}dB "
        f"null={best.metrics.null_relative_db:.2f}dB"
    )


def parse_float_csv(value: str) -> list[float]:
    try:
        return [float(part.strip()) for part in value.split(",") if part.strip()]
    except ValueError as exc:
        raise SystemExit(f"--values expects comma-separated numbers: {value}") from exc


if __name__ == "__main__":
    main()
