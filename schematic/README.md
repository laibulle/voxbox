# AC30 Top Boost schematic references

The model targets a JMI-era AC30/6 fitted with the optional Top Boost unit.
There was no single canonical AC30 circuit across all production years, so the
reference is explicitly split into these original drawings:

- `jmi-os065-ac30-6-normal.jpg`: JMI OS/065 AC30/6 chassis, including the
  long-tail-pair phase inverter, Cut control, cathode-biased EL84 quartet,
  output transformer, and GZ34 supply.
- `jmi-os010-top-boost.jpg`: JMI OS/010 add-on Top Boost circuit, including the
  bright-capped 500k volume control, two ECC83 stages, and interactive treble
  and bass network.
- `vox-ac30-reference.pdf` and `vox-top-boost-reference.pdf`: clearer service
  reference copies used to cross-check the original drawings.

The extracted component/topology map used by the DSP is in
`circuit-map.toml`. This remains a real-time graybox model, not a SPICE or
component-exact wave-digital simulation.

## Sources

- https://www.voxac30.org.uk/vox_ac30_circuit_diagrams.html
- https://www.voxac30.org.uk/vox_ac30_top_boost_circuit.html
- https://el34world.com/charts/Schematics/Files/Vox/Vox_Schematics.htm
