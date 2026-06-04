use std::fmt;

use crate::{
    Deck, GamePhase, HandState, HandStateError, PlayerError, SeatIndex, Table, TableConfig,
    TableError,
};

#[derive(Debug, Clone)]
pub struct GameEngine {
    table: Table,
    current_hand: Option<HandState>,
    deck: Option<Deck>,
}

impl GameEngine {
    pub fn new(config: TableConfig) -> Self {
        Self {
            table: Table::new(config),
            current_hand: None,
            deck: None,
        }
    }

    pub fn table(&self) -> &Table {
        &self.table
    }

    pub fn table_mut(&mut self) -> &mut Table {
        &mut self.table
    }

    pub fn current_hand(&self) -> Option<&HandState> {
        self.current_hand.as_ref()
    }

    pub fn deck(&self) -> Option<&Deck> {
        self.deck.as_ref()
    }

    pub fn start_hand(&mut self, dealer_seat: SeatIndex) -> Result<&HandState, GameEngineError> {
        if self.current_hand.is_some() {
            return Err(GameEngineError::HandAlreadyInProgress);
        }

        let positions = self
            .table
            .hand_positions(dealer_seat)?
            .ok_or(GameEngineError::NotEnoughPlayersToStartHand)?;

        let playing_seats = self.table.playing_seats();
        let mut deck = Deck::new_shuffled();

        for seat in &playing_seats {
            self.table
                .player_at_mut(*seat)
                .expect("playing_seats only returns occupied seats")
                .clear_hole_cards();
        }

        for _ in 0..2 {
            for seat in &playing_seats {
                let card = deck.deal_one().ok_or(GameEngineError::DeckExhausted)?;
                self.table
                    .player_at_mut(*seat)
                    .expect("playing_seats only returns occupied seats")
                    .receive_card(card)?;
            }
        }

        self.deck = Some(deck);
        self.current_hand = Some(HandState::new(positions));

        Ok(self
            .current_hand
            .as_ref()
            .expect("current_hand was just inserted"))
    }

    pub fn advance_hand_phase(&mut self) -> Result<GamePhase, GameEngineError> {
        let hand = self
            .current_hand
            .as_mut()
            .ok_or(GameEngineError::NoActiveHand)?;

        hand.advance_phase().map_err(GameEngineError::HandState)
    }

