use crate::ChipAmount;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerAction {
    Fold,
    Check,
    Call,
    Bet { amount: ChipAmount },
    Raise { amount: ChipAmount },
    AllIn,
}
