from __future__ import annotations

import json
import math
import struct
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import numpy as np

from greybound_lab.render import git_revision, relative_or_absolute
from greybound_lab.spice import sha256_file


@dataclass(frozen=True)
class PreparedSplit:
    x: np.ndarray
    y: np.ndarray


@dataclass(frozen=True)
class PreparedDataset:
    train: PreparedSplit
    validation: PreparedSplit
    test: PreparedSplit
    input_mean: float
    input_std: float
    output_mean: float
    output_std: float
    sample_rate_hz: int
    history_samples: int
    train_ids: list[str]
    validation_ids: list[str]
    test_ids: list[str]


@dataclass(frozen=True)
class PreparedVectorDataset:
    train: PreparedSplit
    validation: PreparedSplit
    test: PreparedSplit
    input_mean: np.ndarray
    input_std: np.ndarray
    output_mean: float
    output_std: float
    sample_rate_hz: int
    history_samples: int
    input_ids: list[str]
    output_id: str
    train_ids: list[str]
    validation_ids: list[str]
    test_ids: list[str]


@dataclass(frozen=True)
class NeuralCellEvaluationRow:
    stimulus_id: str
    split: str
    kind: str
    samples: int
    reference_rms_v: float
    rmse_v: float
    mae_v: float
    max_abs_error_v: float
    relative_rmse: float
    zero_baseline_rmse_v: float


@dataclass(frozen=True)
class NeuralCellHistoryProbeRow:
    stimulus_id: str
    reference_pre_gain_db: float
    reference_post_gain_db: float
    reference_delta_db: float
    prediction_pre_gain_db: float
    prediction_post_gain_db: float
    prediction_delta_db: float
    delta_error_db: float


def train_common_cathode_mlp(
    *,
    manifest_path: Path,
    output_dir: Path,
    repo_root: Path,
    epochs: int = 300,
    hidden_size: int = 16,
    learning_rate: float = 1.0e-3,
    stride: int = 16,
    history_samples: int = 1,
    seed: int = 59,
) -> tuple[Path, Path, Path]:
    torch = _import_torch()
    output_dir.mkdir(parents=True, exist_ok=True)
    descriptor_path = output_dir / "model.greybound.json"
    weights_path = output_dir / "weights.greybound.bin"
    report_path = output_dir / "training-report.md"

    manifest = _read_json(manifest_path)
    prepared = prepare_common_cathode_dataset(
        manifest_path,
        stride=stride,
        history_samples=history_samples,
    )
    torch.manual_seed(seed)
    model = torch.nn.Sequential(
        torch.nn.Linear(prepared.history_samples, hidden_size),
        torch.nn.Tanh(),
        torch.nn.Linear(hidden_size, hidden_size),
        torch.nn.Tanh(),
        torch.nn.Linear(hidden_size, 1),
    )
    optimizer = torch.optim.AdamW(model.parameters(), lr=learning_rate)
    loss_fn = torch.nn.MSELoss()

    x_train = torch.from_numpy(prepared.train.x.astype(np.float32))
    y_train = torch.from_numpy(prepared.train.y.astype(np.float32))
    x_val = torch.from_numpy(prepared.validation.x.astype(np.float32))
    y_val = torch.from_numpy(prepared.validation.y.astype(np.float32))
    x_test = torch.from_numpy(prepared.test.x.astype(np.float32))
    y_test = torch.from_numpy(prepared.test.y.astype(np.float32))

    best_state = None
    best_val = math.inf
    for _ in range(epochs):
        model.train()
        optimizer.zero_grad()
        loss = loss_fn(model(x_train), y_train)
        loss.backward()
        optimizer.step()
        model.eval()
        with torch.no_grad():
            val_loss = float(loss_fn(model(x_val), y_val).item()) if x_val.numel() else float(loss.item())
        if val_loss < best_val:
            best_val = val_loss
            best_state = {key: value.detach().clone() for key, value in model.state_dict().items()}

    if best_state is not None:
        model.load_state_dict(best_state)

    model.eval()
    with torch.no_grad():
        train_pred = model(x_train).detach().cpu().numpy()
        val_pred = model(x_val).detach().cpu().numpy()
        test_pred = model(x_test).detach().cpu().numpy()

    layers = _extract_mlp_layers(model)
    write_mlp_weights(weights_path, layers)
    numpy_test_pred = infer_mlp_numpy(
        prepared.test.x,
        layers,
    )
    export_max_abs_error = float(np.max(np.abs(numpy_test_pred - test_pred))) if test_pred.size else 0.0
    metrics = {
        "train_mse_normalized": _mse(train_pred, prepared.train.y),
        "validation_mse_normalized": _mse(val_pred, prepared.validation.y),
        "test_mse_normalized": _mse(test_pred, prepared.test.y),
        "test_baseline_mse_normalized": _mse(np.zeros_like(prepared.test.y), prepared.test.y),
        "export_max_abs_error_normalized": export_max_abs_error,
        "epochs": epochs,
        "hidden_size": hidden_size,
        "learning_rate": learning_rate,
        "stride": stride,
        "history_samples": prepared.history_samples,
        "train_samples": int(prepared.train.x.shape[0]),
        "validation_samples": int(prepared.validation.x.shape[0]),
        "test_samples": int(prepared.test.x.shape[0]),
    }
    descriptor = build_mlp_descriptor(
        manifest=manifest,
        manifest_path=manifest_path,
        output_dir=output_dir,
        repo_root=repo_root,
        weights_path=weights_path,
        hidden_size=hidden_size,
        prepared=prepared,
        metrics=metrics,
    )
    descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n", encoding="utf-8")
    write_training_report(report_path, descriptor, metrics, prepared)
    return descriptor_path, weights_path, report_path


