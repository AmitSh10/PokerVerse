use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::{Card, ChipAmount, HandPositions, SeatIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamePhase {
    WaitingForPlayers,
    StartingHand,
    PostingBlinds,
    PreFlop,
    Flop,
    Turn,
    River,
    Showdown,
    HandComplete,
}

impl GamePhase {
    pub fn next(self) -> Option<Self> {
        match self {
            Self::WaitingForPlayers => Some(Self::StartingHand),
            Self::StartingHand => Some(Self::PostingBlinds),
            Self::PostingBlinds => Some(Self::PreFlop),
            Self::PreFlop => Some(Self::Flop),
            Self::Flop => Some(Self::Turn),
            Self::Turn => Some(Self::River),
            Self::River => Some(Self::Showdown),
            Self::Showdown => Some(Self::HandComplete),
            Self::HandComplete => None,
        }
    }

    pub fn is_betting_phase(self) -> bool {
        matches!(self, Self::PreFlop | Self::Flop | Self::Turn | Self::River)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandState {
    phase: GamePhase,
    positions: HandPositions,
    acting_seat: Option<SeatIndex>,
    board: Vec<Card>,
    pot: ChipAmount,
    contributions: HashMap<SeatIndex, ChipAmount>,
    round_contributions: HashMap<SeatIndex, ChipAmount>,
    acted_this_round: HashSet<SeatIndex>,
    current_bet: ChipAmount,
}

impl HandState {
    pub const MAX_BOARD_CARDS: usize = 5;
    pub const FLOP_CARD_COUNT: usize = 3;

    pub fn new(positions: HandPositions) -> Self {
        Self {
            phase: GamePhase::StartingHand,
            positions,
            acting_seat: None,
            board: Vec::with_capacity(Self::MAX_BOARD_CARDS),
            pot: 0,
            contributions: HashMap::new(),
            round_contributions: HashMap::new(),
            acted_this_round: HashSet::new(),
            current_bet: 0,
        }
    }

    pub fn phase(&self) -> GamePhase {
        self.phase
    }

    pub fn dealer_seat(&self) -> SeatIndex {
        self.positions.dealer()
    }

    pub fn small_blind_seat(&self) -> SeatIndex {
        self.positions.small_blind()
    }

    pub fn big_blind_seat(&self) -> SeatIndex {
        self.positions.big_blind()
    }

    pub fn first_to_act_seat(&self) -> SeatIndex {
        self.positions.first_to_act()
    }

    pub fn positions(&self) -> HandPositions {
        self.positions
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

    pub fn contribution_for(&self, seat: SeatIndex) -> ChipAmount {
        self.contributions.get(&seat).copied().unwrap_or(0)
    }

    pub fn contributions(&self) -> impl Iterator<Item = (SeatIndex, ChipAmount)> + '_ {
        self.contributions
            .iter()
            .map(|(seat, amount)| (*seat, *amount))
    }

    pub fn round_contribution_for(&self, seat: SeatIndex) -> ChipAmount {
        self.round_contributions.get(&seat).copied().unwrap_or(0)
    }

    pub fn current_bet(&self) -> ChipAmount {
        self.current_bet
    }

    pub fn amount_to_call(&self, seat: SeatIndex) -> ChipAmount {
        self.current_bet
            .saturating_sub(self.round_contribution_for(seat))
    }

    pub fn set_acting_seat(&mut self, acting_seat: Option<SeatIndex>) {
        self.acting_seat = acting_seat;
    }

    pub fn mark_player_acted(&mut self, seat: SeatIndex) {
        self.acted_this_round.insert(seat);
    }

    pub fn has_player_acted_this_round(&self, seat: SeatIndex) -> bool {
        self.acted_this_round.contains(&seat)
    }

    pub fn reset_round_actions(&mut self) {
        self.acted_this_round.clear();
    }

    pub fn record_contribution(&mut self, seat: SeatIndex, amount: ChipAmount) {
        let contribution = self.contributions.entry(seat).or_insert(0);
        *contribution += amount;

        let round_contribution = self.round_contributions.entry(seat).or_insert(0);
        *round_contribution += amount;

        self.current_bet = self.current_bet.max(*round_contribution);
        self.pot += amount;
    }

    pub fn take_pot(&mut self) -> ChipAmount {
        let pot = self.pot;
        self.pot = 0;
        pot
    }

    pub fn advance_phase(&mut self) -> Result<GamePhase, HandStateError> {
        let next_phase = self
            .phase
            .next()
            .ok_or(HandStateError::HandAlreadyComplete)?;

        self.phase = next_phase;

        match self.phase {
            GamePhase::PreFlop => {
                self.acting_seat = Some(self.positions.first_to_act());
            }
            GamePhase::Flop | GamePhase::Turn | GamePhase::River => {
                self.reset_round_betting();
            }
            _ => {}
        }

        Ok(self.phase)
    }

    fn reset_round_betting(&mut self) {
        self.round_contributions.clear();
        self.reset_round_actions();
        self.current_bet = 0;
    }

    pub fn reveal_flop(
        &mut self,
        cards: [Card; Self::FLOP_CARD_COUNT],
    ) -> Result<(), HandStateError> {
        if self.phase != GamePhase::Flop {
            return Err(HandStateError::InvalidRevealPhase {
                expected: GamePhase::Flop,
                actual: self.phase,
            });
        }

        if !self.board.is_empty() {
            return Err(HandStateError::BoardAlreadyHasCards {
                current_count: self.board.len(),
            });
        }

        self.board.extend(cards);
        Ok(())
    }

    pub fn reveal_turn(&mut self, card: Card) -> Result<(), HandStateError> {
        self.reveal_single_board_card(GamePhase::Turn, card)
    }

    pub fn reveal_river(&mut self, card: Card) -> Result<(), HandStateError> {
        self.reveal_single_board_card(GamePhase::River, card)
    }

    fn reveal_single_board_card(
        &mut self,
        expected_phase: GamePhase,
        card: Card,
    ) -> Result<(), HandStateError> {
        if self.phase != expected_phase {
            return Err(HandStateError::InvalidRevealPhase {
                expected: expected_phase,
                actual: self.phase,
            });
        }

        let expected_count = match expected_phase {
            GamePhase::Turn => Self::FLOP_CARD_COUNT,
            GamePhase::River => Self::FLOP_CARD_COUNT + 1,
            _ => unreachable!("single board card reveal only supports turn and river"),
        };

        if self.board.len() != expected_count {
            return Err(HandStateError::InvalidBoardCardCount {
                expected: expected_count,
                actual: self.board.len(),
            });
        }

        self.board.push(card);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandStateError {
    HandAlreadyComplete,
    InvalidRevealPhase {
        expected: GamePhase,
        actual: GamePhase,
    },
    BoardAlreadyHasCards {
        current_count: usize,
    },
    InvalidBoardCardCount {
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for HandStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HandAlreadyComplete => write!(f, "hand is already complete"),
            Self::InvalidRevealPhase { expected, actual } => {
                write!(
                    f,
                    "cannot reveal board cards in phase {actual:?}; expected {expected:?}"
                )
            }
            Self::BoardAlreadyHasCards { current_count } => {
                write!(f, "board already has {current_count} cards")
            }
            Self::InvalidBoardCardCount { expected, actual } => {
                write!(f, "board has {actual} cards; expected {expected}")
            }
        }
    }
}

impl std::error::Error for HandStateError {}

#[cfg(test)]
mod tests {
    use crate::{Rank, Suit};

    use super::*;

    fn card(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    fn positions() -> HandPositions {
        HandPositions::new(SeatIndex(0), SeatIndex(1), SeatIndex(2), SeatIndex(3))
    }

    fn advance_to(hand: &mut HandState, target_phase: GamePhase) {
        while hand.phase() != target_phase {
            hand.advance_phase()
                .expect("phase should advance before hand is complete");
        }
    }

    #[test]
    fn new_hand_starts_in_starting_hand_phase() {
        let hand = HandState::new(HandPositions::new(
            SeatIndex(2),
            SeatIndex(3),
            SeatIndex(4),
            SeatIndex(5),
        ));

        assert_eq!(hand.phase(), GamePhase::StartingHand);
        assert_eq!(hand.dealer_seat(), SeatIndex(2));
        assert_eq!(hand.small_blind_seat(), SeatIndex(3));
        assert_eq!(hand.big_blind_seat(), SeatIndex(4));
        assert_eq!(hand.first_to_act_seat(), SeatIndex(5));
        assert_eq!(hand.acting_seat(), None);
        assert_eq!(hand.pot(), 0);
        assert_eq!(hand.current_bet(), 0);
        assert!(!hand.has_player_acted_this_round(SeatIndex(1)));
        assert!(hand.board().is_empty());
    }

    #[test]
    fn record_contribution_adds_to_total_round_current_bet_and_pot() {
        let mut hand = HandState::new(positions());

        hand.record_contribution(SeatIndex(1), 5);
        hand.record_contribution(SeatIndex(2), 10);
        hand.record_contribution(SeatIndex(1), 15);

        assert_eq!(hand.contribution_for(SeatIndex(1)), 20);
        assert_eq!(hand.contribution_for(SeatIndex(2)), 10);
        assert_eq!(hand.contribution_for(SeatIndex(3)), 0);
        assert_eq!(hand.round_contribution_for(SeatIndex(1)), 20);
        assert_eq!(hand.round_contribution_for(SeatIndex(2)), 10);
        assert_eq!(hand.current_bet(), 20);
        assert_eq!(hand.pot(), 30);
        assert_eq!(
            hand.contributions().collect::<Vec<_>>().len(),
            2,
            "contributions iterator should expose seats that committed chips"
        );
    }

    #[test]
    fn take_pot_returns_and_clears_pot() {
        let mut hand = HandState::new(positions());
        hand.record_contribution(SeatIndex(1), 25);
        hand.record_contribution(SeatIndex(2), 50);

        assert_eq!(hand.take_pot(), 75);
        assert_eq!(hand.pot(), 0);
    }

    #[test]
    fn amount_to_call_is_current_bet_minus_round_contribution() {
        let mut hand = HandState::new(positions());

        hand.record_contribution(SeatIndex(1), 5);
        hand.record_contribution(SeatIndex(2), 10);

        assert_eq!(hand.amount_to_call(SeatIndex(1)), 5);
        assert_eq!(hand.amount_to_call(SeatIndex(2)), 0);
        assert_eq!(hand.amount_to_call(SeatIndex(3)), 10);
    }

    #[test]
    fn entering_new_board_betting_round_resets_round_bet_but_not_pot() {
        let mut hand = HandState::new(positions());
        hand.record_contribution(SeatIndex(1), 5);
        hand.record_contribution(SeatIndex(2), 10);
        advance_to(&mut hand, GamePhase::Flop);

        assert_eq!(hand.contribution_for(SeatIndex(1)), 5);
        assert_eq!(hand.contribution_for(SeatIndex(2)), 10);
        assert_eq!(hand.round_contribution_for(SeatIndex(1)), 0);
        assert_eq!(hand.round_contribution_for(SeatIndex(2)), 0);
        assert_eq!(hand.current_bet(), 0);
        assert_eq!(hand.pot(), 15);
    }

    #[test]
    fn round_actions_can_be_marked_and_reset() {
        let mut hand = HandState::new(positions());

        hand.mark_player_acted(SeatIndex(1));
        assert!(hand.has_player_acted_this_round(SeatIndex(1)));
        assert!(!hand.has_player_acted_this_round(SeatIndex(2)));

        hand.reset_round_actions();

        assert!(!hand.has_player_acted_this_round(SeatIndex(1)));
    }

    #[test]
    fn game_phase_advances_in_expected_order() {
        let mut hand = HandState::new(positions());

        assert_eq!(hand.advance_phase(), Ok(GamePhase::PostingBlinds));
        assert_eq!(hand.advance_phase(), Ok(GamePhase::PreFlop));
        assert_eq!(hand.advance_phase(), Ok(GamePhase::Flop));
        assert_eq!(hand.advance_phase(), Ok(GamePhase::Turn));
        assert_eq!(hand.advance_phase(), Ok(GamePhase::River));
        assert_eq!(hand.advance_phase(), Ok(GamePhase::Showdown));
        assert_eq!(hand.advance_phase(), Ok(GamePhase::HandComplete));
        assert_eq!(
            hand.advance_phase(),
            Err(HandStateError::HandAlreadyComplete)
        );
    }

    #[test]
    fn game_phase_identifies_betting_phases() {
        assert!(!GamePhase::StartingHand.is_betting_phase());
        assert!(!GamePhase::PostingBlinds.is_betting_phase());
        assert!(GamePhase::PreFlop.is_betting_phase());
        assert!(GamePhase::Flop.is_betting_phase());
        assert!(GamePhase::Turn.is_betting_phase());
        assert!(GamePhase::River.is_betting_phase());
        assert!(!GamePhase::Showdown.is_betting_phase());
        assert!(!GamePhase::HandComplete.is_betting_phase());
    }

    #[test]
    fn acting_seat_can_be_set_and_cleared() {
        let mut hand = HandState::new(positions());

        hand.set_acting_seat(Some(SeatIndex(3)));
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));

        hand.set_acting_seat(None);
        assert_eq!(hand.acting_seat(), None);
    }

    #[test]
    fn entering_pre_flop_sets_first_player_to_act() {
        let mut hand = HandState::new(positions());

        hand.advance_phase()
            .expect("hand should advance to posting blinds");
        hand.advance_phase()
            .expect("hand should advance to pre-flop");

        assert_eq!(hand.phase(), GamePhase::PreFlop);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));
    }

    #[test]
    fn flop_reveals_three_board_cards() {
        let mut hand = HandState::new(positions());
        advance_to(&mut hand, GamePhase::Flop);

        hand.reveal_flop([
            card(Rank::Ace, Suit::Spades),
            card(Rank::King, Suit::Spades),
            card(Rank::Queen, Suit::Spades),
        ])
        .expect("flop should reveal in flop phase");

        assert_eq!(hand.board().len(), 3);
    }

    #[test]
    fn flop_cannot_be_revealed_before_flop_phase() {
        let mut hand = HandState::new(positions());

        let result = hand.reveal_flop([
            card(Rank::Ace, Suit::Spades),
            card(Rank::King, Suit::Spades),
            card(Rank::Queen, Suit::Spades),
        ]);

        assert_eq!(
            result,
            Err(HandStateError::InvalidRevealPhase {
                expected: GamePhase::Flop,
                actual: GamePhase::StartingHand,
            })
        );
        assert!(hand.board().is_empty());
    }

    #[test]
    fn turn_requires_existing_flop_cards() {
        let mut hand = HandState::new(positions());
        advance_to(&mut hand, GamePhase::Turn);

        let result = hand.reveal_turn(card(Rank::Jack, Suit::Spades));

        assert_eq!(
            result,
            Err(HandStateError::InvalidBoardCardCount {
                expected: 3,
                actual: 0,
            })
        );
    }

    #[test]
    fn river_requires_existing_flop_and_turn_cards() {
        let mut hand = HandState::new(positions());
        advance_to(&mut hand, GamePhase::River);

        let result = hand.reveal_river(card(Rank::Ten, Suit::Spades));

        assert_eq!(
            result,
            Err(HandStateError::InvalidBoardCardCount {
                expected: 4,
                actual: 0,
            })
        );
    }

    #[test]
    fn board_can_reveal_full_five_cards_in_order() {
        let mut hand = HandState::new(positions());
        advance_to(&mut hand, GamePhase::Flop);

        hand.reveal_flop([
            card(Rank::Ace, Suit::Spades),
            card(Rank::King, Suit::Spades),
            card(Rank::Queen, Suit::Spades),
        ])
        .expect("flop should reveal");

        advance_to(&mut hand, GamePhase::Turn);
        hand.reveal_turn(card(Rank::Jack, Suit::Spades))
            .expect("turn should reveal");

        advance_to(&mut hand, GamePhase::River);
        hand.reveal_river(card(Rank::Ten, Suit::Spades))
            .expect("river should reveal");

        assert_eq!(hand.board().len(), HandState::MAX_BOARD_CARDS);
    }
}
