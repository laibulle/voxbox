from __future__ import annotations

import argparse
from pathlib import Path

from greybound_lab.audio import read_wav_mono
from greybound_lab.evaluation import evaluate_metrics, write_evaluation_json, write_evaluation_report
from greybound_lab.external_inputs import download_tone3000_inputs, download_tone3000_irs
from greybound_lab.graybox_cell import fit_common_cathode_graybox
from greybound_lab.integrated_neural import evaluate_integrated_neural_cell
from greybound_lab.metrics import compare_signals
from greybound_lab.minotaur import write_minotaur_klon_triage
from greybound_lab.nam import write_nam_pack_manifest
from greybound_lab.nam_render import render_nam
from greybound_lab.neural_blend import parse_alpha_csv, run_neural_blend_sweep
from greybound_lab.neural_cell import evaluate_neural_cell_against_spice, export_neural_cell_vectors
from greybound_lab.neural_cell import train_common_cathode_mlp, train_klon_drive_clip_tone_mlp
from greybound_lab.report import write_markdown_report
from greybound_lab.render import DEFAULT_IR_WAV, render_rig
from greybound_lab.reverb import evaluate_reverb, write_reverb_json, write_reverb_report
from greybound_lab.rig_sweep import run_amp_control_grid_sweep, sweep_score
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

    evaluate = subparsers.add_parser(
        "evaluate-wav",
        help="Compare a candidate WAV against a reference WAV and apply profile-specific quality gates.",
    )
    evaluate.add_argument("--candidate", required=True, type=Path)
    evaluate.add_argument("--reference", required=True, type=Path)
    evaluate.add_argument("--report", required=True, type=Path)
    evaluate.add_argument("--json", type=Path)
    evaluate.add_argument("--metadata", type=Path)
    evaluate.add_argument("--segments", type=Path)
    evaluate.add_argument("--max-lag-ms", type=float, default=100.0)
    evaluate.add_argument("--profile", choices=["amp-tone", "clipper", "regression"], default="amp-tone")

    evaluate_reverb_parser = subparsers.add_parser(
        "evaluate-reverb",
        help="Evaluate a reverb render against a dry/reference render with tail and health diagnostics.",
    )
    evaluate_reverb_parser.add_argument("--dry", required=True, type=Path)
    evaluate_reverb_parser.add_argument("--wet", required=True, type=Path)
    evaluate_reverb_parser.add_argument("--report", required=True, type=Path)
    evaluate_reverb_parser.add_argument("--json", type=Path)

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
    render.add_argument("--disable-neural-cell", action="store_true")

    stimuli = subparsers.add_parser("generate-stimuli", help="Generate standard lab WAV stimuli and marker files.")
    stimuli.add_argument("--output-dir", type=Path, default=Path("lab/stimuli"))
    stimuli.add_argument("--sample-rate", type=int, default=44_100)

    spice = subparsers.add_parser("spice-run", help="Run a supported SPICE fixture and write a lab report.")
    spice.add_argument("--fixture", required=True, choices=["common-cathode-12ax7", "klon-centaur"])
    spice.add_argument("--output-dir", type=Path, default=Path("lab/references/spice"))

    spice_dataset = subparsers.add_parser(
        "spice-dataset",
        help="Run a supported SPICE fixture and write a dataset artifact plus manifest.",
    )
    spice_dataset.add_argument("--fixture", required=True, choices=["common-cathode-12ax7", "klon-centaur"])
    spice_dataset.add_argument("--output-dir", type=Path, default=Path("lab/datasets/spice"))

    train_cell = subparsers.add_parser(
        "train-neural-cell",
        help="Train an experimental neural-cell model from a SPICE dataset manifest.",
    )
    train_cell.add_argument("--cell", required=True, choices=["common-cathode-12ax7-mlp", "klon-drive-clip-tone-mlp"])
    train_cell.add_argument("--dataset-manifest", required=True, type=Path)
    train_cell.add_argument("--output-dir", type=Path, default=Path("lab/models/common-cathode-12ax7-mlp-current"))
    train_cell.add_argument("--target", default="tone_ac_v")
    train_cell.add_argument("--epochs", type=int, default=1200)
    train_cell.add_argument("--hidden-size", type=int, default=32)
    train_cell.add_argument("--learning-rate", type=float, default=5.0e-4)
    train_cell.add_argument("--stride", type=int, default=8)
    train_cell.add_argument("--history-samples", type=int, default=1)
    train_cell.add_argument("--seed", type=int, default=59)

    fit_graybox = subparsers.add_parser(
        "fit-graybox-cell",
        help="Fit an experimental differentiable gray-box cell against a SPICE dataset manifest.",
    )
    fit_graybox.add_argument("--cell", required=True, choices=["common-cathode-12ax7-state"])
    fit_graybox.add_argument("--dataset-manifest", required=True, type=Path)
    fit_graybox.add_argument("--output-dir", type=Path, default=Path("lab/models/common-cathode-12ax7-graybox-state-current"))
    fit_graybox.add_argument("--epochs", type=int, default=220)
    fit_graybox.add_argument("--learning-rate", type=float, default=8.0e-3)
    fit_graybox.add_argument("--stride", type=int, default=16)
    fit_graybox.add_argument("--max-train-samples-per-stimulus", type=int, default=2048)
    fit_graybox.add_argument("--seed", type=int, default=59)

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
    integrated_cell.add_argument("--descriptor", type=Path)
    integrated_cell.add_argument("--graybox-config", type=Path)
    integrated_cell.add_argument("--component", default="nox30.first_stage")
    integrated_cell.add_argument("--rig", type=Path, default=Path("rigs/grey-nox.json5"))
    integrated_cell.add_argument("--input-wav", type=Path, default=Path("lab/references/tone3000-inputs/Brit - Guitar.wav"))
    integrated_cell.add_argument("--binary", type=Path, default=Path("target/release/greybound-cli"))
    integrated_cell.add_argument(
        "--output-dir",
        type=Path,
        default=Path("lab/reports/integrated-neural-first-stage-anchor-current"),
    )
    integrated_cell.add_argument(
        "--report",
        type=Path,
        default=Path("lab/reports/integrated-neural-first-stage-anchor-current.md"),
    )
    integrated_cell.add_argument("--render-seconds", type=float, default=20.0)
    integrated_cell.add_argument("--sample-rate", type=int, default=48_000)
    integrated_cell.add_argument("--period-size", type=int, default=16)
    integrated_cell.add_argument("--input-db", type=float, default=0.0)
    integrated_cell.add_argument("--output-db", type=float, default=-12.0)
    integrated_cell.add_argument("--ir", action="store_true")
    integrated_cell.add_argument("--ir-wav", type=Path, default=DEFAULT_IR_WAV)
    integrated_cell.add_argument("--segments", type=Path)
    integrated_cell.add_argument("--reference-wav", type=Path)

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
        help="Sweep normalized amp controls and compare generated Greybound renders against a NAM reference WAV.",
    )
    rig_sweep.add_argument("--rig", required=True, type=Path)
    rig_sweep.add_argument("--control", default="drive")
    rig_sweep.add_argument("--values", help="Comma-separated normalized values for --control, for example 0.3,0.5,0.7")
    rig_sweep.add_argument(
        "--sweep",
        action="append",
        help="Repeatable control=value,value grid spec, for example --sweep drive=0.5,0.8 --sweep volume=0.6,0.8",
    )
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

    blend_sweep = subparsers.add_parser(
        "sweep-neural-blend",
        help="Blend analytic and neural-replace WAVs offline and score each alpha against a NAM reference.",
    )
    blend_sweep.add_argument("--analytic-wav", required=True, type=Path)
    blend_sweep.add_argument("--replace-wav", required=True, type=Path)
    blend_sweep.add_argument("--reference-wav", required=True, type=Path)
    blend_sweep.add_argument("--output-dir", type=Path, default=Path("lab/reports/neural-blend-sweep"))
    blend_sweep.add_argument("--report", type=Path, default=Path("lab/reports/neural-blend-sweep.md"))
    blend_sweep.add_argument("--metadata", type=Path, default=Path("lab/reports/neural-blend-sweep.run.json"))
    blend_sweep.add_argument("--alphas", default="0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1")
    blend_sweep.add_argument("--segments", type=Path)
    blend_sweep.add_argument("--max-lag-ms", type=float, default=100.0)

    minotaur_triage = subparsers.add_parser(
        "minotaur-klon-triage",
        help="Write a SPICE/NAM/Rust triage report for the Minotaur/Klon pedal model.",
    )
    minotaur_triage.add_argument("--spice-data", type=Path, default=Path("lab/references/spice/klon-centaur.dat"))
    minotaur_triage.add_argument("--candidate-wav", required=True, type=Path)
    minotaur_triage.add_argument("--reference-wav", required=True, type=Path)
    minotaur_triage.add_argument("--report", type=Path, default=Path("lab/reports/klon-minotaur/minotaur-klon-triage.md"))
    minotaur_triage.add_argument("--metadata", type=Path)
    minotaur_triage.add_argument("--sweep-report", type=Path)
    minotaur_triage.add_argument("--segments", type=Path)
    minotaur_triage.add_argument("--max-lag-ms", type=float, default=100.0)

    args = parser.parse_args()
    if args.command == "compare-wav":
        run_compare_wav(args)
    elif args.command == "evaluate-wav":
        run_evaluate_wav(args)
    elif args.command == "evaluate-reverb":
        run_evaluate_reverb(args)
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
    elif args.command == "fit-graybox-cell":
        run_fit_graybox_cell(args)
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
    elif args.command == "sweep-neural-blend":
        run_sweep_neural_blend(args)
    elif args.command == "minotaur-klon-triage":
        run_minotaur_klon_triage(args)


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


