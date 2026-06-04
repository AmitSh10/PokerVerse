use std::fmt;

use crate::{GamePhase, HandState, HandStateError, SeatIndex, Table, TableConfig, TableError};

#[derive(Debug, Clone)]
pub struct GameEngine {
    table: Table,
    current_hand: Option<HandState>,
}

impl GameEngine {
    pub fn new(config: TableConfig) -> Self {
        Self {
            table: Table::new(config),
            current_hand: None,
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

    pub fn start_hand(&mut self, dealer_seat: SeatIndex) -> Result<&HandState, GameEngineError> {
        if self.current_hand.is_some() {
            return Err(GameEngineError::HandAlreadyInProgress);
        }

        let positions = self
            .table
            .hand_positions(dealer_seat)?
            .ok_or(GameEngineError::NotEnoughPlayersToStartHand)?;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEngineError {
    NoActiveHand,
    HandAlreadyInProgress,
    NotEnoughPlayersToStartHand,
    Table(TableError),
    HandState(HandStateError),
}

impl fmt::Display for GameEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoActiveHand => write!(f, "no active hand"),
            Self::HandAlreadyInProgress => write!(f, "a hand is already in progress"),
            Self::NotEnoughPlayersToStartHand => write!(f, "not enough players to start hand"),
            Self::Table(error) => write!(f, "{error}"),
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

#[cfg(test)]
mod tests {
    use crate::{PlayerId, TableConfig};

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
    fn advance_hand_phase_requires_active_hand() {
        let mut engine = engine();

        assert_eq!(
            engine.advance_hand_phase(),
            Err(GameEngineError::NoActiveHand)
        );
    }
}
