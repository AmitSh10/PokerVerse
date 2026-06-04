use std::fmt;

use crate::Card;

pub type ChipAmount = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SeatIndex(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerStatus {
    Active,
    Folded,
    AllIn,
    SittingOut,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    id: PlayerId,
    display_name: String,
    seat: SeatIndex,
    stack: ChipAmount,
    status: PlayerStatus,
    hole_cards: Vec<Card>,
}

impl Player {
    pub const MAX_HOLE_CARDS: usize = 2;

    pub fn new(
        id: PlayerId,
        display_name: impl Into<String>,
        seat: SeatIndex,
        stack: ChipAmount,
    ) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            seat,
            stack,
            status: PlayerStatus::Active,
            hole_cards: Vec::with_capacity(Self::MAX_HOLE_CARDS),
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

    pub fn hole_cards(&self) -> &[Card] {
        &self.hole_cards
    }

    pub fn can_play_hand(&self) -> bool {
        self.status == PlayerStatus::Active && self.stack > 0
    }

    pub fn receive_card(&mut self, card: Card) -> Result<(), PlayerError> {
        if self.hole_cards.len() >= Self::MAX_HOLE_CARDS {
            return Err(PlayerError::TooManyHoleCards);
        }

        self.hole_cards.push(card);
        Ok(())
    }

    pub fn clear_hole_cards(&mut self) {
        self.hole_cards.clear();
    }

    pub fn debit_chips(&mut self, amount: ChipAmount) -> Result<ChipAmount, PlayerError> {
        if amount > self.stack {
            return Err(PlayerError::NotEnoughChips {
                requested: amount,
                available: self.stack,
            });
        }

        self.stack -= amount;

        if self.stack == 0 {
            self.status = PlayerStatus::AllIn;
        }

        Ok(amount)
    }

    pub fn credit_chips(&mut self, amount: ChipAmount) {
        self.stack += amount;

        if self.status == PlayerStatus::AllIn && self.stack > 0 {
            self.status = PlayerStatus::Active;
        }
    }

    pub fn move_all_in(&mut self) -> ChipAmount {
        let committed = self.stack;
        self.stack = 0;
        self.status = PlayerStatus::AllIn;
        committed
    }

    pub fn fold(&mut self) {
        self.status = PlayerStatus::Folded;
    }

    pub fn sit_out(&mut self) {
        self.status = PlayerStatus::SittingOut;
    }

    pub fn disconnect(&mut self) {
        self.status = PlayerStatus::Disconnected;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerError {
    TooManyHoleCards,
    NotEnoughChips {
        requested: ChipAmount,
        available: ChipAmount,
    },
}

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyHoleCards => write!(f, "player cannot hold more than two hole cards"),
            Self::NotEnoughChips {
                requested,
                available,
            } => write!(
                f,
                "not enough chips: requested {requested}, available {available}"
            ),
        }
    }
}

impl std::error::Error for PlayerError {}

#[cfg(test)]
mod tests {
    use crate::{Rank, Suit};

    use super::*;

    fn player() -> Player {
        Player::new(PlayerId(1), "Ada", SeatIndex(0), 1_000)
    }

    fn card(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn new_player_starts_active_with_no_hole_cards() {
        let player = player();

        assert_eq!(player.id(), PlayerId(1));
        assert_eq!(player.display_name(), "Ada");
        assert_eq!(player.seat(), SeatIndex(0));
        assert_eq!(player.stack(), 1_000);
        assert_eq!(player.status(), PlayerStatus::Active);
        assert!(player.hole_cards().is_empty());
    }

    #[test]
    fn player_can_receive_two_hole_cards() {
        let mut player = player();

        player
            .receive_card(card(Rank::Ace, Suit::Spades))
            .expect("first card should be accepted");
        player
            .receive_card(card(Rank::King, Suit::Spades))
            .expect("second card should be accepted");

        assert_eq!(player.hole_cards().len(), Player::MAX_HOLE_CARDS);
    }

    #[test]
    fn player_cannot_receive_more_than_two_hole_cards() {
        let mut player = player();

        player
            .receive_card(card(Rank::Ace, Suit::Spades))
            .expect("first card should be accepted");
        player
            .receive_card(card(Rank::King, Suit::Spades))
            .expect("second card should be accepted");

        let result = player.receive_card(card(Rank::Queen, Suit::Spades));

        assert_eq!(result, Err(PlayerError::TooManyHoleCards));
        assert_eq!(player.hole_cards().len(), Player::MAX_HOLE_CARDS);
    }

    #[test]
    fn debit_chips_removes_chips_from_stack() {
        let mut player = player();

        let committed = player
            .debit_chips(250)
            .expect("player should have enough chips");

        assert_eq!(committed, 250);
        assert_eq!(player.stack(), 750);
    }

    #[test]
    fn debit_chips_fails_when_amount_exceeds_stack() {
        let mut player = player();

        let result = player.debit_chips(1_001);

        assert_eq!(
            result,
            Err(PlayerError::NotEnoughChips {
                requested: 1_001,
                available: 1_000,
            })
        );
        assert_eq!(player.stack(), 1_000);
    }

    #[test]
    fn debit_chips_marks_player_all_in_when_stack_reaches_zero() {
        let mut player = player();

        player
            .debit_chips(1_000)
            .expect("player should be able to commit full stack");

        assert_eq!(player.stack(), 0);
        assert_eq!(player.status(), PlayerStatus::AllIn);
    }

    #[test]
    fn move_all_in_commits_entire_stack() {
        let mut player = player();

        let committed = player.move_all_in();

        assert_eq!(committed, 1_000);
        assert_eq!(player.stack(), 0);
        assert_eq!(player.status(), PlayerStatus::AllIn);
    }

    #[test]
    fn credit_chips_adds_to_stack() {
        let mut player = player();

        player.credit_chips(500);

        assert_eq!(player.stack(), 1_500);
    }

    #[test]
    fn clear_hole_cards_removes_private_cards() {
        let mut player = player();
        player
            .receive_card(card(Rank::Ace, Suit::Spades))
            .expect("card should be accepted");

        player.clear_hole_cards();

        assert!(player.hole_cards().is_empty());
    }

    #[test]
    fn active_player_with_chips_can_play_hand() {
        let player = player();

        assert!(player.can_play_hand());
    }

    #[test]
    fn folded_player_cannot_play_hand() {
        let mut player = player();

        player.fold();

        assert!(!player.can_play_hand());
    }

    #[test]
    fn all_in_player_with_no_chips_cannot_start_new_hand() {
        let mut player = player();

        player.move_all_in();

        assert!(!player.can_play_hand());
    }
}
