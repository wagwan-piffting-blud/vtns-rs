//! Native reimplementation of the VoiceText English engine (vt_eng.dll 3.11.7.1)
//! for the Paul M16 voice. Module layout follows the engine's pipeline.

pub mod bank;
pub mod engine;
pub mod json;
pub mod lexicon;
pub mod ling;
pub mod phones;
pub mod prosody;
pub mod shorten;
pub mod text;
pub mod tree3;
pub mod unitsel;
pub mod wave;

/// Output sample rate of every M16 voice.
pub const SAMPLE_RATE: u32 = 16_000;