def train_klon_drive_clip_tone_mlp(
    *,
    manifest_path: Path,
    output_dir: Path,
    repo_root: Path,
    target: str = "tone_ac_v",
    epochs: int = 300,
    hidden_size: int = 32,
    learning_rate: float = 1.0e-3,
    stride: int = 16,
    history_samples: int = 4,
    seed: int = 59,
) -> tuple[Path, Path, Path]:
    if target not in {"clip_ac_v", "mix_ac_v", "tone_ac_v"}:
        raise ValueError("--target must be one of clip_ac_v, mix_ac_v, tone_ac_v")
    torch = _import_torch()
    output_dir.mkdir(parents=True, exist_ok=True)
    descriptor_path = output_dir / "model.greybound.json"
    weights_path = output_dir / "weights.greybound.bin"
    report_path = output_dir / "training-report.md"

    manifest = _read_json(manifest_path)
    prepared = prepare_klon_drive_clip_tone_dataset(
        manifest_path,
        target=target,
        stride=stride,
        history_samples=history_samples,
    )
    torch.manual_seed(seed)
    input_features = int(prepared.train.x.shape[1])
    model = torch.nn.Sequential(
        torch.nn.Linear(input_features, hidden_size),
        torch.nn.Tanh(),
        torch.nn.Linear(hidden_size, hidden_size),
        torch.nn.Tanh(),
        torch.nn.Linear(hidden_size, 1),
    )
    optimizer = torch.optim.AdamW(model.parameters(), lr=learning_rate)
    loss_fn = torch.nn.MSELoss()

    x_train = torch.from_numpy(prepared.train.x.astype(np.float32))
    y_train = torch.from_numpy(prepared.train.y.astype(np.float32))
    x_val = torch.from_numpy(prepared.validation.x.astype(np.float32))
    y_val = torch.from_numpy(prepared.validation.y.astype(np.float32))
    x_test = torch.from_numpy(prepared.test.x.astype(np.float32))
    y_test = torch.from_numpy(prepared.test.y.astype(np.float32))

    best_state = None
    best_val = math.inf
    for _ in range(epochs):
        model.train()
        optimizer.zero_grad()
        loss = loss_fn(model(x_train), y_train)
        loss.backward()
        optimizer.step()
        model.eval()
        with torch.no_grad():
            val_loss = float(loss_fn(model(x_val), y_val).item()) if x_val.numel() else float(loss.item())
        if val_loss < best_val:
            best_val = val_loss
            best_state = {key: value.detach().clone() for key, value in model.state_dict().items()}

    if best_state is not None:
        model.load_state_dict(best_state)

    model.eval()
    with torch.no_grad():
        train_pred = model(x_train).detach().cpu().numpy()
        val_pred = model(x_val).detach().cpu().numpy()
        test_pred = model(x_test).detach().cpu().numpy()

    layers = _extract_mlp_layers(model)
    write_mlp_weights(weights_path, layers)
    numpy_test_pred = infer_mlp_numpy(prepared.test.x, layers)
    export_max_abs_error = float(np.max(np.abs(numpy_test_pred - test_pred))) if test_pred.size else 0.0
    metrics = {
        "train_mse_normalized": _mse(train_pred, prepared.train.y),
        "validation_mse_normalized": _mse(val_pred, prepared.validation.y),
        "test_mse_normalized": _mse(test_pred, prepared.test.y),
        "test_baseline_mse_normalized": _mse(np.zeros_like(prepared.test.y), prepared.test.y),
        "export_max_abs_error_normalized": export_max_abs_error,
        "epochs": epochs,
        "hidden_size": hidden_size,
        "learning_rate": learning_rate,
        "stride": stride,
        "history_samples": prepared.history_samples,
        "train_samples": int(prepared.train.x.shape[0]),
        "validation_samples": int(prepared.validation.x.shape[0]),
        "test_samples": int(prepared.test.x.shape[0]),
        "input_features": input_features,
    }
    descriptor = build_klon_mlp_descriptor(
        manifest=manifest,
        manifest_path=manifest_path,
        output_dir=output_dir,
        repo_root=repo_root,
        weights_path=weights_path,
        hidden_size=hidden_size,
        prepared=prepared,
        metrics=metrics,
    )
    descriptor_path.write_text(json.dumps(descriptor, indent=2) + "\n", encoding="utf-8")
    write_klon_training_report(report_path, descriptor, metrics, prepared)
    return descriptor_path, weights_path, report_path


def prepare_common_cathode_dataset(
    manifest_path: Path,
    *,
    stride: int = 16,
    history_samples: int = 1,
) -> PreparedDataset:
    if history_samples < 1:
        raise ValueError("history_samples must be at least 1")
    manifest = _read_json(manifest_path)
    dataset_path = _resolve_manifest_path(manifest_path, _artifact_path(manifest, "output"))
    npz = np.load(dataset_path)
    train_ids = list(manifest["splits"]["train"])
    validation_ids = list(manifest["splits"]["validation"])
    test_ids = list(manifest["splits"]["test"])
    train_raw = _collect_split(npz, manifest, train_ids, stride=stride, history_samples=history_samples)
    validation_raw = _collect_split(npz, manifest, validation_ids, stride=stride, history_samples=history_samples)
    test_raw = _collect_split(npz, manifest, test_ids, stride=stride, history_samples=history_samples)

    input_mean = float(np.mean(train_raw.x[:, :1]))
    input_std = float(np.std(train_raw.x[:, :1]))
    output_mean = float(np.mean(train_raw.y))
    output_std = float(np.std(train_raw.y))
    input_std = input_std if input_std > 1.0e-12 else 1.0
    output_std = output_std if output_std > 1.0e-12 else 1.0

    return PreparedDataset(
        train=_normalize_split(train_raw, input_mean, input_std, output_mean, output_std),
        validation=_normalize_split(validation_raw, input_mean, input_std, output_mean, output_std),
        test=_normalize_split(test_raw, input_mean, input_std, output_mean, output_std),
        input_mean=input_mean,
        input_std=input_std,
        output_mean=output_mean,
        output_std=output_std,
        sample_rate_hz=int(manifest["sample_rate_hz"]),
        history_samples=history_samples,
        train_ids=train_ids,
        validation_ids=validation_ids,
        test_ids=test_ids,
    )


