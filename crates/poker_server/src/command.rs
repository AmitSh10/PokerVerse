use poker_engine::{ChipAmount, GameEvent, GameSnapshot, PlayerAction, PlayerId, SeatIndex};
use serde::{Deserialize, Serialize};

use crate::{Room, RoomError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomCommand {
    SitPlayer {
        id: PlayerId,
        display_name: String,
        seat: SeatIndex,
        buy_in: ChipAmount,
    },
    StartHand {
        dealer_seat: SeatIndex,
    },
    AdvanceHandPhase,
    PostBlinds,
    ApplyPlayerAction {
        seat: SeatIndex,
        action: PlayerAction,
    },
    PublicSnapshot,
    PrivateSnapshot {
        seat: SeatIndex,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomCommandResult {
    events: Vec<GameEvent>,
    snapshot: GameSnapshot,
}

impl RoomCommandResult {
    pub fn new(events: Vec<GameEvent>, snapshot: GameSnapshot) -> Self {
        Self { events, snapshot }
    }

    pub fn events(&self) -> &[GameEvent] {
        &self.events
    }

    pub fn snapshot(&self) -> &GameSnapshot {
        &self.snapshot
    }
}

impl Room {
    pub fn handle_command(&mut self, command: RoomCommand) -> Result<RoomCommandResult, RoomError> {
        let (events, snapshot) = match command {
            RoomCommand::SitPlayer {
                id,
                display_name,
                seat,
                buy_in,
            } => {
                self.sit_player(id, display_name, seat, buy_in)?;
                (self.drain_events(), self.public_snapshot())
            }
            RoomCommand::StartHand { dealer_seat } => {
                let events = self.start_hand(dealer_seat)?;
                (events, self.public_snapshot())
            }
            RoomCommand::AdvanceHandPhase => {
                let events = self.advance_hand_phase()?;
                (events, self.public_snapshot())
            }
            RoomCommand::PostBlinds => {
                let events = self.post_blinds()?;
                (events, self.public_snapshot())
            }
            RoomCommand::ApplyPlayerAction { seat, action } => {
                let events = self.apply_player_action(seat, action)?;
                (events, self.public_snapshot())
            }
            RoomCommand::PublicSnapshot => (self.drain_events(), self.public_snapshot()),
            RoomCommand::PrivateSnapshot { seat } => {
                (self.drain_events(), self.private_snapshot_for(seat)?)
            }
        };

        Ok(RoomCommandResult::new(events, snapshot))
    }
}

#[cfg(test)]
mod tests {
    use poker_engine::{GameEvent, TableConfig};

    use super::*;
    use crate::{Room, RoomId};

    fn table_config() -> TableConfig {
        TableConfig::new(6, 5, 10, 100, 1_000).expect("table config should be valid")
    }

    fn room_with_two_players() -> Room {
        let mut room = Room::new(RoomId(1), table_config());
        room.handle_command(RoomCommand::SitPlayer {
            id: PlayerId(1),
            display_name: "Ada".to_string(),
            seat: SeatIndex(0),
            buy_in: 1_000,
        })
        .expect("first player should sit");
        room.handle_command(RoomCommand::SitPlayer {
            id: PlayerId(2),
            display_name: "Linus".to_string(),
            seat: SeatIndex(3),
            buy_in: 1_000,
        })
        .expect("second player should sit");
        room
    }

    #[test]
    fn sit_player_command_returns_updated_snapshot_without_events() {
        let mut room = Room::new(RoomId(1), table_config());

        let result = room
            .handle_command(RoomCommand::SitPlayer {
                id: PlayerId(1),
                display_name: "Ada".to_string(),
                seat: SeatIndex(0),
                buy_in: 1_000,
            })
            .expect("player should sit");

        assert!(result.events().is_empty());
        assert_eq!(result.snapshot().players().len(), 1);
    }

    #[test]
    fn start_hand_command_returns_start_events_and_snapshot() {
        let mut room = room_with_two_players();

        let result = room
            .handle_command(RoomCommand::StartHand {
                dealer_seat: SeatIndex(0),
            })
            .expect("hand should start");

        assert_eq!(
            result.events(),
            &[
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
        assert!(result.snapshot().hand().is_some());
    }

    #[test]
    fn player_action_command_returns_action_event() {
        let mut room = room_with_two_players();
        room.handle_command(RoomCommand::StartHand {
            dealer_seat: SeatIndex(0),
        })
        .expect("hand should start");
        room.handle_command(RoomCommand::AdvanceHandPhase)
            .expect("hand should advance to posting blinds");
        room.handle_command(RoomCommand::AdvanceHandPhase)
            .expect("hand should advance to preflop");

        let result = room
            .handle_command(RoomCommand::ApplyPlayerAction {
                seat: SeatIndex(0),
                action: PlayerAction::Check,
            })
            .expect("player should act");

        assert_eq!(
            result.events(),
            &[GameEvent::PlayerActed {
                seat: SeatIndex(0),
                action: PlayerAction::Check,
            }]
        );
    }

    #[test]
    fn private_snapshot_command_reveals_only_viewer_hole_cards() {
        let mut room = room_with_two_players();
        room.handle_command(RoomCommand::StartHand {
            dealer_seat: SeatIndex(0),
        })
        .expect("hand should start");

        let result = room
            .handle_command(RoomCommand::PrivateSnapshot { seat: SeatIndex(0) })
            .expect("private snapshot should be returned");

        let viewer = result
            .snapshot()
            .players()
            .iter()
            .find(|player| player.seat() == SeatIndex(0))
            .expect("viewer should be in snapshot");
        let other = result
            .snapshot()
            .players()
            .iter()
            .find(|player| player.seat() == SeatIndex(3))
            .expect("other player should be in snapshot");

        assert_eq!(
            viewer
                .visible_hole_cards()
                .expect("cards should be visible")
                .len(),
            2
        );
        assert!(other.visible_hole_cards().is_none());
    }

    #[test]
    fn room_command_round_trips_through_json() {
        let command = RoomCommand::ApplyPlayerAction {
            seat: SeatIndex(3),
            action: PlayerAction::Raise { amount: 40 },
        };

        let json = serde_json::to_string(&command).expect("command should serialize");
        let decoded: RoomCommand = serde_json::from_str(&json).expect("command should deserialize");

        assert_eq!(decoded, command);
    }

    #[test]
    fn room_command_result_round_trips_through_json() {
        let mut room = room_with_two_players();
        let result = room
            .handle_command(RoomCommand::StartHand {
                dealer_seat: SeatIndex(0),
            })
            .expect("hand should start");

        let json = serde_json::to_string(&result).expect("result should serialize");
        let decoded: RoomCommandResult =
            serde_json::from_str(&json).expect("result should deserialize");

        assert_eq!(decoded, result);
    }
}
