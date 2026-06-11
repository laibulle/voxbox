from __future__ import annotations

from pathlib import Path

from greybound_lab.audio import read_wav_mono
from greybound_lab.segments import load_segments
from greybound_lab.stimuli import generate_stimuli


def test_generate_stimuli_writes_wavs_and_markers(tmp_path: Path) -> None:
    generated = generate_stimuli(tmp_path, sample_rate_hz=44_100)

    assert {item.wav_path.name for item in generated} == {
        "sine-level-sweep.wav",
        "frequency-response-sweep.wav",
        "nonlinear-transfer-probe.wav",
        "two-tone-imd.wav",
        "aliasing-stress.wav",
        "sag-bursts.wav",
        "pluck-attacks.wav",
        "reverb-diagnostics.wav",
    }
    for item in generated:
        assert item.wav_path.exists()
        assert item.markers_path.exists()
        audio = read_wav_mono(item.wav_path)
        segments = load_segments(item.markers_path)
        assert audio.sample_rate == 44_100
        assert audio.samples.size > 0
        assert segments
