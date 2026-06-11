from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
from dataclasses import asdict, dataclass
from datetime import UTC, datetime
from pathlib import Path

import numpy as np

from greybound_lab.metrics import linear_to_db, rms
from greybound_lab.render import git_revision, relative_or_absolute


@dataclass(frozen=True)
class SpiceFixture:
    name: str
    netlist_path: Path
    tmp_data_path: Path
    signals: tuple[str, ...]


@dataclass(frozen=True)
class SpiceTrace:
    time_s: np.ndarray
    signals: dict[str, np.ndarray]


@dataclass(frozen=True)
class CommonCathodeSpiceMetrics:
    plate_dc_v: float
    cathode_dc_v: float
    bplus_dc_v: float
    input_rms_v: float
    grid_rms_v: float
    plate_rms_v: float
    cathode_rms_v: float
    plate_gain: float
    plate_gain_db: float
    grid_coupling_loss_db: float


@dataclass(frozen=True)
class KlonCentaurSpiceMetrics:
    input_rms_v: float
    buffer_rms_v: float
    clean_rms_v: float
    drive_rms_v: float
    clip_rms_v: float
    mix_rms_v: float
    tone_rms_v: float
    output_rms_v: float
    output_peak_v: float
    output_gain: float
    output_gain_db: float
    clip_peak_v: float
    clip_asymmetry_v: float


@dataclass(frozen=True)
class CommonCathodeDatasetCase:
    stimulus_id: str
    kind: str
    expression: str
    parameters: dict[str, float | str]
    split: str
    settle_time_s: float = 0.030
    transient_stop_s: float = 0.060
    transient_step_s: float = 1.0e-6


@dataclass(frozen=True)
class KlonCentaurDatasetCase:
    stimulus_id: str
    kind: str
    expression: str
    parameters: dict[str, float | str]
    split: str
    gain: float
    treble: float
    level: float = 0.70
    transient_stop_s: float = 0.120
    transient_step_s: float = 2.0e-6


FIXTURES = {
    "common-cathode-12ax7": SpiceFixture(
        name="common-cathode-12ax7",
        netlist_path=Path("tests/fixtures/circuit/common_cathode_12ax7.cir"),
        tmp_data_path=Path("/tmp/greybound_common_cathode_12ax7.dat"),
        signals=("input", "grid", "plate", "cathode", "bplus"),
    ),
    "klon-centaur": SpiceFixture(
        name="klon-centaur",
        netlist_path=Path("tests/fixtures/circuit/klon_centaur.cir"),
        tmp_data_path=Path("/tmp/greybound_klon_centaur.dat"),
        signals=("input", "buffer", "clean", "drive", "clip", "mix", "tone", "output"),
    )
}


def run_spice_fixture(name: str, output_dir: Path, repo_root: Path) -> tuple[Path, Path]:
    fixture = FIXTURES.get(name)
    if fixture is None:
        supported = ", ".join(sorted(FIXTURES))
        raise ValueError(f"unknown SPICE fixture {name!r}; supported fixtures: {supported}")

    output_dir.mkdir(parents=True, exist_ok=True)
    subprocess.run(["ngspice", "-b", str(fixture.netlist_path)], cwd=repo_root, check=True)
    if not fixture.tmp_data_path.exists():
        raise FileNotFoundError(f"SPICE did not produce {fixture.tmp_data_path}")

    data_path = output_dir / f"{fixture.name}.dat"
    report_path = output_dir / f"{fixture.name}.md"
    shutil.copyfile(fixture.tmp_data_path, data_path)
    trace = parse_wrdata(data_path, fixture.signals)
    if fixture.name == "common-cathode-12ax7":
        metrics = common_cathode_metrics(trace)
        write_common_cathode_report(report_path, fixture, data_path, metrics)
    elif fixture.name == "klon-centaur":
        metrics = klon_centaur_metrics(trace)
        write_klon_centaur_report(report_path, fixture, data_path, metrics)
    else:
        raise ValueError(f"no report writer for {fixture.name}")
    return data_path, report_path


def write_spice_dataset(
    name: str,
    output_dir: Path,
    repo_root: Path,
) -> tuple[Path, Path]:
    fixture = FIXTURES.get(name)
    if fixture is None:
        supported = ", ".join(sorted(FIXTURES))
        raise ValueError(f"unknown SPICE fixture {name!r}; supported fixtures: {supported}")
    if fixture.name == "klon-centaur":
        return write_klon_centaur_dataset(fixture, output_dir, repo_root)
    if fixture.name != "common-cathode-12ax7":
        raise ValueError(f"no dataset writer for {fixture.name}")

    output_dir.mkdir(parents=True, exist_ok=True)
    netlist_dir = output_dir / "netlists"
    trace_dir = output_dir / "traces"
    netlist_dir.mkdir(parents=True, exist_ok=True)
    trace_dir.mkdir(parents=True, exist_ok=True)

    cases = common_cathode_dataset_cases()
    traces: dict[str, SpiceTrace] = {}
    raw_paths: dict[str, Path] = {}
    netlist_paths: dict[str, Path] = {}
    for case in cases:
        netlist_path = netlist_dir / f"{case.stimulus_id}.cir"
        raw_path = trace_dir / f"{case.stimulus_id}.dat"
        netlist_path.write_text(
            common_cathode_generated_netlist(case, raw_path),
            encoding="utf-8",
        )
        subprocess.run(["ngspice", "-b", str(netlist_path)], cwd=repo_root, check=True)
        if not raw_path.exists():
            raise FileNotFoundError(f"SPICE did not produce {raw_path}")
        raw_paths[case.stimulus_id] = raw_path
        netlist_paths[case.stimulus_id] = netlist_path
        traces[case.stimulus_id] = parse_wrdata(raw_path, fixture.signals)

    reference_case = "sine_1khz_20mv"
    trace = traces[reference_case]
    metrics = common_cathode_metrics(trace, settle_time_s=0.030)
    dataset_path = output_dir / f"{fixture.name}.dataset.npz"
    manifest_path = output_dir / f"{fixture.name}.dataset.json"
    report_path = output_dir / f"{fixture.name}.dataset.md"

    arrays = {}
    for stimulus_id, case_trace in traces.items():
        prefix = stimulus_id + "__"
        arrays[prefix + "time_s"] = case_trace.time_s.astype(np.float64)
        arrays[prefix + "input_v"] = case_trace.signals["input"].astype(np.float64)
        arrays[prefix + "grid_v"] = case_trace.signals["grid"].astype(np.float64)
        arrays[prefix + "plate_v"] = case_trace.signals["plate"].astype(np.float64)
        arrays[prefix + "cathode_v"] = case_trace.signals["cathode"].astype(np.float64)
        arrays[prefix + "bplus_v"] = case_trace.signals["bplus"].astype(np.float64)
        arrays[prefix + "plate_ac_v"] = _remove_dc(case_trace.signals["plate"]).astype(np.float64)
    np.savez(dataset_path, **arrays)

    write_common_cathode_dataset_report(report_path, fixture, cases, metrics)
    manifest = common_cathode_sweep_dataset_manifest(
        fixture=fixture,
        repo_root=repo_root,
        cases=cases,
        raw_paths=raw_paths,
        netlist_paths=netlist_paths,
        dataset_path=dataset_path,
        report_path=report_path,
        metrics=metrics,
    )
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return dataset_path, manifest_path


