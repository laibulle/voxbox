pub mod amp;
pub mod ir;

use nih_plug::prelude::*;

#[derive(Params)]
pub struct VoxBoxParams {
    #[id = "gain"]
    pub gain: FloatParam,
    #[id = "bass"]
    pub bass: FloatParam,
    #[id = "cut"]
    pub cut: FloatParam,
    #[id = "tone"]
    pub tone: FloatParam,
    #[id = "master"]
    pub master: FloatParam,
    #[id = "speaker_ir"]
    pub speaker_ir: BoolParam,
}

impl Default for VoxBoxParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new(
                "Top Boost Volume",
                0.55,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 1.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            bass: FloatParam::new("Bass", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            cut: FloatParam::new("Cut", 0.35, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            tone: FloatParam::new("Treble", 0.6, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            master: FloatParam::new(
                "Output Trim",
                util::db_to_gain(-9.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-36.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            speaker_ir: BoolParam::new("Speaker IR", false),
        }
    }
}
