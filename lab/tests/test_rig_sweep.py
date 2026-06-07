from __future__ import annotations

from greybound_lab.rig_sweep import replace_amp_control


def test_replace_amp_control_updates_control_and_name() -> None:
    rig = """{
  name: 'nox30-driven',
  amp: {
    model: 'nox30',
    controls: {
      volume: 0.76,
      drive: 0.68,
    },
  },
}
"""

    updated = replace_amp_control(rig, "drive", 0.42, "sweep-drive-0p420")

    assert "name: 'sweep-drive-0p420'," in updated
    assert "drive: 0.420000," in updated
    assert "volume: 0.76," in updated


def test_replace_amp_control_rejects_missing_control() -> None:
    try:
        replace_amp_control("{ amp: { controls: { drive: 0.5 } } }", "presence", 0.2, "generated")
    except ValueError as exc:
        assert "could not find amp.controls.presence" in str(exc)
    else:
        raise AssertionError("expected missing control to fail")
