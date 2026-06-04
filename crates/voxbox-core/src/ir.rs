use anyhow::{bail, Context, Result};
use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::io::Cursor;
use std::sync::Arc;

pub const CONVOLUTION_LATENCY: usize = 256;
const FFT_SIZE: usize = CONVOLUTION_LATENCY * 2;

pub struct SpeakerStage {
    convolver: Option<PartitionedConvolver>,
    dry_delay: [f32; CONVOLUTION_LATENCY],
    dry_pos: usize,
}

impl SpeakerStage {
    pub fn from_embedded_ir(sample_rate: u32) -> Result<Self> {
        Self::new(load_embedded_ir(sample_rate)?)
    }

    pub fn new(ir: Vec<f32>) -> Result<Self> {
        Ok(Self {
            convolver: Some(PartitionedConvolver::new(&ir)?),
            dry_delay: [0.0; CONVOLUTION_LATENCY],
            dry_pos: 0,
        })
    }

    pub fn bypassed() -> Self {
        Self {
            convolver: None,
            dry_delay: [0.0; CONVOLUTION_LATENCY],
            dry_pos: 0,
        }
    }

    #[inline]
    pub fn process(&mut self, input: f32, enabled: bool) -> f32 {
        let dry_output = self.dry_delay[self.dry_pos];
        self.dry_delay[self.dry_pos] = input;
        self.dry_pos = (self.dry_pos + 1) % CONVOLUTION_LATENCY;

        if enabled {
            self.convolver
                .as_mut()
                .map_or(dry_output, |convolver| convolver.process(input))
        } else {
            dry_output
        }
    }

    pub fn reset(&mut self) {
        if let Some(convolver) = &mut self.convolver {
            convolver.reset();
        }
        self.dry_delay.fill(0.0);
        self.dry_pos = 0;
    }
}

struct PartitionedConvolver {
    r2c: Arc<dyn RealToComplex<f32>>,
    c2r: Arc<dyn ComplexToReal<f32>>,
    ir_partitions: Vec<Vec<Complex32>>,
    input_history: Vec<Vec<Complex32>>,
    history_pos: usize,
    input_block: [f32; CONVOLUTION_LATENCY],
    output_block: [f32; CONVOLUTION_LATENCY],
    overlap: [f32; CONVOLUTION_LATENCY],
    block_pos: usize,
    real_buffer: Vec<f32>,
    input_spectrum: Vec<Complex32>,
    output_spectrum: Vec<Complex32>,
    r2c_scratch: Vec<Complex32>,
    c2r_scratch: Vec<Complex32>,
}

impl PartitionedConvolver {
    fn new(ir: &[f32]) -> Result<Self> {
        if ir.is_empty() {
            bail!("speaker IR is empty");
        }

        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(FFT_SIZE);
        let c2r = planner.plan_fft_inverse(FFT_SIZE);
        let spectrum_len = r2c.make_output_vec().len();
        let partition_count = ir.len().div_ceil(CONVOLUTION_LATENCY);
        let mut real_buffer = r2c.make_input_vec();
        let mut spectrum = r2c.make_output_vec();
        let mut r2c_scratch = r2c.make_scratch_vec();
        let c2r_scratch = c2r.make_scratch_vec();
        let mut ir_partitions = Vec::with_capacity(partition_count);

        for partition in ir.chunks(CONVOLUTION_LATENCY) {
            real_buffer.fill(0.0);
            real_buffer[..partition.len()].copy_from_slice(partition);
            r2c.process_with_scratch(&mut real_buffer, &mut spectrum, &mut r2c_scratch)
                .context("could not transform speaker IR")?;
            ir_partitions.push(spectrum.clone());
        }

        Ok(Self {
            r2c,
            c2r,
            ir_partitions,
            input_history: vec![vec![Complex32::default(); spectrum_len]; partition_count],
            history_pos: 0,
            input_block: [0.0; CONVOLUTION_LATENCY],
            output_block: [0.0; CONVOLUTION_LATENCY],
            overlap: [0.0; CONVOLUTION_LATENCY],
            block_pos: 0,
            real_buffer,
            input_spectrum: vec![Complex32::default(); spectrum_len],
            output_spectrum: vec![Complex32::default(); spectrum_len],
            r2c_scratch,
            c2r_scratch,
        })
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = self.output_block[self.block_pos];
        self.input_block[self.block_pos] = input;
        self.block_pos += 1;

        if self.block_pos == CONVOLUTION_LATENCY {
            self.process_block();
            self.block_pos = 0;
        }

        output
    }