def write_klon_centaur_dataset(
    fixture: SpiceFixture,
    output_dir: Path,
    repo_root: Path,
) -> tuple[Path, Path]:
    output_dir.mkdir(parents=True, exist_ok=True)
    netlist_dir = output_dir / "netlists"
    trace_dir = output_dir / "traces"
    netlist_dir.mkdir(parents=True, exist_ok=True)
    trace_dir.mkdir(parents=True, exist_ok=True)

    cases = klon_centaur_dataset_cases()
    traces: dict[str, SpiceTrace] = {}
    raw_paths: dict[str, Path] = {}
    netlist_paths: dict[str, Path] = {}
    source_netlist = (repo_root / fixture.netlist_path).read_text(encoding="utf-8")
    for case in cases:
        netlist_path = netlist_dir / f"{case.stimulus_id}.cir"
        raw_path = trace_dir / f"{case.stimulus_id}.dat"
        netlist_path.write_text(
            klon_centaur_generated_netlist(source_netlist, case, raw_path),
            encoding="utf-8",
        )
        subprocess.run(["ngspice", "-b", str(netlist_path)], cwd=repo_root, check=True)
        if not raw_path.exists():
            raise FileNotFoundError(f"SPICE did not produce {raw_path}")
        raw_paths[case.stimulus_id] = raw_path
        netlist_paths[case.stimulus_id] = netlist_path
        traces[case.stimulus_id] = parse_wrdata(raw_path, fixture.signals)

    reference_case = "sine_1khz_120mv_gain55_treble60"
    metrics = klon_centaur_metrics(traces[reference_case])
    dataset_path = output_dir / f"{fixture.name}.dataset.npz"
    manifest_path = output_dir / f"{fixture.name}.dataset.json"
    report_path = output_dir / f"{fixture.name}.dataset.md"

    arrays = {}
    for stimulus_id, case_trace in traces.items():
        prefix = stimulus_id + "__"
        arrays[prefix + "time_s"] = case_trace.time_s.astype(np.float64)
        for signal_name in fixture.signals:
            samples = case_trace.signals[signal_name]
            arrays[prefix + signal_name + "_v"] = samples.astype(np.float64)
            arrays[prefix + signal_name + "_ac_v"] = _remove_dc(samples).astype(np.float64)
    np.savez(dataset_path, **arrays)

    write_klon_centaur_dataset_report(report_path, fixture, cases, metrics)
    manifest = klon_centaur_dataset_manifest(
        fixture=fixture,
        repo_root=repo_root,
        cases=cases,
        raw_paths=raw_paths,
        netlist_paths=netlist_paths,
        dataset_path=dataset_path,
        report_path=report_path,
        metrics=metrics,
    )
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return dataset_path, manifest_path


def klon_centaur_dataset_cases() -> list[KlonCentaurDatasetCase]:
    return [
        KlonCentaurDatasetCase(
            stimulus_id="sine_1khz_40mv_gain55_treble60",
            kind="sine_level_sweep",
            expression="0.040*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.040},
            split="train",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_1khz_120mv_gain55_treble60",
            kind="sine_level_sweep",
            expression="0.120*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.120},
            split="train",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_1khz_240mv_gain55_treble60",
            kind="sine_level_sweep",
            expression="0.240*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.240},
            split="train",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_1khz_120mv_gain25_treble60",
            kind="gain_control_sweep",
            expression="0.120*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.120},
            split="train",
            gain=0.25,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_1khz_120mv_gain80_treble60",
            kind="gain_control_sweep",
            expression="0.120*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.120},
            split="validation",
            gain=0.80,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_1khz_120mv_gain55_treble30",
            kind="treble_control_sweep",
            expression="0.120*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.120},
            split="train",
            gain=0.55,
            treble=0.30,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_1khz_120mv_gain55_treble85",
            kind="treble_control_sweep",
            expression="0.120*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.120},
            split="validation",
            gain=0.55,
            treble=0.85,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_250hz_120mv_gain55_treble60",
            kind="frequency_sweep",
            expression="0.120*sin(2*pi*250*time)",
            parameters={"frequency_hz": 250.0, "amplitude_v": 0.120},
            split="train",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_500hz_120mv_gain55_treble60",
            kind="frequency_sweep",
            expression="0.120*sin(2*pi*500*time)",
            parameters={"frequency_hz": 500.0, "amplitude_v": 0.120},
            split="train",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_2khz_120mv_gain55_treble60",
            kind="frequency_sweep",
            expression="0.120*sin(2*pi*2000*time)",
            parameters={"frequency_hz": 2000.0, "amplitude_v": 0.120},
            split="train",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_4khz_120mv_gain55_treble60",
            kind="frequency_sweep",
            expression="0.120*sin(2*pi*4000*time)",
            parameters={"frequency_hz": 4000.0, "amplitude_v": 0.120},
            split="train",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="sine_3khz_120mv_gain55_treble60",
            kind="frequency_sweep",
            expression="0.120*sin(2*pi*3000*time)",
            parameters={"frequency_hz": 3000.0, "amplitude_v": 0.120},
            split="test",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="two_tone_997_1499_120mv_gain55_treble60",
            kind="two_tone_imd",
            expression="0.060*sin(2*pi*997*time)+0.060*sin(2*pi*1499*time)",
            parameters={"first_hz": 997.0, "second_hz": 1499.0, "combined_peak_v": 0.120},
            split="train",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="two_tone_701_1301_120mv_gain55_treble60",
            kind="two_tone_imd",
            expression="0.060*sin(2*pi*701*time)+0.060*sin(2*pi*1301*time)",
            parameters={"first_hz": 701.0, "second_hz": 1301.0, "combined_peak_v": 0.120},
            split="test",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="burst_1khz_180mv_gain55_treble60",
            kind="dynamic_burst",
            expression=(
                "0.180*sin(2*pi*1000*time)"
                "*(0.5+0.5*tanh((time-0.032)/0.0003))"
                "*(0.5-0.5*tanh((time-0.072)/0.0003))"
            ),
            parameters={
                "frequency_hz": 1000.0,
                "amplitude_v": 0.180,
                "event_start_s": 0.032,
                "event_stop_s": 0.072,
                "edge_time_s": 0.0003,
            },
            split="validation",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="pluck_750hz_160mv_gain55_treble60",
            kind="dynamic_decay",
            expression=(
                "0.160*sin(2*pi*750*time)"
                "*exp(-(time-0.032)/0.028)"
                "*(0.5+0.5*tanh((time-0.032)/0.0003))"
            ),
            parameters={
                "frequency_hz": 750.0,
                "amplitude_v": 0.160,
                "event_start_s": 0.032,
                "decay_time_s": 0.028,
                "edge_time_s": 0.0003,
            },
            split="train",
            gain=0.55,
            treble=0.60,
        ),
        KlonCentaurDatasetCase(
            stimulus_id="pluck_1100hz_140mv_gain55_treble60",
            kind="dynamic_decay",
            expression=(
                "0.140*sin(2*pi*1100*time)"
                "*exp(-(time-0.032)/0.022)"
                "*(0.5+0.5*tanh((time-0.032)/0.0003))"
            ),
            parameters={
                "frequency_hz": 1100.0,
                "amplitude_v": 0.140,
                "event_start_s": 0.032,
                "decay_time_s": 0.022,
                "edge_time_s": 0.0003,
            },
            split="test",
            gain=0.55,
            treble=0.60,
        ),
    ]


