#[cfg(windows)]
pub mod capture;
pub mod classifier;
pub mod esp;
#[cfg(windows)]
mod game;
pub mod hold;
#[cfg(all(windows, feature = "probe"))]
mod probe;
pub mod recognizer;
pub mod resample;
#[cfg(windows)]
mod skse;
pub mod table;
pub mod tone;
#[cfg(windows)]
pub mod worker;
