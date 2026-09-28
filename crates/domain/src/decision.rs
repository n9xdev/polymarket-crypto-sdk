use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::Side;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    Hold,
    Buy {
        side: Side,
        px: Decimal,
        reason: String,
    },
    Reject {
        reason: String,
    },
}
