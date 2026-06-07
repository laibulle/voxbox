use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, SampleFormat, SampleRate, StreamConfig, SupportedStreamConfigRange,
};
use crossterm::{
    cursor,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{self, ClearType},
};
use greybound::amp::{
    configure_nox30_first_stage_neural, AmpControls, NeuralCellMode, Nox30OperatingPoint,
};
use greybound::ir::SpeakerStage;
use greybound::{
    amp_model_descriptor, BrigadeControls, CelesteControls, ControlDescriptor, ControlKind,
    DartfordControls, DartfordWave, DeviceConfig, DeviceControls, DeviceSlotConfig,
    DeviceSlotControls, GodessOneControls, GodessOneMode, JetstreamControls, LumenControls,
    MinotaurControls, MonarchControls, MuffinControls, MuonControls, RigConfig, SignalChain,
    SignalChainConfig, SignalChainControls, SpringfieldControls, TronControls,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame, Terminal,
};
use rtrb::{Consumer, RingBuffer};
use std::collections::VecDeque;
use std::env;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc,
};
use std::thread;
use std::time::Instant;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RMS_SCALE: f64 = 1_000_000_000.0;
const TELEMETRY_SCALE: f64 = 1_000_000_000.0;
const NEAR_CLIP_LEVEL: f32 = 0.98;
const CLIP_LEVEL: f32 = 1.0;
const MONITOR_LOG_LINES: usize = 5_000;
const MONITOR_REFRESH: Duration = Duration::from_millis(250);
const MONITOR_EVENT_POLL: Duration = Duration::from_millis(16);
const VU_WIDTH: usize = 28;
type MonitorTerminal = Terminal<CrosstermBackend<io::Stderr>>;

#[derive(Clone)]
struct SharedAmpControls {
    enabled: Arc<AtomicBool>,
    volume: Arc<AtomicU32>,
    bass: Arc<AtomicU32>,
    treble: Arc<AtomicU32>,
    cut: Arc<AtomicU32>,
    output: Arc<AtomicU32>,
    drive: Arc<AtomicU32>,
    presence: Arc<AtomicU32>,
    sag: Arc<AtomicU32>,
}

#[derive(Clone)]
struct SharedDeviceControls {
    slots: Arc<Vec<SharedDeviceSlotControls>>,
}

struct SharedDeviceSlotControls {
    bypassed: AtomicBool,
    controls: SharedDeviceControl,
}

#[derive(Clone)]
struct SharedCabControls {
    enabled: Arc<AtomicBool>,
}

enum SharedDeviceControl {
    Default,
    Lumen {
        peak_reduction: AtomicU32,
        gain: AtomicU32,
        emphasis: AtomicU32,
        mix: AtomicU32,
    },
    Muon {
        sensitivity: AtomicU32,
        range: AtomicU32,
        resonance: AtomicU32,
        mix: AtomicU32,
    },
    Muffin {
        sustain: AtomicU32,
        tone: AtomicU32,
        level: AtomicU32,
    },
    Minotaur {
        gain: AtomicU32,
        treble: AtomicU32,
        output: AtomicU32,
    },
    Monarch {
        gain: AtomicU32,
        tone: AtomicU32,
        output: AtomicU32,
    },
    GodessOne {
        distortion: AtomicU32,
        tone: AtomicU32,
        level: AtomicU32,
        mode: GodessOneMode,
    },
    Dartford {
        rate_hz: AtomicU32,
        depth: AtomicU32,
        level: AtomicU32,
        wave: DartfordWave,
    },
    Tron {
        rate_hz: AtomicU32,
        depth: AtomicU32,
        feedback: AtomicU32,
        mix: AtomicU32,
    },
    Jetstream {
        manual: AtomicU32,
        rate_hz: AtomicU32,
        depth: AtomicU32,
        feedback: AtomicU32,
        mix: AtomicU32,
    },
    Celeste {
        rate_hz: AtomicU32,
        depth: AtomicU32,
        tone: AtomicU32,
        mix: AtomicU32,
    },
    Brigade {
        time_ms: AtomicU32,
        repeats: AtomicU32,
        tone: AtomicU32,
        mix: AtomicU32,
    },
    Springfield {
        dwell: AtomicU32,
        tone: AtomicU32,
        mix: AtomicU32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AmpControlParam {
    Volume,
    Bass,
    Treble,
    Cut,
    Drive,
    Presence,
    Sag,
}

impl AmpControlParam {
    const ALL: [Self; 7] = [
        Self::Volume,
        Self::Bass,
        Self::Treble,
        Self::Cut,
        Self::Drive,
        Self::Presence,
        Self::Sag,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Volume => "volume",
            Self::Bass => "bass",
            Self::Treble => "treble",
            Self::Cut => "cut",
            Self::Drive => "drive",
            Self::Presence => "presence",
            Self::Sag => "sag",
        }
    }

    fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

impl SharedAmpControls {
    fn new(controls: AmpControls, enabled: bool) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(enabled)),
            volume: Arc::new(AtomicU32::new(controls.volume.to_bits())),
            bass: Arc::new(AtomicU32::new(controls.bass.to_bits())),
            treble: Arc::new(AtomicU32::new(controls.treble.to_bits())),
            cut: Arc::new(AtomicU32::new(controls.cut.to_bits())),
            output: Arc::new(AtomicU32::new(controls.output.to_bits())),
            drive: Arc::new(AtomicU32::new(controls.drive.to_bits())),
            presence: Arc::new(AtomicU32::new(controls.presence.to_bits())),
            sag: Arc::new(AtomicU32::new(controls.sag.to_bits())),
        }
    }

    fn load(&self) -> AmpControls {
        AmpControls {
            volume: load_atomic_f32(&self.volume),
            bass: load_atomic_f32(&self.bass),
            treble: load_atomic_f32(&self.treble),
            cut: load_atomic_f32(&self.cut),
            output: load_atomic_f32(&self.output),
            drive: load_atomic_f32(&self.drive),
            presence: load_atomic_f32(&self.presence),
            sag: load_atomic_f32(&self.sag),
        }
    }

    fn load_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn toggle_enabled(&self) {
        let current = self.enabled.load(Ordering::Relaxed);
        self.enabled.store(!current, Ordering::Relaxed);
    }

    fn adjust(&self, param: AmpControlParam, delta: f32) {
        let slot = match param {
            AmpControlParam::Volume => &self.volume,
            AmpControlParam::Bass => &self.bass,
            AmpControlParam::Treble => &self.treble,
            AmpControlParam::Cut => &self.cut,
            AmpControlParam::Drive => &self.drive,
            AmpControlParam::Presence => &self.presence,
            AmpControlParam::Sag => &self.sag,
        };
        let current = load_atomic_f32(slot);
        slot.store(
            (current + delta).clamp(0.0, 1.0).to_bits(),
            Ordering::Relaxed,
        );
    }
}

impl SharedDeviceControls {
    fn new(controls: &[DeviceSlotControls]) -> Self {
        Self {
            slots: Arc::new(controls.iter().map(SharedDeviceSlotControls::new).collect()),
        }
    }

    fn load(&self) -> Vec<DeviceSlotControls> {
        self.slots
            .iter()
            .map(SharedDeviceSlotControls::load)
            .collect()
    }

    fn load_into(&self, target: &mut Vec<DeviceSlotControls>) {
        target.clear();
        target.extend(self.slots.iter().map(SharedDeviceSlotControls::load));
    }

    fn adjust(&self, slot_index: usize, param_index: usize, delta: f32, min: f32, max: f32) {
        if let Some(slot) = self.slots.get(slot_index) {
            slot.adjust(param_index, delta, min, max);
        }
    }

    fn toggle_bypass(&self, slot_index: usize) {
        if let Some(slot) = self.slots.get(slot_index) {
            slot.toggle_bypass();
        }
    }
}

impl SharedDeviceSlotControls {
    fn new(controls: &DeviceSlotControls) -> Self {
        Self {
            bypassed: AtomicBool::new(controls.bypassed),
            controls: SharedDeviceControl::new(controls.controls),
        }
    }

    fn load(&self) -> DeviceSlotControls {
        DeviceSlotControls {
            bypassed: self.bypassed.load(Ordering::Relaxed),
            controls: self.controls.load(),
        }
    }

    fn adjust(&self, param_index: usize, delta: f32, min: f32, max: f32) {
        self.controls.adjust(param_index, delta, min, max);
    }

    fn toggle_bypass(&self) {
        let current = self.bypassed.load(Ordering::Relaxed);
        self.bypassed.store(!current, Ordering::Relaxed);
    }
}

impl SharedCabControls {
    fn new(enabled: bool) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(enabled)),
        }
    }

    fn load(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn toggle(&self) {
        let current = self.enabled.load(Ordering::Relaxed);
        self.enabled.store(!current, Ordering::Relaxed);
    }
}

impl SharedDeviceControl {
    fn new(controls: DeviceControls) -> Self {
        match controls {
            DeviceControls::Default => Self::Default,
            DeviceControls::Lumen(controls) => Self::Lumen {
                peak_reduction: AtomicU32::new(controls.peak_reduction.to_bits()),
                gain: AtomicU32::new(controls.gain.to_bits()),
                emphasis: AtomicU32::new(controls.emphasis.to_bits()),
                mix: AtomicU32::new(controls.mix.to_bits()),
            },
            DeviceControls::Muon(controls) => Self::Muon {
                sensitivity: AtomicU32::new(controls.sensitivity.to_bits()),
                range: AtomicU32::new(controls.range.to_bits()),
                resonance: AtomicU32::new(controls.resonance.to_bits()),
                mix: AtomicU32::new(controls.mix.to_bits()),
            },
            DeviceControls::Muffin(controls) => Self::Muffin {
                sustain: AtomicU32::new(controls.sustain.to_bits()),
                tone: AtomicU32::new(controls.tone.to_bits()),
                level: AtomicU32::new(controls.level.to_bits()),
            },
            DeviceControls::Minotaur(controls) => Self::Minotaur {
                gain: AtomicU32::new(controls.gain.to_bits()),
                treble: AtomicU32::new(controls.treble.to_bits()),
                output: AtomicU32::new(controls.output.to_bits()),
            },
            DeviceControls::Monarch(controls) => Self::Monarch {
                gain: AtomicU32::new(controls.gain.to_bits()),
                tone: AtomicU32::new(controls.tone.to_bits()),
                output: AtomicU32::new(controls.output.to_bits()),
            },
            DeviceControls::GodessOne(controls) => Self::GodessOne {
                distortion: AtomicU32::new(controls.distortion.to_bits()),
                tone: AtomicU32::new(controls.tone.to_bits()),
                level: AtomicU32::new(controls.level.to_bits()),
                mode: controls.mode,
            },
            DeviceControls::Dartford(controls) => Self::Dartford {
                rate_hz: AtomicU32::new(controls.rate_hz.to_bits()),
                depth: AtomicU32::new(controls.depth.to_bits()),
                level: AtomicU32::new(controls.level.to_bits()),
                wave: controls.wave,
            },
            DeviceControls::Tron(controls) => Self::Tron {
                rate_hz: AtomicU32::new(controls.rate_hz.to_bits()),
                depth: AtomicU32::new(controls.depth.to_bits()),
                feedback: AtomicU32::new(controls.feedback.to_bits()),
                mix: AtomicU32::new(controls.mix.to_bits()),
            },
            DeviceControls::Jetstream(controls) => Self::Jetstream {
                manual: AtomicU32::new(controls.manual.to_bits()),
                rate_hz: AtomicU32::new(controls.rate_hz.to_bits()),
                depth: AtomicU32::new(controls.depth.to_bits()),
                feedback: AtomicU32::new(controls.feedback.to_bits()),
                mix: AtomicU32::new(controls.mix.to_bits()),
            },
            DeviceControls::Celeste(controls) => Self::Celeste {
                rate_hz: AtomicU32::new(controls.rate_hz.to_bits()),
                depth: AtomicU32::new(controls.depth.to_bits()),
                tone: AtomicU32::new(controls.tone.to_bits()),
                mix: AtomicU32::new(controls.mix.to_bits()),
            },
            DeviceControls::Brigade(controls) => Self::Brigade {
                time_ms: AtomicU32::new(controls.time_ms.to_bits()),
                repeats: AtomicU32::new(controls.repeats.to_bits()),
                tone: AtomicU32::new(controls.tone.to_bits()),
                mix: AtomicU32::new(controls.mix.to_bits()),
            },
            DeviceControls::Springfield(controls) => Self::Springfield {
                dwell: AtomicU32::new(controls.dwell.to_bits()),
                tone: AtomicU32::new(controls.tone.to_bits()),
                mix: AtomicU32::new(controls.mix.to_bits()),
            },
        }
    }

    fn load(&self) -> DeviceControls {
        match self {
            Self::Default => DeviceControls::Default,
            Self::Lumen {
                peak_reduction,
                gain,
                emphasis,
                mix,
            } => DeviceControls::Lumen(LumenControls {
                peak_reduction: load_atomic_f32(peak_reduction),
                gain: load_atomic_f32(gain),
                emphasis: load_atomic_f32(emphasis),
                mix: load_atomic_f32(mix),
            }),
            Self::Muon {
                sensitivity,
                range,
                resonance,
                mix,
            } => DeviceControls::Muon(MuonControls {
                sensitivity: load_atomic_f32(sensitivity),
                range: load_atomic_f32(range),
                resonance: load_atomic_f32(resonance),
                mix: load_atomic_f32(mix),
            }),
            Self::Muffin {
                sustain,
                tone,
                level,
            } => DeviceControls::Muffin(MuffinControls {
                sustain: load_atomic_f32(sustain),
                tone: load_atomic_f32(tone),
                level: load_atomic_f32(level),
            }),
            Self::Minotaur {
                gain,
                treble,
                output,
            } => DeviceControls::Minotaur(MinotaurControls {
                gain: load_atomic_f32(gain),
                treble: load_atomic_f32(treble),
                output: load_atomic_f32(output),
            }),
            Self::Monarch { gain, tone, output } => DeviceControls::Monarch(MonarchControls {
                gain: load_atomic_f32(gain),
                tone: load_atomic_f32(tone),
                output: load_atomic_f32(output),
            }),
            Self::GodessOne {
                distortion,
                tone,
                level,
                mode,
            } => DeviceControls::GodessOne(GodessOneControls {
                distortion: load_atomic_f32(distortion),
                tone: load_atomic_f32(tone),
                level: load_atomic_f32(level),
                mode: *mode,
            }),
            Self::Dartford {
                rate_hz,
                depth,
                level,
                wave,
            } => DeviceControls::Dartford(DartfordControls {
                rate_hz: load_atomic_f32(rate_hz),
                depth: load_atomic_f32(depth),
                level: load_atomic_f32(level),
                wave: *wave,
            }),
            Self::Tron {
                rate_hz,
                depth,
                feedback,
                mix,
            } => DeviceControls::Tron(TronControls {
                rate_hz: load_atomic_f32(rate_hz),
                depth: load_atomic_f32(depth),
                feedback: load_atomic_f32(feedback),
                mix: load_atomic_f32(mix),
            }),
            Self::Jetstream {
                manual,
                rate_hz,
                depth,
                feedback,
                mix,
            } => DeviceControls::Jetstream(JetstreamControls {
                manual: load_atomic_f32(manual),
                rate_hz: load_atomic_f32(rate_hz),
                depth: load_atomic_f32(depth),
                feedback: load_atomic_f32(feedback),
                mix: load_atomic_f32(mix),
            }),
            Self::Celeste {
                rate_hz,
                depth,
                tone,
                mix,
            } => DeviceControls::Celeste(CelesteControls {
                rate_hz: load_atomic_f32(rate_hz),
                depth: load_atomic_f32(depth),
                tone: load_atomic_f32(tone),
                mix: load_atomic_f32(mix),
            }),
            Self::Brigade {
                time_ms,
                repeats,
                tone,
                mix,
            } => DeviceControls::Brigade(BrigadeControls {
                time_ms: load_atomic_f32(time_ms),
                repeats: load_atomic_f32(repeats),
                tone: load_atomic_f32(tone),
                mix: load_atomic_f32(mix),
            }),
            Self::Springfield { dwell, tone, mix } => {
                DeviceControls::Springfield(SpringfieldControls {
                    dwell: load_atomic_f32(dwell),
                    tone: load_atomic_f32(tone),
                    mix: load_atomic_f32(mix),
                })
            }
        }
    }