def run_evaluate_wav(args: argparse.Namespace) -> None:
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
    result = evaluate_metrics(metrics, candidate.samples, profile=args.profile)
    write_evaluation_report(
        args.report,
        candidate_path=candidate.path,
        reference_path=reference.path,
        metrics=metrics,
        result=result,
        metadata_path=args.metadata,
    )
    if args.json:
        write_evaluation_json(args.json, metrics=metrics, result=result)
        print(f"wrote {args.json}")
    print(f"wrote {args.report}")
    print(f"verdict {result.verdict}")


def run_evaluate_reverb(args: argparse.Namespace) -> None:
    dry = read_wav_mono(args.dry)
    wet = read_wav_mono(args.wet)
    try:
        metrics = evaluate_reverb(dry, wet)
    except ValueError as error:
        raise SystemExit(str(error)) from error
    write_reverb_report(args.report, dry_path=dry.path, wet_path=wet.path, metrics=metrics)
    if args.json:
        write_reverb_json(args.json, metrics)
        print(f"wrote {args.json}")
    print(f"wrote {args.report}")
    print(f"verdict {metrics.verdict}")


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
        disable_neural_cell=args.disable_neural_cell,
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
    if args.cell == "klon-drive-clip-tone-mlp":
        descriptor_path, weights_path, report_path = train_klon_drive_clip_tone_mlp(
            manifest_path=args.dataset_manifest,
            output_dir=args.output_dir,
            repo_root=Path.cwd(),
            target=args.target,
            epochs=args.epochs,
            hidden_size=args.hidden_size,
            learning_rate=args.learning_rate,
            stride=args.stride,
            history_samples=args.history_samples,
            seed=args.seed,
        )
        print(f"wrote {descriptor_path}")
        print(f"wrote {weights_path}")
        print(f"wrote {report_path}")
        return
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
        history_samples=args.history_samples,
        seed=args.seed,
    )
    print(f"wrote {descriptor_path}")
    print(f"wrote {weights_path}")
    print(f"wrote {report_path}")