def klon_centaur_generated_netlist(source_netlist: str, case: KlonCentaurDatasetCase, raw_path: Path) -> str:
    replacements = {
        ".param GAIN=0.55": f".param GAIN={case.gain:g}",
        ".param TREBLE=0.60": f".param TREBLE={case.treble:g}",
        ".param LEVEL=0.70": f".param LEVEL={case.level:g}",
        "VIN guitar 0 SIN(0 120m 1k)": f"BVIN guitar 0 V={{ {case.expression} }}",
        "tran 1u 120m 0 1u": f"tran {case.transient_step_s:g} {case.transient_stop_s:g} 0 {case.transient_step_s:g}",
        "wrdata /tmp/greybound_klon_centaur.dat v(j2_tip) v(u2a_out) v(clean_feed) v(u2b_out) v(clip) v(mix_out) v(treble_wiper) v(vout)": (
            f"wrdata {raw_path.resolve()} v(j2_tip) v(u2a_out) v(clean_feed) "
            "v(u2b_out) v(clip) v(mix_out) v(treble_wiper) v(vout)"
        ),
    }
    generated = source_netlist
    for old, new in replacements.items():
        if old not in generated:
            raise ValueError(f"cannot generate Klon dataset netlist; missing line: {old}")
        generated = generated.replace(old, new, 1)
    generated = generated.replace(".param V9=9", ".param pi=3.141592653589793\n.param V9=9", 1)
    return f"* Generated Greybound Klon dataset case: {case.stimulus_id}\n" + generated


