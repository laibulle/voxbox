export type AmpControlId =
  | "volume"
  | "bass"
  | "treble"
  | "cut"
  | "drive"
  | "presence"
  | "sag";

export type PedalSection = "pre" | "fx" | "post";

export type Pedal = {
  id: string;
  label: string;
  device: string;
  section: PedalSection;
  bypassed: boolean;
  color: string;
  controls: Record<string, number | string>;
};

export type RigPreset = {
  id: string;
  name: string;
  file: string;
  model: "nox30";
  cabEnabled: boolean;
  cableCapacitancePf: number;
  ampBypassed: boolean;
  amp: Record<AmpControlId, number>;
  pedals: Pedal[];
};

export const ampControls: { id: AmpControlId; label: string }[] = [
  { id: "volume", label: "Volume" },
  { id: "bass", label: "Bass" },
  { id: "treble", label: "Treble" },
  { id: "cut", label: "Cut" },
  { id: "drive", label: "Drive" },
  { id: "presence", label: "Presence" },
  { id: "sag", label: "Sag" },
];

const baseAmp: Record<AmpControlId, number> = {
  volume: 0.42,
  bass: 0.6,
  treble: 0.54,
  cut: 0.42,
  drive: 0.12,
  presence: 0.24,
  sag: 0.25,
};

const pedalColors: Record<string, string> = {
  lumen: "#6d7f3a",
  muon: "#d6aa30",
  muffin: "#653e88",
  minotaur: "#d7962b",
  monarch: "#147d7c",
  "godess-one": "#d75f2e",
  dartford: "#4c91c6",
  tron: "#365eb7",
  jetstream: "#7c58b5",
  celeste: "#81b9c9",
  brigade: "#4c9d83",
  springfield: "#9c6a34",
};

const pedalLabels: Record<string, string> = {
  lumen: "Lumen",
  muon: "Muon",
  muffin: "Muffin",
  minotaur: "Minotaur",
  monarch: "Monarch",
  "godess-one": "Godess One",
  dartford: "Dartford",
  tron: "Tron",
  jetstream: "Jetstream",
  celeste: "Celeste",
  brigade: "Brigade",
  springfield: "Springfield",
};

function pedal(device: string, section: PedalSection, controls: Record<string, number | string>) {
  return {
    id: `${section}-${device}`,
    label: pedalLabels[device],
    device,
    section,
    bypassed: false,
    color: pedalColors[device],
    controls,
  };
}

type PresetOverrides = Partial<Omit<RigPreset, "id" | "name" | "file" | "model" | "amp">> & {
  amp?: Partial<Record<AmpControlId, number>>;
};

function preset(id: string, overrides: PresetOverrides): RigPreset {
  return {
    id,
    name: id,
    file: `rigs/${id}.json5`,
    model: "nox30",
    cabEnabled: false,
    cableCapacitancePf: 470,
    ampBypassed: false,
    pedals: [],
    ...overrides,
    amp: { ...baseAmp, ...(overrides.amp ?? {}) },
  };
}

