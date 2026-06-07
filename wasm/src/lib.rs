use greybound::{AmpControls, DeviceSlotControls, RigConfig, SignalChain, SignalChainConfig, SignalChainControls};
use js_sys::Float32Array;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct GreyboundNox30 {
    chain: SignalChain,
    controls: AmpControls,
    amp_enabled: bool,
    device_controls: Vec<DeviceSlotControls>,
}

#[wasm_bindgen]
impl GreyboundNox30 {
    #[wasm_bindgen(constructor)]
    pub fn new(sample_rate: f32) -> Self {
        Self {
            chain: SignalChain::new(sample_rate, SignalChainConfig::amp_only("nox30")),
            controls: default_controls(),
            amp_enabled: true,
            device_controls: Vec::new(),
        }
    }

    #[wasm_bindgen(js_name = fromRigJson)]
    pub fn from_rig_json(sample_rate: f32, rig_json: &str, output_gain: f32) -> Result<Self, JsValue> {
        let rig = RigConfig::from_json5(rig_json).map_err(|error| JsValue::from_str(&error.to_string()))?;
        let chain_config = rig
            .signal_chain_config()
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let device_controls = rig
            .device_controls()
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        Ok(Self {
            chain: SignalChain::new(sample_rate, chain_config),
            controls: rig.amp_controls(output_gain),
            amp_enabled: rig.amp_enabled(),
            device_controls,
        })
    }

    pub fn reset(&mut self) {
        self.chain.reset();
    }

    pub fn set_amp_controls(
        &mut self,
        volume: f32,
        bass: f32,
        treble: f32,
        cut: f32,
        drive: f32,
        presence: f32,
        sag: f32,
        output: f32,
    ) {
        self.controls = AmpControls {
            volume: clamp_unit(volume),
            bass: clamp_unit(bass),
            treble: clamp_unit(treble),
            cut: clamp_unit(cut),
            drive: clamp_unit(drive),
            presence: clamp_unit(presence),
            sag: clamp_unit(sag),
            output: output.clamp(0.0, 2.0),
        };
    }

    pub fn set_amp_enabled(&mut self, enabled: bool) {
        self.amp_enabled = enabled;
    }

    pub fn set_device_bypassed(&mut self, slot_index: usize, bypassed: bool) {
        if let Some(slot) = self.device_controls.get_mut(slot_index) {
            slot.bypassed = bypassed;
        }
    }

    pub fn process_sample(&mut self, input: f32) -> f32 {
        let controls = SignalChainControls {
            amp: self.controls,
            devices: &self.device_controls,
        };
        if self.amp_enabled {
            self.chain.process(input, controls)
        } else {
            input
        }
    }

    pub fn process_block(&mut self, input: &Float32Array) -> Float32Array {
        let mut input_samples = vec![0.0; input.length() as usize];
        input.copy_to(&mut input_samples);

        let mut output_samples = Vec::with_capacity(input_samples.len());
        for sample in input_samples {
            output_samples.push(self.process_sample(sample));
        }

        Float32Array::from(output_samples.as_slice())
    }

    pub fn latency_samples(&self) -> usize {
        greybound::amp::AMP_LATENCY
    }
}

#[wasm_bindgen]
pub fn greybound_wasm_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

fn default_controls() -> AmpControls {
    AmpControls {
        volume: 0.55,
        bass: 0.5,
        treble: 0.6,
        cut: 0.35,
        output: 1.0,
        drive: 0.0,
        presence: 0.0,
        sag: 0.0,
    }
}

fn clamp_unit(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}