def common_cathode_dataset_cases() -> list[CommonCathodeDatasetCase]:
    return [
        CommonCathodeDatasetCase(
            stimulus_id="sine_1khz_5mv",
            kind="sine_level_sweep",
            expression="0.005*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.005},
            split="train",
        ),
        CommonCathodeDatasetCase(
            stimulus_id="sine_1khz_20mv",
            kind="sine_level_sweep",
            expression="0.020*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.020},
            split="train",
        ),
        CommonCathodeDatasetCase(
            stimulus_id="sine_1khz_80mv",
            kind="sine_level_sweep",
            expression="0.080*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.080},
            split="train",
        ),
        CommonCathodeDatasetCase(
            stimulus_id="sine_1khz_400mv",
            kind="sine_level_sweep",
            expression="0.400*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.400},
            split="train",
        ),
        CommonCathodeDatasetCase(
            stimulus_id="sine_1khz_40mv",
            kind="sine_level_sweep",
            expression="0.040*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.040},
            split="validation",
        ),
        CommonCathodeDatasetCase(
            stimulus_id="sine_1khz_300mv",
            kind="sine_level_sweep",
            expression="0.300*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.300},
            split="validation",
        ),
        CommonCathodeDatasetCase(
            stimulus_id="sine_1khz_120mv",
            kind="sine_level_sweep",
            expression="0.120*sin(2*pi*1000*time)",
            parameters={"frequency_hz": 1000.0, "amplitude_v": 0.120},
            split="test",
        ),
        CommonCathodeDatasetCase(
            stimulus_id="two_tone_997_1499_20mv",
            kind="two_tone_imd",
            expression="0.010*sin(2*pi*997*time)+0.010*sin(2*pi*1499*time)",
            parameters={"first_hz": 997.0, "second_hz": 1499.0, "combined_peak_v": 0.020},
            split="train",
        ),
        CommonCathodeDatasetCase(
            stimulus_id="two_tone_997_1499_80mv",
            kind="two_tone_imd",
            expression="0.040*sin(2*pi*997*time)+0.040*sin(2*pi*1499*time)",
            parameters={"first_hz": 997.0, "second_hz": 1499.0, "combined_peak_v": 0.080},
            split="test",
        ),
        CommonCathodeDatasetCase(
            stimulus_id="sine_burst_1khz_80mv",
            kind="dynamic_burst",
            expression=(
                "0.080*sin(2*pi*1000*time)"
                "*(0.5+0.5*tanh((time-0.032)/0.0003))"
                "*(0.5-0.5*tanh((time-0.052)/0.0003))"
            ),
            parameters={
                "frequency_hz": 1000.0,
                "amplitude_v": 0.080,
                "event_start_s": 0.032,
                "event_stop_s": 0.052,
                "edge_time_s": 0.0003,
            },
            split="train",
            transient_stop_s=0.080,
        ),
        CommonCathodeDatasetCase(
            stimulus_id="sine_burst_1khz_40mv",
            kind="dynamic_burst",
            expression=(
                "0.040*sin(2*pi*1000*time)"
                "*(0.5+0.5*tanh((time-0.032)/0.0003))"
                "*(0.5-0.5*tanh((time-0.052)/0.0003))"
            ),
            parameters={
                "frequency_hz": 1000.0,
                "amplitude_v": 0.040,
                "event_start_s": 0.032,
                "event_stop_s": 0.052,
                "edge_time_s": 0.0003,
            },
            split="validation",
            transient_stop_s=0.080,
        ),
        CommonCathodeDatasetCase(
            stimulus_id="pluck_decay_750hz_90mv",
            kind="dynamic_decay",
            expression=(
                "0.090*sin(2*pi*750*time)"
                "*exp(-(time-0.032)/0.018)"
                "*(0.5+0.5*tanh((time-0.032)/0.0003))"
            ),
            parameters={
                "frequency_hz": 750.0,
                "amplitude_v": 0.090,
                "event_start_s": 0.032,
                "decay_time_s": 0.018,
                "edge_time_s": 0.0003,
            },
            split="test",
            transient_stop_s=0.100,
        ),
        CommonCathodeDatasetCase(
            stimulus_id="bias_recovery_probe_20mv_after_400mv",
            kind="dynamic_bias_recovery",
            expression=(
                "0.020*sin(2*pi*1000*time)"
                "*(0.5+0.5*tanh((time-0.032)/0.0003))"
                "*(0.5-0.5*tanh((time-0.052)/0.0003))"
                "+0.400*sin(2*pi*1000*time)"
                "*(0.5+0.5*tanh((time-0.060)/0.0003))"
                "*(0.5-0.5*tanh((time-0.130)/0.0003))"
                "+0.020*sin(2*pi*1000*time)"
                "*(0.5+0.5*tanh((time-0.150)/0.0003))"
                "*(0.5-0.5*tanh((time-0.190)/0.0003))"
            ),
            parameters={
                "frequency_hz": 1000.0,
                "probe_amplitude_v": 0.020,
                "stress_amplitude_v": 0.400,
                "pre_probe_start_s": 0.032,
                "pre_probe_stop_s": 0.052,
                "stress_start_s": 0.060,
                "stress_stop_s": 0.130,
                "post_probe_start_s": 0.150,
                "post_probe_stop_s": 0.190,
                "edge_time_s": 0.0003,
            },
            split="test",
            transient_stop_s=0.210,
        ),
    ]


def common_cathode_generated_netlist(case: CommonCathodeDatasetCase, raw_path: Path) -> str:
    return f"""* Generated Greybound common-cathode dataset case: {case.stimulus_id}
.param BRAW=280
.param pi=3.141592653589793

VRAW raw 0 DC {{BRAW}}
RSUP raw bplus 10k
CSUP bplus 0 22u IC={{BRAW}}

BVIN in 0 V={{ {case.expression} }}
CIN in grid 22n
RGRID grid 0 1Meg

RPLATE bplus plate 100k
RK cath 0 1.5k
CK cath 0 25u
XTRIODE plate grid cath 12AX7_KOREN

.save v(in) v(grid) v(plate) v(cath) v(bplus)

.control
set filetype=ascii
tran {case.transient_step_s:g} {case.transient_stop_s:g} 0 {case.transient_step_s:g}
wrdata {raw_path.resolve()} v(in) v(grid) v(plate) v(cath) v(bplus)
quit
.endc

.subckt 12AX7_KOREN P G K
.param MU=100 EX=1.4 KG1=1060 KP=600 KVB=300
E1 n1 0 VALUE={{ln(1 + exp(KP * (1 / MU + V(G,K) / max(V(P,K), 1)))) / KP}}
G1 P K VALUE={{(V(P,K) / KG1) * pow(max(V(n1), 0), EX) * sqrt(max(V(P,K), 0) / KVB)}}
Cpk P K 1.7p
Cgp G P 1.6p
Cgk G K 1.6p
.ends 12AX7_KOREN

.end
"""


def parse_wrdata(path: Path, signals: tuple[str, ...]) -> SpiceTrace:
    data = np.loadtxt(path, dtype=np.float64)
    if data.ndim != 2:
        raise ValueError(f"{path} does not contain tabular data")
    expected_columns = len(signals) * 2
    if data.shape[1] != expected_columns:
        raise ValueError(f"{path} has {data.shape[1]} columns, expected {expected_columns}")
    time_s = data[:, 0]
    parsed = {}
    for index, signal_name in enumerate(signals):
        time_column = data[:, index * 2]
        if not np.allclose(time_column, time_s, rtol=1e-7, atol=1e-12):
            raise ValueError(f"{path} has mismatched time column for {signal_name}")
        parsed[signal_name] = data[:, index * 2 + 1]
    return SpiceTrace(time_s=time_s, signals=parsed)


