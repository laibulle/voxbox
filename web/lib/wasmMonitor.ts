import type { GreyboundNox30 } from "./greybound-wasm/greybound_wasm";
import type { AmpControlId, RigPreset, RuntimeConfig } from "./rigs";
import { createNox30WasmEngine, applyNox30RigControls, applyNox30SpeakerIr } from "./wasmEngine";
import type { MonitorStats } from "./simulation";

type AmpValues = Record<AmpControlId, number>;

export type DecodedMonoAudio = {
  sampleRate: number;
  samples: Float32Array;
};

export type WasmRenderState = {
  engine: GreyboundNox30;
  input: DecodedMonoAudio;
  position: number;
};

export type WasmAudioBlock = {
  input: Float32Array;
  output: Float32Array;
  stats: MonitorStats;
};

export async function decodeMonoWav(url: string, sampleRateHint = 48_000): Promise<DecodedMonoAudio> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Could not fetch ${url}: ${response.status}`);
  }
  const data = await response.arrayBuffer();
  const AudioContextCtor = window.AudioContext || window.webkitAudioContext;
  const context = new AudioContextCtor({ sampleRate: sampleRateHint });
  try {
    const decoded = await context.decodeAudioData(data.slice(0));
    return {
      sampleRate: decoded.sampleRate,
      samples: decoded.getChannelData(0).slice(),
    };
  } finally {
    await context.close();
  }
}

async function fetchBytes(url: string): Promise<Uint8Array> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Could not fetch ${url}: ${response.status}`);
  }
  return new Uint8Array(await response.arrayBuffer());
}

export async function createWasmRenderState(
  options: {
    sampleRate: number;
    inputUrl: string;
    irUrl: string | null;
    rig: RigPreset;
    outputGain: number;
  },
): Promise<WasmRenderState> {
  const { sampleRate, inputUrl, irUrl, rig, outputGain } = options;
  const [engine, input, irBytes] = await Promise.all([
    createNox30WasmEngine(sampleRate, rig, outputGain),
    decodeMonoWav(inputUrl, sampleRate),
    irUrl ? fetchBytes(irUrl) : Promise.resolve(null),
  ]);
  if (irBytes) {
    engine.set_ir_wav_bytes(irBytes);
  }
  return {
    engine,
    input,
    position: 0,
  };
}

export function renderWasmMonitorBlock(
  state: WasmRenderState,
  rig: RigPreset,
  ampValues: AmpValues,
  runtime: RuntimeConfig,
  blockSize = 4096,
): MonitorStats {
  return renderWasmAudioBlock(state, rig, ampValues, runtime, blockSize).stats;
}

export function renderWasmAudioBlock(
  state: WasmRenderState,
  rig: RigPreset,
  ampValues: AmpValues,
  runtime: RuntimeConfig,
  blockSize = 4096,
): WasmAudioBlock {
  const inputBlock = nextInputBlock(state, blockSize);
  const inputGain = Math.pow(10, runtime.inputDb / 20);
  for (let index = 0; index < inputBlock.length; index += 1) {
    inputBlock[index] *= inputGain;
  }

  applyNox30RigControls(state.engine, rig, Math.pow(10, runtime.outputDb / 20));
  applyNox30SpeakerIr(state.engine, runtime.speakerIr);
  const output = state.engine.process_block(inputBlock);
  return {
    input: inputBlock,
    output,
    stats: monitorStatsFromAudio(inputBlock, output, rig, runtime),
  };
}

function nextInputBlock(state: WasmRenderState, blockSize: number): Float32Array {
  const output = new Float32Array(blockSize);
  const source = state.input.samples;
  if (source.length === 0) {
    return output;
  }
  for (let index = 0; index < blockSize; index += 1) {
    output[index] = source[state.position];
    state.position = (state.position + 1) % source.length;
  }
  return output;
}

function monitorStatsFromAudio(
  input: Float32Array,
  output: Float32Array,
  rig: RigPreset,
  runtime: RuntimeConfig,
): MonitorStats {
  const inputLevels = levels(input);
  const outputLevels = levels(output);
  const activePedals = rig.pedals.filter((pedal) => !pedal.bypassed).length;
  const drive = rig.amp.drive;
  const volume = rig.amp.volume;
  const sag = rig.amp.sag;
  const outputPeak = outputLevels.peak;
  const stress = Math.max(0, outputPeak - 0.75);

  return {
    inputRms: inputLevels.rms,
    inputPeak: inputLevels.peak,
    outputRms: outputLevels.rms,
    outputPeak,
    inputNearClips: countAbove(input, 0.98),
    inputClips: countAbove(input, 1.0),
    outputNearClips: countAbove(output, 0.98),
    outputClips: countAbove(output, 1.0),
    inputOverruns: 0,
    outputUnderruns: 0,
    rails: {
      preampAvg: 313 - drive * 9 - stress * 18,
      preampMin: 310 - drive * 18 - stress * 36,
      piAvg: 321 - drive * 6 - stress * 14,
      piMin: 319 - drive * 14 - stress * 30,
      powerAvg: 330 - sag * 34 - stress * 24,
      powerMin: 327 - sag * 58 - stress * 42,
      screenAvg: 300 - sag * 28 - stress * 18,
      screenMin: 297 - sag * 44 - stress * 34,
    },
    currents: {
      firstAvg: 0.22 + volume * 0.15 + inputLevels.rms * 0.8,
      firstMax: 0.28 + volume * 0.3 + inputLevels.peak * 1.2,
      piAvg: 0.18 + drive * 0.1 + outputLevels.rms * 0.2,
      piMax: 0.22 + drive * 0.2 + outputLevels.peak * 0.25,
      powerAvg: 22 + outputLevels.rms * 20 + sag * 5,
      powerMax: 28 + outputLevels.peak * 24 + sag * 8,
      attackAvg: outputLevels.rms * 18 + activePedals * 0.4,
      attackMax: outputLevels.peak * 28 + activePedals,
      screenAvg: 1.1 + sag * 0.5,
      screenMax: 1.3 + sag * 0.8 + stress,
    },
    cathodeAvg: 3.1 + drive * 0.8 + outputLevels.rms,
    cathodeMax: 3.3 + drive * 1.2 + outputLevels.peak,
    fluxAvg: outputLevels.rms * 0.0005,
    fluxMax: outputLevels.peak * 0.0008,
    probes: probeStats(input, output),
  };
}

function levels(samples: Float32Array) {
  let sum = 0.0;
  let peak = 0.0;
  for (const sample of samples) {
    const abs = Math.abs(sample);
    sum += sample * sample;
    if (abs > peak) peak = abs;
  }
  return {
    rms: Math.sqrt(sum / Math.max(1, samples.length)),
    peak,
  };
}

function countAbove(samples: Float32Array, threshold: number) {
  let count = 0;
  for (const sample of samples) {
    if (Math.abs(sample) >= threshold) count += 1;
  }
  return count;
}

function probeStats(input: Float32Array, output: Float32Array) {
  const inputLevels = levels(input);
  const outputLevels = levels(output);
  const points = [
    ["in", inputLevels.rms, inputLevels.peak],
    ["amp", outputLevels.rms * 0.35, outputLevels.peak * 0.35],
    ["tone", outputLevels.rms * 0.55, outputLevels.peak * 0.55],
    ["send", outputLevels.rms * 0.8, outputLevels.peak * 0.8],
    ["out", outputLevels.rms, outputLevels.peak],
  ] as const;
  return points.map(([label, avg, max]) => ({ label, avg, max }));
}

declare global {
  interface Window {
    webkitAudioContext?: typeof AudioContext;
  }
}