    fn adjust(&self, param_index: usize, delta: f32, min: f32, max: f32) {
        match self {
            Self::Lumen {
                peak_reduction,
                gain,
                emphasis,
                mix,
            } => match param_index {
                0 => adjust_control_param(Some(peak_reduction), delta, min, max),
                1 => adjust_control_param(Some(gain), delta, min, max),
                2 => adjust_control_param(Some(emphasis), delta, min, max),
                3 => adjust_control_param(Some(mix), delta, min, max),
                _ => {}
            },
            Self::Muon {
                sensitivity,
                range,
                resonance,
                mix,
            } => match param_index {
                0 => adjust_control_param(Some(sensitivity), delta, min, max),
                1 => adjust_control_param(Some(range), delta, min, max),
                2 => adjust_control_param(Some(resonance), delta, min, max),
                3 => adjust_control_param(Some(mix), delta, min, max),
                _ => {}
            },
            Self::Muffin {
                sustain,
                tone,
                level,
            } => adjust_control_param(
                [sustain, tone, level].get(param_index).copied(),
                delta,
                min,
                max,
            ),
            Self::Minotaur {
                gain,
                treble,
                output,
            } => adjust_control_param(
                [gain, treble, output].get(param_index).copied(),
                delta,
                min,
                max,
            ),
            Self::Monarch { gain, tone, output } => adjust_control_param(
                [gain, tone, output].get(param_index).copied(),
                delta,
                min,
                max,
            ),
            Self::GodessOne {
                distortion,
                tone,
                level,
                ..
            } => adjust_control_param(
                [distortion, tone, level].get(param_index).copied(),
                delta,
                min,
                max,
            ),
            Self::Dartford {
                rate_hz,
                depth,
                level,
                ..
            } => match param_index {
                0 => adjust_control_param(Some(rate_hz), delta, min, max),
                1 => adjust_control_param(Some(depth), delta, min, max),
                2 => adjust_control_param(Some(level), delta, min, max),
                _ => {}
            },
            Self::Tron {
                rate_hz,
                depth,
                feedback,
                mix,
            } => match param_index {
                0 => adjust_control_param(Some(rate_hz), delta, min, max),
                1 => adjust_control_param(Some(depth), delta, min, max),
                2 => adjust_control_param(Some(feedback), delta, min, max),
                3 => adjust_control_param(Some(mix), delta, min, max),
                _ => {}
            },
            Self::Jetstream {
                manual,
                rate_hz,
                depth,
                feedback,
                mix,
            } => match param_index {
                0 => adjust_control_param(Some(manual), delta, min, max),
                1 => adjust_control_param(Some(rate_hz), delta, min, max),
                2 => adjust_control_param(Some(depth), delta, min, max),
                3 => adjust_control_param(Some(feedback), delta, min, max),
                4 => adjust_control_param(Some(mix), delta, min, max),
                _ => {}
            },
            Self::Celeste {
                rate_hz,
                depth,
                tone,
                mix,
            } => match param_index {
                0 => adjust_control_param(Some(rate_hz), delta, min, max),
                1 => adjust_control_param(Some(depth), delta, min, max),
                2 => adjust_control_param(Some(tone), delta, min, max),
                3 => adjust_control_param(Some(mix), delta, min, max),
                _ => {}
            },
            Self::Brigade {
                time_ms,
                repeats,
                tone,
                mix,
            } => match param_index {
                0 => adjust_control_param(Some(time_ms), delta, min, max),
                1 => adjust_control_param(Some(repeats), delta, min, max),
                2 => adjust_control_param(Some(tone), delta, min, max),
                3 => adjust_control_param(Some(mix), delta, min, max),
                _ => {}
            },
            Self::Springfield { dwell, tone, mix } => adjust_control_param(
                [dwell, tone, mix].get(param_index).copied(),
                delta,
                min,
                max,
            ),
            Self::Default => {}
        }
    }
}

fn adjust_control_param(slot: Option<&AtomicU32>, delta: f32, min: f32, max: f32) {
    if let Some(slot) = slot {
        adjust_range_param(slot, delta, min, max);
    }
}

fn adjust_range_param(slot: &AtomicU32, delta: f32, min: f32, max: f32) {
    let current = load_atomic_f32(slot);
    slot.store(
        (current + delta).clamp(min, max).to_bits(),
        Ordering::Relaxed,
    );
}

fn load_atomic_f32(value: &AtomicU32) -> f32 {
    f32::from_bits(value.load(Ordering::Relaxed))
}

#[derive(Default)]
struct MonitorStats {
    input_sum_squares: AtomicU64,
    output_sum_squares: AtomicU64,
    input_count: AtomicU64,
    output_count: AtomicU64,
    input_peak_bits: AtomicU32,
    output_peak_bits: AtomicU32,
    input_near_clips: AtomicU64,
    output_near_clips: AtomicU64,
    input_clips: AtomicU64,
    output_clips: AtomicU64,
    input_overruns: AtomicU64,
    output_underruns: AtomicU64,
}

#[derive(Clone, Copy, Default)]
struct MonitorSnapshot {
    input_sum_squares: u64,
    output_sum_squares: u64,
    input_count: u64,
    output_count: u64,
    input_peak: f32,
    output_peak: f32,
    input_near_clips: u64,
    output_near_clips: u64,
    input_clips: u64,
    output_clips: u64,
    input_overruns: u64,
    output_underruns: u64,
}

const NOX30_SIGNAL_PROBE_COUNT: usize = 8;
const NOX30_SIGNAL_PROBE_LABELS: [&str; NOX30_SIGNAL_PROBE_COUNT] = [
    "vol", "first", "follow", "tone", "send", "pi", "power", "ot",
];

struct SignalProbeTelemetry {
    abs_sum: AtomicU64,
    abs_max_bits: AtomicU32,
}

struct ComponentTelemetry {
    count: AtomicU64,
    preamp_voltage_sum: AtomicU64,
    preamp_voltage_min_bits: AtomicU32,
    phase_inverter_voltage_sum: AtomicU64,
    phase_inverter_voltage_min_bits: AtomicU32,
    power_voltage_sum: AtomicU64,
    power_voltage_min_bits: AtomicU32,
    power_screen_voltage_sum: AtomicU64,
    power_screen_voltage_min_bits: AtomicU32,
    first_stage_current_sum: AtomicU64,
    first_stage_current_max_bits: AtomicU32,
    phase_inverter_current_sum: AtomicU64,
    phase_inverter_current_max_bits: AtomicU32,
    power_current_sum: AtomicU64,
    power_current_max_bits: AtomicU32,
    power_attack_current_sum: AtomicU64,
    power_attack_current_max_bits: AtomicU32,
    power_screen_current_sum: AtomicU64,
    power_screen_current_max_bits: AtomicU32,
    power_cathode_bias_sum: AtomicU64,
    power_cathode_bias_max_bits: AtomicU32,
    transformer_flux_abs_sum: AtomicU64,
    transformer_flux_abs_max_bits: AtomicU32,
    signal_probes: [SignalProbeTelemetry; NOX30_SIGNAL_PROBE_COUNT],
    first_stage_shadow_count: AtomicU64,
    first_stage_shadow_abs_error_sum: AtomicU64,
    first_stage_shadow_abs_error_max_bits: AtomicU32,
}

#[derive(Clone, Copy, Default)]
struct ComponentTelemetrySnapshot {
    count: u64,
    preamp_voltage_avg: f32,
    preamp_voltage_min: f32,
    phase_inverter_voltage_avg: f32,
    phase_inverter_voltage_min: f32,
    power_voltage_avg: f32,
    power_voltage_min: f32,
    power_screen_voltage_avg: f32,
    power_screen_voltage_min: f32,
    first_stage_current_avg: f32,
    first_stage_current_max: f32,
    phase_inverter_current_avg: f32,
    phase_inverter_current_max: f32,
    power_current_avg: f32,
    power_current_max: f32,
    power_attack_current_avg: f32,
    power_attack_current_max: f32,
    power_screen_current_avg: f32,
    power_screen_current_max: f32,
    power_cathode_bias_avg: f32,
    power_cathode_bias_max: f32,
    transformer_flux_abs_avg: f32,
    transformer_flux_abs_max: f32,
    signal_probe_abs_avg: [f32; NOX30_SIGNAL_PROBE_COUNT],
    signal_probe_abs_max: [f32; NOX30_SIGNAL_PROBE_COUNT],
    first_stage_shadow_count: u64,
    first_stage_shadow_abs_error_avg: f32,
    first_stage_shadow_abs_error_max: f32,
}

impl MonitorStats {
    fn record_input(&self, sample: f32) {
        self.record_sample(
            sample,
            &self.input_sum_squares,
            &self.input_count,
            &self.input_peak_bits,
            &self.input_near_clips,
            &self.input_clips,
        );
    }

    fn record_output(&self, sample: f32) {
        self.record_sample(
            sample,
            &self.output_sum_squares,
            &self.output_count,
            &self.output_peak_bits,
            &self.output_near_clips,
            &self.output_clips,
        );
    }

    fn record_input_overrun(&self) {
        self.input_overruns.fetch_add(1, Ordering::Relaxed);
    }