def common_cathode_dataset_manifest(
    *,
    fixture: SpiceFixture,
    repo_root: Path,
    data_path: Path,
    dataset_path: Path,
    report_path: Path,
    metrics: CommonCathodeSpiceMetrics,
) -> dict:
    return {
        "schema_version": 1,
        "dataset_id": fixture.name + "-settled-sine-v1",
        "fixture_id": fixture.name,
        "cell_kind": "triode_gain_stage",
        "created_at": datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "generator": {
            "name": "greybound-lab spice-dataset",
            "version": "0.1.0",
            "git_revision": git_revision(repo_root),
        },
        "spice": {
            "engine": "ngspice",
            "version": _ngspice_version(repo_root),
            "options": {
                "filetype": "ascii",
                "transient_step_s": 1.0e-6,
                "transient_stop_s": 0.100,
            },
        },
        "circuit": {
            "netlist_sha256": sha256_file(repo_root / fixture.netlist_path),
            "source_impedance_ohm": 0.0,
            "load_impedance_ohm": 1_000_000.0,
            "operating_point": {
                "plate_dc_v": metrics.plate_dc_v,
                "cathode_dc_v": metrics.cathode_dc_v,
                "bplus_dc_v": metrics.bplus_dc_v,
            },
            "components": {
                "tube_model": "12AX7_KOREN",
                "vin": "SIN(0 20m 1k)",
                "input_coupling_cap_f": 22.0e-9,
                "grid_leak_ohm": 1_000_000.0,
                "plate_resistor_ohm": 100_000.0,
                "cathode_resistor_ohm": 1_500.0,
                "cathode_bypass_cap_f": 25.0e-6,
                "raw_supply_v": 280.0,
                "supply_resistor_ohm": 10_000.0,
                "supply_cap_f": 22.0e-6,
            },
        },
        "sample_rate_hz": _sample_rate_from_trace(data_path, fixture.signals),
        "oversampling": {
            "factor": 1,
            "filter": "none",
        },
        "stimuli": [
            {
                "id": "settled_1khz_20mv_sine",
                "kind": "settled_sine",
                "path": relative_or_absolute(data_path, repo_root),
                "sha256": sha256_file(data_path),
                "parameters": {
                    "frequency_hz": 1000.0,
                    "amplitude_v": 0.020,
                    "settle_time_s": 0.050,
                },
            }
        ],
        "targets": [
            {"node": "in", "unit": "V", "role": "input"},
            {"node": "grid", "unit": "V", "role": "state"},
            {"node": "plate", "unit": "V", "role": "output"},
            {"node": "cathode", "unit": "V", "role": "state"},
            {"node": "bplus", "unit": "V", "role": "reference"},
        ],
        "splits": {
            "train": ["settled_1khz_20mv_sine"],
            "validation": [],
            "test": [],
            "policy": "Bootstrap dataset only. Future datasets must hold out stimulus families and level ranges.",
        },
        "artifacts": [
            {
                "path": relative_or_absolute(dataset_path, repo_root),
                "kind": "output",
                "sha256": sha256_file(dataset_path),
            },
            {
                "path": relative_or_absolute(report_path, repo_root),
                "kind": "report",
                "sha256": sha256_file(report_path),
            },
        ],
        "notes": (
            "Bootstrap dataset from the first common-cathode fixture. It is useful "
            "for testing the data/export loop, but it is not sufficient for training "
            "a robust neural cell."
        ),
    }


def common_cathode_sweep_dataset_manifest(
    *,
    fixture: SpiceFixture,
    repo_root: Path,
    cases: list[CommonCathodeDatasetCase],
    raw_paths: dict[str, Path],
    netlist_paths: dict[str, Path],
    dataset_path: Path,
    report_path: Path,
    metrics: CommonCathodeSpiceMetrics,
) -> dict:
    train = [case.stimulus_id for case in cases if case.split == "train"]
    validation = [case.stimulus_id for case in cases if case.split == "validation"]
    test = [case.stimulus_id for case in cases if case.split == "test"]
    artifacts = [
        {
            "path": relative_or_absolute(dataset_path, repo_root),
            "kind": "output",
            "sha256": sha256_file(dataset_path),
        },
        {
            "path": relative_or_absolute(report_path, repo_root),
            "kind": "report",
            "sha256": sha256_file(report_path),
        },
    ]
    for case in cases:
        artifacts.append(
            {
                "path": relative_or_absolute(netlist_paths[case.stimulus_id], repo_root),
                "kind": "netlist",
                "sha256": sha256_file(netlist_paths[case.stimulus_id]),
            }
        )

    return {
        "schema_version": 1,
        "dataset_id": fixture.name + "-sweep-current",
        "fixture_id": fixture.name,
        "cell_kind": "triode_gain_stage",
        "created_at": datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "generator": {
            "name": "greybound-lab spice-dataset",
            "version": "0.1.0",
            "git_revision": git_revision(repo_root),
        },
        "spice": {
            "engine": "ngspice",
            "version": _ngspice_version(repo_root),
            "options": {
                "filetype": "ascii",
                "transient_step_s": 1.0e-6,
                "transient_stop_s": 0.060,
            },
        },
        "circuit": {
            "netlist_sha256": sha256_file(netlist_paths[cases[0].stimulus_id]),
            "source_impedance_ohm": 0.0,
            "load_impedance_ohm": 1_000_000.0,
            "operating_point": {
                "plate_dc_v": metrics.plate_dc_v,
                "cathode_dc_v": metrics.cathode_dc_v,
                "bplus_dc_v": metrics.bplus_dc_v,
            },
            "components": {
                "tube_model": "12AX7_KOREN",
                "input_coupling_cap_f": 22.0e-9,
                "grid_leak_ohm": 1_000_000.0,
                "plate_resistor_ohm": 100_000.0,
                "cathode_resistor_ohm": 1_500.0,
                "cathode_bypass_cap_f": 25.0e-6,
                "raw_supply_v": 280.0,
                "supply_resistor_ohm": 10_000.0,
                "supply_cap_f": 22.0e-6,
            },
        },
        "sample_rate_hz": _sample_rate_from_trace(raw_paths[cases[0].stimulus_id], fixture.signals),
        "oversampling": {
            "factor": 1,
            "filter": "none",
        },
        "stimuli": [
            {
                "id": case.stimulus_id,
                "kind": case.kind,
                "path": relative_or_absolute(raw_paths[case.stimulus_id], repo_root),
                "sha256": sha256_file(raw_paths[case.stimulus_id]),
                "parameters": {
                    **case.parameters,
                    "transient_stop_s": case.transient_stop_s,
                    "settle_time_s": case.settle_time_s,
                },
            }
            for case in cases
        ],
        "targets": [
            {"node": "in", "unit": "V", "role": "input"},
            {"node": "grid", "unit": "V", "role": "state"},
            {"node": "plate", "unit": "V", "role": "output"},
            {"node": "cathode", "unit": "V", "role": "state"},
            {"node": "bplus", "unit": "V", "role": "reference"},
        ],
        "splits": {
            "train": train,
            "validation": validation,
            "test": test,
            "policy": (
                "Train covers low/nominal/hot sine plus a nominal two-tone case. "
                "Training also includes a deliberately high 400 mV sine so the "
                "bias-recovery stress test is not merely an amplitude extrapolation "
                "case. Validation holds out intermediate 40 mV and 300 mV sine "
                "levels. Test holds out an extra-hot sine and a hotter two-tone IMD "
                "case. Dynamic burst and decay cases probe whether static curve "
                "fits survive onset and release behavior. The bias recovery probe "
                "repeats the same small signal before and after a hot stress window "
                "to expose state memory."
            ),
        },
        "artifacts": artifacts,
        "notes": (
            "First multi-stimulus common-cathode dataset. It is suitable for a "
            "baseline MLP/TCN training smoke test and now includes first dynamic "
            "burst/decay and bias-recovery probes. It still lacks source/load "
            "impedance sweeps, B+ perturbation, component tolerances, and real DI."
        ),
    }