    pub fn post_blinds(&mut self) -> Result<(), GameEngineError> {
        let (small_blind_seat, big_blind_seat, small_blind_amount, big_blind_amount) = {
            let hand = self
                .current_hand
                .as_ref()
                .ok_or(GameEngineError::NoActiveHand)?;

            if hand.phase() != GamePhase::PostingBlinds {
                return Err(GameEngineError::InvalidPhaseForPostingBlinds {
                    actual: hand.phase(),
                });
            }

            (
                hand.small_blind_seat(),
                hand.big_blind_seat(),
                self.table.config().small_blind(),
                self.table.config().big_blind(),
            )
        };

        let small_blind_committed = self
            .table
            .player_at_mut(small_blind_seat)
            .ok_or(TableError::SeatEmpty {
                seat: small_blind_seat,
            })?
            .debit_chips(small_blind_amount)?;

        let big_blind_committed = self
            .table
            .player_at_mut(big_blind_seat)
            .ok_or(TableError::SeatEmpty {
                seat: big_blind_seat,
            })?
            .debit_chips(big_blind_amount)?;

        let hand = self
            .current_hand
            .as_mut()
            .expect("active hand was checked before posting blinds");

        hand.record_contribution(small_blind_seat, small_blind_committed);
        hand.record_contribution(big_blind_seat, big_blind_committed);

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEngineError {
    NoActiveHand,
    HandAlreadyInProgress,
    NotEnoughPlayersToStartHand,
    DeckExhausted,
    InvalidPhaseForPostingBlinds { actual: GamePhase },
    Table(TableError),
    Player(PlayerError),
    HandState(HandStateError),
}

impl fmt::Display for GameEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoActiveHand => write!(f, "no active hand"),
            Self::HandAlreadyInProgress => write!(f, "a hand is already in progress"),
            Self::NotEnoughPlayersToStartHand => write!(f, "not enough players to start hand"),
            Self::DeckExhausted => write!(f, "deck does not have enough cards"),
            Self::InvalidPhaseForPostingBlinds { actual } => {
                write!(f, "cannot post blinds in phase {actual:?}")
            }
            Self::Table(error) => write!(f, "{error}"),
            Self::Player(error) => write!(f, "{error}"),
            Self::HandState(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for GameEngineError {}

impl From<TableError> for GameEngineError {
    fn from(error: TableError) -> Self {
        Self::Table(error)
    }
}

impl From<PlayerError> for GameEngineError {
    fn from(error: PlayerError) -> Self {
        Self::Player(error)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Player, PlayerId, TableConfig};

    use super::*;

    fn config() -> TableConfig {
        TableConfig::new(6, 5, 10, 500, 2_000).expect("config should be valid")
    }

    fn engine() -> GameEngine {
        GameEngine::new(config())
    }

    fn engine_with_two_players() -> GameEngine {
        let mut engine = engine();
        engine
            .table_mut()
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("first player should sit");
        engine
            .table_mut()
            .sit_player(PlayerId(2), "Grace", SeatIndex(3), 1_000)
            .expect("second player should sit");
        engine
    }

    #[test]
    fn new_engine_starts_without_active_hand() {
        let engine = engine();

        assert!(engine.current_hand().is_none());
        assert!(engine.deck().is_none());
        assert_eq!(engine.table().seat_count(), 6);
    }

    #[test]
    fn start_hand_requires_two_playing_players() {
        let mut engine = engine();
        engine
            .table_mut()
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should sit");

        let result = engine.start_hand(SeatIndex(0));

        assert_eq!(result, Err(GameEngineError::NotEnoughPlayersToStartHand));
        assert!(engine.current_hand().is_none());
    }

    #[test]
    fn start_hand_creates_hand_from_table_positions() {
        let mut engine = engine_with_two_players();

        let hand = engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with two players");

        assert_eq!(hand.phase(), GamePhase::StartingHand);
        assert_eq!(hand.dealer_seat(), SeatIndex(0));
        assert_eq!(hand.small_blind_seat(), SeatIndex(0));
        assert_eq!(hand.big_blind_seat(), SeatIndex(3));
        assert_eq!(hand.first_to_act_seat(), SeatIndex(0));
    }

    #[test]
    fn start_hand_creates_live_deck_and_deals_two_hole_cards_to_each_playing_player() {
        let mut engine = engine_with_two_players();

        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with two players");

        let ada = engine
            .table()
            .player_at(SeatIndex(0))
            .expect("player should be seated");
        let grace = engine
            .table()
            .player_at(SeatIndex(3))
            .expect("player should be seated");

        assert_eq!(ada.hole_cards().len(), 2);
        assert_eq!(grace.hole_cards().len(), 2);
        assert_eq!(
            engine.deck().map(Deck::len),
            Some(Deck::CARD_COUNT - Player::MAX_HOLE_CARDS * 2)
        );
    }

    #[test]
    fn start_hand_does_not_deal_to_sitting_out_players() {
        let mut engine = engine_with_two_players();
        engine
            .table_mut()
            .sit_player(PlayerId(3), "Linus", SeatIndex(5), 1_000)
            .expect("third player should sit");
        engine
            .table_mut()
            .player_at_mut(SeatIndex(5))
            .expect("third player should be seated")
            .sit_out();

        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with two playing players");

        let sitting_out_player = engine
            .table()
            .player_at(SeatIndex(5))
            .expect("third player should remain seated");

        assert!(sitting_out_player.hole_cards().is_empty());
        assert_eq!(
            engine.deck().map(Deck::len),
            Some(Deck::CARD_COUNT - Player::MAX_HOLE_CARDS * 2)
        );
    }

    #[test]
    fn start_hand_rejects_second_active_hand() {
        let mut engine = engine_with_two_players();
        engine
            .start_hand(SeatIndex(0))
            .expect("first hand should start");

        let result = engine.start_hand(SeatIndex(0));

        assert_eq!(result, Err(GameEngineError::HandAlreadyInProgress));
    }

    #[test]
    fn advance_hand_phase_moves_active_hand_forward() {
        let mut engine = engine_with_two_players();
        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with two players");

        assert_eq!(engine.advance_hand_phase(), Ok(GamePhase::PostingBlinds));
        assert_eq!(engine.advance_hand_phase(), Ok(GamePhase::PreFlop));
        assert_eq!(
            engine.current_hand().and_then(HandState::acting_seat),
            Some(SeatIndex(0))
        );
    }

    #[test]
    fn post_blinds_requires_active_hand() {
        let mut engine = engine();

        assert_eq!(engine.post_blinds(), Err(GameEngineError::NoActiveHand));
    }

    #[test]
    fn post_blinds_requires_posting_blinds_phase() {
        let mut engine = engine_with_two_players();
        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with two players");

        assert_eq!(
            engine.post_blinds(),
            Err(GameEngineError::InvalidPhaseForPostingBlinds {
                actual: GamePhase::StartingHand,
            })
        );
    }

    #[test]
    fn post_blinds_debits_players_and_records_pot() {
        let mut engine = engine_with_two_players();
        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with two players");
        engine
            .advance_hand_phase()
            .expect("hand should enter posting blinds");

        engine.post_blinds().expect("blinds should post");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.contribution_for(SeatIndex(0)), 5);
        assert_eq!(hand.contribution_for(SeatIndex(3)), 10);
        assert_eq!(hand.pot(), 15);

        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(0))
                .expect("small blind should be seated")
                .stack(),
            995
        );
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(3))
                .expect("big blind should be seated")
                .stack(),
            990
        );
    }

    #[test]
    fn advance_hand_phase_requires_active_hand() {
        let mut engine = engine();

        assert_eq!(
            engine.advance_hand_phase(),
            Err(GameEngineError::NoActiveHand)
        );
    }
}
