use crate::{Card, ChipAmount, GamePhase, PlayerId, PlayerStatus, SeatIndex};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSnapshot {
    players: Vec<PlayerSnapshot>,
    hand: Option<HandSnapshot>,
}

impl GameSnapshot {
    pub fn new(players: Vec<PlayerSnapshot>, hand: Option<HandSnapshot>) -> Self {
        Self { players, hand }
    }

    pub fn players(&self) -> &[PlayerSnapshot] {
        &self.players
    }

    pub fn hand(&self) -> Option<&HandSnapshot> {
        self.hand.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    id: PlayerId,
    display_name: String,
    seat: SeatIndex,
    stack: ChipAmount,
    status: PlayerStatus,
    hole_card_count: usize,
    visible_hole_cards: Option<Vec<Card>>,
}

impl PlayerSnapshot {
    pub fn new(
        id: PlayerId,
        display_name: impl Into<String>,
        seat: SeatIndex,
        stack: ChipAmount,
        status: PlayerStatus,
        hole_card_count: usize,
        visible_hole_cards: Option<Vec<Card>>,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            seat,
            stack,
            status,
            hole_card_count,
            visible_hole_cards,
        }
    }

    pub fn id(&self) -> PlayerId {
        self.id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn seat(&self) -> SeatIndex {
        self.seat
    }

    pub fn stack(&self) -> ChipAmount {
        self.stack
    }

    pub fn status(&self) -> PlayerStatus {
        self.status
    }

    pub fn hole_card_count(&self) -> usize {
        self.hole_card_count
    }

    pub fn visible_hole_cards(&self) -> Option<&[Card]> {
        self.visible_hole_cards.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandSnapshot {
    phase: GamePhase,
    acting_seat: Option<SeatIndex>,
    board: Vec<Card>,
    pot: ChipAmount,
    current_bet: ChipAmount,
    dealer_seat: SeatIndex,
    small_blind_seat: SeatIndex,
    big_blind_seat: SeatIndex,
    first_to_act_seat: SeatIndex,
    contributions: Vec<ContributionSnapshot>,
}

impl HandSnapshot {
    pub fn new(
        phase: GamePhase,
        acting_seat: Option<SeatIndex>,
        board: Vec<Card>,
        pot: ChipAmount,
        current_bet: ChipAmount,
        dealer_seat: SeatIndex,
        small_blind_seat: SeatIndex,
        big_blind_seat: SeatIndex,
        first_to_act_seat: SeatIndex,
        contributions: Vec<ContributionSnapshot>,
    ) -> Self {
        Self {
            phase,
            acting_seat,
            board,
            pot,
            current_bet,
            dealer_seat,
            small_blind_seat,
            big_blind_seat,
            first_to_act_seat,
            contributions,
        }
    }

    pub fn phase(&self) -> GamePhase {
        self.phase
    }

    pub fn acting_seat(&self) -> Option<SeatIndex> {
        self.acting_seat
    }

    pub fn board(&self) -> &[Card] {
        &self.board
    }

    pub fn pot(&self) -> ChipAmount {
        self.pot
    }

    pub fn current_bet(&self) -> ChipAmount {
        self.current_bet
    }

    pub fn dealer_seat(&self) -> SeatIndex {
        self.dealer_seat
    }

    pub fn small_blind_seat(&self) -> SeatIndex {
        self.small_blind_seat
    }

    pub fn big_blind_seat(&self) -> SeatIndex {
        self.big_blind_seat
    }

    pub fn first_to_act_seat(&self) -> SeatIndex {
        self.first_to_act_seat
    }

    pub fn contributions(&self) -> &[ContributionSnapshot] {
        &self.contributions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionSnapshot {
    seat: SeatIndex,
    total: ChipAmount,
    round: ChipAmount,
}

impl ContributionSnapshot {
    pub fn new(seat: SeatIndex, total: ChipAmount, round: ChipAmount) -> Self {
        Self { seat, total, round }
    }

    pub fn seat(&self) -> SeatIndex {
        self.seat
    }

    pub fn total(&self) -> ChipAmount {
        self.total
    }

    pub fn round(&self) -> ChipAmount {
        self.round
    }
}
