//! Experimental component-level circuit solvers.
//!
//! This module is deliberately separate from `amp` so future amp and pedal
//! models can share the same WDF/MNA-style building blocks. The first cell is a
//! Newton-solved triode stage with supply interaction; future cells can replace
//! analytic nonlinearities with WDF or Neural WDF scattering relations.

pub mod triode;
