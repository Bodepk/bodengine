//! Núcleo del motor: reloj de paso fijo, input y estado del juego.
//!
//! No depende de ventana, GPU, física ni audio (ADR-002).

pub mod clock;
pub mod engine;
pub mod input;

pub use clock::{Clock, FIXED_DT, FixedStepClock, MAX_SUBSTEPS, ManualClock, SystemClock, Tick};
pub use engine::{Engine, EngineConfig};
pub use input::{InputState, Key};