def klon_centaur_dataset_manifest(
    *,
    fixture: SpiceFixture,
    repo_root: Path,
    cases: list[KlonCentaurDatasetCase],
    raw_paths: dict[str, Path],
    netlist_paths: dict[str, Path],
    dataset_path: Path,
    report_path: Path,
    metrics: KlonCentaurSpiceMetrics,
) -> dict:
    train = [case.stimulus_id for case in cases if case.split == "train"]
    validation = [case.stimulus_id for case in cases if case.split == "validation"]
    test = [case.stimulus_id for case in cases if case.split == "test"]
    artifacts = [
        {"path": relative_or_absolute(dataset_path, repo_root), "kind": "output", "sha256": sha256_file(dataset_path)},
        {"path": relative_or_absolute(report_path, repo_root), "kind": "report", "sha256": sha256_file(report_path)},
    ]
    for case in cases:
        artifacts.append(
            {
                "path": relative_or_absolute(netlist_paths[case.stimulus_id], repo_root),
                "kind": "netlist",
                "sha256": sha256_file(netlist_paths[case.stimulus_id]),
            }
        )

    return {
        "schema_version": 1,
        "dataset_id": fixture.name + "-drive-clip-tone-v1",
        "fixture_id": fixture.name,
        "cell_kind": "klon_drive_clip_tone",
        "created_at": datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "generator": {
            "name": "greybound-lab spice-dataset",
            "version": "0.1.0",
            "git_revision": git_revision(repo_root),
        },
        "spice": {
            "engine": "ngspice",
            "version": _ngspice_version(repo_root),
            "options": {
                "filetype": "ascii",
                "transient_step_s": 2.0e-6,
                "transient_stop_s": 0.120,
            },
        },
        "sample_rate_hz": _sample_rate_from_trace(raw_paths[cases[0].stimulus_id], fixture.signals),
        "signals": {
            "inputs": ["input_v", "buffer_v"],
            "controls": ["gain", "treble", "level"],
            "targets": ["drive_ac_v", "clip_ac_v", "mix_ac_v", "tone_ac_v"],
            "context": ["clean_ac_v", "output_ac_v"],
        },
        "reference_metrics": asdict(metrics),
        "stimuli": [
            {
                "id": case.stimulus_id,
                "kind": case.kind,
                "path": relative_or_absolute(raw_paths[case.stimulus_id], repo_root),
                "sha256": sha256_file(raw_paths[case.stimulus_id]),
                "parameters": {
                    **case.parameters,
                    "gain": case.gain,
                    "treble": case.treble,
                    "level": case.level,
                    "transient_stop_s": case.transient_stop_s,
                },
                "split": case.split,
            }
            for case in cases
        ],
        "targets": [
            {"node": "j2_tip", "unit": "V", "role": "input"},
            {"node": "u2a_out", "unit": "V", "role": "buffer"},
            {"node": "clean_feed", "unit": "V", "role": "analytic_context"},
            {"node": "u2b_out", "unit": "V", "role": "drive"},
            {"node": "clip", "unit": "V", "role": "clip_target"},
            {"node": "mix_out", "unit": "V", "role": "mix_target"},
            {"node": "treble_wiper", "unit": "V", "role": "tone_target"},
            {"node": "vout", "unit": "V", "role": "output_guardrail"},
        ],
        "splits": {
            "train": train,
            "validation": validation,
            "test": test,
            "policy": (
                "Train covers nominal level, low/hot amplitude, gain, treble, and low-frequency cases. "
                "Validation holds out high gain, bright treble, and burst dynamics. "
                "Test holds out high-frequency, IMD, and decay probes."
            ),
        },
        "artifacts": artifacts,
        "notes": (
            "Synthetic SPICE corpus for a targeted Klon drive/clip/tone neural cell. "
            "It is intentionally not a full-pedal black-box dataset."
        ),
    }


def common_cathode_metrics(trace: SpiceTrace, settle_time_s: float = 0.050) -> CommonCathodeSpiceMetrics:
    mask = trace.time_s >= settle_time_s
    if not np.any(mask):
        raise ValueError("SPICE trace is too short for settled metrics")
    input_v = trace.signals["input"][mask]
    grid_v = trace.signals["grid"][mask]
    plate_v = trace.signals["plate"][mask]
    cathode_v = trace.signals["cathode"][mask]
    bplus_v = trace.signals["bplus"][mask]

    input_ac = _remove_dc(input_v)
    grid_ac = _remove_dc(grid_v)
    plate_ac = _remove_dc(plate_v)
    cathode_ac = _remove_dc(cathode_v)
    input_rms = rms(input_ac)
    grid_rms = rms(grid_ac)
    plate_rms = rms(plate_ac)

    return CommonCathodeSpiceMetrics(
        plate_dc_v=float(np.mean(plate_v)),
        cathode_dc_v=float(np.mean(cathode_v)),
        bplus_dc_v=float(np.mean(bplus_v)),
        input_rms_v=input_rms,
        grid_rms_v=rms(grid_ac),
        plate_rms_v=plate_rms,
        cathode_rms_v=rms(cathode_ac),
        plate_gain=plate_rms / max(input_rms, 1.0e-12),
        plate_gain_db=linear_to_db(plate_rms / max(input_rms, 1.0e-12)),
        grid_coupling_loss_db=linear_to_db(rms(grid_ac) / max(input_rms, 1.0e-12)),
    )


