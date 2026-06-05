use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, RwLock},
};

use poker_engine::{
    ChipAmount, GameEngine, GameEngineError, GameEvent, GamePhase, GameSnapshot, PlayerAction,
    PlayerId, SeatIndex, TableConfig, TableError,
};
use serde::{Deserialize, Serialize};

use crate::command::{RoomCommand, RoomCommandResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomId(pub u64);

pub type SharedRoom = Arc<RwLock<Room>>;

#[derive(Debug)]
pub struct Room {
    id: RoomId,
    engine: GameEngine,
}

impl Room {
    pub fn new(id: RoomId, table_config: TableConfig) -> Self {
        Self {
            id,
            engine: GameEngine::new(table_config),
        }
    }

    pub fn id(&self) -> RoomId {
        self.id
    }

    pub fn engine(&self) -> &GameEngine {
        &self.engine
    }

    pub fn summary(&self) -> RoomSummary {
        RoomSummary::new(
            self.id,
            self.engine.table().config(),
            self.engine.table().seat_count(),
            self.engine.table().seated_player_count(),
            self.engine.table().can_start_hand(),
            self.engine.current_hand().map(|hand| hand.phase()),
        )
    }

    pub fn public_snapshot(&self) -> GameSnapshot {
        self.engine.public_snapshot()
    }

    pub fn private_snapshot_for(&self, seat: SeatIndex) -> Result<GameSnapshot, RoomError> {
        self.engine
            .private_snapshot_for(seat)
            .map_err(RoomError::from)
    }

    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        self.engine.drain_events()
    }

    pub fn sit_player(
        &mut self,
        id: PlayerId,
        display_name: impl Into<String>,
        seat: SeatIndex,
        buy_in: ChipAmount,
    ) -> Result<(), RoomError> {
        self.engine
            .table_mut()
            .sit_player(id, display_name, seat, buy_in)
            .map_err(GameEngineError::from)?;

        Ok(())
    }

    pub fn leave_seat(&mut self, seat: SeatIndex) -> Result<(), RoomError> {
        self.engine
            .table_mut()
            .leave_seat(seat)
            .map_err(GameEngineError::from)?;

        Ok(())
    }

    pub fn sit_out(&mut self, seat: SeatIndex) -> Result<(), RoomError> {
        self.engine
            .table_mut()
            .player_at_mut(seat)
            .ok_or(TableError::SeatEmpty { seat })
            .map_err(GameEngineError::from)?
            .sit_out();

        Ok(())
    }

    pub fn sit_in(&mut self, seat: SeatIndex) -> Result<(), RoomError> {
        self.engine
            .table_mut()
            .player_at_mut(seat)
            .ok_or(TableError::SeatEmpty { seat })
            .map_err(GameEngineError::from)?
            .sit_in();

        Ok(())
    }

    pub fn start_hand(&mut self, dealer_seat: SeatIndex) -> Result<Vec<GameEvent>, RoomError> {
        self.engine.start_hand(dealer_seat)?;
        Ok(self.engine.drain_events())
    }

    pub fn advance_hand_phase(&mut self) -> Result<Vec<GameEvent>, RoomError> {
        self.engine.advance_hand_phase()?;
        Ok(self.engine.drain_events())
    }

    pub fn post_blinds(&mut self) -> Result<Vec<GameEvent>, RoomError> {
        self.engine.post_blinds()?;
        Ok(self.engine.drain_events())
    }

    pub fn apply_player_action(
        &mut self,
        seat: SeatIndex,
        action: PlayerAction,
    ) -> Result<Vec<GameEvent>, RoomError> {
        self.engine.apply_player_action(seat, action)?;
        Ok(self.engine.drain_events())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSummary {
    id: RoomId,
    table_config: TableConfig,
    seat_count: usize,
    seated_player_count: usize,
    can_start_hand: bool,
    active_phase: Option<GamePhase>,
}

impl RoomSummary {
    pub fn new(
        id: RoomId,
        table_config: TableConfig,
        seat_count: usize,
        seated_player_count: usize,
        can_start_hand: bool,
        active_phase: Option<GamePhase>,
    ) -> Self {
        Self {
            id,
            table_config,
            seat_count,
            seated_player_count,
            can_start_hand,
            active_phase,
        }
    }

    pub fn id(&self) -> RoomId {
        self.id
    }

    pub fn table_config(&self) -> TableConfig {
        self.table_config
    }

    pub fn seat_count(&self) -> usize {
        self.seat_count
    }

    pub fn seated_player_count(&self) -> usize {
        self.seated_player_count
    }

    pub fn can_start_hand(&self) -> bool {
        self.can_start_hand
    }

    pub fn active_phase(&self) -> Option<GamePhase> {
        self.active_phase
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomError {
    Engine(GameEngineError),
}

impl fmt::Display for RoomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Engine(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RoomError {}

impl From<GameEngineError> for RoomError {
    fn from(error: GameEngineError) -> Self {
        Self::Engine(error)
    }
}

#[derive(Debug, Default)]
pub struct RoomManager {
    rooms: HashMap<RoomId, SharedRoom>,
}

impl RoomManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_room(
        &mut self,
        id: RoomId,
        table_config: TableConfig,
    ) -> Result<SharedRoom, RoomManagerError> {
        if self.rooms.contains_key(&id) {
            return Err(RoomManagerError::RoomAlreadyExists { id });
        }

        let room = Arc::new(RwLock::new(Room::new(id, table_config)));
        self.rooms.insert(id, Arc::clone(&room));

        Ok(room)
    }

    pub fn room(&self, id: RoomId) -> Option<SharedRoom> {
        self.rooms.get(&id).map(Arc::clone)
    }

    pub fn remove_room(&mut self, id: RoomId) -> Option<SharedRoom> {
        self.rooms.remove(&id)
    }

    pub fn close_room(&mut self, id: RoomId) -> Result<SharedRoom, RoomManagerError> {
        self.remove_room(id)
            .ok_or(RoomManagerError::RoomNotFound { id })
    }

    pub fn room_summary(&self, id: RoomId) -> Result<RoomSummary, RoomManagerError> {
        let room = self.room(id).ok_or(RoomManagerError::RoomNotFound { id })?;
        let locked_room = room
            .read()
            .map_err(|_| RoomManagerError::RoomLockPoisoned { id })?;

        Ok(locked_room.summary())
    }

    pub fn room_summaries(&self) -> Result<Vec<RoomSummary>, RoomManagerError> {
        let mut summaries = self
            .rooms
            .keys()
            .copied()
            .map(|id| self.room_summary(id))
            .collect::<Result<Vec<_>, _>>()?;

        summaries.sort_by_key(|summary| summary.id().0);
        Ok(summaries)
    }

    pub fn public_snapshot(&self, id: RoomId) -> Result<GameSnapshot, RoomManagerError> {
        let room = self.room(id).ok_or(RoomManagerError::RoomNotFound { id })?;
        let locked_room = room
            .read()
            .map_err(|_| RoomManagerError::RoomLockPoisoned { id })?;

        Ok(locked_room.public_snapshot())
    }

    pub fn private_snapshot_for(
        &self,
        id: RoomId,
        seat: SeatIndex,
    ) -> Result<GameSnapshot, RoomManagerError> {
        let room = self.room(id).ok_or(RoomManagerError::RoomNotFound { id })?;
        let locked_room = room
            .read()
            .map_err(|_| RoomManagerError::RoomLockPoisoned { id })?;

        locked_room
            .private_snapshot_for(seat)
            .map_err(RoomManagerError::Room)
    }

    pub fn handle_room_command(
        &self,
        id: RoomId,
        command: RoomCommand,
    ) -> Result<RoomCommandResult, RoomManagerError> {
        let room = self.room(id).ok_or(RoomManagerError::RoomNotFound { id })?;
        let mut locked_room = room
            .write()
            .map_err(|_| RoomManagerError::RoomLockPoisoned { id })?;

        locked_room
            .handle_command(command)
            .map_err(RoomManagerError::Room)
    }

    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomManagerError {
    RoomAlreadyExists { id: RoomId },
    RoomNotFound { id: RoomId },
    RoomLockPoisoned { id: RoomId },
    Room(RoomError),
}

impl fmt::Display for RoomManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoomAlreadyExists { id } => write!(f, "room {} already exists", id.0),
            Self::RoomNotFound { id } => write!(f, "room {} was not found", id.0),
            Self::RoomLockPoisoned { id } => write!(f, "room {} lock was poisoned", id.0),
            Self::Room(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RoomManagerError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_config() -> TableConfig {
        TableConfig::new(6, 5, 10, 100, 1_000).expect("table config should be valid")
    }

    fn room_with_two_players() -> Room {
        let mut room = Room::new(RoomId(1), table_config());
        room.sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("first player should sit");
        room.sit_player(PlayerId(2), "Linus", SeatIndex(3), 1_000)
            .expect("second player should sit");
        room
    }

    #[test]
    fn room_starts_hand_and_returns_engine_events() {
        let mut room = room_with_two_players();

        let events = room
            .start_hand(SeatIndex(0))
            .expect("hand should start through room");

        assert_eq!(
            events,
            vec![
                GameEvent::HandStarted {
                    dealer_seat: SeatIndex(0),
                    playing_seats: vec![SeatIndex(0), SeatIndex(3)],
                },
                GameEvent::HoleCardsDealt {
                    seats: vec![SeatIndex(0), SeatIndex(3)],
                    cards_per_player: 2,
                },
            ]
        );
        assert!(room.engine().events().is_empty());
    }

    #[test]
    fn room_player_action_returns_events_and_drains_engine_log() {
        let mut room = room_with_two_players();
        room.start_hand(SeatIndex(0))
            .expect("hand should start through room");
        room.advance_hand_phase()
            .expect("hand should advance to posting blinds");
        room.advance_hand_phase()
            .expect("hand should advance to preflop");

        let events = room
            .apply_player_action(SeatIndex(0), PlayerAction::Check)
            .expect("acting player should check");

        assert_eq!(
            events,
            vec![GameEvent::PlayerActed {
                seat: SeatIndex(0),
                action: PlayerAction::Check,
            }]
        );
        assert!(room.engine().events().is_empty());
    }

    #[test]
    fn room_manager_creates_and_returns_shared_room_handles() {
        let mut manager = RoomManager::new();

        let room = manager
            .create_room(RoomId(7), table_config())
            .expect("room should be created");
        let fetched = manager.room(RoomId(7)).expect("room should be stored");

        assert!(Arc::ptr_eq(&room, &fetched));
        assert_eq!(manager.room_count(), 1);
    }

    #[test]
    fn room_manager_rejects_duplicate_room_ids() {
        let mut manager = RoomManager::new();
        manager
            .create_room(RoomId(7), table_config())
            .expect("room should be created");

        let result = manager.create_room(RoomId(7), table_config());

        assert_eq!(
            result.expect_err("duplicate room should be rejected"),
            RoomManagerError::RoomAlreadyExists { id: RoomId(7) }
        );
    }

    #[test]
    fn room_manager_dispatches_commands_to_shared_room() {
        let mut manager = RoomManager::new();
        manager
            .create_room(RoomId(7), table_config())
            .expect("room should be created");

        let result = manager
            .handle_room_command(
                RoomId(7),
                RoomCommand::SitPlayer {
                    id: PlayerId(1),
                    display_name: "Ada".to_string(),
                    seat: SeatIndex(0),
                    buy_in: 1_000,
                },
            )
            .expect("command should be handled");

        assert!(result.events().is_empty());
        assert_eq!(result.snapshot().players().len(), 1);
    }

    #[test]
    fn room_manager_rejects_commands_for_missing_rooms() {
        let manager = RoomManager::new();

        let result = manager.handle_room_command(RoomId(404), RoomCommand::PublicSnapshot);

        assert_eq!(
            result.expect_err("missing room should be rejected"),
            RoomManagerError::RoomNotFound { id: RoomId(404) }
        );
    }

    #[test]
    fn room_manager_closes_room() {
        let mut manager = RoomManager::new();
        manager
            .create_room(RoomId(7), table_config())
            .expect("room should be created");

        let closed_room = manager.close_room(RoomId(7)).expect("room should close");

        assert_eq!(
            closed_room
                .read()
                .expect("room lock should be available")
                .id(),
            RoomId(7)
        );
        assert_eq!(manager.room_count(), 0);
        assert!(manager.room(RoomId(7)).is_none());
    }

    #[test]
    fn room_manager_rejects_closing_missing_room() {
        let mut manager = RoomManager::new();

        let result = manager.close_room(RoomId(404));

        assert!(matches!(
            result,
            Err(RoomManagerError::RoomNotFound { id: RoomId(404) })
        ));
    }

    #[test]
    fn room_summary_reports_lobby_visible_state() {
        let mut room = room_with_two_players();

        let summary = room.summary();

        assert_eq!(summary.id(), RoomId(1));
        assert_eq!(summary.table_config(), table_config());
        assert_eq!(summary.seat_count(), 6);
        assert_eq!(summary.seated_player_count(), 2);
        assert!(summary.can_start_hand());
        assert_eq!(summary.active_phase(), None);

        room.start_hand(SeatIndex(0))
            .expect("hand should start through room");

        assert_eq!(room.summary().active_phase(), Some(GamePhase::StartingHand));
    }

    #[test]
    fn room_leave_seat_removes_player_from_snapshot_and_summary() {
        let mut room = room_with_two_players();

        room.leave_seat(SeatIndex(0))
            .expect("player should leave their seat");

        assert_eq!(room.public_snapshot().players().len(), 1);
        assert_eq!(room.summary().seated_player_count(), 1);
        assert!(!room.summary().can_start_hand());
    }

    #[test]
    fn room_sit_out_and_sit_in_update_player_readiness() {
        let mut room = room_with_two_players();

        room.sit_out(SeatIndex(0))
            .expect("player should be able to sit out");

        let player = room
            .public_snapshot()
            .players()
            .iter()
            .find(|player| player.seat() == SeatIndex(0))
            .expect("player should stay seated")
            .clone();
        assert_eq!(player.status(), poker_engine::PlayerStatus::SittingOut);
        assert!(!room.summary().can_start_hand());

        room.sit_in(SeatIndex(0))
            .expect("player should be able to sit in");

        let player = room
            .public_snapshot()
            .players()
            .iter()
            .find(|player| player.seat() == SeatIndex(0))
            .expect("player should stay seated")
            .clone();
        assert_eq!(player.status(), poker_engine::PlayerStatus::Active);
        assert!(room.summary().can_start_hand());
    }

    #[test]
    fn room_manager_returns_sorted_room_summaries() {
        let mut manager = RoomManager::new();
        manager
            .create_room(RoomId(9), table_config())
            .expect("room should be created");
        manager
            .create_room(RoomId(3), table_config())
            .expect("room should be created");

        let summaries = manager
            .room_summaries()
            .expect("room summaries should be returned");

        assert_eq!(
            summaries.iter().map(RoomSummary::id).collect::<Vec<_>>(),
            vec![RoomId(3), RoomId(9)]
        );
    }

    #[test]
    fn room_manager_returns_public_snapshot_for_room() {
        let mut manager = RoomManager::new();
        manager
            .create_room(RoomId(7), table_config())
            .expect("room should be created");
        manager
            .handle_room_command(
                RoomId(7),
                RoomCommand::SitPlayer {
                    id: PlayerId(1),
                    display_name: "Ada".to_string(),
                    seat: SeatIndex(0),
                    buy_in: 1_000,
                },
            )
            .expect("player should sit");

        let snapshot = manager
            .public_snapshot(RoomId(7))
            .expect("snapshot should be returned");

        assert_eq!(snapshot.players().len(), 1);
    }

    #[test]
    fn room_manager_returns_private_snapshot_for_viewer() {
        let mut manager = RoomManager::new();
        manager
            .create_room(RoomId(7), table_config())
            .expect("room should be created");
        for (id, name, seat) in [
            (PlayerId(1), "Ada", SeatIndex(0)),
            (PlayerId(2), "Linus", SeatIndex(3)),
        ] {
            manager
                .handle_room_command(
                    RoomId(7),
                    RoomCommand::SitPlayer {
                        id,
                        display_name: name.to_string(),
                        seat,
                        buy_in: 1_000,
                    },
                )
                .expect("player should sit");
        }
        manager
            .handle_room_command(
                RoomId(7),
                RoomCommand::StartHand {
                    dealer_seat: SeatIndex(0),
                },
            )
            .expect("hand should start");

        let snapshot = manager
            .private_snapshot_for(RoomId(7), SeatIndex(0))
            .expect("private snapshot should be returned");
        let viewer = snapshot
            .players()
            .iter()
            .find(|player| player.seat() == SeatIndex(0))
            .expect("viewer should be in snapshot");
        let other = snapshot
            .players()
            .iter()
            .find(|player| player.seat() == SeatIndex(3))
            .expect("other player should be in snapshot");

        assert_eq!(
            viewer
                .visible_hole_cards()
                .expect("viewer cards should be visible")
                .len(),
            2
        );
        assert!(other.visible_hole_cards().is_none());
    }

    #[test]
    fn shared_room_allows_mutation_through_write_lock() {
        let mut manager = RoomManager::new();
        let room = manager
            .create_room(RoomId(9), table_config())
            .expect("room should be created");

        {
            let mut locked_room = room.write().expect("room lock should be available");
            locked_room
                .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
                .expect("player should sit through locked room");
        }

        let locked_room = room.read().expect("room lock should be available");
        assert_eq!(locked_room.engine().table().seated_player_count(), 1);
    }

    #[test]
    fn room_reports_public_snapshots_from_inner_engine() {
        let room = room_with_two_players();

        let snapshot = room.public_snapshot();

        assert_eq!(snapshot.players().len(), 2);
        assert!(snapshot.hand().is_none());
    }
}
