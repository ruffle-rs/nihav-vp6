#[cfg(feature="decoder_indeo2")]
pub mod indeo2;
#[cfg(feature="decoder_indeo3")]
pub mod indeo3;
#[cfg(feature="decoder_indeo4")]
pub mod indeo4;
#[cfg(feature="decoder_indeo5")]
pub mod indeo5;

#[cfg(any(feature="decoder_indeo4", feature="decoder_indeo5"))]
mod ivi;
#[cfg(any(feature="decoder_indeo4", feature="decoder_indeo5"))]
mod ivibr;
#[cfg(any(feature="decoder_indeo4", feature="decoder_indeo5"))]
mod ividsp;

#[cfg(feature="decoder_imc")]
pub mod imc;