def prepare_klon_drive_clip_tone_dataset(
    manifest_path: Path,
    *,
    target: str = "tone_ac_v",
    stride: int = 16,
    history_samples: int = 4,
) -> PreparedVectorDataset:
    if history_samples < 1:
        raise ValueError("history_samples must be at least 1")
    manifest = _read_json(manifest_path)
    dataset_path = _resolve_manifest_path(manifest_path, _artifact_path(manifest, "output"))
    npz = np.load(dataset_path)
    controls_by_id = {
        str(stimulus["id"]): stimulus.get("parameters", {})
        for stimulus in manifest.get("stimuli", [])
    }
    train_ids = list(manifest["splits"]["train"])
    validation_ids = list(manifest["splits"]["validation"])
    test_ids = list(manifest["splits"]["test"])
    input_ids = klon_input_ids_for_target(target, history_samples)
    train_raw = _collect_klon_split(npz, controls_by_id, train_ids, target, stride, history_samples, input_ids)
    validation_raw = _collect_klon_split(
        npz,
        controls_by_id,
        validation_ids,
        target,
        stride,
        history_samples,
        input_ids,
    )
    test_raw = _collect_klon_split(npz, controls_by_id, test_ids, target, stride, history_samples, input_ids)

    input_mean = np.mean(train_raw.x, axis=0).astype(np.float32)
    input_std = np.std(train_raw.x, axis=0).astype(np.float32)
    input_std = np.where(input_std > 1.0e-12, input_std, 1.0).astype(np.float32)
    output_mean = float(np.mean(train_raw.y))
    output_std = float(np.std(train_raw.y))
    output_std = output_std if output_std > 1.0e-12 else 1.0
    return PreparedVectorDataset(
        train=_normalize_vector_split(train_raw, input_mean, input_std, output_mean, output_std),
        validation=_normalize_vector_split(validation_raw, input_mean, input_std, output_mean, output_std),
        test=_normalize_vector_split(test_raw, input_mean, input_std, output_mean, output_std),
        input_mean=input_mean,
        input_std=input_std,
        output_mean=output_mean,
        output_std=output_std,
        sample_rate_hz=int(manifest["sample_rate_hz"]),
        history_samples=history_samples,
        input_ids=input_ids,
        output_id=target,
        train_ids=train_ids,
        validation_ids=validation_ids,
        test_ids=test_ids,
    )