    fn record_output_underrun(&self) {
        self.output_underruns.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot_and_reset(&self) -> MonitorSnapshot {
        MonitorSnapshot {
            input_sum_squares: self.input_sum_squares.swap(0, Ordering::Relaxed),
            output_sum_squares: self.output_sum_squares.swap(0, Ordering::Relaxed),
            input_count: self.input_count.swap(0, Ordering::Relaxed),
            output_count: self.output_count.swap(0, Ordering::Relaxed),
            input_peak: f32::from_bits(self.input_peak_bits.swap(0, Ordering::Relaxed)),
            output_peak: f32::from_bits(self.output_peak_bits.swap(0, Ordering::Relaxed)),
            input_near_clips: self.input_near_clips.swap(0, Ordering::Relaxed),
            output_near_clips: self.output_near_clips.swap(0, Ordering::Relaxed),
            input_clips: self.input_clips.swap(0, Ordering::Relaxed),
            output_clips: self.output_clips.swap(0, Ordering::Relaxed),
            input_overruns: self.input_overruns.swap(0, Ordering::Relaxed),
            output_underruns: self.output_underruns.swap(0, Ordering::Relaxed),
        }
    }

    fn record_sample(
        &self,
        sample: f32,
        sum_squares: &AtomicU64,
        count: &AtomicU64,
        peak_bits: &AtomicU32,
        near_clips: &AtomicU64,
        clips: &AtomicU64,
    ) {
        let magnitude = sample.abs();
        let square = (magnitude as f64 * magnitude as f64 * RMS_SCALE).round() as u64;
        sum_squares.fetch_add(square, Ordering::Relaxed);
        count.fetch_add(1, Ordering::Relaxed);
        update_peak(peak_bits, magnitude);
        if magnitude >= NEAR_CLIP_LEVEL {
            near_clips.fetch_add(1, Ordering::Relaxed);
        }
        if magnitude >= CLIP_LEVEL {
            clips.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Default for ComponentTelemetry {
    fn default() -> Self {
        Self {
            count: AtomicU64::new(0),
            preamp_voltage_sum: AtomicU64::new(0),
            preamp_voltage_min_bits: AtomicU32::new(f32::INFINITY.to_bits()),
            phase_inverter_voltage_sum: AtomicU64::new(0),
            phase_inverter_voltage_min_bits: AtomicU32::new(f32::INFINITY.to_bits()),
            power_voltage_sum: AtomicU64::new(0),
            power_voltage_min_bits: AtomicU32::new(f32::INFINITY.to_bits()),
            power_screen_voltage_sum: AtomicU64::new(0),
            power_screen_voltage_min_bits: AtomicU32::new(f32::INFINITY.to_bits()),
            first_stage_current_sum: AtomicU64::new(0),
            first_stage_current_max_bits: AtomicU32::new(0),
            phase_inverter_current_sum: AtomicU64::new(0),
            phase_inverter_current_max_bits: AtomicU32::new(0),
            power_current_sum: AtomicU64::new(0),
            power_current_max_bits: AtomicU32::new(0),
            power_attack_current_sum: AtomicU64::new(0),
            power_attack_current_max_bits: AtomicU32::new(0),
            power_screen_current_sum: AtomicU64::new(0),
            power_screen_current_max_bits: AtomicU32::new(0),
            power_cathode_bias_sum: AtomicU64::new(0),
            power_cathode_bias_max_bits: AtomicU32::new(0),
            transformer_flux_abs_sum: AtomicU64::new(0),
            transformer_flux_abs_max_bits: AtomicU32::new(0),
            signal_probes: std::array::from_fn(|_| SignalProbeTelemetry::default()),
            first_stage_shadow_count: AtomicU64::new(0),
            first_stage_shadow_abs_error_sum: AtomicU64::new(0),
            first_stage_shadow_abs_error_max_bits: AtomicU32::new(0),
        }
    }
}

impl Default for SignalProbeTelemetry {
    fn default() -> Self {
        Self {
            abs_sum: AtomicU64::new(0),
            abs_max_bits: AtomicU32::new(0),
        }
    }
}

impl ComponentTelemetry {
    fn record_nox30(&self, operating_point: Nox30OperatingPoint) {
        let phase_inverter_current = operating_point.phase_inverter_plate_a_current
            + operating_point.phase_inverter_plate_b_current;
        let power_current = operating_point.power_positive_current
            + operating_point.power_negative_current
            + operating_point.power_attack_current * 0.65;
        let power_screen_current = operating_point.power_positive_screen_current
            + operating_point.power_negative_screen_current;
        let transformer_flux_abs = operating_point.transformer_core_flux.abs();
        let signal_probes = [
            operating_point.input_volume_output_v,
            operating_point.first_stage_output_v,
            operating_point.follower_output_v,
            operating_point.tone_stack_output_v,
            operating_point.preamp_send_v,
            operating_point.phase_inverter_output_v,
            operating_point.power_stage_output_v,
            operating_point.output_transformer_output_v,
        ];
        for (probe, value) in self.signal_probes.iter().zip(signal_probes) {
            record_telemetry_max(value.abs(), &probe.abs_sum, &probe.abs_max_bits);
        }
        if let Some(error_v) = operating_point.first_stage_shadow_error_v {
            record_telemetry_max(
                error_v.abs(),
                &self.first_stage_shadow_abs_error_sum,
                &self.first_stage_shadow_abs_error_max_bits,
            );
            self.first_stage_shadow_count
                .fetch_add(1, Ordering::Relaxed);
        }

        record_telemetry_min(
            operating_point.preamp_voltage,
            &self.preamp_voltage_sum,
            &self.preamp_voltage_min_bits,
        );
        record_telemetry_min(
            operating_point.phase_inverter_voltage,
            &self.phase_inverter_voltage_sum,
            &self.phase_inverter_voltage_min_bits,
        );
        record_telemetry_min(
            operating_point.power_voltage,
            &self.power_voltage_sum,
            &self.power_voltage_min_bits,
        );
        record_telemetry_min(
            operating_point.power_screen_voltage,
            &self.power_screen_voltage_sum,
            &self.power_screen_voltage_min_bits,
        );
        record_telemetry_max(
            operating_point.first_stage_plate_current,
            &self.first_stage_current_sum,
            &self.first_stage_current_max_bits,
        );
        record_telemetry_max(
            phase_inverter_current,
            &self.phase_inverter_current_sum,
            &self.phase_inverter_current_max_bits,
        );
        record_telemetry_max(
            power_current,
            &self.power_current_sum,
            &self.power_current_max_bits,
        );
        record_telemetry_max(
            operating_point.power_attack_current,
            &self.power_attack_current_sum,
            &self.power_attack_current_max_bits,
        );
        record_telemetry_max(
            power_screen_current,
            &self.power_screen_current_sum,
            &self.power_screen_current_max_bits,
        );
        record_telemetry_max(
            operating_point.power_cathode_bias_voltage,
            &self.power_cathode_bias_sum,
            &self.power_cathode_bias_max_bits,
        );
        record_telemetry_max(
            transformer_flux_abs,
            &self.transformer_flux_abs_sum,
            &self.transformer_flux_abs_max_bits,
        );
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot_and_reset(&self) -> ComponentTelemetrySnapshot {
        let count = self.count.swap(0, Ordering::Relaxed);
        let first_stage_shadow_count = self.first_stage_shadow_count.swap(0, Ordering::Relaxed);
        ComponentTelemetrySnapshot {
            count,
            preamp_voltage_avg: telemetry_average(
                self.preamp_voltage_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            preamp_voltage_min: reset_min(&self.preamp_voltage_min_bits),
            phase_inverter_voltage_avg: telemetry_average(
                self.phase_inverter_voltage_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            phase_inverter_voltage_min: reset_min(&self.phase_inverter_voltage_min_bits),
            power_voltage_avg: telemetry_average(
                self.power_voltage_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            power_voltage_min: reset_min(&self.power_voltage_min_bits),
            power_screen_voltage_avg: telemetry_average(
                self.power_screen_voltage_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            power_screen_voltage_min: reset_min(&self.power_screen_voltage_min_bits),
            first_stage_current_avg: telemetry_average(
                self.first_stage_current_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            first_stage_current_max: reset_max(&self.first_stage_current_max_bits),
            phase_inverter_current_avg: telemetry_average(
                self.phase_inverter_current_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            phase_inverter_current_max: reset_max(&self.phase_inverter_current_max_bits),
            power_current_avg: telemetry_average(
                self.power_current_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            power_current_max: reset_max(&self.power_current_max_bits),
            power_attack_current_avg: telemetry_average(
                self.power_attack_current_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            power_attack_current_max: reset_max(&self.power_attack_current_max_bits),
            power_screen_current_avg: telemetry_average(
                self.power_screen_current_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            power_screen_current_max: reset_max(&self.power_screen_current_max_bits),
            power_cathode_bias_avg: telemetry_average(
                self.power_cathode_bias_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            power_cathode_bias_max: reset_max(&self.power_cathode_bias_max_bits),
            transformer_flux_abs_avg: telemetry_average(
                self.transformer_flux_abs_sum.swap(0, Ordering::Relaxed),
                count,
            ),
            transformer_flux_abs_max: reset_max(&self.transformer_flux_abs_max_bits),
            signal_probe_abs_avg: std::array::from_fn(|index| {
                telemetry_average(
                    self.signal_probes[index]
                        .abs_sum
                        .swap(0, Ordering::Relaxed),
                    count,
                )
            }),
            signal_probe_abs_max: std::array::from_fn(|index| {
                reset_max(&self.signal_probes[index].abs_max_bits)
            }),
            first_stage_shadow_count,
            first_stage_shadow_abs_error_avg: telemetry_average(
                self.first_stage_shadow_abs_error_sum
                    .swap(0, Ordering::Relaxed),
                first_stage_shadow_count,
            ),
            first_stage_shadow_abs_error_max: reset_max(
                &self.first_stage_shadow_abs_error_max_bits,
            ),
        }
    }
}

fn update_peak(peak_bits: &AtomicU32, magnitude: f32) {
    let magnitude_bits = magnitude.to_bits();
    let mut current = peak_bits.load(Ordering::Relaxed);
    while magnitude_bits > current {
        match peak_bits.compare_exchange_weak(
            current,
            magnitude_bits,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(value) => current = value,
        }
    }
}

fn record_telemetry_min(value: f32, sum: &AtomicU64, min_bits: &AtomicU32) {
    let value = value.max(0.0);
    add_telemetry_sum(value, sum);
    update_min(min_bits, value);
}

fn record_telemetry_max(value: f32, sum: &AtomicU64, max_bits: &AtomicU32) {
    let value = value.max(0.0);
    add_telemetry_sum(value, sum);
    update_peak(max_bits, value);
}

fn add_telemetry_sum(value: f32, sum: &AtomicU64) {
    let scaled = (value as f64 * TELEMETRY_SCALE).round() as u64;
    sum.fetch_add(scaled, Ordering::Relaxed);
}

fn update_min(min_bits: &AtomicU32, value: f32) {
    let value_bits = value.to_bits();
    let mut current = min_bits.load(Ordering::Relaxed);
    while value_bits < current {
        match min_bits.compare_exchange_weak(
            current,
            value_bits,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

fn reset_min(min_bits: &AtomicU32) -> f32 {
    let value = f32::from_bits(min_bits.swap(f32::INFINITY.to_bits(), Ordering::Relaxed));
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn reset_max(max_bits: &AtomicU32) -> f32 {
    f32::from_bits(max_bits.swap(0, Ordering::Relaxed))
}

fn telemetry_average(sum: u64, count: u64) -> f32 {
    if count == 0 {
        0.0
    } else {
        (sum as f64 / TELEMETRY_SCALE / count as f64) as f32
    }
}

fn rms_from_scaled(sum_squares: u64, count: u64) -> f32 {
    if count == 0 {
        0.0
    } else {
        (sum_squares as f64 / RMS_SCALE / count as f64).sqrt() as f32
    }
}

fn dbfs(level: f32) -> f32 {
    if level > 0.0 {
        20.0 * level.log10()
    } else {
        f32::NEG_INFINITY
    }
}

fn format_dbfs(level: f32) -> String {
    let db = dbfs(level);
    if db.is_finite() {
        format!("{db:+.1}")
    } else {
        "-inf".to_owned()
    }
}

fn format_audio_monitor(stats: &MonitorSnapshot) -> String {
    let in_rms = rms_from_scaled(stats.input_sum_squares, stats.input_count);
    let out_rms = rms_from_scaled(stats.output_sum_squares, stats.output_count);
    format!(
        "MON input rms {:.5} ({} dBFS) peak {:.5} ({} dBFS) near/clip {}/{} | output rms {:.5} ({} dBFS) peak {:.5} ({} dBFS) near/clip {}/{} | xrun in/out {}/{}",
        in_rms,
        format_dbfs(in_rms),
        stats.input_peak,
        format_dbfs(stats.input_peak),
        stats.input_near_clips,
        stats.input_clips,
        out_rms,
        format_dbfs(out_rms),
        stats.output_peak,
        format_dbfs(stats.output_peak),
        stats.output_near_clips,
        stats.output_clips,
        stats.input_overruns,
        stats.output_underruns
    )
}

fn format_component_telemetry(components: ComponentTelemetrySnapshot) -> String {
    let mut line = format!(
        "CMP n={} rails avg/min pre {:.0}/{:.0} pi {:.0}/{:.0} pwr {:.0}/{:.0} scr {:.0}/{:.0} V | I avg/max first {:.2}/{:.2} pi {:.2}/{:.2} pwr {:.1}/{:.1} atk {:.1}/{:.1} scr {:.1}/{:.1} mA | cath avg/max {:.2}/{:.2} V | flux abs avg/max {:.5}/{:.5}",
        components.count,
        components.preamp_voltage_avg,
        components.preamp_voltage_min,
        components.phase_inverter_voltage_avg,
        components.phase_inverter_voltage_min,
        components.power_voltage_avg,
        components.power_voltage_min,
        components.power_screen_voltage_avg,
        components.power_screen_voltage_min,
        components.first_stage_current_avg * 1_000.0,
        components.first_stage_current_max * 1_000.0,
        components.phase_inverter_current_avg * 1_000.0,
        components.phase_inverter_current_max * 1_000.0,
        components.power_current_avg * 1_000.0,
        components.power_current_max * 1_000.0,
        components.power_attack_current_avg * 1_000.0,
        components.power_attack_current_max * 1_000.0,
        components.power_screen_current_avg * 1_000.0,
        components.power_screen_current_max * 1_000.0,
        components.power_cathode_bias_avg,
        components.power_cathode_bias_max,
        components.transformer_flux_abs_avg,
        components.transformer_flux_abs_max,
    );
    if components.first_stage_shadow_count > 0 {
        line.push_str(&format!(
            " | shadow first abs err avg/max {:.5}/{:.5} V n {}",
            components.first_stage_shadow_abs_error_avg,
            components.first_stage_shadow_abs_error_max,
            components.first_stage_shadow_count,
        ));
    }
    line.push_str(" | sig abs avg/max V");
    for index in 0..NOX30_SIGNAL_PROBE_COUNT {
        line.push_str(&format!(
            " {} {:.5}/{:.5}",
            NOX30_SIGNAL_PROBE_LABELS[index],
            components.signal_probe_abs_avg[index],
            components.signal_probe_abs_max[index],
        ));
    }
    line
}

fn vu_meter(level: f32, width: usize) -> String {
    let db = dbfs(level);
    let normalized = if db.is_finite() {
        ((db + 60.0) / 60.0).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = (normalized * width as f32).round() as usize;
    format!(
        "[{}{}]",
        "#".repeat(filled),
        ".".repeat(width.saturating_sub(filled))
    )
}

fn format_monitor_dashboard(
    stats: &MonitorSnapshot,
    components: Option<ComponentTelemetrySnapshot>,
    model: &str,
    log_path: &Path,
) -> String {
    let in_rms = rms_from_scaled(stats.input_sum_squares, stats.input_count);
    let out_rms = rms_from_scaled(stats.output_sum_squares, stats.output_count);
    let mut text = format!(
        "🎸 Greybound monitor  model {model}  log {}\n\
         🎚 input  rms {:>6} dBFS {} peak {:>6} dBFS near/clip {}/{}\n\
         🔊 output rms {:>6} dBFS {} peak {:>6} dBFS near/clip {}/{}\n\
         ⚡ xrun in/out {}/{}\n",
        log_path.display(),
        format_dbfs(in_rms),
        vu_meter(in_rms, VU_WIDTH),
        format_dbfs(stats.input_peak),
        stats.input_near_clips,
        stats.input_clips,
        format_dbfs(out_rms),
        vu_meter(out_rms, VU_WIDTH),
        format_dbfs(stats.output_peak),
        stats.output_near_clips,
        stats.output_clips,
        stats.input_overruns,
        stats.output_underruns
    );

    if let Some(components) = components {
        text.push_str(&format!(
            "🔋 rails avg/min pre {:>5.0}/{:>5.0} pi {:>5.0}/{:>5.0} pwr {:>5.0}/{:>5.0} scr {:>5.0}/{:>5.0} V\n\
             🔥 I avg/max mA first {:>5.2}/{:>5.2} pi {:>5.2}/{:>5.2} pwr {:>5.1}/{:>5.1} atk {:>5.1}/{:>5.1} scr {:>5.1}/{:>5.1}\n\
             🧲 cath avg/max {:>5.2}/{:>5.2} V   flux |avg/max| {:.5}/{:.5}   n {}\n",
            components.preamp_voltage_avg,
            components.preamp_voltage_min,
            components.phase_inverter_voltage_avg,
            components.phase_inverter_voltage_min,
            components.power_voltage_avg,
            components.power_voltage_min,
            components.power_screen_voltage_avg,
            components.power_screen_voltage_min,
            components.first_stage_current_avg * 1_000.0,
            components.first_stage_current_max * 1_000.0,
            components.phase_inverter_current_avg * 1_000.0,
            components.phase_inverter_current_max * 1_000.0,
            components.power_current_avg * 1_000.0,
            components.power_current_max * 1_000.0,
            components.power_attack_current_avg * 1_000.0,
            components.power_attack_current_max * 1_000.0,
            components.power_screen_current_avg * 1_000.0,
            components.power_screen_current_max * 1_000.0,
            components.power_cathode_bias_avg,
            components.power_cathode_bias_max,
            components.transformer_flux_abs_avg,
            components.transformer_flux_abs_max,
            components.count,
        ));
        text.push_str("📍 sig |avg/max| V");
        for index in 0..NOX30_SIGNAL_PROBE_COUNT {
            text.push_str(&format!(
                " {} {:.3}/{:.3}",
                NOX30_SIGNAL_PROBE_LABELS[index],
                components.signal_probe_abs_avg[index],
                components.signal_probe_abs_max[index],
            ));
        }
        text.push('\n');
        if components.first_stage_shadow_count > 0 {
            text.push_str(&format!(
                "🧪 shadow first abs err avg/max {:.5}/{:.5} V   n {}\n",
                components.first_stage_shadow_abs_error_avg,
                components.first_stage_shadow_abs_error_max,
                components.first_stage_shadow_count,
            ));
        }
    }

    text.push_str("Press Ctrl-C to stop.\n");
    text
}

fn device_label(device: DeviceConfig) -> &'static str {
    device.model_descriptor().label
}

fn slot_bypassed(slot: &DeviceSlotConfig, controls: Option<&DeviceSlotControls>) -> bool {
    controls.map_or(slot.bypassed, |controls| controls.bypassed)
}

fn chain_slot_controls(
    controls: &[DeviceSlotControls],
    section_offset: usize,
    slot_index: usize,
) -> Option<&DeviceSlotControls> {
    controls.get(section_offset + slot_index)
}

fn chain_slot_config(
    config: &SignalChainConfig,
    global_index: usize,
) -> Option<(usize, &DeviceSlotConfig)> {
    if global_index < config.pre_amp.len() {
        return config
            .pre_amp
            .get(global_index)
            .map(|slot| (global_index, slot));
    }
    let fx_index = global_index.checked_sub(config.pre_amp.len())?;
    if fx_index < config.fx_loop.len() {
        return config
            .fx_loop
            .get(fx_index)
            .map(|slot| (global_index, slot));
    }
    let post_index = fx_index.checked_sub(config.fx_loop.len())?;
    config
        .post_amp
        .get(post_index)
        .map(|slot| (global_index, slot))
}

fn format_pedalboard_text(config: &SignalChainConfig, controls: &[DeviceSlotControls]) -> String {
    format_pedalboard_text_with_cab(config, controls, true, None)
}

fn format_pedalboard_text_with_cab(
    config: &SignalChainConfig,
    controls: &[DeviceSlotControls],
    amp_enabled: bool,
    cab_enabled: Option<bool>,
) -> String {
    let mut lines = Vec::new();
    lines.push("Pedalboard:".to_owned());
    lines.extend(render_pedalboard_section(
        "GTR",
        &config.pre_amp,
        controls,
        0,
        Some((&config.amp_model, amp_enabled)),
        cab_enabled,
        "OUT",
    ));
    if !config.fx_loop.is_empty() {
        lines.push("FX LOOP PEDALS:".to_owned());
        lines.extend(render_pedalboard_section(
            "SEND",
            &config.fx_loop,
            controls,
            config.pre_amp.len(),
            None,
            None,
            "RETURN",
        ));
    }
    if !config.post_amp.is_empty() {
        lines.push("Post:".to_owned());
        lines.extend(render_pedalboard_section(
            "AMP OUT",
            &config.post_amp,
            controls,
            config.pre_amp.len() + config.fx_loop.len(),
            None,
            None,
            "OUT",
        ));
    }
    format!("{}\n", lines.join("\n"))
}

fn render_pedalboard_section(
    input: &str,
    slots: &[DeviceSlotConfig],
    controls: &[DeviceSlotControls],
    section_offset: usize,
    amp_model: Option<(&str, bool)>,
    cab_enabled: Option<bool>,
    output: &str,
) -> Vec<String> {
    let boxes = pedalboard_section_boxes(slots, controls, section_offset, amp_model, cab_enabled);
    board_rows(input, &boxes, output)
}

fn render_pedalboard_section_styled(
    input: &str,
    slots: &[DeviceSlotConfig],
    controls: &[DeviceSlotControls],
    section_offset: usize,
    amp_model: Option<(&str, bool)>,
    cab_enabled: Option<bool>,
    output: &str,
) -> Vec<Line<'static>> {
    let boxes = pedalboard_section_boxes(slots, controls, section_offset, amp_model, cab_enabled);
    board_styled_rows(input, &boxes, output)
}

fn pedalboard_section_boxes(
    slots: &[DeviceSlotConfig],
    controls: &[DeviceSlotControls],
    section_offset: usize,
    amp_model: Option<(&str, bool)>,
    cab_enabled: Option<bool>,
) -> Vec<BoardBox> {
    let mut boxes = Vec::new();
    for (slot_index, slot) in slots.iter().enumerate() {
        let bypassed = slot_bypassed(
            slot,
            chain_slot_controls(controls, section_offset, slot_index),
        );
        let descriptor = slot.device.model_descriptor();
        boxes.push(BoardBox {
            title: descriptor.label.to_owned(),
            indicator_color: Some(if bypassed {
                descriptor.visual.bypass_color
            } else {
                descriptor.visual.active_color
            }),
            status: String::new(),
            footswitch: "(O)".to_owned(),
            width: descriptor.visual.width,
            color: descriptor.visual.color,
        });
    }
    if let Some((model, enabled)) = amp_model {
        let descriptor = amp_model_descriptor(model);
        boxes.push(BoardBox {
            title: format!("AMP {}", descriptor.label),
            indicator_color: Some(if enabled {
                descriptor.visual.active_color
            } else {
                descriptor.visual.bypass_color
            }),
            status: String::new(),
            footswitch: "(O)".to_owned(),
            width: descriptor.visual.width,
            color: descriptor.visual.color,
        });
    }
    if let Some(enabled) = cab_enabled {
        boxes.push(cab_board_box(enabled));
    }

    boxes
}

fn cab_board_box(enabled: bool) -> BoardBox {
    BoardBox {
        title: "CAB IR".to_owned(),
        indicator_color: Some(if enabled { "green" } else { "gray" }),
        status: String::new(),
        footswitch: "(O)".to_owned(),
        width: 14,
        color: "charcoal",
    }
}

struct BoardBox {
    title: String,
    indicator_color: Option<&'static str>,
    status: String,
    footswitch: String,
    width: usize,
    color: &'static str,
}

#[derive(Clone, Default)]
struct MonitorHitAreas {
    amp_controls: Rect,
    amp_box: Rect,
    amp_footswitch: Rect,
    pedals: Vec<PedalHitArea>,
    footswitches: Vec<PedalHitArea>,
    cab_footswitch: Option<Rect>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PedalHitArea {
    global_index: usize,
    rect: Rect,
}

fn board_rows(input: &str, boxes: &[BoardBox], output: &str) -> Vec<String> {
    let input_cell = format!("{input:<8}");
    let output_cell = format!("{output:<8}");
    let mut top = " ".repeat(input_cell.len());
    let mut label = " ".repeat(input_cell.len());
    let mut cable = input_cell.clone();
    let mut status = " ".repeat(input_cell.len());
    let mut footswitch = " ".repeat(input_cell.len());
    let mut bottom = " ".repeat(input_cell.len());

    for box_spec in boxes {
        top.push_str("  ");
        top.push_str(&box_top(box_spec.width));
        label.push_str("  ");
        label.push_str(&box_title_content(
            &box_spec.title,
            box_spec.indicator_color.is_some(),
            box_spec.width,
        ));
        cable.push_str("──");
        cable.push_str(&box_cable(box_spec.width));
        status.push_str("  ");
        status.push_str(&box_content_centered(&box_spec.status, box_spec.width));
        footswitch.push_str("  ");
        footswitch.push_str(&box_content_centered(&box_spec.footswitch, box_spec.width));
        bottom.push_str("  ");
        bottom.push_str(&box_bottom(box_spec.width));
    }
    cable.push_str("──");
    cable.push_str(output_cell.trim_end());

    vec![top, label, cable, status, footswitch, bottom]
}

fn board_styled_rows(input: &str, boxes: &[BoardBox], output: &str) -> Vec<Line<'static>> {
    let input_cell = format!("{input:<8}");
    let output_cell = format!("{output:<8}");
    let mut top = vec![Span::raw(" ".repeat(input_cell.len()))];
    let mut label = vec![Span::raw(" ".repeat(input_cell.len()))];
    let mut cable = vec![Span::raw(input_cell)];
    let mut status = vec![Span::raw(" ".repeat(8))];
    let mut footswitch = vec![Span::raw(" ".repeat(8))];
    let mut bottom = vec![Span::raw(" ".repeat(8))];

    for box_spec in boxes {
        let border_style = Style::default().fg(descriptor_color(box_spec.color));
        top.push(Span::raw("  "));
        top.push(Span::styled(box_top(box_spec.width), border_style));
        label.push(Span::raw("  "));
        label.extend(styled_box_title_content(
            &box_spec.title,
            box_spec.indicator_color,
            box_spec.width,
            border_style,
            Style::default(),
        ));
        cable.push(Span::styled("──", Style::default().fg(Color::DarkGray)));
        cable.push(Span::styled(box_cable(box_spec.width), border_style));
        status.push(Span::raw("  "));
        status.extend(styled_box_content_centered(
            &box_spec.status,
            box_spec.width,
            border_style,
            Style::default(),
        ));
        footswitch.push(Span::raw("  "));
        footswitch.extend(styled_box_content_centered(
            &box_spec.footswitch,
            box_spec.width,
            border_style,
            Style::default(),
        ));
        bottom.push(Span::raw("  "));
        bottom.push(Span::styled(box_bottom(box_spec.width), border_style));
    }
    cable.push(Span::styled("──", Style::default().fg(Color::DarkGray)));
    cable.push(Span::raw(output_cell.trim_end().to_owned()));

    vec![
        Line::from(top),
        Line::from(label),
        Line::from(cable),
        Line::from(status),
        Line::from(footswitch),
        Line::from(bottom),
    ]
}

fn styled_box_content(
    text: &str,
    width: usize,
    border_style: Style,
    content_style: Style,
) -> Vec<Span<'static>> {
    let inner_width = width.saturating_sub(2);
    let mut content = text.chars().take(inner_width).collect::<String>();
    let padding = inner_width.saturating_sub(content.chars().count());
    content.push_str(&" ".repeat(padding));
    vec![
        Span::styled("│", border_style),
        Span::styled(content, content_style),
        Span::styled("│", border_style),
    ]
}

fn styled_box_title_content(
    title: &str,
    indicator_color: Option<&str>,
    width: usize,
    border_style: Style,
    content_style: Style,
) -> Vec<Span<'static>> {
    let inner_width = width.saturating_sub(2);
    if let Some(indicator_color) = indicator_color {
        let title_width = inner_width.saturating_sub(2);
        let mut content = title.chars().take(title_width).collect::<String>();
        let padding = title_width.saturating_sub(content.chars().count());
        content.push_str(&" ".repeat(padding));
        return vec![
            Span::styled("│", border_style),
            Span::styled(content, content_style),
            Span::raw(" "),
            Span::styled("o", Style::default().fg(descriptor_color(indicator_color))),
            Span::styled("│", border_style),
        ];
    }

    styled_box_content(title, width, border_style, content_style)
}

fn styled_box_content_centered(
    text: &str,
    width: usize,
    border_style: Style,
    content_style: Style,
) -> Vec<Span<'static>> {
    let inner_width = width.saturating_sub(2);
    let content = text.chars().take(inner_width).collect::<String>();
    let padding = inner_width.saturating_sub(content.chars().count());
    let left = padding / 2;
    let right = padding - left;
    vec![
        Span::styled("│", border_style),
        Span::raw(" ".repeat(left)),
        Span::styled(content, content_style),
        Span::raw(" ".repeat(right)),
        Span::styled("│", border_style),
    ]
}

fn descriptor_color(name: &str) -> Color {
    match name {
        "olive" => Color::Rgb(85, 107, 47),
        "gold" => Color::Rgb(212, 175, 55),
        "royal-purple" => Color::Rgb(102, 51, 153),
        "orange" => Color::Rgb(255, 128, 0),
        "teal" => Color::Rgb(0, 128, 128),
        "lamp-orange" => Color::Rgb(230, 112, 35),
        "sky-blue" => Color::Rgb(64, 156, 220),
        "cobalt-blue" => Color::Rgb(42, 94, 196),
        "violet" => Color::Rgb(138, 84, 190),
        "silver" => Color::Rgb(185, 190, 196),
        "seafoam" => Color::Rgb(88, 188, 166),
        "surf-green" => Color::Rgb(82, 164, 140),
        "copper" => Color::Rgb(184, 115, 51),
        "black-gold" => Color::Rgb(218, 165, 32),
        "tan" => Color::Rgb(210, 180, 140),
        "charcoal" => Color::Rgb(72, 72, 72),
        "green" => Color::Green,
        "gray" => Color::DarkGray,
        "amber" => Color::Yellow,
        _ => Color::White,
    }
}

fn box_top(width: usize) -> String {
    format!("┌{}┐", "─".repeat(width.saturating_sub(2)))
}

fn box_bottom(width: usize) -> String {
    format!("└{}┘", "─".repeat(width.saturating_sub(2)))
}

fn box_cable(width: usize) -> String {
    format!("├{}┤", "─".repeat(width.saturating_sub(2)))
}

fn box_content(text: &str, width: usize) -> String {
    let inner_width = width.saturating_sub(2);
    let mut content = text.chars().take(inner_width).collect::<String>();
    let padding = inner_width.saturating_sub(content.chars().count());
    content.push_str(&" ".repeat(padding));
    format!("│{content}│")
}

fn box_title_content(title: &str, has_indicator: bool, width: usize) -> String {
    let inner_width = width.saturating_sub(2);
    if has_indicator {
        let title_width = inner_width.saturating_sub(2);
        let mut content = title.chars().take(title_width).collect::<String>();
        let padding = title_width.saturating_sub(content.chars().count());
        content.push_str(&" ".repeat(padding));
        return format!("│{content} o│");
    }

    box_content(title, width)
}

fn box_content_centered(text: &str, width: usize) -> String {
    let inner_width = width.saturating_sub(2);
    let content = text.chars().take(inner_width).collect::<String>();
    let padding = inner_width.saturating_sub(content.chars().count());
    let left = padding / 2;
    let right = padding - left;
    format!("│{}{}{}│", " ".repeat(left), content, " ".repeat(right))
}

fn format_control_panel(controls: AmpControls, selected: AmpControlParam) -> String {
    let knob = |param: AmpControlParam, value: f32| {
        let marker = if param == selected { ">" } else { " " };
        format!("{marker} {:<8} {:>4.1}", param.label(), value * 10.0)
    };
    format!(
        "\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n\nTab/Shift-Tab select   Left/Right adjust 0.1   Up/Down adjust 1.0   q quit\n",
        knob(AmpControlParam::Volume, controls.volume),
        knob(AmpControlParam::Bass, controls.bass),
        knob(AmpControlParam::Treble, controls.treble),
        knob(AmpControlParam::Cut, controls.cut),
        knob(AmpControlParam::Drive, controls.drive),
        knob(AmpControlParam::Presence, controls.presence),
        knob(AmpControlParam::Sag, controls.sag),
    )
}

fn format_pedal_control_panel(
    selected_pedal: Option<usize>,
    selected_param: usize,
    config: &SignalChainConfig,
    controls: &[DeviceSlotControls],
) -> Option<String> {
    let (slot_index, slot) = selected_pedal.and_then(|index| chain_slot_config(config, index))?;
    let controls = controls.get(slot_index).copied();
    let bypassed = slot_bypassed(slot, controls.as_ref());
    let mut lines = vec![
        format!(
            "\nPedal controls: {} slot {}",
            device_label(slot.device),
            slot_index + 1
        ),
        format!("status    {}", if bypassed { "bypass" } else { "active" }),
    ];

    let controls = controls.map_or_else(
        || default_device_controls(slot.device),
        |controls| controls.controls,
    );
    for (param_index, descriptor) in slot.device.model_descriptor().controls.iter().enumerate() {
        push_control_line(
            &mut lines,
            selected_param,
            param_index,
            descriptor,
            device_control_display_value(controls, descriptor),
        );
    }

    lines.push("\nClick a pedal to inspect it. Click amp controls to return.".to_owned());
    Some(lines.join("\n"))
}

fn push_control_line(
    lines: &mut Vec<String>,
    selected_param: usize,
    param_index: usize,
    descriptor: &ControlDescriptor,
    value: String,
) {
    let marker = if selected_param == param_index {
        ">"
    } else {
        " "
    };
    lines.push(format!(
        "{marker} {:<8} {:<10} {:>8}",
        descriptor.label,
        control_kind_label(descriptor.kind),
        value
    ));
}

fn control_kind_label(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Pot => "pot",
        ControlKind::Slider => "slider",
        ControlKind::Switch => "switch",
        ControlKind::Footswitch => "footsw",
    }
}

fn default_device_controls(device: DeviceConfig) -> DeviceControls {
    match device {
        DeviceConfig::Lumen => DeviceControls::Lumen(LumenControls::default()),
        DeviceConfig::Muon => DeviceControls::Muon(MuonControls::default()),
        DeviceConfig::Muffin => DeviceControls::Muffin(MuffinControls::default()),
        DeviceConfig::Minotaur => DeviceControls::Minotaur(MinotaurControls::default()),
        DeviceConfig::Monarch => DeviceControls::Monarch(MonarchControls::default()),
        DeviceConfig::GodessOne => DeviceControls::GodessOne(GodessOneControls::default()),
        DeviceConfig::Dartford => DeviceControls::Dartford(DartfordControls::default()),
        DeviceConfig::Tron => DeviceControls::Tron(TronControls::default()),
        DeviceConfig::Jetstream => DeviceControls::Jetstream(JetstreamControls::default()),
        DeviceConfig::Celeste => DeviceControls::Celeste(CelesteControls::default()),
        DeviceConfig::Brigade => DeviceControls::Brigade(BrigadeControls::default()),
        DeviceConfig::Springfield => DeviceControls::Springfield(SpringfieldControls::default()),
    }
}

fn device_control_display_value(
    controls: DeviceControls,
    descriptor: &ControlDescriptor,
) -> String {
    match device_control_value(controls, descriptor.id) {
        ControlValue::Number(value) => format!("{:>4.1}", value * descriptor.display_scale),
        ControlValue::Text(value) => value.to_owned(),
        ControlValue::Missing => "-".to_owned(),
    }
}

enum ControlValue {
    Number(f32),
    Text(&'static str),
    Missing,
}

fn device_control_value(controls: DeviceControls, id: &str) -> ControlValue {
    match controls {
        DeviceControls::Lumen(controls) => match id {
            "peak_reduction" => ControlValue::Number(controls.peak_reduction),
            "gain" => ControlValue::Number(controls.gain),
            "emphasis" => ControlValue::Number(controls.emphasis),
            "mix" => ControlValue::Number(controls.mix),
            _ => ControlValue::Missing,
        },
        DeviceControls::Muon(controls) => match id {
            "sensitivity" => ControlValue::Number(controls.sensitivity),
            "range" => ControlValue::Number(controls.range),
            "resonance" => ControlValue::Number(controls.resonance),
            "mix" => ControlValue::Number(controls.mix),
            _ => ControlValue::Missing,
        },
        DeviceControls::Muffin(controls) => match id {
            "sustain" => ControlValue::Number(controls.sustain),
            "tone" => ControlValue::Number(controls.tone),
            "level" => ControlValue::Number(controls.level),
            _ => ControlValue::Missing,
        },
        DeviceControls::Minotaur(controls) => match id {
            "gain" => ControlValue::Number(controls.gain),
            "treble" => ControlValue::Number(controls.treble),
            "output" => ControlValue::Number(controls.output),
            _ => ControlValue::Missing,
        },
        DeviceControls::Monarch(controls) => match id {
            "gain" => ControlValue::Number(controls.gain),
            "tone" => ControlValue::Number(controls.tone),
            "output" => ControlValue::Number(controls.output),
            _ => ControlValue::Missing,
        },
        DeviceControls::GodessOne(controls) => match id {
            "distortion" => ControlValue::Number(controls.distortion),
            "tone" => ControlValue::Number(controls.tone),
            "level" => ControlValue::Number(controls.level),
            "mode" => ControlValue::Text(match controls.mode {
                GodessOneMode::Standard => "standard",
                GodessOneMode::Custom => "custom",
            }),
            _ => ControlValue::Missing,
        },
        DeviceControls::Dartford(controls) => match id {
            "rate_hz" => ControlValue::Number(controls.rate_hz),
            "depth" => ControlValue::Number(controls.depth),
            "level" => ControlValue::Number(controls.level),
            "wave" => ControlValue::Text(match controls.wave {
                DartfordWave::Sine => "sine",
                DartfordWave::Triangle => "triangle",
                DartfordWave::Square => "square",
            }),
            _ => ControlValue::Missing,
        },
        DeviceControls::Tron(controls) => match id {
            "rate_hz" => ControlValue::Number(controls.rate_hz),
            "depth" => ControlValue::Number(controls.depth),
            "feedback" => ControlValue::Number(controls.feedback),
            "mix" => ControlValue::Number(controls.mix),
            _ => ControlValue::Missing,
        },
        DeviceControls::Jetstream(controls) => match id {
            "manual" => ControlValue::Number(controls.manual),
            "rate_hz" => ControlValue::Number(controls.rate_hz),
            "depth" => ControlValue::Number(controls.depth),
            "feedback" => ControlValue::Number(controls.feedback),
            "mix" => ControlValue::Number(controls.mix),
            _ => ControlValue::Missing,
        },
        DeviceControls::Celeste(controls) => match id {
            "rate_hz" => ControlValue::Number(controls.rate_hz),
            "depth" => ControlValue::Number(controls.depth),
            "tone" => ControlValue::Number(controls.tone),
            "mix" => ControlValue::Number(controls.mix),
            _ => ControlValue::Missing,
        },
        DeviceControls::Brigade(controls) => match id {
            "time_ms" => ControlValue::Number(controls.time_ms),
            "repeats" => ControlValue::Number(controls.repeats),
            "tone" => ControlValue::Number(controls.tone),
            "mix" => ControlValue::Number(controls.mix),
            _ => ControlValue::Missing,
        },
        DeviceControls::Springfield(controls) => match id {
            "dwell" => ControlValue::Number(controls.dwell),
            "tone" => ControlValue::Number(controls.tone),
            "mix" => ControlValue::Number(controls.mix),
            _ => ControlValue::Missing,
        },
        DeviceControls::Default => ControlValue::Missing,
    }
}

fn format_monitor_tui(
    stats: &MonitorSnapshot,
    components: Option<ComponentTelemetrySnapshot>,
    model: &str,
    log_path: &Path,
    chain_config: &SignalChainConfig,
    device_controls: &[DeviceSlotControls],
    controls: AmpControls,
    selected: AmpControlParam,
    selected_pedal: Option<usize>,
    selected_pedal_param: usize,
    amp_enabled: bool,
    cab_enabled: Option<bool>,
) -> String {
    let mut text = format_monitor_dashboard(stats, components, model, log_path);
    text.push_str(&format_pedalboard_text_with_cab(
        chain_config,
        device_controls,
        amp_enabled,
        cab_enabled,
    ));
    if let Some(pedal_panel) = format_pedal_control_panel(
        selected_pedal,
        selected_pedal_param,
        chain_config,
        device_controls,
    ) {
        text.push_str(&pedal_panel);
    } else {
        text.push_str(&format_control_panel(controls, selected));
    }
    text
}

fn format_terminal_frame(text: &str) -> String {
    text.replace('\n', "\r\n")
}

fn vu_ratio(level: f32) -> f64 {
    let db = dbfs(level);
    if db.is_finite() {
        ((db + 60.0) / 60.0).clamp(0.0, 1.0) as f64
    } else {
        0.0
    }
}

fn draw_monitor_frame(
    frame: &mut Frame<'_>,
    stats: &MonitorSnapshot,
    components: Option<ComponentTelemetrySnapshot>,
    model: &str,
    log_path: &Path,
    chain_config: &SignalChainConfig,
    device_controls: &[DeviceSlotControls],
    controls: AmpControls,
    selected: AmpControlParam,
    selected_pedal: Option<usize>,
    selected_pedal_param: usize,
    amp_enabled: bool,
    cab_enabled: Option<bool>,
) -> MonitorHitAreas {
    let pedalboard_height = pedalboard_panel_height(chain_config);
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(pedalboard_height),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(if components.is_some() { 5 } else { 1 }),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(frame.area());

    let in_rms = rms_from_scaled(stats.input_sum_squares, stats.input_count);
    let out_rms = rms_from_scaled(stats.output_sum_squares, stats.output_count);
    let title = format!(
        " Greybound monitor  model {model}  log {} ",
        log_path.display()
    );
    frame.render_widget(
        Paragraph::new(title).block(Block::default().borders(Borders::BOTTOM)),
        root[0],
    );
    draw_pedalboard(
        frame,
        root[1],
        chain_config,
        device_controls,
        amp_enabled,
        cab_enabled,
    );
    frame.render_widget(
        Gauge::default()
            .block(Block::bordered().title(format!(
                "input rms {} dBFS peak {} dBFS near/clip {}/{}",
                format_dbfs(in_rms),
                format_dbfs(stats.input_peak),
                stats.input_near_clips,
                stats.input_clips
            )))
            .gauge_style(Style::default().fg(Color::Green))
            .ratio(vu_ratio(in_rms)),
        root[2],
    );
    frame.render_widget(
        Gauge::default()
            .block(Block::bordered().title(format!(
                "output rms {} dBFS peak {} dBFS near/clip {}/{}",
                format_dbfs(out_rms),
                format_dbfs(stats.output_peak),
                stats.output_near_clips,
                stats.output_clips
            )))
            .gauge_style(Style::default().fg(Color::Cyan))
            .ratio(vu_ratio(out_rms)),
        root[3],
    );

    if let Some(components) = components {
        let lines = vec![
            Line::from(format!(
                "xrun in/out {}/{}",
                stats.input_overruns, stats.output_underruns
            )),
            Line::from(format!(
                "rails avg/min pre {:>5.0}/{:>5.0} pi {:>5.0}/{:>5.0} pwr {:>5.0}/{:>5.0} scr {:>5.0}/{:>5.0} V",
                components.preamp_voltage_avg,
                components.preamp_voltage_min,
                components.phase_inverter_voltage_avg,
                components.phase_inverter_voltage_min,
                components.power_voltage_avg,
                components.power_voltage_min,
                components.power_screen_voltage_avg,
                components.power_screen_voltage_min
            )),
            Line::from(format!(
                "I avg/max mA first {:>5.2}/{:>5.2} pi {:>5.2}/{:>5.2} pwr {:>5.1}/{:>5.1} atk {:>5.1}/{:>5.1} scr {:>5.1}/{:>5.1}",
                components.first_stage_current_avg * 1_000.0,
                components.first_stage_current_max * 1_000.0,
                components.phase_inverter_current_avg * 1_000.0,
                components.phase_inverter_current_max * 1_000.0,
                components.power_current_avg * 1_000.0,
                components.power_current_max * 1_000.0,
                components.power_attack_current_avg * 1_000.0,
                components.power_attack_current_max * 1_000.0,
                components.power_screen_current_avg * 1_000.0,
                components.power_screen_current_max * 1_000.0
            )),
            Line::from(format!(
                "cath avg/max {:>5.2}/{:>5.2} V   flux |avg/max| {:.5}/{:.5}   n {}",
                components.power_cathode_bias_avg,
                components.power_cathode_bias_max,
                components.transformer_flux_abs_avg,
                components.transformer_flux_abs_max,
                components.count
            )),
        ];
        frame.render_widget(
            Paragraph::new(lines).block(Block::bordered().title("nox30 components")),
            root[4],
        );
    } else {
        frame.render_widget(
            Paragraph::new(format!(
                "xrun in/out {}/{}",
                stats.input_overruns, stats.output_underruns
            )),
            root[4],
        );
    }

    let controls_area = root[5];
    draw_monitor_control_panel(
        frame,
        controls_area,
        controls,
        selected,
        selected_pedal,
        selected_pedal_param,
        chain_config,
        device_controls,
    );

    frame.render_widget(
        Paragraph::new("Tab/Shift-Tab select   Left/Right adjust 0.1   Up/Down adjust 1.0   mouse click/scroll   q quit"),
        root[6],
    );

    MonitorHitAreas {
        amp_controls: controls_area,
        amp_box: amp_hit_area(chain_config, root[1]),
        amp_footswitch: amp_footswitch_hit_area(chain_config, root[1]),
        pedals: pedal_hit_areas(chain_config, root[1]),
        footswitches: footswitch_hit_areas(chain_config, root[1]),
        cab_footswitch: cab_enabled.map(|_| cab_footswitch_hit_area(chain_config, root[1])),
    }
}

fn draw_monitor_control_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    controls: AmpControls,
    selected: AmpControlParam,
    selected_pedal: Option<usize>,
    selected_pedal_param: usize,
    chain_config: &SignalChainConfig,
    device_controls: &[DeviceSlotControls],
) {
    if let Some(panel) = format_pedal_control_panel(
        selected_pedal,
        selected_pedal_param,
        chain_config,
        device_controls,
    ) {
        frame.render_widget(
            Paragraph::new(panel.trim_start().to_owned())
                .block(Block::bordered().title("pedal controls")),
            area,
        );
        return;
    }

    let control_lines = AmpControlParam::ALL
        .iter()
        .map(|param| {
            let value = match param {
                AmpControlParam::Volume => controls.volume,
                AmpControlParam::Bass => controls.bass,
                AmpControlParam::Treble => controls.treble,
                AmpControlParam::Cut => controls.cut,
                AmpControlParam::Drive => controls.drive,
                AmpControlParam::Presence => controls.presence,
                AmpControlParam::Sag => controls.sag,
            };
            let marker = if *param == selected { ">" } else { " " };
            let style = if *param == selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::from(vec![
                Span::styled(format!("{marker} {:<8}", param.label()), style),
                Span::raw(format!(" {:>4.1}", value * 10.0)),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(control_lines).block(Block::bordered().title("amp controls")),
        area,
    );
}

fn draw_pedalboard(
    frame: &mut Frame<'_>,
    area: Rect,
    config: &SignalChainConfig,
    controls: &[DeviceSlotControls],
    amp_enabled: bool,
    cab_enabled: Option<bool>,
) {
    let mut rows = Vec::new();
    rows.extend(render_pedalboard_section_styled(
        "GTR",
        &config.pre_amp,
        controls,
        0,
        Some((&config.amp_model, amp_enabled)),
        cab_enabled,
        "OUT",
    ));
    if !config.fx_loop.is_empty() {
        rows.push(Line::from(Span::raw("FX LOOP PEDALS")));
        rows.extend(render_pedalboard_section_styled(
            "SEND",
            &config.fx_loop,
            controls,
            config.pre_amp.len(),
            None,
            None,
            "RETURN",
        ));
    }
    if !config.post_amp.is_empty() {
        rows.push(Line::from(Span::raw("POST")));
        rows.extend(render_pedalboard_section_styled(
            "AMP OUT",
            &config.post_amp,
            controls,
            config.pre_amp.len() + config.fx_loop.len(),
            None,
            None,
            "OUT",
        ));
    }
    frame.render_widget(
        Paragraph::new(rows).block(Block::bordered().title("pedalboard")),
        area,
    );
}

fn pedalboard_panel_height(config: &SignalChainConfig) -> u16 {
    let mut content_rows = 6;
    if !config.fx_loop.is_empty() {
        content_rows += 7;
    }
    if !config.post_amp.is_empty() {
        content_rows += 7;
    }
    content_rows + 2
}

fn pedal_hit_areas(config: &SignalChainConfig, pedalboard_area: Rect) -> Vec<PedalHitArea> {
    let content_x = pedalboard_area.x.saturating_add(1);
    let mut content_y = pedalboard_area.y.saturating_add(1);
    let mut areas = Vec::new();

    append_pedal_hit_areas(&mut areas, content_x, content_y, 0, &config.pre_amp);
    content_y = content_y.saturating_add(6);
    if !config.fx_loop.is_empty() {
        content_y = content_y.saturating_add(1);
        append_pedal_hit_areas(
            &mut areas,
            content_x,
            content_y,
            config.pre_amp.len(),
            &config.fx_loop,
        );
        content_y = content_y.saturating_add(6);
    }
    if !config.post_amp.is_empty() {
        content_y = content_y.saturating_add(1);
        append_pedal_hit_areas(
            &mut areas,
            content_x,
            content_y,
            config.pre_amp.len() + config.fx_loop.len(),
            &config.post_amp,
        );
    }

    areas
}

fn footswitch_hit_areas(config: &SignalChainConfig, pedalboard_area: Rect) -> Vec<PedalHitArea> {
    pedal_hit_areas(config, pedalboard_area)
        .into_iter()
        .map(|area| {
            let switch_width = 3.min(area.rect.width);
            let switch_x = area
                .rect
                .x
                .saturating_add((area.rect.width.saturating_sub(switch_width)) / 2);
            PedalHitArea {
                global_index: area.global_index,
                rect: Rect {
                    x: switch_x,
                    y: area.rect.y.saturating_add(4),
                    width: switch_width,
                    height: 1,
                },
            }
        })
        .collect()
}

fn cab_footswitch_hit_area(config: &SignalChainConfig, pedalboard_area: Rect) -> Rect {
    let amp = amp_hit_area(config, pedalboard_area);
    let cab_width = cab_board_box(true).width as u16;
    let cab_x = amp.x.saturating_add(amp.width).saturating_add(2);
    let switch_width = 3.min(cab_width);
    Rect {
        x: cab_x.saturating_add((cab_width.saturating_sub(switch_width)) / 2),
        y: amp.y.saturating_add(4),
        width: switch_width,
        height: 1,
    }
}

fn amp_footswitch_hit_area(config: &SignalChainConfig, pedalboard_area: Rect) -> Rect {
    centered_footswitch_hit_area(amp_hit_area(config, pedalboard_area))
}

fn centered_footswitch_hit_area(area: Rect) -> Rect {
    let switch_width = 3.min(area.width);
    Rect {
        x: area
            .x
            .saturating_add((area.width.saturating_sub(switch_width)) / 2),
        y: area.y.saturating_add(4),
        width: switch_width,
        height: 1,
    }
}

fn amp_hit_area(config: &SignalChainConfig, pedalboard_area: Rect) -> Rect {
    let content_x = pedalboard_area.x.saturating_add(1);
    let content_y = pedalboard_area.y.saturating_add(1);
    let pre_amp_width = config.pre_amp.iter().fold(0u16, |width, slot| {
        width.saturating_add(slot.device.model_descriptor().visual.width as u16 + 2)
    });
    let amp_width = amp_model_descriptor(&config.amp_model).visual.width as u16;
    let amp_x = content_x.saturating_add(10).saturating_add(pre_amp_width);
    Rect {
        x: amp_x,
        y: content_y,
        width: amp_width,
        height: 6,
    }
}

fn append_pedal_hit_areas(
    areas: &mut Vec<PedalHitArea>,
    content_x: u16,
    content_y: u16,
    section_offset: usize,
    slots: &[DeviceSlotConfig],
) {
    let mut x = content_x.saturating_add(10);
    for (slot_index, slot) in slots.iter().enumerate() {
        let width = slot.device.model_descriptor().visual.width as u16;
        areas.push(PedalHitArea {
            global_index: section_offset + slot_index,
            rect: Rect {
                x,
                y: content_y,
                width,
                height: 6,
            },
        });
        x = x.saturating_add(width + 2);
    }
}

fn pedal_from_mouse(column: u16, row: u16, areas: &[PedalHitArea]) -> Option<usize> {
    areas
        .iter()
        .find(|area| {
            column >= area.rect.x
                && column < area.rect.x + area.rect.width
                && row >= area.rect.y
                && row < area.rect.y + area.rect.height
        })
        .map(|area| area.global_index)
}

fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

fn control_from_mouse_row(row: u16, controls_area: Rect) -> Option<AmpControlParam> {
    if row <= controls_area.y || row >= controls_area.y + controls_area.height {
        return None;
    }
    let index = usize::from(row - controls_area.y - 1);
    AmpControlParam::ALL.get(index).copied()
}

fn amp_control_from_mouse(
    column: u16,
    row: u16,
    controls_area: Rect,
    selected_pedal: Option<usize>,
) -> Option<AmpControlParam> {
    if selected_pedal.is_some() || !rect_contains(controls_area, column, row) {
        return None;
    }
    control_from_mouse_row(row, controls_area)
}

fn pedal_control_from_mouse_row(row: u16, controls_area: Rect) -> Option<usize> {
    if row < controls_area.y.saturating_add(3)
        || row >= controls_area.y.saturating_add(controls_area.height)
    {
        return None;
    }
    Some(usize::from(row - controls_area.y - 3))
}

fn selected_pedal_control_descriptor(
    config: &SignalChainConfig,
    selected_pedal: Option<usize>,
    selected_param: usize,
) -> Option<ControlDescriptor> {
    let (_, slot) = selected_pedal.and_then(|index| chain_slot_config(config, index))?;
    slot.device
        .model_descriptor()
        .controls
        .get(selected_param)
        .copied()
}

fn pedal_control_count(config: &SignalChainConfig, selected_pedal: Option<usize>) -> usize {
    selected_pedal
        .and_then(|index| chain_slot_config(config, index))
        .map_or(0, |(_, slot)| slot.device.model_descriptor().controls.len())
}

fn enter_terminal_ui() -> io::Result<MonitorTerminal> {
    terminal::enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(
        stderr,
        terminal::EnterAlternateScreen,
        EnableMouseCapture,
        cursor::Hide,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stderr))?;
    terminal.clear()?;
    Ok(terminal)
}

fn leave_terminal_ui(terminal: Option<&mut MonitorTerminal>) {
    if let Some(terminal) = terminal {
        let _ = terminal.show_cursor();
        let _ = execute!(
            terminal.backend_mut(),
            cursor::Show,
            DisableMouseCapture,
            terminal::LeaveAlternateScreen,
            terminal::Clear(ClearType::All)
        );
    } else {
        let _ = execute!(
            io::stderr(),
            cursor::Show,
            DisableMouseCapture,
            terminal::LeaveAlternateScreen,
            terminal::Clear(ClearType::All)
        );
    }
    let _ = terminal::disable_raw_mode();
}

fn handle_monitor_key(
    key: KeyCode,
    selected: &mut AmpControlParam,
    controls: &SharedAmpControls,
) -> bool {
    match key {
        KeyCode::Tab => *selected = selected.next(),
        KeyCode::BackTab => *selected = selected.previous(),
        KeyCode::Left => controls.adjust(*selected, -0.01),
        KeyCode::Right => controls.adjust(*selected, 0.01),
        KeyCode::Down => controls.adjust(*selected, -0.1),
        KeyCode::Up => controls.adjust(*selected, 0.1),
        KeyCode::Char('-') => controls.adjust(*selected, -0.01),
        KeyCode::Char('+') | KeyCode::Char('=') => controls.adjust(*selected, 0.01),
        KeyCode::Char('q') => return true,
        _ => {}
    }
    false
}

fn spawn_monitor_ui(
    monitor: Arc<MonitorStats>,
    components: Arc<ComponentTelemetry>,
    model: String,
    monitor_log_path: PathBuf,
    chain_config: SignalChainConfig,
    device_controls: SharedDeviceControls,
    cab_controls: Option<SharedCabControls>,
    controls: SharedAmpControls,
) {
    std::thread::spawn(move || {
        let mut log = RotatingMonitorLog::new(monitor_log_path.clone(), MONITOR_LOG_LINES);
        let show_components = model == "nox30";
        let mut selected = AmpControlParam::Volume;
        let mut selected_pedal = None;
        let mut selected_pedal_param = 0usize;
        let mut terminal_ui = enter_terminal_ui().ok();
        let mut hit_areas = MonitorHitAreas::default();
        let mut stats = MonitorSnapshot::default();
        let mut component_snapshot = None;
        let mut last_monitor_refresh = Instant::now() - MONITOR_REFRESH;
        let mut needs_redraw = true;

        loop {
            if terminal_ui.is_some() {
                let poll_timeout = MONITOR_EVENT_POLL.min(
                    MONITOR_REFRESH
                        .checked_sub(last_monitor_refresh.elapsed())
                        .unwrap_or(Duration::ZERO),
                );
                while event::poll(poll_timeout).unwrap_or(false) {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            if let Some(slot_index) = selected_pedal {
                                match key.code {
                                    KeyCode::Tab => {
                                        let count =
                                            pedal_control_count(&chain_config, selected_pedal)
                                                .max(1);
                                        selected_pedal_param = (selected_pedal_param + 1) % count;
                                    }
                                    KeyCode::BackTab => {
                                        let count =
                                            pedal_control_count(&chain_config, selected_pedal)
                                                .max(1);
                                        selected_pedal_param =
                                            (selected_pedal_param + count - 1) % count;
                                    }
                                    KeyCode::Left | KeyCode::Char('-') => {
                                        if let Some(descriptor) = selected_pedal_control_descriptor(
                                            &chain_config,
                                            selected_pedal,
                                            selected_pedal_param,
                                        ) {
                                            device_controls.adjust(
                                                slot_index,
                                                selected_pedal_param,
                                                -descriptor.step,
                                                descriptor.min,
                                                descriptor.max,
                                            );
                                        }
                                    }
                                    KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
                                        if let Some(descriptor) = selected_pedal_control_descriptor(
                                            &chain_config,
                                            selected_pedal,
                                            selected_pedal_param,
                                        ) {
                                            device_controls.adjust(
                                                slot_index,
                                                selected_pedal_param,
                                                descriptor.step,
                                                descriptor.min,
                                                descriptor.max,
                                            );
                                        }
                                    }
                                    KeyCode::Down => {
                                        if let Some(descriptor) = selected_pedal_control_descriptor(
                                            &chain_config,
                                            selected_pedal,
                                            selected_pedal_param,
                                        ) {
                                            device_controls.adjust(
                                                slot_index,
                                                selected_pedal_param,
                                                -descriptor.large_step,
                                                descriptor.min,
                                                descriptor.max,
                                            );
                                        }
                                    }
                                    KeyCode::Up => {
                                        if let Some(descriptor) = selected_pedal_control_descriptor(
                                            &chain_config,
                                            selected_pedal,
                                            selected_pedal_param,
                                        ) {
                                            device_controls.adjust(
                                                slot_index,
                                                selected_pedal_param,
                                                descriptor.large_step,
                                                descriptor.min,
                                                descriptor.max,
                                            );
                                        }
                                    }
                                    KeyCode::Char('q') => {
                                        leave_terminal_ui(terminal_ui.as_mut());
                                        std::process::exit(0);
                                    }
                                    _ => {}
                                }
                            } else if handle_monitor_key(key.code, &mut selected, &controls) {
                                leave_terminal_ui(terminal_ui.as_mut());
                                std::process::exit(0);
                            }
                            needs_redraw = true;
                        }
                        Ok(Event::Mouse(mouse)) => match mouse.kind {
                            MouseEventKind::Down(_) => {
                                if let Some(cab_footswitch) = hit_areas.cab_footswitch {
                                    if rect_contains(cab_footswitch, mouse.column, mouse.row) {
                                        if let Some(cab_controls) = &cab_controls {
                                            cab_controls.toggle();
                                        }
                                        selected_pedal = None;
                                        needs_redraw = true;
                                        continue;
                                    }
                                }
                                if rect_contains(hit_areas.amp_footswitch, mouse.column, mouse.row)
                                {
                                    controls.toggle_enabled();
                                    selected_pedal = None;
                                    needs_redraw = true;
                                    continue;
                                }
                                if let Some(pedal_index) = pedal_from_mouse(
                                    mouse.column,
                                    mouse.row,
                                    &hit_areas.footswitches,
                                ) {
                                    device_controls.toggle_bypass(pedal_index);
                                    selected_pedal = Some(pedal_index);
                                    selected_pedal_param = 0;
                                    needs_redraw = true;
                                } else if let Some(pedal_index) =
                                    pedal_from_mouse(mouse.column, mouse.row, &hit_areas.pedals)
                                {
                                    selected_pedal = Some(pedal_index);
                                    selected_pedal_param = 0;
                                    needs_redraw = true;
                                } else if rect_contains(hit_areas.amp_box, mouse.column, mouse.row)
                                {
                                    selected_pedal = None;
                                    needs_redraw = true;
                                } else if let Some(param) = amp_control_from_mouse(
                                    mouse.column,
                                    mouse.row,
                                    hit_areas.amp_controls,
                                    selected_pedal,
                                ) {
                                    selected = param;
                                    needs_redraw = true;
                                } else if selected_pedal.is_some()
                                    && rect_contains(
                                        hit_areas.amp_controls,
                                        mouse.column,
                                        mouse.row,
                                    )
                                {
                                    if let Some(param) = pedal_control_from_mouse_row(
                                        mouse.row,
                                        hit_areas.amp_controls,
                                    ) {
                                        let count =
                                            pedal_control_count(&chain_config, selected_pedal);
                                        if count > 0 {
                                            selected_pedal_param = param.min(count - 1);
                                        }
                                        needs_redraw = true;
                                    }
                                }
                            }
                            MouseEventKind::ScrollUp => {
                                if let Some(slot_index) = selected_pedal {
                                    if let Some(descriptor) = selected_pedal_control_descriptor(
                                        &chain_config,
                                        selected_pedal,
                                        selected_pedal_param,
                                    ) {
                                        device_controls.adjust(
                                            slot_index,
                                            selected_pedal_param,
                                            descriptor.step,
                                            descriptor.min,
                                            descriptor.max,
                                        );
                                    }
                                    needs_redraw = true;
                                } else {
                                    controls.adjust(selected, 0.01);
                                    needs_redraw = true;
                                }
                            }
                            MouseEventKind::ScrollDown => {
                                if let Some(slot_index) = selected_pedal {
                                    if let Some(descriptor) = selected_pedal_control_descriptor(
                                        &chain_config,
                                        selected_pedal,
                                        selected_pedal_param,
                                    ) {
                                        device_controls.adjust(
                                            slot_index,
                                            selected_pedal_param,
                                            -descriptor.step,
                                            descriptor.min,
                                            descriptor.max,
                                        );
                                    }
                                    needs_redraw = true;
                                } else {
                                    controls.adjust(selected, -0.01);
                                    needs_redraw = true;
                                }
                            }
                            _ => {}
                        },
                        Ok(_) => {}
                        Err(_) => {
                            leave_terminal_ui(terminal_ui.as_mut());
                            terminal_ui = None;
                            break;
                        }
                    }
                    if !event::poll(Duration::from_millis(0)).unwrap_or(false) {
                        break;
                    }
                }
            } else {
                std::thread::sleep(MONITOR_EVENT_POLL);
            }

            if last_monitor_refresh.elapsed() >= MONITOR_REFRESH {
                stats = monitor.snapshot_and_reset();
                component_snapshot = show_components.then(|| components.snapshot_and_reset());
                last_monitor_refresh = Instant::now();
                needs_redraw = true;

                let timestamp = unix_timestamp();
                let mut log_lines =
                    vec![format!("ts={timestamp} {}", format_audio_monitor(&stats))];
                if let Some(components) = component_snapshot {
                    log_lines.push(format!(
                        "ts={timestamp} {}",
                        format_component_telemetry(components)
                    ));
                }
                let _ = log.push_many(log_lines);
            }

            if !needs_redraw {
                continue;
            }

            let current_controls = controls.load();
            let amp_enabled = controls.load_enabled();
            let current_device_controls = device_controls.load();
            let cab_enabled = cab_controls.as_ref().map(SharedCabControls::load);

            if let Some(terminal) = terminal_ui.as_mut() {
                let result = terminal.draw(|frame| {
                    hit_areas = draw_monitor_frame(
                        frame,
                        &stats,
                        component_snapshot,
                        &model,
                        &monitor_log_path,
                        &chain_config,
                        &current_device_controls,
                        current_controls,
                        selected,
                        selected_pedal,
                        selected_pedal_param,
                        amp_enabled,
                        cab_enabled,
                    );
                });
                let draw_failed = result.is_err();
                if draw_failed {
                    leave_terminal_ui(Some(terminal));
                }
                if draw_failed {
                    terminal_ui = None;
                }
            } else {
                let dashboard = format_monitor_tui(
                    &stats,
                    component_snapshot,
                    &model,
                    &monitor_log_path,
                    &chain_config,
                    &current_device_controls,
                    current_controls,
                    selected,
                    selected_pedal,
                    selected_pedal_param,
                    amp_enabled,
                    cab_enabled,
                );
                eprint!("\x1B[2J\x1B[H{}", format_terminal_frame(&dashboard));
                let _ = io::stderr().flush();
            }
            needs_redraw = false;
        }
    });
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

struct RotatingMonitorLog {
    path: PathBuf,
    capacity: usize,
    lines: VecDeque<String>,
}

impl RotatingMonitorLog {
    fn new(path: PathBuf, capacity: usize) -> Self {
        Self {
            path,
            capacity,
            lines: VecDeque::with_capacity(capacity.min(1024)),
        }
    }

    fn push_many(&mut self, lines: impl IntoIterator<Item = String>) -> io::Result<()> {
        for line in lines {
            self.lines.push_back(line);
            while self.lines.len() > self.capacity {
                self.lines.pop_front();
            }
        }
        self.flush()
    }

    fn flush(&self) -> io::Result<()> {
        let mut file = File::create(&self.path)?;
        for line in &self.lines {
            writeln!(file, "{line}")?;
        }
        Ok(())
    }
}

struct WavInput {
    path: PathBuf,
    samples: Vec<f32>,
    channels: usize,
    sample_rate: u32,
}

enum RuntimeInput {
    Live(Consumer<f32>),
    Wav { samples: Vec<f32>, position: usize },
}

impl RuntimeInput {
    fn next_sample(&mut self, monitoring: &MonitorStats, monitor: bool) -> f32 {
        match self {
            Self::Live(consumer) => match consumer.pop() {
                Ok(sample) => sample,
                Err(_) => {
                    if monitor {
                        monitoring.record_output_underrun();
                    }
                    0.0
                }
            },
            Self::Wav { samples, position } => {
                let sample = samples[*position];
                *position = (*position + 1) % samples.len();
                if monitor {
                    monitoring.record_input(sample);
                }
                sample
            }
        }
    }
}

struct Args {
    input_device: Option<String>,
    output_device: String,
    null_output: bool,
    output_wav: Option<PathBuf>,
    render_seconds: f32,
    input_wav: Option<PathBuf>,
    input_channel: usize,
    output_channels: Vec<usize>,
    sample_rate: u32,
    period_size: u32,
    controls: AmpControls,
    amp_enabled: bool,
    chain_config: SignalChainConfig,
    device_controls: Vec<DeviceSlotControls>,
    rig_name: Option<String>,
    input_db: f32,
    input_gain: f32,
    output_db: f32,
    ir: bool,
    ir_path: Option<PathBuf>,
    monitor: bool,
    monitor_log: PathBuf,
    model: String,
    neural_cells: Vec<NeuralCellOverride>,
    neural_cell_mode: NeuralCellMode,
}

struct NeuralCellOverride {
    component: String,
    descriptor_path: PathBuf,
}

fn load_wav_input(path: &Path, input_channel: usize) -> Result<WavInput> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("could not open input WAV '{}'", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if input_channel >= channels {
        bail!(
            "input channel {} is unavailable; '{}' has {} channel(s)",
            input_channel + 1,
            path.display(),
            channels
        );
    }

    let mut samples = Vec::new();
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for (index, sample) in reader.samples::<f32>().enumerate() {
                let sample = sample.with_context(|| {
                    format!("could not read float sample from '{}'", path.display())
                })?;
                if index % channels == input_channel {
                    samples.push(sample);
                }
            }
        }
        hound::SampleFormat::Int => {
            let scale = 2.0_f32.powi(spec.bits_per_sample as i32 - 1);
            for (index, sample) in reader.samples::<i32>().enumerate() {
                let sample = sample.with_context(|| {
                    format!("could not read int sample from '{}'", path.display())
                })? as f32
                    / scale;
                if index % channels == input_channel {
                    samples.push(sample);
                }
            }
        }
    }

    if samples.is_empty() {
        bail!("input WAV '{}' contains no samples", path.display());
    }

    Ok(WavInput {
        path: path.to_path_buf(),
        samples,
        channels,
        sample_rate: spec.sample_rate,
    })
}

fn main() -> Result<()> {
    let host = cpal::default_host();
    let args = parse_args(&host)?;
    let output_setup = if args.null_output || args.output_wav.is_some() {
        None
    } else {
        let output_device = find_device(host.output_devices()?, &args.output_device, "output")?;
        let output_range = select_config(
            output_device.supported_output_configs()?,
            args.sample_rate,
            args.period_size,
            "output",
        )?;
        let output_channels = output_range.channels() as usize;

        if let Some(channel) = args
            .output_channels
            .iter()
            .find(|&&ch| ch >= output_channels)
        {
            bail!(
                "output channel {} is unavailable; '{}' exposes {} output channels",
                channel + 1,
                args.output_device,
                output_channels
            );
        }

        Some((
            output_device,
            stream_config(&output_range, args.sample_rate, args.period_size),
            output_channels,
        ))
    };
    let monitoring = Arc::new(MonitorStats::default());
    let component_telemetry = Arc::new(ComponentTelemetry::default());

    let (input_stream, input_description, input_channels, mut input_source) =
        if let Some(path) = &args.input_wav {
            let wav = load_wav_input(path, args.input_channel)?;
            if wav.sample_rate != args.sample_rate {
                bail!(
                    "input WAV '{}' is {} Hz, but --sample-rate is {}; use a matching sample rate",
                    wav.path.display(),
                    wav.sample_rate,
                    args.sample_rate
                );
            }
            let description = format!(
                "WAV '{}' channel {}",
                wav.path.display(),
                args.input_channel + 1
            );
            (
                None,
                description,
                wav.channels,
                RuntimeInput::Wav {
                    samples: wav.samples,
                    position: 0,
                },
            )
        } else {
            let input_device_name = args
                .input_device
                .as_ref()
                .context("missing --device, --input-device, or --input-wav")?;
            let input_device = find_device(host.input_devices()?, input_device_name, "input")?;
            let input_range = select_config(
                input_device.supported_input_configs()?,
                args.sample_rate,
                args.period_size,
                "input",
            )?;
            let input_channels = input_range.channels() as usize;
            if args.input_channel >= input_channels {
                bail!(
                    "input channel {} is unavailable; '{}' exposes {} input channels",
                    args.input_channel + 1,
                    input_device_name,
                    input_channels
                );
            }
            let input_config = stream_config(&input_range, args.sample_rate, args.period_size);
            let (mut producer, consumer) = RingBuffer::<f32>::new(args.period_size as usize * 8);
            let input_channel = args.input_channel;
            let monitor_enabled = args.monitor;
            let monitoring_input = monitoring.clone();
            let input_stream = input_device.build_input_stream(
                &input_config,
                move |data: &[f32], _| {
                    for frame in data.chunks_exact(input_channels) {
                        let sample = frame[input_channel];
                        if monitor_enabled {
                            monitoring_input.record_input(sample);
                        }
                        if producer.push(sample).is_err() && monitor_enabled {
                            monitoring_input.record_input_overrun();
                        }
                    }
                },
                |error| eprintln!("input stream error: {error}"),
                None,
            )?;
            (
                Some(input_stream),
                format!(
                    "device '{input_device_name}' channel {}",
                    args.input_channel + 1
                ),
                input_channels,
                RuntimeInput::Live(consumer),
            )
        };

    let monitor_enabled = args.monitor;
    let controls = SharedAmpControls::new(args.controls, args.amp_enabled);
    let shared_device_controls = SharedDeviceControls::new(&args.device_controls);
    let cab_controls = SharedCabControls::new(args.ir);
    let mut device_controls_snapshot = shared_device_controls.load();
    let input_gain = args.input_gain;
    apply_neural_overrides(&args.neural_cells, args.neural_cell_mode)?;
    let mut chain = SignalChain::new(args.sample_rate as f32, args.chain_config.clone());
    let mut speaker = args
        .ir_path
        .as_ref()
        .map(|path| SpeakerStage::from_wav_path(path, args.sample_rate))
        .transpose()?;
    let ir_enabled = speaker.is_some();
    let monitoring_output = monitoring.clone();
    let component_output = component_telemetry.clone();
    let output_stream = if let Some(output_wav) = &args.output_wav {
        render_output_wav(
            output_wav,
            args.render_seconds,
            args.sample_rate,
            &mut input_source,
            &monitoring_output,
            monitor_enabled,
            input_gain,
            &mut chain,
            &controls,
            &shared_device_controls,
            &cab_controls,
            &mut device_controls_snapshot,
            &component_output,
            &mut speaker,
            &args.monitor_log,
            &args.model,
        )?;
        eprintln!(
            "Rendered {:.2} seconds to '{}'",
            args.render_seconds,
            output_wav.display()
        );
        return Ok(());
    } else if let Some((output_device, output_config, output_channels)) = output_setup {
        let selected_outputs = args.output_channels.clone();
        let output_controls = controls.clone();
        let output_device_controls = shared_device_controls.clone();
        let output_cab_controls = cab_controls.clone();
        let mut output_device_controls_snapshot = device_controls_snapshot.clone();
        let stream = output_device.build_output_stream(
            &output_config,
            move |data: &mut [f32], _| {
                for frame in data.chunks_exact_mut(output_channels) {
                    let output = process_one_sample(
                        &mut input_source,
                        &monitoring_output,
                        monitor_enabled,
                        input_gain,
                        &mut chain,
                        &output_controls,
                        &output_device_controls,
                        &output_cab_controls,
                        &mut output_device_controls_snapshot,
                        &component_output,
                        &mut speaker,
                    );
                    frame.fill(0.0);
                    for &channel in &selected_outputs {
                        frame[channel] = output;
                    }
                }
            },
            |error| eprintln!("output stream error: {error}"),
            None,
        )?;
        stream.play()?;
        Some(stream)
    } else {
        if args.input_wav.is_none() {
            bail!(
                "--null-output requires --input-wav so rendering can run without an input device"
            );
        }
        let period_size = args.period_size as usize;
        let sample_rate = args.sample_rate;
        let null_controls = controls.clone();
        let null_device_controls = shared_device_controls.clone();
        let null_cab_controls = cab_controls.clone();
        let mut null_device_controls_snapshot = device_controls_snapshot.clone();
        std::thread::spawn(move || {
            let period = Duration::from_secs_f64(period_size as f64 / sample_rate as f64);
            loop {
                for _ in 0..period_size {
                    let _ = process_one_sample(
                        &mut input_source,
                        &monitoring_output,
                        monitor_enabled,
                        input_gain,
                        &mut chain,
                        &null_controls,
                        &null_device_controls,
                        &null_cab_controls,
                        &mut null_device_controls_snapshot,
                        &component_output,
                        &mut speaker,
                    );
                }
                std::thread::sleep(period);
            }
        });
        None
    };

    if let Some(input_stream) = &input_stream {
        input_stream.play()?;
    }
    if args.monitor {
        spawn_monitor_ui(
            monitoring.clone(),
            component_telemetry.clone(),
            args.model.clone(),
            args.monitor_log.clone(),
            args.chain_config.clone(),
            shared_device_controls.clone(),
            ir_enabled.then_some(cab_controls.clone()),
            controls.clone(),
        );
    }
    eprintln!(
        "Greybound running: {} input channels, {} output channels, {} Hz, {} samples",
        input_channels,
        output_stream
            .as_ref()
            .map_or(0, |_| args.output_channels.len()),
        args.sample_rate,
        args.period_size
    );
    eprintln!("Input source: {input_description}");
    if args.null_output {
        eprintln!("Output sink: null");
    }
    eprintln!(
        "Speaker IR: {}",
        if ir_enabled { "enabled" } else { "disabled" }
    );
    let startup_controls = controls.load();
    eprintln!(
        "Controls: Model {}, Input {:+.1} dB, Volume {:.1}, Bass {:.1}, Treble {:.1}, Cut/Mid {:.1}, Output {:+.1} dB",
        args.model,
        args.input_db,
        startup_controls.volume * 10.0,
        startup_controls.bass * 10.0,
        startup_controls.treble * 10.0,
        startup_controls.cut * 10.0,
        args.output_db
    );
    if let Some(rig_name) = &args.rig_name {
        eprintln!("Rig: {rig_name}");
    }
    if !args.device_controls.is_empty() {
        eprintln!("Rig devices: {} active slot(s)", args.device_controls.len());
    }
    if !args.neural_cells.is_empty() {
        eprintln!(
            "Neural cells: {} component(s), mode {}",
            args.neural_cells.len(),
            neural_cell_mode_name(args.neural_cell_mode)
        );
        for neural_cell in &args.neural_cells {
            eprintln!(
                "  {} -> {}",
                neural_cell.component,
                neural_cell.descriptor_path.display()
            );
        }
    }
    eprintln!("Press Ctrl-C to stop.");

    loop {
        thread::park();
    }
}

#[allow(clippy::too_many_arguments)]
fn render_output_wav(
    path: &Path,
    seconds: f32,
    sample_rate: u32,
    input_source: &mut RuntimeInput,
    monitoring: &MonitorStats,
    monitor_enabled: bool,
    input_gain: f32,
    chain: &mut SignalChain,
    controls: &SharedAmpControls,
    device_controls: &SharedDeviceControls,
    cab_controls: &SharedCabControls,
    device_controls_snapshot: &mut Vec<DeviceSlotControls>,
    component_telemetry: &ComponentTelemetry,
    speaker: &mut Option<SpeakerStage>,
    monitor_log_path: &Path,
    model: &str,
) -> Result<()> {
    if seconds <= 0.0 {
        bail!("--render-seconds must be greater than zero");
    }

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("could not create output WAV '{}'", path.display()))?;
    let mut log = monitor_enabled
        .then(|| RotatingMonitorLog::new(monitor_log_path.to_path_buf(), MONITOR_LOG_LINES));
    let total_samples = (seconds * sample_rate as f32).round() as usize;
    let refresh_samples =
        ((MONITOR_REFRESH.as_secs_f32() * sample_rate as f32).round() as usize).max(1);

    for sample_index in 0..total_samples {
        let output = process_one_sample(
            input_source,
            monitoring,
            monitor_enabled,
            input_gain,
            chain,
            controls,
            device_controls,
            cab_controls,
            device_controls_snapshot,
            component_telemetry,
            speaker,
        );
        writer
            .write_sample(output)
            .with_context(|| format!("could not write output WAV '{}'", path.display()))?;

        if let Some(log) = &mut log {
            if (sample_index + 1) % refresh_samples == 0 || sample_index + 1 == total_samples {
                let stats = monitoring.snapshot_and_reset();
                let component_snapshot =
                    (model == "nox30").then(|| component_telemetry.snapshot_and_reset());
                let timestamp = unix_timestamp();
                let mut log_lines =
                    vec![format!("ts={timestamp} {}", format_audio_monitor(&stats))];
                if let Some(components) = component_snapshot {
                    log_lines.push(format!(
                        "ts={timestamp} {}",
                        format_component_telemetry(components)
                    ));
                }
                log.push_many(log_lines)?;
            }
        }
    }

    writer
        .finalize()
        .with_context(|| format!("could not finalize output WAV '{}'", path.display()))?;
    Ok(())
}

fn process_one_sample(
    input_source: &mut RuntimeInput,
    monitoring: &MonitorStats,
    monitor_enabled: bool,
    input_gain: f32,
    chain: &mut SignalChain,
    controls: &SharedAmpControls,
    device_controls: &SharedDeviceControls,
    cab_controls: &SharedCabControls,
    device_controls_snapshot: &mut Vec<DeviceSlotControls>,
    component_telemetry: &ComponentTelemetry,
    speaker: &mut Option<SpeakerStage>,
) -> f32 {
    let input = input_source.next_sample(monitoring, monitor_enabled) * input_gain;
    device_controls.load_into(device_controls_snapshot);
    let chain_controls = SignalChainControls {
        amp: controls.load(),
        devices: device_controls_snapshot,
    };
    let amp_enabled = controls.load_enabled();
    let amp_output = if amp_enabled {
        chain.process(input, chain_controls)
    } else {
        input
    };
    if monitor_enabled && amp_enabled {
        if let Some(operating_point) = chain.nox30_operating_point() {
            component_telemetry.record_nox30(operating_point);
        }
    }
    let output = speaker.as_mut().map_or(amp_output, |speaker| {
        speaker.process(amp_output, cab_controls.load())
    });
    if monitor_enabled {
        monitoring.record_output(output);
    }
    output
}

fn parse_args(host: &cpal::Host) -> Result<Args> {
    let mut input_device = None;
    let mut output_device = None;
    let mut null_output = false;
    let mut output_wav = None;
    let mut render_seconds = 20.0;
    let mut rig_path = None;
    let mut input_wav = None;
    let mut input_channel = 1;
    let mut output_channels = "1,2".to_owned();
    let mut sample_rate = 48_000;
    let mut period_size = 256;
    let mut input_db = 0.0;
    let mut output_db = -9.0;
    let mut ir_path = None;
    let mut monitor = false;
    let mut monitor_log = PathBuf::from("greybound-monitor.log");
    let mut neural_cells = Vec::new();
    let mut neural_cell_mode = NeuralCellMode::Shadow;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--device" => {
                let name = next_value(&mut args, "--device")?;
                input_device = Some(name.clone());
                output_device = Some(name);
            }
            "--input-device" => input_device = Some(next_value(&mut args, "--input-device")?),
            "--output-device" => output_device = Some(next_value(&mut args, "--output-device")?),
            "--null-output" => null_output = true,
            "--output-wav" => {
                output_wav = Some(PathBuf::from(next_value(&mut args, "--output-wav")?))
            }
            "--render-seconds" => {
                render_seconds = next_value(&mut args, "--render-seconds")?.parse()?
            }
            "--rig" => rig_path = Some(PathBuf::from(next_value(&mut args, "--rig")?)),
            "--input-wav" => input_wav = Some(PathBuf::from(next_value(&mut args, "--input-wav")?)),
            "--input-channel" => {
                input_channel = next_value(&mut args, "--input-channel")?.parse()?
            }
            "--output-channels" => output_channels = next_value(&mut args, "--output-channels")?,
            "--sample-rate" => sample_rate = next_value(&mut args, "--sample-rate")?.parse()?,
            "--period-size" => period_size = next_value(&mut args, "--period-size")?.parse()?,
            "--input-db" => input_db = next_value(&mut args, "--input-db")?.parse()?,
            "--output-db" => output_db = next_value(&mut args, "--output-db")?.parse()?,
            "--ir" => ir_path = Some(PathBuf::from(next_value(&mut args, "--ir")?)),
            "--monitor" => monitor = true,
            "--monitor-log" => monitor_log = PathBuf::from(next_value(&mut args, "--monitor-log")?),
            "--neural-cell" => neural_cells.push(parse_neural_cell_override(&next_value(
                &mut args,
                "--neural-cell",
            )?)?),
            "--neural-cell-mode" => {
                neural_cell_mode =
                    parse_neural_cell_mode(&next_value(&mut args, "--neural-cell-mode")?)?
            }
            "--list-devices" => {
                print_devices(host)?;
                std::process::exit(0);
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ => bail!("unknown argument '{arg}'; use --help"),
        }
    }

    if input_channel == 0 {
        bail!("--input-channel is one-based and must be at least 1");
    }
    if sample_rate == 0 {
        bail!("--sample-rate must be greater than zero");
    }
    if period_size == 0 {
        bail!("--period-size must be greater than zero");
    }
    if !(-60.0..=6.0).contains(&output_db) {
        bail!("--output-db must be between -60 and +6");
    }
    if !(-60.0..=24.0).contains(&input_db) {
        bail!("--input-db must be between -60 and +24");
    }
    if render_seconds <= 0.0 {
        bail!("--render-seconds must be greater than zero");
    }
    let path = rig_path
        .as_ref()
        .context("missing --rig PATH; rig files define amp/pedal topology and controls")?;
    let output_channels = output_channels
        .split(',')
        .map(|value| value.trim().parse::<usize>())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if output_channels.is_empty() || output_channels.contains(&0) {
        bail!("--output-channels must contain one-based channel numbers");
    }
    if output_wav.is_some() && null_output {
        bail!("--output-wav and --null-output are mutually exclusive");
    }
    if !null_output && output_wav.is_none() && output_device.is_none() {
        bail!("missing --device or --output-device");
    }
    if (null_output || output_wav.is_some()) && input_wav.is_none() {
        bail!("--null-output and --output-wav require --input-wav");
    }
    let output_gain = 10.0_f32.powf(output_db / 20.0);
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read rig file '{}'", path.display()))?;
    let rig = RigConfig::from_json5(&text)
        .with_context(|| format!("could not parse rig file '{}'", path.display()))?;
    let model = rig.amp.model.clone();
    let controls = rig.amp_controls(output_gain);
    let amp_enabled = rig.amp_enabled();
    let chain_config = rig.signal_chain_config()?;
    let device_controls = rig.device_controls()?;
    let rig_ir_path = rig.cab_ir_path().map(PathBuf::from);
    let ir_path = ir_path.or(rig_ir_path);
    let ir = ir_path.is_some();
    let rig_name = rig.name.or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
    });

    Ok(Args {
        input_device,
        output_device: output_device.unwrap_or_default(),
        null_output,
        output_wav,
        render_seconds,
        input_wav,
        input_channel: input_channel - 1,
        output_channels: output_channels.into_iter().map(|ch| ch - 1).collect(),
        sample_rate,
        period_size,
        controls,
        amp_enabled,
        chain_config,
        device_controls,
        rig_name,
        input_db,
        input_gain: 10.0_f32.powf(input_db / 20.0),
        output_db,
        ir,
        ir_path,
        monitor,
        monitor_log,
        model,
        neural_cells,
        neural_cell_mode,
    })
}

fn parse_neural_cell_override(value: &str) -> Result<NeuralCellOverride> {
    let (component, descriptor) = value
        .split_once('=')
        .context("--neural-cell expects COMPONENT=DESCRIPTOR, for example nox30.first_stage=lab/models/common-cathode-12ax7-mlp-v1/model.greybound.json")?;
    if component.trim().is_empty() || descriptor.trim().is_empty() {
        bail!("--neural-cell expects non-empty COMPONENT=DESCRIPTOR");
    }
    Ok(NeuralCellOverride {
        component: component.trim().to_owned(),
        descriptor_path: PathBuf::from(descriptor.trim()),
    })
}

fn parse_neural_cell_mode(value: &str) -> Result<NeuralCellMode> {
    match value {
        "shadow" => Ok(NeuralCellMode::Shadow),
        "replace" => Ok(NeuralCellMode::Replace),
        _ => bail!("--neural-cell-mode must be 'shadow' or 'replace'"),
    }
}

fn neural_cell_mode_name(mode: NeuralCellMode) -> &'static str {
    match mode {
        NeuralCellMode::Shadow => "shadow",
        NeuralCellMode::Replace => "replace",
    }
}

fn apply_neural_overrides(overrides: &[NeuralCellOverride], mode: NeuralCellMode) -> Result<()> {
    configure_nox30_first_stage_neural(None, NeuralCellMode::Shadow);
    for override_ in overrides {
        match override_.component.as_str() {
            "nox30.first_stage" => {
                configure_nox30_first_stage_neural(Some(override_.descriptor_path.clone()), mode);
            }
            other => bail!(
                "unsupported --neural-cell component '{}'; supported: nox30.first_stage",
                other
            ),
        }
    }
    Ok(())
}

fn next_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("missing value for {option}"))
}

fn find_device(
    devices: impl Iterator<Item = Device>,
    wanted: &str,
    direction: &str,
) -> Result<Device> {
    devices
        .filter_map(|device| device.name().ok().map(|name| (device, name)))
        .find(|(_, name)| name == wanted)
        .map(|(device, _)| device)
        .with_context(|| {
            format!("could not find {direction} device '{wanted}'; use --list-devices")
        })
}

fn select_config(
    configs: impl Iterator<Item = SupportedStreamConfigRange>,
    sample_rate: u32,
    period_size: u32,
    direction: &str,
) -> Result<SupportedStreamConfigRange> {
    let rate = SampleRate(sample_rate);
    configs
        .filter(|config| config.sample_format() == SampleFormat::F32)
        .find(|config| {
            (config.min_sample_rate()..=config.max_sample_rate()).contains(&rate)
                && match config.buffer_size() {
                    cpal::SupportedBufferSize::Range { min, max } => {
                        (*min..=*max).contains(&period_size)
                    }
                    cpal::SupportedBufferSize::Unknown => true,
                }
        })
        .with_context(|| {
            format!(
                "no f32 {direction} configuration supports {sample_rate} Hz / {period_size} samples"
            )
        })
}

fn stream_config(
    range: &SupportedStreamConfigRange,
    sample_rate: u32,
    period_size: u32,
) -> StreamConfig {
    StreamConfig {
        channels: range.channels(),
        sample_rate: SampleRate(sample_rate),
        buffer_size: BufferSize::Fixed(period_size),
    }
}

fn print_devices(host: &cpal::Host) -> Result<()> {
    eprintln!("Input devices:");
    for device in host.input_devices()? {
        eprintln!("  {}", device.name()?);
    }
    eprintln!("Output devices:");
    for device in host.output_devices()? {
        eprintln!("  {}", device.name()?);
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "Usage: greybound-cli --rig PATH [OPTIONS]\n\
         \n\
         Options:\n\
         \x20 --device NAME             Use the same input and output device\n\
         \x20 --input-device NAME       Input device name\n\
         \x20 --output-device NAME      Output device name\n\
         \x20 --null-output             Run file input through DSP/monitoring without opening an output device\n\
         \x20 --output-wav PATH         Render file input through DSP to a mono float WAV file\n\
         \x20 --render-seconds N        Offline render duration for --output-wav [default: 20]\n\
         \x20 --rig PATH                Load rig topology/controls from a JSON5 rig file\n\
         \x20 --input-wav PATH          Loop a mono/stereo WAV file instead of live input\n\
         \x20 --input-channel N         One-based guitar input [default: 1]\n\
         \x20 --output-channels N,N     One-based monitor outputs [default: 1,2]\n\
         \x20 --sample-rate HZ          Sample rate [default: 48000]\n\
         \x20 --period-size SAMPLES     Buffer size [default: 256]\n\
         \x20 --input-db DB             Interface input calibration [default: 0]\n\
         \x20 --output-db DB            Safety output trim [default: -9]\n\
         \x20 --monitor                 Show interactive VU meters and amp controls\n\
         \x20 --monitor-log PATH        Rotating monitor log [default: greybound-monitor.log]\n\
         \x20 --neural-cell COMPONENT=PATH  Run a neural counterpart for a supported component\n\
         \x20 --neural-cell-mode MODE    Neural mode: shadow or replace [default: shadow]\n\
         \x20 --ir PATH                 Force-enable a speaker IR WAV path, even if the rig has no active cab\n\
         \x20 --list-devices            List CoreAudio devices"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_sample_wav_input_channel() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../lab/references/tone3000-inputs/Brit - Guitar.wav");
        let wav = load_wav_input(&path, 0).unwrap();

        assert_eq!(wav.sample_rate, 48_000);
        assert_eq!(wav.channels, 1);
        assert!(!wav.samples.is_empty());
        assert!(wav.samples.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn formats_component_telemetry_for_monitoring() {
        let telemetry = ComponentTelemetrySnapshot {
            count: 11_025,
            preamp_voltage_avg: 274.2,
            preamp_voltage_min: 270.1,
            phase_inverter_voltage_avg: 296.7,
            phase_inverter_voltage_min: 292.0,
            power_voltage_avg: 314.8,
            power_voltage_min: 306.3,
            power_screen_voltage_avg: 298.4,
            power_screen_voltage_min: 289.6,
            first_stage_current_avg: 0.00124,
            first_stage_current_max: 0.0021,
            phase_inverter_current_avg: 0.0028,
            phase_inverter_current_max: 0.0035,
            power_current_avg: 0.071,
            power_current_max: 0.120,
            power_attack_current_avg: 0.014,
            power_attack_current_max: 0.026,
            power_screen_current_avg: 0.0047,
            power_screen_current_max: 0.0094,
            power_cathode_bias_avg: 9.4,
            power_cathode_bias_max: 10.8,
            transformer_flux_abs_avg: 0.00031,
            transformer_flux_abs_max: 0.0012,
            signal_probe_abs_avg: [0.01, 0.2, 0.18, 0.05, 0.04, 0.03, 0.5, 0.02],
            signal_probe_abs_max: [0.08, 1.2, 0.9, 0.3, 0.2, 0.18, 2.0, 0.12],
            first_stage_shadow_count: 0,
            first_stage_shadow_abs_error_avg: 0.0,
            first_stage_shadow_abs_error_max: 0.0,
        };

        let line = format_component_telemetry(telemetry);

        assert!(line.starts_with("CMP n=11025 rails avg/min pre 274/270"));
        assert!(line.contains("pwr 315/306 scr 298/290 V"));
        assert!(line.contains("pwr 71.0/120.0 atk 14.0/26.0 scr 4.7/9.4 mA"));
        assert!(line.contains("cath avg/max 9.40/10.80 V"));
        assert!(line.contains("flux abs avg/max 0.00031/0.00120"));
        assert!(line.contains("sig abs avg/max V vol 0.01000/0.08000"));
        assert!(line.contains("ot 0.02000/0.12000"));
        assert!(!line.contains("shadow first"));
    }

    #[test]
    fn component_telemetry_formats_shadow_error_when_present() {
        let telemetry = ComponentTelemetrySnapshot {
            count: 128,
            first_stage_shadow_count: 128,
            first_stage_shadow_abs_error_avg: 0.0123,
            first_stage_shadow_abs_error_max: 0.0456,
            ..ComponentTelemetrySnapshot::default()
        };

        let line = format_component_telemetry(telemetry);

        assert!(line.contains("shadow first abs err avg/max 0.01230/0.04560 V n 128"));
    }

    #[test]
    fn parses_neural_cell_override_and_mode() {
        let override_ =
            parse_neural_cell_override("nox30.first_stage=lab/models/cell/model.greybound.json")
                .unwrap();
        assert_eq!(override_.component, "nox30.first_stage");
        assert_eq!(
            override_.descriptor_path,
            PathBuf::from("lab/models/cell/model.greybound.json")
        );
        assert_eq!(
            parse_neural_cell_mode("shadow").unwrap(),
            NeuralCellMode::Shadow
        );
        assert_eq!(
            parse_neural_cell_mode("replace").unwrap(),
            NeuralCellMode::Replace
        );
        assert!(parse_neural_cell_override("nox30.first_stage").is_err());
        assert!(parse_neural_cell_mode("audit").is_err());
    }

    #[test]
    fn component_telemetry_keeps_interval_extrema() {
        let telemetry = ComponentTelemetry::default();
        telemetry.record_nox30(Nox30OperatingPoint {
            input_volume_output_v: 0.1,
            first_stage_output_v: -0.2,
            follower_output_v: 0.3,
            tone_stack_output_v: -0.4,
            preamp_send_v: 0.5,
            phase_inverter_input_v: -0.6,
            phase_inverter_output_v: 0.7,
            power_stage_output_v: -0.8,
            output_transformer_output_v: 0.9,
            preamp_voltage: 280.0,
            phase_inverter_voltage: 300.0,
            power_voltage: 320.0,
            first_stage_plate_current: 0.001,
            first_stage_cathode_voltage: 1.0,
            follower_plate_current: 0.001,
            follower_cathode_voltage: 100.0,
            drive_stage_plate_current: 0.001,
            recovery_stage_plate_current: 0.001,
            first_stage_shadow_output_v: None,
            first_stage_shadow_error_v: None,
            phase_inverter_plate_a_current: 0.001,
            phase_inverter_plate_b_current: 0.001,
            phase_inverter_cathode_voltage: 35.0,
            power_positive_current: 0.020,
            power_negative_current: 0.020,
            power_positive_screen_current: 0.001,
            power_negative_screen_current: 0.001,
            power_screen_voltage: 300.0,
            power_cathode_bias_voltage: 4.0,
            power_attack_current: 0.0,
            transformer_core_flux: -0.0001,
        });
        telemetry.record_nox30(Nox30OperatingPoint {
            input_volume_output_v: -0.2,
            first_stage_output_v: 0.4,
            follower_output_v: -0.6,
            tone_stack_output_v: 0.8,
            preamp_send_v: -1.0,
            phase_inverter_input_v: 1.2,
            phase_inverter_output_v: -1.4,
            power_stage_output_v: 1.6,
            output_transformer_output_v: -1.8,
            preamp_voltage: 260.0,
            phase_inverter_voltage: 285.0,
            power_voltage: 295.0,
            first_stage_plate_current: 0.004,
            first_stage_cathode_voltage: 1.0,
            follower_plate_current: 0.001,
            follower_cathode_voltage: 100.0,
            drive_stage_plate_current: 0.001,
            recovery_stage_plate_current: 0.001,
            first_stage_shadow_output_v: None,
            first_stage_shadow_error_v: None,
            phase_inverter_plate_a_current: 0.004,
            phase_inverter_plate_b_current: 0.003,
            phase_inverter_cathode_voltage: 35.0,
            power_positive_current: 0.080,
            power_negative_current: 0.070,
            power_positive_screen_current: 0.006,
            power_negative_screen_current: 0.005,
            power_screen_voltage: 286.0,
            power_cathode_bias_voltage: 9.0,
            power_attack_current: 0.010,
            transformer_core_flux: 0.002,
        });

        let snapshot = telemetry.snapshot_and_reset();

        assert_eq!(snapshot.count, 2);
        assert_eq!(snapshot.preamp_voltage_min.round(), 260.0);
        assert_eq!(snapshot.power_screen_voltage_min.round(), 286.0);
        assert!((snapshot.power_current_max - 0.1565).abs() < 0.0001);
        assert!((snapshot.power_attack_current_avg - 0.005).abs() < 0.0001);
        assert!((snapshot.power_attack_current_max - 0.010).abs() < 0.0001);
        assert!((snapshot.power_screen_current_max - 0.011).abs() < 0.0001);
        assert!((snapshot.transformer_flux_abs_max - 0.002).abs() < 0.0001);
        assert!((snapshot.signal_probe_abs_avg[0] - 0.15).abs() < 0.0001);
        assert!((snapshot.signal_probe_abs_max[7] - 1.8).abs() < 0.0001);
    }

    #[test]
    fn formats_monitor_dashboard_as_vu_meters() {
        let stats = MonitorSnapshot {
            input_sum_squares: (0.25 * RMS_SCALE) as u64,
            output_sum_squares: (0.01 * RMS_SCALE) as u64,
            input_count: 1,
            output_count: 1,
            input_peak: 0.8,
            output_peak: 0.2,
            input_near_clips: 1,
            output_near_clips: 0,
            input_clips: 0,
            output_clips: 0,
            input_overruns: 2,
            output_underruns: 3,
        };

        let dashboard = format_monitor_dashboard(
            &stats,
            Some(ComponentTelemetrySnapshot {
                count: 11_025,
                preamp_voltage_avg: 274.2,
                preamp_voltage_min: 270.1,
                phase_inverter_voltage_avg: 296.7,
                phase_inverter_voltage_min: 292.0,
                power_voltage_avg: 314.8,
                power_voltage_min: 306.3,
                power_screen_voltage_avg: 298.4,
                power_screen_voltage_min: 289.6,
                first_stage_current_avg: 0.00124,
                first_stage_current_max: 0.0021,
                phase_inverter_current_avg: 0.0028,
                phase_inverter_current_max: 0.0035,
                power_current_avg: 0.071,
                power_current_max: 0.120,
                power_attack_current_avg: 0.014,
                power_attack_current_max: 0.026,
                power_screen_current_avg: 0.0047,
                power_screen_current_max: 0.0094,
                power_cathode_bias_avg: 9.4,
                power_cathode_bias_max: 10.8,
                transformer_flux_abs_avg: 0.00031,
                transformer_flux_abs_max: 0.0012,
                signal_probe_abs_avg: [0.01, 0.2, 0.18, 0.05, 0.04, 0.03, 0.5, 0.02],
                signal_probe_abs_max: [0.08, 1.2, 0.9, 0.3, 0.2, 0.18, 2.0, 0.12],
                first_stage_shadow_count: 0,
                first_stage_shadow_abs_error_avg: 0.0,
                first_stage_shadow_abs_error_max: 0.0,
            }),
            "nox30",
            Path::new("greybound-monitor.log"),
        );

        assert!(dashboard.contains("🎸 Greybound monitor  model nox30"));
        assert!(dashboard.contains("🎚 input  rms"));
        assert!(dashboard.contains("-6.0 dBFS ["));
        assert!(dashboard.contains("🔊 output rms"));
        assert!(dashboard.contains("-20.0 dBFS ["));
        assert!(dashboard.contains("🔋 rails avg/min pre"));
        assert!(dashboard.contains("I avg/max mA"));
        assert!(dashboard.contains("atk  14.0/ 26.0"));
        assert!(dashboard.contains("📍 sig |avg/max| V vol 0.010/0.080"));
        assert!(dashboard.contains("Press Ctrl-C to stop."));
    }

    #[test]
    fn terminal_frames_use_carriage_returns_for_raw_mode() {
        let frame = format_terminal_frame("alpha\nbeta\n");

        assert_eq!(frame, "alpha\r\nbeta\r\n");
        assert!(!frame.contains("alpha\nbeta"));
    }

    #[test]
    fn formats_pedalboard_from_chain_config() {
        let mut config = SignalChainConfig::amp_only("nox30");
        config
            .pre_amp
            .push(DeviceSlotConfig::active(DeviceConfig::Minotaur));
        config
            .pre_amp
            .push(DeviceSlotConfig::bypassed(DeviceConfig::Muffin));
        config
            .fx_loop
            .push(DeviceSlotConfig::active(DeviceConfig::Muffin));
        let controls = [
            DeviceSlotControls::active(DeviceControls::Minotaur(MinotaurControls::default())),
            DeviceSlotControls::active(DeviceControls::Muffin(MuffinControls::default())),
            DeviceSlotControls::bypassed(DeviceControls::Muffin(MuffinControls::default())),
        ];

        let board = format_pedalboard_text(&config, &controls);

        assert!(board.contains("          ┌────────────┐  ┌────────────┐  ┌────────────────┐"));
        assert!(board.contains("          │Minotaur   o│  │Muffin     o│  │AMP Nox30      o│"));
        assert!(board.contains("GTR     ──├────────────┤──├────────────┤──├────────────────┤──OUT"));
        assert!(board.contains("          │            │  │            │  │                │"));
        assert!(board.contains("          │    (O)     │  │    (O)     │  │      (O)       │"));
        assert!(board.contains("FX LOOP PEDALS:"));
        assert!(board.contains("SEND    ──├────────────┤──RETURN"));
        assert!(board.contains("          │            │"));
        assert!(board.contains("          │    (O)     │"));
    }

    #[test]
    fn runtime_slot_controls_override_configured_bypass_state() {
        let slot = DeviceSlotConfig::bypassed(DeviceConfig::Muffin);

        assert!(slot_bypassed(&slot, None));
        assert!(!slot_bypassed(
            &slot,
            Some(&DeviceSlotControls::active(DeviceControls::Muffin(
                MuffinControls::default()
            )))
        ));
        assert!(slot_bypassed(
            &DeviceSlotConfig::active(DeviceConfig::Muffin),
            Some(&DeviceSlotControls::bypassed(DeviceControls::Muffin(
                MuffinControls::default()
            )))
        ));
    }

    #[test]
    fn formats_fx_loop_only_pedals_from_chain_config() {
        let mut config = SignalChainConfig::amp_only("nox30");
        config
            .fx_loop
            .push(DeviceSlotConfig::active(DeviceConfig::Dartford));
        let controls = [DeviceSlotControls::active(DeviceControls::Dartford(
            DartfordControls {
                rate_hz: 5.2,
                depth: 1.0,
                level: 1.0,
                wave: DartfordWave::Sine,
            },
        ))];

        let board = format_pedalboard_text(&config, &controls);

        assert!(board.contains("│AMP Nox30      o│"));
        assert!(board.contains("FX LOOP PEDALS:"));
        assert!(board.contains("│Dartford   o│"));
        assert!(board.contains("SEND    ──├────────────┤──RETURN"));
        assert!(board.contains("│            │"));
    }

    #[test]
    fn formats_cab_after_amp_when_ir_is_available() {
        let config = SignalChainConfig::amp_only("nox30");
        let board = format_pedalboard_text_with_cab(&config, &[], true, Some(true));

        assert!(board.contains("│AMP Nox30      o│  │CAB IR     o│"));
        assert!(board.contains("GTR     ──├────────────────┤──├────────────┤──OUT"));
        assert!(board.contains("│                │  │            │"));
        assert!(board.contains("│      (O)       │  │    (O)     │"));
    }

    #[test]
    fn pedal_hit_areas_select_configured_slots() {
        let mut config = SignalChainConfig::amp_only("nox30");
        config
            .pre_amp
            .push(DeviceSlotConfig::active(DeviceConfig::Minotaur));
        config
            .pre_amp
            .push(DeviceSlotConfig::active(DeviceConfig::Muffin));
        config
            .fx_loop
            .push(DeviceSlotConfig::active(DeviceConfig::Muffin));

        let areas = pedal_hit_areas(
            &config,
            Rect {
                x: 0,
                y: 3,
                width: 100,
                height: 20,
            },
        );

        assert_eq!(pedal_from_mouse(11, 5, &areas), Some(0));
        assert_eq!(pedal_from_mouse(27, 5, &areas), Some(1));
        assert_eq!(pedal_from_mouse(11, 12, &areas), Some(2));
        assert_eq!(pedal_from_mouse(2, 5, &areas), None);

        let switches = footswitch_hit_areas(
            &config,
            Rect {
                x: 0,
                y: 3,
                width: 100,
                height: 20,
            },
        );
        assert_eq!(pedal_from_mouse(16, 8, &switches), Some(0));
        assert_eq!(pedal_from_mouse(32, 8, &switches), Some(1));
        assert_eq!(pedal_from_mouse(11, 8, &switches), None);

        let amp_area = amp_hit_area(
            &config,
            Rect {
                x: 0,
                y: 3,
                width: 100,
                height: 20,
            },
        );
        assert!(rect_contains(amp_area, 43, 5));
        assert_eq!(pedal_from_mouse(43, 5, &areas), None);

        let amp_switch = amp_footswitch_hit_area(
            &config,
            Rect {
                x: 0,
                y: 3,
                width: 100,
                height: 20,
            },
        );
        assert!(rect_contains(amp_switch, 50, 8));
        assert!(!rect_contains(amp_switch, 43, 8));
    }

    #[test]
    fn cab_footswitch_hit_area_follows_amp_box() {
        let mut config = SignalChainConfig::amp_only("nox30");
        config
            .pre_amp
            .push(DeviceSlotConfig::active(DeviceConfig::Minotaur));
        let area = cab_footswitch_hit_area(
            &config,
            Rect {
                x: 0,
                y: 3,
                width: 100,
                height: 20,
            },
        );

        assert!(rect_contains(area, 53, 8));
        assert!(!rect_contains(area, 51, 8));
    }

    #[test]
    fn pedal_hit_areas_include_fx_loop_without_pre_amp_pedals() {
        let mut config = SignalChainConfig::amp_only("nox30");
        config
            .fx_loop
            .push(DeviceSlotConfig::active(DeviceConfig::Dartford));

        let areas = pedal_hit_areas(
            &config,
            Rect {
                x: 0,
                y: 3,
                width: 100,
                height: 20,
            },
        );

        assert_eq!(pedal_from_mouse(11, 12, &areas), Some(0));
        assert_eq!(pedal_from_mouse(11, 5, &areas), None);
    }

    #[test]
    fn selected_pedal_displays_its_controls() {
        let mut config = SignalChainConfig::amp_only("nox30");
        config
            .pre_amp
            .push(DeviceSlotConfig::active(DeviceConfig::Minotaur));
        let controls = [DeviceSlotControls::active(DeviceControls::Minotaur(
            MinotaurControls {
                gain: 0.35,
                treble: 0.55,
                output: 0.65,
            },
        ))];

        let panel = format_pedal_control_panel(Some(0), 0, &config, &controls).unwrap();

        assert!(panel.contains("Pedal controls: Minotaur slot 1"));
        assert!(panel.contains("status    active"));
        assert!(panel.contains("> gain"));
        assert!(panel.contains("3.5"));
        assert!(panel.contains("treble"));
        assert!(panel.contains("5.5"));
        assert!(panel.contains("output"));
        assert!(panel.contains("6.5"));
    }

    #[test]
    fn shared_device_controls_adjust_dartford_depth_at_runtime() {
        let controls = [DeviceSlotControls::active(DeviceControls::Dartford(
            DartfordControls {
                rate_hz: 5.2,
                depth: 0.46,
                level: 1.0,
                wave: DartfordWave::Sine,
            },
        ))];
        let shared = SharedDeviceControls::new(&controls);

        shared.adjust(0, 1, 0.1, 0.0, 1.0);
        let loaded = shared.load();

        match loaded[0].controls {
            DeviceControls::Dartford(controls) => {
                assert!((controls.depth - 0.56).abs() < 1e-6);
            }
            _ => panic!("expected Dartford controls"),
        }
    }

    #[test]
    fn shared_device_controls_adjust_lumen_peak_reduction_at_runtime() {
        let controls = [DeviceSlotControls::active(DeviceControls::Lumen(
            LumenControls {
                peak_reduction: 0.42,
                gain: 0.52,
                emphasis: 0.48,
                mix: 0.86,
            },
        ))];
        let shared = SharedDeviceControls::new(&controls);

        shared.adjust(0, 0, 0.1, 0.0, 1.0);
        let loaded = shared.load();

        match loaded[0].controls {
            DeviceControls::Lumen(controls) => {
                assert!((controls.peak_reduction - 0.52).abs() < 1e-6);
            }
            _ => panic!("expected Lumen controls"),
        }
    }

    #[test]
    fn shared_device_controls_adjust_muon_sensitivity_at_runtime() {
        let controls = [DeviceSlotControls::active(DeviceControls::Muon(
            MuonControls {
                sensitivity: 0.58,
                range: 0.62,
                resonance: 0.46,
                mix: 0.82,
            },
        ))];
        let shared = SharedDeviceControls::new(&controls);

        shared.adjust(0, 0, 0.1, 0.0, 1.0);
        let loaded = shared.load();

        match loaded[0].controls {
            DeviceControls::Muon(controls) => {
                assert!((controls.sensitivity - 0.68).abs() < 1e-6);
            }
            _ => panic!("expected Muon controls"),
        }
    }

    #[test]
    fn shared_device_controls_adjust_jetstream_feedback_at_runtime() {
        let controls = [DeviceSlotControls::active(DeviceControls::Jetstream(
            JetstreamControls {
                manual: 0.44,
                rate_hz: 0.32,
                depth: 0.78,
                feedback: 0.52,
                mix: 0.62,
            },
        ))];
        let shared = SharedDeviceControls::new(&controls);

        shared.adjust(0, 3, 0.1, 0.0, 0.94);
        let loaded = shared.load();

        match loaded[0].controls {
            DeviceControls::Jetstream(controls) => {
                assert!((controls.feedback - 0.62).abs() < 1e-6);
            }
            _ => panic!("expected Jetstream controls"),
        }
    }

    #[test]
    fn shared_device_controls_adjust_celeste_mix_at_runtime() {
        let controls = [DeviceSlotControls::active(DeviceControls::Celeste(
            CelesteControls {
                rate_hz: 0.72,
                depth: 0.72,
                tone: 0.58,
                mix: 0.48,
            },
        ))];
        let shared = SharedDeviceControls::new(&controls);

        shared.adjust(0, 3, 0.1, 0.0, 1.0);
        let loaded = shared.load();

        match loaded[0].controls {
            DeviceControls::Celeste(controls) => {
                assert!((controls.mix - 0.58).abs() < 1e-6);
            }
            _ => panic!("expected Celeste controls"),
        }
    }

    #[test]
    fn shared_device_controls_adjust_brigade_time_at_runtime() {
        let controls = [DeviceSlotControls::active(DeviceControls::Brigade(
            BrigadeControls {
                time_ms: 320.0,
                repeats: 0.46,
                tone: 0.38,
                mix: 0.34,
            },
        ))];
        let shared = SharedDeviceControls::new(&controls);

        shared.adjust(0, 0, 25.0, 60.0, 700.0);
        let loaded = shared.load();

        match loaded[0].controls {
            DeviceControls::Brigade(controls) => {
                assert!((controls.time_ms - 345.0).abs() < 1e-6);
            }
            _ => panic!("expected Brigade controls"),
        }
    }

    #[test]
    fn shared_device_controls_toggle_bypass_at_runtime() {
        let controls = [DeviceSlotControls::active(DeviceControls::Minotaur(
            MinotaurControls::default(),
        ))];
        let shared = SharedDeviceControls::new(&controls);

        shared.toggle_bypass(0);
        assert!(shared.load()[0].bypassed);

        shared.toggle_bypass(0);
        assert!(!shared.load()[0].bypassed);
    }

    #[test]
    fn shared_cab_controls_toggle_ir_at_runtime() {
        let cab = SharedCabControls::new(true);

        cab.toggle();
        assert!(!cab.load());

        cab.toggle();
        assert!(cab.load());
    }

    #[test]
    fn mouse_rows_select_amp_controls_inside_control_panel() {
        let area = Rect {
            x: 0,
            y: 10,
            width: 24,
            height: 9,
        };

        assert_eq!(
            control_from_mouse_row(11, area),
            Some(AmpControlParam::Volume)
        );
        assert_eq!(control_from_mouse_row(17, area), Some(AmpControlParam::Sag));
        assert_eq!(control_from_mouse_row(10, area), None);
        assert_eq!(control_from_mouse_row(19, area), None);
    }

    #[test]
    fn pedal_control_clicks_do_not_select_amp_controls() {
        let area = Rect {
            x: 0,
            y: 10,
            width: 24,
            height: 9,
        };

        assert_eq!(
            amp_control_from_mouse(3, 11, area, None),
            Some(AmpControlParam::Volume)
        );
        assert_eq!(amp_control_from_mouse(3, 11, area, Some(0)), None);
    }

    #[test]
    fn rotating_monitor_log_keeps_latest_lines() {
        let path =
            std::env::temp_dir().join(format!("greybound-monitor-test-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let mut log = RotatingMonitorLog::new(path.clone(), 3);
        log.push_many(["a", "b", "c", "d"].into_iter().map(str::to_owned))
            .unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let lines = text.lines().collect::<Vec<_>>();
        assert_eq!(lines, vec!["b", "c", "d"]);

        let _ = std::fs::remove_file(path);
    }
}