def klon_centaur_metrics(trace: SpiceTrace, settle_time_s: float = 0.050) -> KlonCentaurSpiceMetrics:
    mask = trace.time_s >= settle_time_s
    if not np.any(mask):
        raise ValueError("SPICE trace is too short for settled metrics")

    input_ac = _remove_dc(trace.signals["input"][mask])
    buffer_ac = _remove_dc(trace.signals["buffer"][mask])
    clean_ac = _remove_dc(trace.signals["clean"][mask])
    drive_ac = _remove_dc(trace.signals["drive"][mask])
    clip_ac = _remove_dc(trace.signals["clip"][mask])
    mix_ac = _remove_dc(trace.signals["mix"][mask])
    tone_ac = _remove_dc(trace.signals["tone"][mask])
    output_ac = _remove_dc(trace.signals["output"][mask])

    input_rms = rms(input_ac)
    output_rms = rms(output_ac)
    clip_positive = float(np.max(clip_ac))
    clip_negative = float(np.min(clip_ac))

    return KlonCentaurSpiceMetrics(
        input_rms_v=input_rms,
        buffer_rms_v=rms(buffer_ac),
        clean_rms_v=rms(clean_ac),
        drive_rms_v=rms(drive_ac),
        clip_rms_v=rms(clip_ac),
        mix_rms_v=rms(mix_ac),
        tone_rms_v=rms(tone_ac),
        output_rms_v=output_rms,
        output_peak_v=float(np.max(np.abs(output_ac))),
        output_gain=output_rms / max(input_rms, 1.0e-12),
        output_gain_db=linear_to_db(output_rms / max(input_rms, 1.0e-12)),
        clip_peak_v=max(abs(clip_positive), abs(clip_negative)),
        clip_asymmetry_v=clip_positive + clip_negative,
    )


def write_common_cathode_report(
    path: Path,
    fixture: SpiceFixture,
    data_path: Path,
    metrics: CommonCathodeSpiceMetrics,
) -> None:
    path.write_text(
        f"""# SPICE Fixture Report: {fixture.name}

## Inputs

- Netlist: `{fixture.netlist_path}`
- Data: `{data_path}`
- Source: ngspice batch run

## DC Operating Point

| Node | Voltage |
| --- | ---: |
| Plate | {metrics.plate_dc_v:.3f} V |
| Cathode | {metrics.cathode_dc_v:.3f} V |
| B+ | {metrics.bplus_dc_v:.3f} V |

## Settled 1 kHz Transient

Metrics are computed after the first 50 ms to avoid startup bias.

| Metric | Value |
| --- | ---: |
| Input RMS | {metrics.input_rms_v * 1000.0:.3f} mV |
| Grid RMS | {metrics.grid_rms_v * 1000.0:.3f} mV |
| Plate RMS after DC removal | {metrics.plate_rms_v * 1000.0:.3f} mV |
| Cathode RMS after DC removal | {metrics.cathode_rms_v * 1000.0:.3f} mV |
| Plate gain | {metrics.plate_gain:.2f}x |
| Plate gain | {metrics.plate_gain_db:.2f} dB |
| Grid coupling loss | {metrics.grid_coupling_loss_db:.2f} dB |

## Engineering Notes

This is a cell-level electrical reference, not a full Greybound rig reference.
Use it to validate the common-cathode stage before fitting or tuning higher-level
amp behavior.
""",
        encoding="utf-8",
    )


def write_klon_centaur_report(
    path: Path,
    fixture: SpiceFixture,
    data_path: Path,
    metrics: KlonCentaurSpiceMetrics,
) -> None:
    path.write_text(
        f"""# SPICE Fixture Report: {fixture.name}

## Inputs

- Netlist: `{fixture.netlist_path}`
- Data: `{data_path}`
- Source: ngspice batch run

## Circuit Scope

This fixture models the full Klon-style pedal path as a practical ngspice macro:
input buffer, clean/drive split, non-inverting gain stage, antiparallel germanium
clipping around Vref, passive tone/level shaping, output buffer, and high-Z load.
The passive component values are sourced from the public Klon Centaur BOM. The
TL072 stages use the Texas Instruments SLOJ067 PSpice macromodel, copied locally
for ngspice with the documented `RP` supply-current correction. The charge-pump
switching network is not simulated in this audio-path fixture; the op-amp rails
are idealized nominal Klon rails.

Primary references:

- Zpag Klon Centaur schematic/BOM: `https://www.zpag.net/Electroniques/Guitar/klon_centaur_schematic.html`
- Tiburonboy MNA/LTspice Klon analysis: `https://tiburonboy.github.io/Symbolic-Modified-Nodal-Analysis-using-Python/Klon%20Centaur%20part%202v0.html`
- TI TL072 PSpice model SLOJ067: `https://www.ti.com/lit/zip/sloj067`
- TI E2E TL072 model RP correction: `https://e2e.ti.com/support/tools/simulation-hardware-system-design-tools-group/sim-hw-system-design/f/simulation-hardware-system-design-tools-forum/622836/tina-spice-tl072-supply-current-result-of-tl072-spice-model`

## Settled 1 kHz Transient

Metrics are computed after the first 50 ms to avoid startup bias. All values are
AC after DC removal around the 4.5 V bias point.

| Metric | Value |
| --- | ---: |
| Input RMS | {metrics.input_rms_v * 1000.0:.3f} mV |
| Buffer RMS | {metrics.buffer_rms_v * 1000.0:.3f} mV |
| Clean path RMS | {metrics.clean_rms_v * 1000.0:.3f} mV |
| Drive stage RMS | {metrics.drive_rms_v * 1000.0:.3f} mV |
| Clip node RMS | {metrics.clip_rms_v * 1000.0:.3f} mV |
| Mix node RMS | {metrics.mix_rms_v * 1000.0:.3f} mV |
| Tone node RMS | {metrics.tone_rms_v * 1000.0:.3f} mV |
| Output RMS | {metrics.output_rms_v * 1000.0:.3f} mV |
| Output peak | {metrics.output_peak_v * 1000.0:.3f} mV |
| Output gain | {metrics.output_gain:.2f}x |
| Output gain | {metrics.output_gain_db:.2f} dB |
| Clip peak | {metrics.clip_peak_v * 1000.0:.3f} mV |
| Clip asymmetry | {metrics.clip_asymmetry_v * 1000.0:.3f} mV |

## Engineering Notes

Use this as a component-level reference before tuning the Minotaur Rust model.
The next useful step is generating the same fixture across `GAIN`, `TREBLE`, and
`LEVEL` parameter sweeps, then comparing those traces to Greybound pedal-only
renders and the local NAM Klon references.
""",
        encoding="utf-8",
    )


