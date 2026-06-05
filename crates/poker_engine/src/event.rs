use crate::{Card, ChipAmount, GamePhase, PlayerAction, SeatIndex};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameEvent {
    HandStarted {
        dealer_seat: SeatIndex,
        playing_seats: Vec<SeatIndex>,
    },
    HoleCardsDealt {
        seats: Vec<SeatIndex>,
        cards_per_player: usize,
    },
    BlindsPosted {
        small_blind_seat: SeatIndex,
        small_blind: ChipAmount,
        big_blind_seat: SeatIndex,
        big_blind: ChipAmount,
    },
    BoardRevealed {
        phase: GamePhase,
        cards: Vec<Card>,
    },
    PhaseAdvanced {
        phase: GamePhase,
    },
    PlayerActed {
        seat: SeatIndex,
        action: PlayerAction,
    },
    PotAwarded {
        total_pot: ChipAmount,
        payouts: Vec<PayoutEvent>,
    },
    TwoSevenBountyAwarded {
        winner_seat: SeatIndex,
        bounty_per_player: ChipAmount,
        total_awarded: ChipAmount,
        payments: Vec<BountyPaymentEvent>,
    },
    HandFinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayoutEvent {
    seat: SeatIndex,
    amount: ChipAmount,
}

impl PayoutEvent {
    pub fn new(seat: SeatIndex, amount: ChipAmount) -> Self {
        Self { seat, amount }
    }

    pub fn seat(&self) -> SeatIndex {
        self.seat
    }

    pub fn amount(&self) -> ChipAmount {
        self.amount
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BountyPaymentEvent {
    payer_seat: SeatIndex,
    amount: ChipAmount,
}

impl BountyPaymentEvent {
    pub fn new(payer_seat: SeatIndex, amount: ChipAmount) -> Self {
        Self { payer_seat, amount }
    }

    pub fn payer_seat(&self) -> SeatIndex {
        self.payer_seat
    }

    pub fn amount(&self) -> ChipAmount {
        self.amount
    }
}
