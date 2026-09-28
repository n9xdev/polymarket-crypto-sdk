mod default;

use poly_domain::{Decision, Market, Signal};

pub use default::DefaultStrategy;

pub trait Strategy: Send {
    fn on_signal(&mut self, signal: &Signal, market: &Market) -> Decision;
}
