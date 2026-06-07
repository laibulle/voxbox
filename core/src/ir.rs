use anyhow::{bail, Context, Result};
use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

pub const CONVOLUTION_LATENCY: usize = 0;
const CONVOLUTION_PARTITION_SIZE: usize = 256;
const FFT_SIZE: usize = CONVOLUTION_PARTITION_SIZE * 2;

pub struct SpeakerStage {
    head: Option<DirectConvolver>,
    tail: Option<PartitionedConvolver>,
}

impl SpeakerStage {
    pub fn from_embedded_ir(sample_rate: u32) -> Result<Self> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../lab/references/tone3000-irs/celestion.wav");
        Self::from_wav_path(path, sample_rate)
    }

    pub fn from_wav_path(path: impl AsRef<Path>, sample_rate: u32) -> Result<Self> {
        Self::new(load_wav_ir(path.as_ref(), sample_rate)?)
    }

    pub fn from_wav_bytes(bytes: &[u8], sample_rate: u32) -> Result<Self> {
        Self::new(decode_wav_ir(bytes, sample_rate)?)
    }

    pub fn new(ir: Vec<f32>) -> Result<Self> {
        if ir.is_empty() {
            bail!("speaker IR is empty");
        }
        let head_len = ir.len().min(CONVOLUTION_PARTITION_SIZE);
        let head = DirectConvolver::new(&ir[..head_len]);
        let tail = (ir.len() > CONVOLUTION_PARTITION_SIZE)
            .then(|| PartitionedConvolver::new(&ir[CONVOLUTION_PARTITION_SIZE..]))
            .transpose()?;

        Ok(Self {
            head: Some(head),
            tail,
        })
    }

    pub fn bypassed() -> Self {
        Self {
            head: None,
            tail: None,
        }
    }

    #[inline]
    pub fn process(&mut self, input: f32, enabled: bool) -> f32 {
        if !enabled {
            return input;
        }
        let head = self
            .head
            .as_mut()
            .map_or(input, |convolver| convolver.process(input));
        let tail = self
            .tail
            .as_mut()
            .map_or(0.0, |convolver| convolver.process(input));
        head + tail
    }

    pub fn reset(&mut self) {
        if let Some(convolver) = &mut self.head {
            convolver.reset();
        }
        if let Some(convolver) = &mut self.tail {
            convolver.reset();
        }
    }
}

struct DirectConvolver {
    taps: Vec<f32>,
    delay: Vec<f32>,
    pos: usize,
}

impl DirectConvolver {
    fn new(taps: &[f32]) -> Self {
        Self {
            taps: taps.to_vec(),
            delay: vec![0.0; taps.len()],
            pos: 0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        self.delay[self.pos] = input;
        let mut output = 0.0;
        let mut delay_idx = self.pos;
        for tap in &self.taps {
            output += *tap * self.delay[delay_idx];
            delay_idx = if delay_idx == 0 {
                self.delay.len() - 1
            } else {
                delay_idx - 1
            };
        }
        self.pos = (self.pos + 1) % self.delay.len();
        output
    }

    fn reset(&mut self) {
        self.delay.fill(0.0);
        self.pos = 0;
    }
}

struct PartitionedConvolver {
    r2c: Arc<dyn RealToComplex<f32>>,
    c2r: Arc<dyn ComplexToReal<f32>>,
    ir_partitions: Vec<Vec<Complex32>>,
    input_history: Vec<Vec<Complex32>>,
    history_pos: usize,
    input_block: [f32; CONVOLUTION_PARTITION_SIZE],
    output_block: [f32; CONVOLUTION_PARTITION_SIZE],
    overlap: [f32; CONVOLUTION_PARTITION_SIZE],
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
        let partition_count = ir.len().div_ceil(CONVOLUTION_PARTITION_SIZE);
        let mut real_buffer = r2c.make_input_vec();
        let mut spectrum = r2c.make_output_vec();
        let mut r2c_scratch = r2c.make_scratch_vec();
        let c2r_scratch = c2r.make_scratch_vec();
        let mut ir_partitions = Vec::with_capacity(partition_count);