def run_fit_graybox_cell(args: argparse.Namespace) -> None:
    if args.cell != "common-cathode-12ax7-state":
        raise SystemExit(f"unsupported graybox cell {args.cell}")
    config_path, report_path = fit_common_cathode_graybox(
        manifest_path=args.dataset_manifest,
        output_dir=args.output_dir,
        repo_root=Path.cwd(),
        epochs=args.epochs,
        learning_rate=args.learning_rate,
        stride=args.stride,
        max_train_samples_per_stimulus=args.max_train_samples_per_stimulus,
        seed=args.seed,
    )
    print(f"wrote {config_path}")
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
    if args.descriptor is None and args.graybox_config is None:
        raise SystemExit("--descriptor or --graybox-config is required")
    if args.descriptor is not None and args.graybox_config is not None:
        raise SystemExit("--descriptor and --graybox-config are mutually exclusive")
    result = evaluate_integrated_neural_cell(
        repo_root=Path.cwd(),
        binary=args.binary,
        rig=args.rig,
        input_wav=args.input_wav,
        descriptor=args.descriptor,
        graybox_config=args.graybox_config,
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
        reference_wav=args.reference_wav,
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
    sweeps = parse_sweep_specs(args.sweep, args.control, args.values)
    points = run_amp_control_grid_sweep(
        repo_root=Path.cwd(),
        binary=args.binary,
        rig=args.rig,
        sweeps=sweeps,
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
    best = min(points, key=lambda point: sweep_score(point.metrics).total)
    score = sweep_score(best.metrics)
    print(f"wrote {args.report}")
    print(f"wrote {args.metadata}")
    print(
        "best "
        + " ".join(f"{control}={value:.3f}" for control, value in best.values.items())
        + " "
        f"score={score.total:.3f} "
        f"lsd={best.metrics.log_spectral_distance_db:.2f}dB "
        f"null={best.metrics.null_relative_db:.2f}dB"
    )


def run_sweep_neural_blend(args: argparse.Namespace) -> None:
    try:
        alphas = parse_alpha_csv(args.alphas)
    except ValueError as exc:
        raise SystemExit(str(exc)) from exc
    points = run_neural_blend_sweep(
        analytic_wav=args.analytic_wav,
        replace_wav=args.replace_wav,
        reference_wav=args.reference_wav,
        output_dir=args.output_dir,
        report=args.report,
        metadata=args.metadata,
        alphas=alphas,
        segments=load_segments(args.segments) if args.segments else None,
        max_lag_ms=args.max_lag_ms,
    )
    best = min(points, key=lambda point: point.score.total)
    print(f"wrote {args.report}")
    print(f"wrote {args.metadata}")
    print(f"best alpha={best.alpha:.3f} score={best.score.total:.4f}")


def run_minotaur_klon_triage(args: argparse.Namespace) -> None:
    write_minotaur_klon_triage(
        repo_root=Path.cwd(),
        spice_data=args.spice_data,
        candidate_wav=args.candidate_wav,
        reference_wav=args.reference_wav,
        report=args.report,
        metadata=args.metadata,
        sweep_report=args.sweep_report,
        segments=load_segments(args.segments) if args.segments else None,
        max_lag_ms=args.max_lag_ms,
    )
    print(f"wrote {args.report}")
    if args.metadata:
        print(f"wrote {args.metadata}")


def parse_sweep_specs(sweep_specs: list[str] | None, control: str, values: str | None) -> dict[str, list[float]]:
    if sweep_specs:
        if values is not None:
            raise SystemExit("use either repeatable --sweep or --control/--values, not both")
        sweeps: dict[str, list[float]] = {}
        for spec in sweep_specs:
            name, separator, raw_values = spec.partition("=")
            if not separator or not name.strip() or not raw_values.strip():
                raise SystemExit(f"--sweep expects control=value,value: {spec}")
            sweeps[name.strip()] = parse_float_csv(raw_values)
        return sweeps
    if values is None:
        raise SystemExit("--values is required when --sweep is not used")
    return {control: parse_float_csv(values)}


def parse_float_csv(value: str) -> list[float]:
    try:
        return [float(part.strip()) for part in value.split(",") if part.strip()]
    except ValueError as exc:
        raise SystemExit(f"--values expects comma-separated numbers: {value}") from exc


if __name__ == "__main__":
    main()