export const rigPresets: RigPreset[] = [
  preset("nox30-clean", {}),
  preset("nox30-edge", { amp: { volume: 0.54, drive: 0.2, cut: 0.38 } }),
  preset("nox30-driven", { amp: { volume: 0.7, drive: 0.45, sag: 0.42 } }),
  preset("nox30-dimed", { amp: { volume: 0.92, drive: 0.72, sag: 0.63, cut: 0.55 } }),
  preset("nox30-amp-bypassed", { ampBypassed: true }),
  preset("nox30-all-pedals-bypassed", {
    pedals: [
      { ...pedal("lumen", "pre", { peak_reduction: 0.66, gain: 0.52, emphasis: 0.48, mix: 0.86 }), bypassed: true },
      { ...pedal("muon", "pre", { sensitivity: 0.64, range: 0.68, resonance: 0.52, mix: 0.86 }), bypassed: true },
      { ...pedal("tron", "pre", { rate_hz: 0.68, depth: 0.9, feedback: 0.62, mix: 0.82 }), bypassed: true },
      { ...pedal("jetstream", "pre", { manual: 0.44, rate_hz: 0.32, depth: 0.78, feedback: 0.52, mix: 0.62 }), bypassed: true },
      { ...pedal("muffin", "pre", { sustain: 0.7, tone: 0.46, level: 0.45 }), bypassed: true },
      { ...pedal("minotaur", "pre", { gain: 0.42, treble: 0.61, output: 0.58 }), bypassed: true },
      { ...pedal("monarch", "pre", { gain: 0.48, tone: 0.57, output: 0.62 }), bypassed: true },
      { ...pedal("godess-one", "pre", { distortion: 0.64, tone: 0.47, level: 0.52, mode: "custom" }), bypassed: true },
      { ...pedal("celeste", "fx", { rate_hz: 0.72, depth: 0.72, tone: 0.58, mix: 0.48 }), bypassed: true },
      { ...pedal("brigade", "fx", { time_ms: 320.0, repeats: 0.46, tone: 0.38, mix: 0.34 }), bypassed: true },
      { ...pedal("dartford", "fx", { rate_hz: 10.2, depth: 1.0, level: 1.0, wave: "sine" }), bypassed: true },
      { ...pedal("springfield", "fx", { dwell: 0.48, tone: 0.58, mix: 0.26 }), bypassed: true },
    ],
    amp: { volume: 0.58, bass: 0.56, treble: 0.58, cut: 0.44, drive: 0.24, presence: 0.34, sag: 0.46 },
  }),
  preset("lumen-nox30", {
    pedals: [pedal("lumen", "pre", { peak_reduction: 0.45, gain: 0.52, emphasis: 0.56, mix: 0.9 })],
  }),
  preset("muon-nox30", {
    pedals: [pedal("muon", "pre", { sensitivity: 0.58, range: 0.5, resonance: 0.4, mix: 1 })],
  }),
  preset("muffin-nox30", {
    pedals: [pedal("muffin", "pre", { sustain: 0.7, tone: 0.48, level: 0.58 })],
    amp: { volume: 0.36, drive: 0.18 },
  }),
  preset("minotaur-nox30", {
    pedals: [pedal("minotaur", "pre", { gain: 0.32, treble: 0.56, output: 0.66 })],
  }),
  preset("monarch-nox30", {
    pedals: [pedal("monarch", "pre", { gain: 0.46, tone: 0.53, output: 0.62 })],
  }),
  preset("godess-one-nox30", {
    pedals: [pedal("godess-one", "pre", { distortion: 0.64, tone: 0.5, level: 0.55, mode: "standard" })],
  }),
  preset("dartford-nox30", {
    pedals: [pedal("dartford", "pre", { rate_hz: 0.32, depth: 0.6, level: 0.58, wave: "sine" })],
  }),
  preset("tron-nox30", {
    pedals: [pedal("tron", "fx", { rate_hz: 0.24, depth: 0.54, feedback: 0.38, mix: 0.45 })],
  }),
  preset("jetstream-nox30", {
    pedals: [pedal("jetstream", "fx", { manual: 0.5, rate_hz: 0.3, depth: 0.58, feedback: 0.36, mix: 0.42 })],
  }),
  preset("celeste-nox30", {
    pedals: [pedal("celeste", "fx", { rate_hz: 0.28, depth: 0.52, tone: 0.5, mix: 0.4 })],
  }),
  preset("brigade-nox30", {
    pedals: [pedal("brigade", "fx", { time_ms: 0.44, repeats: 0.34, tone: 0.45, mix: 0.28 })],
  }),
  preset("springfield-nox30", {
    pedals: [pedal("springfield", "post", { dwell: 0.4, tone: 0.52, mix: 0.28 })],
  }),
];

export type RuntimeConfig = {
  device: string;
  inputDevice: string;
  outputDevice: string;
  inputChannel: number;
  outputChannels: string;
  sampleRate: number;
  periodSize: number;
  inputDb: number;
  outputDb: number;
  inputWav: string;
  inputSourceUrl: string;
  outputWav: string;
  renderSeconds: number;
  monitor: boolean;
  nullOutput: boolean;
  speakerIr: boolean;
  irSourceUrl: string;
  monitorLog: string;
  neuralCell: string;
  neuralCellMode: "shadow" | "replace";
};

export const defaultRuntimeConfig: RuntimeConfig = {
  device: "macOS default",
  inputDevice: "",
  outputDevice: "",
  inputChannel: 1,
  outputChannels: "1,2",
  sampleRate: 48000,
  periodSize: 256,
  inputDb: 0,
  outputDb: -9,
  inputWav: "",
  inputSourceUrl: "https://raw.githubusercontent.com/tone-3000/neural-amp-modeler-wasm/main/ui/public/inputs/Brit%20-%20Guitar.wav",
  outputWav: "",
  renderSeconds: 20,
  monitor: true,
  nullOutput: false,
  speakerIr: true,
  irSourceUrl: "https://raw.githubusercontent.com/tone-3000/neural-amp-modeler-wasm/main/ui/public/irs/celestion.wav",
  monitorLog: "greybound-monitor.log",
  neuralCell: "",
  neuralCellMode: "shadow",
};