        for partition in ir.chunks(CONVOLUTION_PARTITION_SIZE) {
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
            input_block: [0.0; CONVOLUTION_PARTITION_SIZE],
            output_block: [0.0; CONVOLUTION_PARTITION_SIZE],
            overlap: [0.0; CONVOLUTION_PARTITION_SIZE],
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

        if self.block_pos == CONVOLUTION_PARTITION_SIZE {
            self.process_block();
            self.block_pos = 0;
        }

        output
    }

    fn process_block(&mut self) {
        self.real_buffer[..CONVOLUTION_PARTITION_SIZE].copy_from_slice(&self.input_block);
        self.real_buffer[CONVOLUTION_PARTITION_SIZE..].fill(0.0);
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
        for sample_idx in 0..CONVOLUTION_PARTITION_SIZE {
            self.output_block[sample_idx] =
                self.real_buffer[sample_idx] * normalization + self.overlap[sample_idx];
            self.overlap[sample_idx] =
                self.real_buffer[sample_idx + CONVOLUTION_PARTITION_SIZE] * normalization;
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

fn load_wav_ir(path: &Path, sample_rate: u32) -> Result<Vec<f32>> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("could not read reference speaker IR at {}", path.display()))?;
    decode_wav_ir(&bytes, sample_rate)
}

fn decode_wav_ir(bytes: &[u8], sample_rate: u32) -> Result<Vec<f32>> {
    let mut reader =
        hound::WavReader::new(Cursor::new(bytes)).context("could not decode speaker IR WAV")?;
    if reader.spec().channels != 1 || reader.spec().sample_rate != sample_rate {
        bail!("reference speaker IR has an unexpected format");
    }

    reader
        .samples::<i32>()
        .map(|sample| {
            sample
                .map(|value| value as f32 / 8_388_608.0)
                .context("could not decode speaker IR")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_starts_without_convolution_latency() {
        let mut stage = SpeakerStage::new(vec![1.0]).unwrap();
        let mut output = Vec::new();
        for sample_idx in 0..4 {
            output.push(stage.process((sample_idx == 0) as u8 as f32, true));
        }

        assert!((output[0] - 1.0).abs() < 1e-6);
        assert!(output[1..].iter().all(|sample| sample.abs() < 1e-6));
    }

    #[test]
    fn bypassed_ir_preserves_dry_path_without_latency() {
        let mut stage = SpeakerStage::new(vec![0.25]).unwrap();
        let input = [0.4, -0.2, 0.1, 0.0];
        let output: Vec<_> = input
            .iter()
            .map(|sample| stage.process(*sample, false))
            .collect();

        assert_eq!(output, input);
    }

    #[test]
    fn convolution_preserves_taps_across_partitions() {
        let mut ir = vec![0.0; CONVOLUTION_PARTITION_SIZE + 2];
        ir[0] = 1.0;
        ir[CONVOLUTION_PARTITION_SIZE] = 0.5;
        ir[CONVOLUTION_PARTITION_SIZE + 1] = -0.25;
        let mut stage = SpeakerStage::new(ir.clone()).unwrap();
        let output: Vec<_> = (0..CONVOLUTION_PARTITION_SIZE + ir.len())
            .map(|sample_idx| stage.process((sample_idx == 0) as u8 as f32, true))
            .collect();

        for (tap_idx, tap) in ir.iter().enumerate() {
            assert!((output[tap_idx] - tap).abs() < 1e-5);
        }
    }

    #[test]
    fn wav_ir_matches_supported_rate() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../lab/references/tone3000-irs/celestion.wav");
        assert!(!load_wav_ir(&path, 48_000).unwrap().is_empty());
        assert!(load_wav_ir(&path, 32_000).is_err());
    }
}
