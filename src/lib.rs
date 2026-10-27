//! Native reimplementation of the VoiceText English engine (vt_eng.dll 3.11.7.1)
//! for the Paul M16 voice. Module layout follows the engine's pipeline.

pub mod bank;
pub mod cp1252;
pub mod engine;
pub mod engine04;
pub mod engine09;
pub mod engine_spa;
pub mod json;
pub mod era;
pub mod install;
pub mod lang;
pub mod lexicon;
pub mod ling;
pub mod ling04;
pub mod ling_spa;
pub mod phones;
pub mod prosody;
pub mod prosody_spa;
pub mod prosody_tables_spa;
pub mod shorten;
pub mod text;
pub mod text04;
pub mod text_spa;
pub mod tree2;
pub mod tree3;
pub mod unitsel;
pub mod unitsel04;
pub mod wave;
pub mod wave04;

/// Output sample rate of every M16 voice.
pub const SAMPLE_RATE: u32 = 16_000;