def write_mlp_weights(path: Path, layers: list[dict[str, np.ndarray]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("wb") as file:
        for layer in layers:
            for item in (layer["weight"], layer["bias"]):
                data = np.asarray(item, dtype="<f4").ravel()
                file.write(struct.pack("<I", int(data.shape[0])))
                file.write(data.tobytes())


def read_mlp_weights(path: Path, descriptor: dict[str, Any]) -> list[dict[str, np.ndarray]]:
    layout = descriptor["weights"]["layout"]
    layers = []
    with path.open("rb") as file:
        for layer in layout:
            out_features = int(layer["out_features"])
            in_features = int(layer["in_features"])
            weight = _read_weight_vector(file, out_features * in_features).reshape((out_features, in_features))
            bias = _read_weight_vector(file, out_features)
            layers.append({"weight": weight, "bias": bias})
    return layers


def infer_artifact_numpy(descriptor_path: Path, input_v: np.ndarray) -> np.ndarray:
    descriptor = _read_json(descriptor_path)
    weights_path = _resolve_manifest_path(descriptor_path, descriptor["weights"]["path"])
    layers = read_mlp_weights(weights_path, descriptor)
    norm = descriptor["io"]["normalization"]
    history_samples = _descriptor_input_features(descriptor)
    x = _causal_history_matrix(input_v.astype(np.float32), history_samples)
    x = ((x - float(norm["input_mean"])) / float(norm["input_std"])).astype(np.float32)
    normalized = infer_mlp_numpy(x, layers)
    return normalized.reshape((-1,)) * float(norm["output_std"]) + float(norm["output_mean"])


def export_neural_cell_vectors(
    *,
    descriptor_path: Path,
    output_path: Path,
    input_values: list[float] | None = None,
) -> Path:
    if input_values is None:
        input_values = [-0.12, -0.08, -0.04, -0.02, 0.0, 0.02, 0.04, 0.08, 0.12]
    descriptor = _read_json(descriptor_path)
    inputs = np.asarray(input_values, dtype=np.float32)
    outputs = infer_artifact_numpy(descriptor_path, inputs).astype(np.float32)
    payload = {
        "schema_version": 1,
        "descriptor": str(descriptor_path),
        "artifact_id": descriptor.get("artifact_id", "unknown"),
        "cases": [
            {
                "input_v": float(input_value),
                "expected_output_v": float(output_value),
            }
            for input_value, output_value in zip(inputs, outputs)
        ],
        "tolerance_abs": 1.0e-5,
        "notes": "Generated by greybound-lab export-neural-cell-vectors for Python/Rust equivalence checks.",
    }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    return output_path


def evaluate_neural_cell_against_spice(
    *,
    descriptor_path: Path,
    dataset_manifest_path: Path,
    report_path: Path,
    stride: int = 16,
    split: str = "all",
) -> Path:
    manifest = _read_json(dataset_manifest_path)
    dataset_path = _resolve_manifest_path(dataset_manifest_path, _artifact_path(manifest, "output"))
    npz = np.load(dataset_path)
    rows = []
    history_rows = []
    split_by_id = _split_by_stimulus_id(manifest)
    for stimulus in manifest["stimuli"]:
        stimulus_id = str(stimulus["id"])
        stimulus_split = split_by_id.get(stimulus_id, "unknown")
        if split != "all" and stimulus_split != split:
            continue
        prefix = stimulus_id + "__"
        time_s = npz[prefix + "time_s"]
        input_v = npz[prefix + "input_v"]
        reference_v = npz[prefix + "plate_ac_v"]
        settle_time_s = float(stimulus.get("parameters", {}).get("settle_time_s", 0.0))
        mask = time_s >= settle_time_s
        full_case_input = input_v[mask].astype(np.float32)
        case_reference = reference_v[mask][::stride].astype(np.float32)
        case_prediction = infer_artifact_numpy(descriptor_path, full_case_input).astype(np.float32)[::stride]
        rows.append(
            _evaluation_row(
                stimulus_id=stimulus_id,
                split=stimulus_split,
                kind=str(stimulus["kind"]),
                reference_v=case_reference,
                prediction_v=case_prediction,
            )
        )
        if str(stimulus["kind"]) == "dynamic_bias_recovery":
            history_rows.append(
                _history_probe_row(
                    stimulus_id=stimulus_id,
                    time_s=time_s,
                    input_v=input_v,
                    reference_v=reference_v,
                    prediction_v=infer_artifact_numpy(descriptor_path, input_v.astype(np.float32)).astype(np.float32),
                    parameters=stimulus.get("parameters", {}),
                )
            )
    descriptor = _read_json(descriptor_path)
    write_evaluation_report(
        report_path,
        descriptor=descriptor,
        dataset_manifest_path=dataset_manifest_path,
        descriptor_path=descriptor_path,
        rows=rows,
        history_rows=history_rows,
        stride=stride,
        split=split,
    )
    return report_path


def infer_mlp_numpy(x: np.ndarray, layers: list[dict[str, np.ndarray]]) -> np.ndarray:
    y = x.astype(np.float32)
    for index, layer in enumerate(layers):
        y = y @ layer["weight"].astype(np.float32).T + layer["bias"].astype(np.float32)
        if index != len(layers) - 1:
            y = np.tanh(y)
    return y.astype(np.float32)


def build_mlp_descriptor(
    *,
    manifest: dict[str, Any],
    manifest_path: Path,
    output_dir: Path,
    repo_root: Path,
    weights_path: Path,
    hidden_size: int,
    prepared: PreparedDataset,
    metrics: dict[str, float | int],
) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "artifact_id": output_dir.name,
        "cell_kind": "triode_gain_stage",
        "architecture": {
            "family": "mlp",
            "activation": "tanh",
            "receptive_field_samples": prepared.history_samples - 1,
            "layers": [
                {"type": "dense", "in_features": prepared.history_samples, "out_features": hidden_size},
                {"type": "dense", "in_features": hidden_size, "out_features": hidden_size},
                {"type": "dense", "in_features": hidden_size, "out_features": 1},
            ],
        },
        "created_at": datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "sample_rate_policy": {
            "mode": "fixed",
            "sample_rate_hz": prepared.sample_rate_hz,
        },
        "io": {
            "inputs": [{"id": "input_v", "unit": "V"}],
            "outputs": [{"id": "plate_ac_v", "unit": "V"}],
            "normalization": {
                "input_mean": prepared.input_mean,
                "input_std": prepared.input_std,
                "output_mean": prepared.output_mean,
                "output_std": prepared.output_std,
            },
        },
        "controls": [],
        "state": {
            "samples": 0,
            "floats": prepared.history_samples - 1,
            "description": "Causal MLP with input-voltage history." if prepared.history_samples > 1 else "Static MLP smoke-test cell. No streaming memory.",
        },
        "weights": {
            "format": "greybound-bin-v1",
            "path": weights_path.name,
            "sha256": sha256_file(weights_path),
            "dtype": "f32",
            "endianness": "little",
            "layout": [
                {"name": "dense0", "in_features": prepared.history_samples, "out_features": hidden_size},
                {"name": "dense1", "in_features": hidden_size, "out_features": hidden_size},
                {"name": "dense2", "in_features": hidden_size, "out_features": 1},
            ],
        },
        "training": {
            "framework": "PyTorch",
            "framework_version": _torch_version_or_unknown(),
            "code_revision": git_revision(repo_root),
            "dataset_manifest": relative_or_absolute(manifest_path, repo_root),
            "dataset_sha256": sha256_file(manifest_path),
        },
        "validation": {
            "status": "experimental",
            "metrics": metrics,
            "report": relative_or_absolute(output_dir / "training-report.md", repo_root),
            "limitations": [
                "Causal input-history MLP; no learned recurrent state or explicit capacitance state."
                if prepared.history_samples > 1
                else "Static input-to-plate-ac MLP; no state or capacitance memory.",
                "Trained on decimated SPICE samples for a smoke test.",
                "No source/load impedance sweep, B+ perturbation, component tolerance sweep, or real DI.",
                "Not approved for the live audio engine.",
            ],
        },
        "runtime": {
            "latency_samples": 0,
            "max_block_size": 1,
            "allocates_on_audio_thread": False,
            "denormal_safe": False,
            "cpu_notes": "Three dense layers plus a fixed input-history buffer. Python/Rust readers and experimental Nox30 integration exist; not approved for default live use.",
            "safety_clamps": {
                "input_v_min": _stimulus_min(manifest, "amplitude_v", default=-0.12),
                "input_v_max": _stimulus_max(manifest, "amplitude_v", default=0.12),
            },
        },
        "notes": "First PyTorch-to-Greybound artifact smoke test for the common-cathode SPICE dataset.",
    }


def build_klon_mlp_descriptor(
    *,
    manifest: dict[str, Any],
    manifest_path: Path,
    output_dir: Path,
    repo_root: Path,
    weights_path: Path,
    hidden_size: int,
    prepared: PreparedVectorDataset,
    metrics: dict[str, float | int],
) -> dict[str, Any]:
    input_features = len(prepared.input_ids)
    return {
        "schema_version": 1,
        "artifact_id": output_dir.name,
        "cell_kind": "klon_drive_clip_tone",
        "architecture": {
            "family": "mlp",
            "activation": "tanh",
            "receptive_field_samples": prepared.history_samples - 1,
            "layers": [
                {"type": "dense", "in_features": input_features, "out_features": hidden_size},
                {"type": "dense", "in_features": hidden_size, "out_features": hidden_size},
                {"type": "dense", "in_features": hidden_size, "out_features": 1},
            ],
        },
        "created_at": datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "sample_rate_policy": {
            "mode": "fixed",
            "sample_rate_hz": prepared.sample_rate_hz,
        },
        "io": {
            "inputs": [{"id": item, "unit": "V" if item.startswith("buffer") else "normalized"} for item in prepared.input_ids],
            "outputs": [{"id": prepared.output_id, "unit": "V"}],
            "normalization": {
                "input_mean": prepared.input_mean.astype(float).tolist(),
                "input_std": prepared.input_std.astype(float).tolist(),
                "output_mean": prepared.output_mean,
                "output_std": prepared.output_std,
            },
        },
        "controls": [
            {"id": "gain", "range": [0.0, 1.0]},
            {"id": "treble", "range": [0.0, 1.0]},
            {"id": "level", "range": [0.0, 1.0]},
        ],
        "state": {
            "samples": 0,
            "floats": prepared.history_samples - 1,
            "description": "Causal MLP over buffer-voltage history plus static pedal controls.",
        },
        "weights": {
            "format": "greybound-bin-v1",
            "path": weights_path.name,
            "sha256": sha256_file(weights_path),
            "dtype": "f32",
            "endianness": "little",
            "layout": [
                {"name": "dense0", "in_features": input_features, "out_features": hidden_size},
                {"name": "dense1", "in_features": hidden_size, "out_features": hidden_size},
                {"name": "dense2", "in_features": hidden_size, "out_features": 1},
            ],
        },
        "training": {
            "framework": "PyTorch",
            "framework_version": _torch_version_or_unknown(),
            "code_revision": git_revision(repo_root),
            "dataset_manifest": relative_or_absolute(manifest_path, repo_root),
            "dataset_sha256": sha256_file(manifest_path),
            "source_dataset_id": manifest.get("dataset_id", "unknown"),
        },
        "validation": {
            "status": "experimental_lab_only",
            "metrics": metrics,
            "report": relative_or_absolute(output_dir / "training-report.md", repo_root),
            "limitations": [
                "Synthetic SPICE only; NAM remains the audio acceptance reference.",
                "Single-target model. Train clip_ac_v, mix_ac_v, and tone_ac_v separately before choosing an integrated runtime strategy.",
                "The clip_ac_v model is useful as a shadow diagnostic; current audio-level blend tests do not justify replacing the analytic Minotaur clip path.",
            ],
        },
        "runtime": {
            "latency_samples": 0,
            "max_block_size": 1,
            "allocates_on_audio_thread": False,
            "denormal_safe": False,
            "cpu_notes": "Lab artifact for Minotaur drive/clip/tone research. Rust runtime vector-input adapter is still required.",
        },
        "notes": "First Greybound Klon/Minotaur neural candidate trained from the generated SPICE drive/clip/tone corpus.",
    }


def write_training_report(
    path: Path,
    descriptor: dict[str, Any],
    metrics: dict[str, float | int],
    prepared: PreparedDataset,
) -> None:
    path.write_text(
        f"""# Neural Cell Training Report: {descriptor['artifact_id']}

## Purpose

Train the smallest useful PyTorch MLP smoke-test model against the first
common-cathode SPICE dataset, then export it as a Greybound neural-cell artifact.

This report does not approve the model for the live engine. It proves the
training/export/equivalence path.

## Dataset

- Train stimuli: {', '.join(f'`{item}`' for item in prepared.train_ids)}
- Validation stimuli: {', '.join(f'`{item}`' for item in prepared.validation_ids)}
- Test stimuli: {', '.join(f'`{item}`' for item in prepared.test_ids)}
- Train samples after stride: `{metrics['train_samples']}`
- Validation samples after stride: `{metrics['validation_samples']}`
- Test samples after stride: `{metrics['test_samples']}`
- Causal input history samples: `{metrics['history_samples']}`

## Metrics

| Metric | Value |
| --- | ---: |
| Train MSE normalized | {float(metrics['train_mse_normalized']):.8g} |
| Validation MSE normalized | {float(metrics['validation_mse_normalized']):.8g} |
| Test MSE normalized | {float(metrics['test_mse_normalized']):.8g} |
| Test baseline MSE normalized | {float(metrics['test_baseline_mse_normalized']):.8g} |
| Export max abs error normalized | {float(metrics['export_max_abs_error_normalized']):.8g} |

## Export

- Descriptor: `model.greybound.json`
- Weights: `weights.greybound.bin`
- Weight format: `greybound-bin-v1`
- Runtime status: experimental descriptor only

## Limitations

- {'Causal input-history MLP only: no learned recurrent state or explicit capacitance model.' if int(metrics['history_samples']) > 1 else 'Static MLP only: no state, no capacitance memory, no bias history.'}
- Trained on decimated samples.
- The dataset is still a small SPICE corpus.
- Rust inference and experimental Nox30 integration exist; not approved for default live use.
""",
        encoding="utf-8",
    )


def write_klon_training_report(
    path: Path,
    descriptor: dict[str, Any],
    metrics: dict[str, float | int],
    prepared: PreparedVectorDataset,
) -> None:
    path.write_text(
        f"""# Minotaur Neural Cell Training Report: {descriptor['artifact_id']}

## Purpose

Train a small causal MLP against the Klon/Minotaur SPICE corpus. This is a
targeted drive/clip/tone experiment, not a full-pedal replacement.

## Dataset

- Target: `{prepared.output_id}`
- Inputs: {', '.join(f'`{item}`' for item in prepared.input_ids)}
- Train stimuli: {', '.join(f'`{item}`' for item in prepared.train_ids)}
- Validation stimuli: {', '.join(f'`{item}`' for item in prepared.validation_ids)}
- Test stimuli: {', '.join(f'`{item}`' for item in prepared.test_ids)}
- Train samples after stride: `{metrics['train_samples']}`
- Validation samples after stride: `{metrics['validation_samples']}`
- Test samples after stride: `{metrics['test_samples']}`
- Causal buffer history samples: `{metrics['history_samples']}`

## Metrics

| Metric | Value |
| --- | ---: |
| Train MSE normalized | {float(metrics['train_mse_normalized']):.8g} |
| Validation MSE normalized | {float(metrics['validation_mse_normalized']):.8g} |
| Test MSE normalized | {float(metrics['test_mse_normalized']):.8g} |
| Test baseline MSE normalized | {float(metrics['test_baseline_mse_normalized']):.8g} |
| Export max abs error normalized | {float(metrics['export_max_abs_error_normalized']):.8g} |

## Export

- Descriptor: `model.greybound.json`
- Weights: `weights.greybound.bin`
- Weight format: `greybound-bin-v1`
- Runtime status: experimental lab artifact

## Integration Notes

The model uses vector input normalization. The Rust neural-cell runtime can load
the descriptor, but this artifact should be treated as a shadow diagnostic until
audio-level NAM comparison improves over the analytic Minotaur path.
""",
        encoding="utf-8",
    )


def write_evaluation_report(
    path: Path,
    *,
    descriptor: dict[str, Any],
    dataset_manifest_path: Path,
    descriptor_path: Path,
    rows: list[NeuralCellEvaluationRow],
    history_rows: list[NeuralCellHistoryProbeRow],
    stride: int,
    split: str,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    table = "\n".join(
        "| `{}` | `{}` | `{}` | {} | {:.3f} | {:.3f} | {:.3f} | {:.3f} | {:.2%} | {:.3f} |".format(
            row.stimulus_id,
            row.split,
            row.kind,
            row.samples,
            row.reference_rms_v * 1000.0,
            row.rmse_v * 1000.0,
            row.mae_v * 1000.0,
            row.max_abs_error_v * 1000.0,
            row.relative_rmse,
            row.zero_baseline_rmse_v * 1000.0,
        )
        for row in rows
    )
    aggregate = _aggregate_rows(rows)
    history_section = _render_history_probe_section(history_rows)
    input_features = _descriptor_input_features(descriptor)
    model_note = (
        "This artifact uses a causal input-history MLP. Large residual error after adding short history means the missing behavior likely needs explicit state, a longer causal model, or fixture changes rather than another static transfer curve."
        if input_features > 1
        else "Large relative error on hot sine or two-tone cases means the static MLP is not yet capturing the nonlinear shape needed for a tube-cell replacement. Large relative error on the bias recovery stress probe is a domain-boundary signal: first check high-amplitude coverage, then check whether residual error remains after coverage is added."
    )
    path.write_text(
        f"""# Neural Cell SPICE Evaluation: {descriptor.get('artifact_id', 'unknown')}

## Purpose

Evaluate an exported Greybound neural-cell artifact against the SPICE dataset in
physical units. This report measures whether the artifact predicts the held
SPICE `plate_ac_v` target from `input_v`.

This is still a cell-level validation report. It does not approve the model for
live audio use.

## Inputs

- Descriptor: `{descriptor_path}`
- Dataset manifest: `{dataset_manifest_path}`
- Split filter: `{split}`
- Evaluation stride: `{stride}`

## Aggregate

| Metric | Value |
| --- | ---: |
| Stimuli evaluated | {len(rows)} |
| Samples evaluated | {aggregate['samples']} |
| Weighted RMSE | {aggregate['rmse_v'] * 1000.0:.3f} mV |
| Weighted MAE | {aggregate['mae_v'] * 1000.0:.3f} mV |
| Weighted relative RMSE | {aggregate['relative_rmse']:.2%} |
| Zero baseline RMSE | {aggregate['zero_baseline_rmse_v'] * 1000.0:.3f} mV |

## Per-Stimulus Metrics

| Stimulus | Split | Kind | Samples | Ref RMS mV | RMSE mV | MAE mV | Max abs mV | Rel RMSE | Zero baseline RMSE mV |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
{table}

## Interpretation

Compare RMSE against the zero baseline. A useful model should beat the baseline
on held-out stimuli, not only on training stimuli. {model_note}
{history_section}
""",
        encoding="utf-8",
    )


def _collect_split(
    npz: Any,
    manifest: dict[str, Any],
    stimulus_ids: list[str],
    *,
    stride: int,
    history_samples: int,
) -> PreparedSplit:
    xs = []
    ys = []
    by_id = {item["id"]: item for item in manifest["stimuli"]}
    for stimulus_id in stimulus_ids:
        stimulus = by_id[stimulus_id]
        prefix = stimulus_id + "__"
        time_s = npz[prefix + "time_s"]
        input_v = npz[prefix + "input_v"]
        plate_ac_v = npz[prefix + "plate_ac_v"]
        settle_time_s = float(stimulus.get("parameters", {}).get("settle_time_s", 0.0))
        mask = time_s >= settle_time_s
        xs.append(_causal_history_matrix(input_v[mask].astype(np.float32), history_samples)[::stride])
        ys.append(plate_ac_v[mask][::stride].reshape((-1, 1)))
    if not xs:
        return PreparedSplit(x=np.zeros((0, 1), dtype=np.float32), y=np.zeros((0, 1), dtype=np.float32))
    return PreparedSplit(x=np.concatenate(xs).astype(np.float32), y=np.concatenate(ys).astype(np.float32))


def _evaluation_row(
    *,
    stimulus_id: str,
    split: str,
    kind: str,
    reference_v: np.ndarray,
    prediction_v: np.ndarray,
) -> NeuralCellEvaluationRow:
    error = prediction_v - reference_v
    reference_rms = _rms(reference_v)
    rmse = _rms(error)
    mae = float(np.mean(np.abs(error))) if error.size else 0.0
    max_abs = float(np.max(np.abs(error))) if error.size else 0.0
    zero_baseline_rmse = _rms(reference_v)
    return NeuralCellEvaluationRow(
        stimulus_id=stimulus_id,
        split=split,
        kind=kind,
        samples=int(reference_v.shape[0]),
        reference_rms_v=reference_rms,
        rmse_v=rmse,
        mae_v=mae,
        max_abs_error_v=max_abs,
        relative_rmse=rmse / max(reference_rms, 1.0e-12),
        zero_baseline_rmse_v=zero_baseline_rmse,
    )


def _history_probe_row(
    *,
    stimulus_id: str,
    time_s: np.ndarray,
    input_v: np.ndarray,
    reference_v: np.ndarray,
    prediction_v: np.ndarray,
    parameters: dict[str, Any],
) -> NeuralCellHistoryProbeRow:
    pre_start = float(parameters["pre_probe_start_s"])
    pre_stop = float(parameters["pre_probe_stop_s"])
    post_start = float(parameters["post_probe_start_s"])
    post_stop = float(parameters["post_probe_stop_s"])
    reference_pre_gain = _window_gain_db(time_s, input_v, reference_v, pre_start, pre_stop)
    reference_post_gain = _window_gain_db(time_s, input_v, reference_v, post_start, post_stop)
    prediction_pre_gain = _window_gain_db(time_s, input_v, prediction_v, pre_start, pre_stop)
    prediction_post_gain = _window_gain_db(time_s, input_v, prediction_v, post_start, post_stop)
    reference_delta = reference_post_gain - reference_pre_gain
    prediction_delta = prediction_post_gain - prediction_pre_gain
    return NeuralCellHistoryProbeRow(
        stimulus_id=stimulus_id,
        reference_pre_gain_db=reference_pre_gain,
        reference_post_gain_db=reference_post_gain,
        reference_delta_db=reference_delta,
        prediction_pre_gain_db=prediction_pre_gain,
        prediction_post_gain_db=prediction_post_gain,
        prediction_delta_db=prediction_delta,
        delta_error_db=prediction_delta - reference_delta,
    )


def _window_gain_db(
    time_s: np.ndarray,
    input_v: np.ndarray,
    output_v: np.ndarray,
    start_s: float,
    stop_s: float,
) -> float:
    margin_s = min((stop_s - start_s) * 0.20, 0.003)
    mask = (time_s >= start_s + margin_s) & (time_s <= stop_s - margin_s)
    if not np.any(mask):
        mask = (time_s >= start_s) & (time_s <= stop_s)
    input_rms = _rms(_remove_mean(input_v[mask]))
    output_rms = _rms(_remove_mean(output_v[mask]))
    return 20.0 * math.log10((output_rms + 1.0e-30) / (input_rms + 1.0e-30))


def _render_history_probe_section(rows: list[NeuralCellHistoryProbeRow]) -> str:
    if not rows:
        return ""
    table = "\n".join(
        "| `{}` | {:.2f} | {:.2f} | {:.2f} | {:.2f} | {:.2f} | {:.2f} | {:.2f} |".format(
            row.stimulus_id,
            row.reference_pre_gain_db,
            row.reference_post_gain_db,
            row.reference_delta_db,
            row.prediction_pre_gain_db,
            row.prediction_post_gain_db,
            row.prediction_delta_db,
            row.delta_error_db,
        )
        for row in rows
    )
    return f"""

## History Probe Metrics

These rows compare the gain of the same low-level probe before and after a hot
stress window. A static input-to-output model can fit the instantaneous transfer
curve, but it cannot know whether the same input sample happened before or after
cathode-bias recovery unless history is encoded in the input or model state.

| Stimulus | Ref pre gain dB | Ref post gain dB | Ref delta dB | Pred pre gain dB | Pred post gain dB | Pred delta dB | Delta error dB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
{table}
"""


def _remove_mean(values: np.ndarray) -> np.ndarray:
    return values - float(np.mean(values)) if values.size else values


def _aggregate_rows(rows: list[NeuralCellEvaluationRow]) -> dict[str, float | int]:
    samples = sum(row.samples for row in rows)
    if samples == 0:
        return {
            "samples": 0,
            "rmse_v": 0.0,
            "mae_v": 0.0,
            "relative_rmse": 0.0,
            "zero_baseline_rmse_v": 0.0,
        }
    rmse = math.sqrt(sum((row.rmse_v**2) * row.samples for row in rows) / samples)
    mae = sum(row.mae_v * row.samples for row in rows) / samples
    reference_rms = math.sqrt(sum((row.reference_rms_v**2) * row.samples for row in rows) / samples)
    zero_baseline = math.sqrt(sum((row.zero_baseline_rmse_v**2) * row.samples for row in rows) / samples)
    return {
        "samples": samples,
        "rmse_v": rmse,
        "mae_v": mae,
        "relative_rmse": rmse / max(reference_rms, 1.0e-12),
        "zero_baseline_rmse_v": zero_baseline,
    }


def _split_by_stimulus_id(manifest: dict[str, Any]) -> dict[str, str]:
    result = {}
    for split in ("train", "validation", "test"):
        for stimulus_id in manifest["splits"].get(split, []):
            result[str(stimulus_id)] = split
    return result


def _normalize_split(
    split: PreparedSplit,
    input_mean: float,
    input_std: float,
    output_mean: float,
    output_std: float,
) -> PreparedSplit:
    return PreparedSplit(
        x=((split.x - input_mean) / input_std).astype(np.float32),
        y=((split.y - output_mean) / output_std).astype(np.float32),
    )


def _normalize_vector_split(
    split: PreparedSplit,
    input_mean: np.ndarray,
    input_std: np.ndarray,
    output_mean: float,
    output_std: float,
) -> PreparedSplit:
    return PreparedSplit(
        x=((split.x - input_mean.reshape((1, -1))) / input_std.reshape((1, -1))).astype(np.float32),
        y=((split.y - output_mean) / output_std).astype(np.float32),
    )


def klon_input_ids_for_target(target: str, history_samples: int) -> list[str]:
    input_ids = [f"buffer_ac_v_t-{offset}" if offset else "buffer_ac_v" for offset in range(history_samples)]
    if target == "clip_ac_v":
        return input_ids + ["gain"]
    if target == "mix_ac_v":
        return input_ids + ["gain"]
    if target == "tone_ac_v":
        return input_ids + ["gain", "treble"]
    return input_ids + ["gain", "treble"]


def _collect_klon_split(
    npz: Any,
    controls_by_id: dict[str, dict[str, Any]],
    stimulus_ids: list[str],
    target: str,
    stride: int,
    history_samples: int,
    input_ids: list[str],
) -> PreparedSplit:
    features = []
    targets = []
    control_ids = [item for item in input_ids if not item.startswith("buffer_ac_v")]
    for stimulus_id in stimulus_ids:
        prefix = stimulus_id + "__"
        buffer_key = prefix + "buffer_ac_v"
        target_key = prefix + target
        if buffer_key not in npz:
            raise ValueError(f"dataset is missing {buffer_key}")
        if target_key not in npz:
            raise ValueError(f"dataset is missing {target_key}")
        buffer = npz[buffer_key].astype(np.float32)[::stride]
        buffer_history = _causal_history_matrix(buffer, history_samples)
        control_values = controls_by_id.get(stimulus_id, {})
        control_array = np.asarray(
            [float(control_values.get(control_id, 0.5)) for control_id in control_ids],
            dtype=np.float32,
        ).reshape((1, len(control_ids)))
        controls = np.tile(control_array, (buffer_history.shape[0], 1))
        case_x = np.concatenate([buffer_history, controls], axis=1)
        case_y = npz[target_key].astype(np.float32).reshape((-1, 1))[::stride]
        features.append(case_x)
        targets.append(case_y)
    if not features:
        return PreparedSplit(x=np.zeros((0, len(input_ids)), dtype=np.float32), y=np.zeros((0, 1), dtype=np.float32))
    return PreparedSplit(
        x=np.concatenate(features, axis=0).astype(np.float32),
        y=np.concatenate(targets, axis=0).astype(np.float32),
    )


def _causal_history_matrix(values: np.ndarray, history_samples: int) -> np.ndarray:
    values = values.reshape((-1,)).astype(np.float32)
    if history_samples < 1:
        raise ValueError("history_samples must be at least 1")
    matrix = np.zeros((values.shape[0], history_samples), dtype=np.float32)
    for offset in range(history_samples):
        if offset == 0:
            matrix[:, offset] = values
        else:
            matrix[offset:, offset] = values[:-offset]
    return matrix


def _descriptor_input_features(descriptor: dict[str, Any]) -> int:
    layout = descriptor.get("weights", {}).get("layout", [])
    if not layout:
        return 1
    return max(1, int(layout[0].get("in_features", 1)))


def _extract_mlp_layers(model: Any) -> list[dict[str, np.ndarray]]:
    layers = []
    for module in model:
        if module.__class__.__name__ == "Linear":
            layers.append(
                {
                    "weight": module.weight.detach().cpu().numpy().astype(np.float32),
                    "bias": module.bias.detach().cpu().numpy().astype(np.float32),
                }
            )
    return layers


def _read_weight_vector(file: Any, expected_count: int) -> np.ndarray:
    count_raw = file.read(4)
    if len(count_raw) != 4:
        raise ValueError("unexpected end of weights file")
    count = struct.unpack("<I", count_raw)[0]
    if count != expected_count:
        raise ValueError(f"weight vector has {count} values, expected {expected_count}")
    data = file.read(count * 4)
    if len(data) != count * 4:
        raise ValueError("unexpected end of weights file")
    return np.frombuffer(data, dtype="<f4").astype(np.float32)


def _artifact_path(manifest: dict[str, Any], kind: str) -> str:
    for artifact in manifest["artifacts"]:
        if artifact["kind"] == kind:
            return str(artifact["path"])
    raise ValueError(f"manifest has no artifact of kind {kind!r}")


def _resolve_manifest_path(base_path: Path, path: str) -> Path:
    candidate = Path(path)
    if candidate.is_absolute():
        return candidate
    repo_relative = Path.cwd() / candidate
    if repo_relative.exists():
        return repo_relative
    return base_path.parent / candidate


def _read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def _mse(a: np.ndarray, b: np.ndarray) -> float:
    if a.size == 0 or b.size == 0:
        return 0.0
    return float(np.mean((a - b) ** 2))


def _rms(samples: np.ndarray) -> float:
    if samples.size == 0:
        return 0.0
    return float(np.sqrt(np.mean(np.square(samples, dtype=np.float64))))


def _import_torch() -> Any:
    try:
        import torch
    except ModuleNotFoundError as error:
        raise RuntimeError(
            "PyTorch is required for training. Run with: "
            "uv --project lab run --with torch greybound-lab train-neural-cell ..."
        ) from error
    return torch


def _torch_version_or_unknown() -> str:
    try:
        import torch
    except ModuleNotFoundError:
        return "unknown"
    return str(torch.__version__)


def _stimulus_min(manifest: dict[str, Any], key: str, *, default: float) -> float:
    values = [float(item.get("parameters", {}).get(key)) for item in manifest["stimuli"] if key in item.get("parameters", {})]
    return -max(values) if values else default


def _stimulus_max(manifest: dict[str, Any], key: str, *, default: float) -> float:
    values = [float(item.get("parameters", {}).get(key)) for item in manifest["stimuli"] if key in item.get("parameters", {})]
    return max(values) if values else default