    fn process_block(&mut self) {
        self.real_buffer[..CONVOLUTION_LATENCY].copy_from_slice(&self.input_block);
        self.real_buffer[CONVOLUTION_LATENCY..].fill(0.0);
        self.r2c
            .process_with_scratch(
                &mut self.real_buffer,
                &mut self.input_spectrum,
                &mut self.r2c_scratch,
            )
            .expect("preallocated FFT buffers have valid sizes");
        self.input_history[self.history_pos].copy_from_slice(&self.input_spectrum);
        self.output_spectrum.fill(Complex32::default());

        for (partition_idx, ir_spectrum) in self.ir_partitions.iter().enumerate() {
            let history_idx = (self.history_pos + self.input_history.len() - partition_idx)
                % self.input_history.len();
            for ((output, input), ir) in self
                .output_spectrum
                .iter_mut()
                .zip(&self.input_history[history_idx])
                .zip(ir_spectrum)
            {
                *output += *input * *ir;
            }
        }

        self.c2r
            .process_with_scratch(
                &mut self.output_spectrum,
                &mut self.real_buffer,
                &mut self.c2r_scratch,
            )
            .expect("preallocated FFT buffers have valid sizes");
        let normalization = 1.0 / FFT_SIZE as f32;
        for sample_idx in 0..CONVOLUTION_LATENCY {
            self.output_block[sample_idx] =
                self.real_buffer[sample_idx] * normalization + self.overlap[sample_idx];
            self.overlap[sample_idx] =
                self.real_buffer[sample_idx + CONVOLUTION_LATENCY] * normalization;
        }

        self.history_pos = (self.history_pos + 1) % self.input_history.len();
    }

    fn reset(&mut self) {
        for spectrum in &mut self.input_history {
            spectrum.fill(Complex32::default());
        }
        self.history_pos = 0;
        self.input_block.fill(0.0);
        self.output_block.fill(0.0);
        self.overlap.fill(0.0);
        self.block_pos = 0;
    }
}

fn load_embedded_ir(sample_rate: u32) -> Result<Vec<f32>> {
    let bytes: &[u8] = match sample_rate {
        44_100 => include_bytes!(
            "../../../irs/Celestion Vintage30 - Cenzo Townshend Mix/44.1 kHz/200 ms/Cenzo Celestion V30 Mix.wav"
        ),
        48_000 => include_bytes!(
            "../../../irs/Celestion Vintage30 - Cenzo Townshend Mix/48.0 kHz/200 ms/Cenzo Celestion V30 Mix.wav"
        ),
        88_200 => include_bytes!(
            "../../../irs/Celestion Vintage30 - Cenzo Townshend Mix/88.2 kHz/200 ms/Cenzo Celestion V30 Mix.wav"
        ),
        96_000 => include_bytes!(
            "../../../irs/Celestion Vintage30 - Cenzo Townshend Mix/96.0 kHz/200 ms/Cenzo Celestion V30 Mix.wav"
        ),
        _ => bail!(
            "no embedded speaker IR for {sample_rate} Hz; supported rates: 44100, 48000, 88200, 96000"
        ),
    };

    let mut reader =
        hound::WavReader::new(Cursor::new(bytes)).context("could not read speaker IR")?;
    if reader.spec().channels != 1 || reader.spec().sample_rate != sample_rate {
        bail!("embedded speaker IR has an unexpected format");
    }

    reader
        .samples::<i32>()
        .map(|sample| {
            sample
                .map(|value| value as f32 / 8_388_608.0)
                .context("could not decode speaker IR")
        })
        .collect::<Result<Vec<f32>>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_is_delayed_by_one_partition() {
        let mut stage = SpeakerStage::new(vec![1.0]).unwrap();
        let mut output = Vec::new();
        for sample_idx in 0..=CONVOLUTION_LATENCY {
            output.push(stage.process((sample_idx == 0) as u8 as f32, true));
        }

        assert!(output[..CONVOLUTION_LATENCY]
            .iter()
            .all(|sample| sample.abs() < 1e-6));
        assert!((output[CONVOLUTION_LATENCY] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn convolution_preserves_taps_across_partitions() {
        let mut ir = vec![0.0; CONVOLUTION_LATENCY + 2];
        ir[0] = 1.0;
        ir[CONVOLUTION_LATENCY] = 0.5;
        ir[CONVOLUTION_LATENCY + 1] = -0.25;
        let mut stage = SpeakerStage::new(ir.clone()).unwrap();
        let output: Vec<_> = (0..CONVOLUTION_LATENCY + ir.len())
            .map(|sample_idx| stage.process((sample_idx == 0) as u8 as f32, true))
            .collect();

        for (tap_idx, tap) in ir.iter().enumerate() {
            assert!((output[CONVOLUTION_LATENCY + tap_idx] - tap).abs() < 1e-5);
        }
    }

    #[test]
    fn embedded_ir_matches_supported_rate() {
        assert!(!load_embedded_ir(48_000).unwrap().is_empty());
        assert!(load_embedded_ir(32_000).is_err());
    }
}
