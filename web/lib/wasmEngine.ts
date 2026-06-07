import type { GreyboundNox30 } from "./greybound-wasm/greybound_wasm";
import type { AmpControlId, Pedal, RigPreset } from "./rigs";

type GreyboundWasmModule = typeof import("./greybound-wasm/greybound_wasm");
type AmpValues = Record<AmpControlId, number>;

let wasmModulePromise: Promise<GreyboundWasmModule> | null = null;

export async function loadGreyboundWasm(): Promise<GreyboundWasmModule> {
  if (!wasmModulePromise) {
    wasmModulePromise = import("./greybound-wasm/greybound_wasm").then(async (module) => {
      await module.default();
      return module;
    });
  }
  return wasmModulePromise;
}

export async function createNox30WasmEngine(sampleRate: number, rig?: RigPreset, outputGain = 1.0): Promise<GreyboundNox30> {
  const module = await loadGreyboundWasm();
  if (!rig) {
    return new module.GreyboundNox30(sampleRate);
  }
  const factory = module.GreyboundNox30 as typeof module.GreyboundNox30 & {
    fromRigJson: (sampleRate: number, rigJson: string, outputGain: number) => GreyboundNox30;
  };
  return factory.fromRigJson(sampleRate, rigToJson(rig), outputGain);
}

export function applyNox30AmpControls(engine: GreyboundNox30, values: AmpValues, output = 1.0) {
  engine.set_amp_controls(
    values.volume,
    values.bass,
    values.treble,
    values.cut,
    values.drive,
    values.presence,
    values.sag,
    output,
  );
}

export function applyNox30RigBypass(engine: GreyboundNox30, rig: RigPreset) {
  engine.set_amp_enabled(!rig.ampBypassed);
  orderedRigPedals(rig).forEach((pedal, index) => {
    engine.set_device_bypassed(index, pedal.bypassed);
  });
}

export function applyNox30SpeakerIr(engine: GreyboundNox30, enabled: boolean) {
  engine.set_speaker_enabled(enabled);
}

function rigToJson(rig: RigPreset) {
  const preAmp = rig.pedals.filter((pedal) => pedal.section === "pre");
  const fxLoop = rig.pedals.filter((pedal) => pedal.section === "fx");
  const postAmp = rig.pedals.filter((pedal) => pedal.section === "post");
  return JSON.stringify({
    name: rig.name,
    chain: {
      cable_capacitance_pf: rig.cableCapacitancePf,
    },
    pre_amp: preAmp.map(pedalToRigSlot),
    fx_loop: fxLoop.map(pedalToRigSlot),
    post_amp: postAmp.map(pedalToRigSlot),
    amp: {
      model: rig.model,
      bypassed: rig.ampBypassed,
      controls: rig.amp,
    },
    cab: {
      ir: "web-selected-ir",
      bypassed: !rig.cabEnabled,
    },
  });
}

function pedalToRigSlot(pedal: Pedal) {
  return {
    id: pedal.id,
    device: pedal.device,
    bypassed: pedal.bypassed,
    controls: pedal.controls,
  };
}

function orderedRigPedals(rig: RigPreset) {
  return [
    ...rig.pedals.filter((pedal) => pedal.section === "pre"),
    ...rig.pedals.filter((pedal) => pedal.section === "fx"),
    ...rig.pedals.filter((pedal) => pedal.section === "post"),
  ];
}
