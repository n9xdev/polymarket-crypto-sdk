mod default;

use poly_domain::{Decision, Market, Signal};

pub use default::DefaultStrategy;

pub trait Strategy: Send {
    fn on_signal(&mut self, signal: &Signal, market: &Market) -> Decision;
    fn on_market_active(&mut self, _market: &Market) {}
    fn on_order_sent(&mut self, _market: &Market) {}
}
