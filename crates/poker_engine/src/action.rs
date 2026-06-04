use crate::ChipAmount;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerAction {
    Fold,
    Check,
    Call,
    Bet { amount: ChipAmount },
    Raise { amount: ChipAmount },
    AllIn,
}