def write_common_cathode_dataset_report(
    path: Path,
    fixture: SpiceFixture,
    cases: list[CommonCathodeDatasetCase],
    metrics: CommonCathodeSpiceMetrics,
) -> None:
    rows = "\n".join(
        f"| `{case.stimulus_id}` | `{case.kind}` | `{case.split}` | `{case.expression}` |"
        for case in cases
    )
    path.write_text(
        f"""# SPICE Dataset Report: {fixture.name}

## Purpose

This dataset is the first multi-stimulus common-cathode corpus for Greybound's
SPICE-to-neural-cell workflow. It is intended for baseline training and export
smoke tests, not for final tube-stage model acceptance.

## Fixture

- Cell: 12AX7/ECC83 common-cathode gain stage
- Plate resistor: `100k`
- Cathode resistor: `1.5k`
- Cathode bypass capacitor: `25u`
- Input coupling capacitor: `22n`
- Grid leak: `1Meg`
- Raw supply: `280 V`
- Supply resistor: `10k`
- SPICE model: Koren-style `12AX7_KOREN`

## Reference Operating Point

Computed from the held nominal `sine_1khz_20mv` case after settling.

| Node | Voltage |
| --- | ---: |
| Plate | {metrics.plate_dc_v:.3f} V |
| Cathode | {metrics.cathode_dc_v:.3f} V |
| B+ | {metrics.bplus_dc_v:.3f} V |

| Metric | Value |
| --- | ---: |
| Input RMS | {metrics.input_rms_v * 1000.0:.3f} mV |
| Plate RMS after DC removal | {metrics.plate_rms_v * 1000.0:.3f} mV |
| Plate gain | {metrics.plate_gain:.2f}x |
| Plate gain | {metrics.plate_gain_db:.2f} dB |

## Stimuli

| Stimulus | Kind | Split | Expression |
| --- | --- | --- | --- |
{rows}

## Limitations

- Source impedance is still idealized at `0 ohm`.
- Load is still the grid leak / fixture context, not a downstream tone stack.
- B+ is fixed; there is no supply perturbation or sag corpus yet.
- Component tolerances are not swept.
- The corpus does not include real DI phrases yet.

Use this dataset to prove the training/export/runtime loop before drawing
conclusions about final model quality.
""",
        encoding="utf-8",
    )


def write_klon_centaur_dataset_report(
    path: Path,
    fixture: SpiceFixture,
    cases: list[KlonCentaurDatasetCase],
    metrics: KlonCentaurSpiceMetrics,
) -> None:
    rows = "\n".join(
        f"| `{case.stimulus_id}` | `{case.kind}` | `{case.split}` | {case.gain:.2f} | {case.treble:.2f} | `{case.expression}` |"
        for case in cases
    )
    path.write_text(
        f"""# SPICE Dataset Report: {fixture.name}

## Purpose

This dataset is the first Klon/Minotaur corpus for Greybound's targeted neural
work. It is designed for a small causal drive/clip/tone cell, not for a
full-pedal black-box replacement.

## Fixture

- Cell target: Klon drive stage, germanium clip node, summing/mix, and treble output
- Preserved analytic context: input buffer, clean feed-forward path, level/output recovery
- Op-amp model: local TI TL072 ngspice copy with the documented supply-current correction
- Rails: idealized nominal Klon rails

## Reference Case

Computed from `sine_1khz_120mv_gain55_treble60` after the first 50 ms.

| Metric | Value |
| --- | ---: |
| Input RMS | {metrics.input_rms_v * 1000.0:.3f} mV |
| Drive stage RMS | {metrics.drive_rms_v * 1000.0:.3f} mV |
| Clip node RMS | {metrics.clip_rms_v * 1000.0:.3f} mV |
| Mix node RMS | {metrics.mix_rms_v * 1000.0:.3f} mV |
| Tone node RMS | {metrics.tone_rms_v * 1000.0:.3f} mV |
| Output RMS | {metrics.output_rms_v * 1000.0:.3f} mV |
| Output gain | {metrics.output_gain_db:.2f} dB |
| Clip peak | {metrics.clip_peak_v * 1000.0:.3f} mV |
| Clip asymmetry | {metrics.clip_asymmetry_v * 1000.0:.3f} mV |

## Stimuli

| Stimulus | Kind | Split | Gain | Treble | Expression |
| --- | --- | --- | ---: | ---: | --- |
{rows}

## Training Scope

Use `buffer_v` plus normalized `gain`, `treble`, and short causal history as the
primary input. The initial targets should be `clip_ac_v`, `mix_ac_v`, and
`tone_ac_v`; keep the clean path and output level as analytic guardrails.

## Limitations

- Component tolerances and diode part variation are not swept yet.
- The charge-pump switching network remains out of scope.
- The dataset is synthetic SPICE only; NAM comparison remains the audio-level acceptance target.
- There is no real DI phrase in this SPICE corpus yet.
""",
        encoding="utf-8",
    )


def _remove_dc(samples: np.ndarray) -> np.ndarray:
    return samples - np.mean(samples)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _sample_rate_from_trace(path: Path, signals: tuple[str, ...]) -> int:
    trace = parse_wrdata(path, signals)
    if trace.time_s.shape[0] < 2:
        raise ValueError("SPICE trace is too short to infer sample rate")
    step_s = float(np.median(np.diff(trace.time_s)))
    if step_s <= 0.0:
        raise ValueError("SPICE trace has non-positive time step")
    return int(round(1.0 / step_s))


def _ngspice_version(repo_root: Path) -> str:
    try:
        result = subprocess.run(
            ["ngspice", "--version"],
            cwd=repo_root,
            check=True,
            capture_output=True,
            text=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError):
        return "unknown"
    return (result.stdout or result.stderr).strip().splitlines()[0]
