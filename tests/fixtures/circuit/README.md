Circuit fixtures
================

These fixtures are reference targets for the component-level solvers in
`core/src/circuit`. They are intentionally kept outside `core/src/amp` so the
same circuit cells can later be reused by amps, pedals, and utility stages.

`common_cathode_12ax7.cir` is a ngspice starting point for the ECC83/12AX7
common-cathode stage implemented in `circuit::triode`. It writes transient data
to `/tmp/voxbox_common_cathode_12ax7.dat`. Use it to compare:

- idle plate voltage, cathode voltage, and B+ sag
- transient gain at 1 kHz with and without cathode bypass
- blocking behavior from the input coupling capacitor and grid leak

The Rust model should eventually load measured or simulated operating points
from these fixtures in regression tests. For now, this file documents the
electrical target while the in-process solver is still evolving.
