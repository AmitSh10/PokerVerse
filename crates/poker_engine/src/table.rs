use std::fmt;

use crate::{ChipAmount, Player, PlayerId, SeatIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableConfig {
    max_seats: u8,
    small_blind: ChipAmount,
    big_blind: ChipAmount,
    min_buy_in: ChipAmount,
    max_buy_in: ChipAmount,
}

impl TableConfig {
    pub fn new(
        max_seats: u8,
        small_blind: ChipAmount,
        big_blind: ChipAmount,
        min_buy_in: ChipAmount,
        max_buy_in: ChipAmount,
    ) -> Result<Self, TableConfigError> {
        if max_seats < 2 {
            return Err(TableConfigError::TooFewSeats {
                requested: max_seats,
            });
        }

        if small_blind == 0 || big_blind <= small_blind {
            return Err(TableConfigError::InvalidBlindStructure {
                small_blind,
                big_blind,
            });
        }

        if min_buy_in == 0 || max_buy_in < min_buy_in {
            return Err(TableConfigError::InvalidBuyInRange {
                min_buy_in,
                max_buy_in,
            });
        }

        Ok(Self {
            max_seats,
            small_blind,
            big_blind,
            min_buy_in,
            max_buy_in,
        })
    }

    pub fn max_seats(&self) -> u8 {
        self.max_seats
    }

    pub fn small_blind(&self) -> ChipAmount {
        self.small_blind
    }

    pub fn big_blind(&self) -> ChipAmount {
        self.big_blind
    }

    pub fn min_buy_in(&self) -> ChipAmount {
        self.min_buy_in
    }

    pub fn max_buy_in(&self) -> ChipAmount {
        self.max_buy_in
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableConfigError {
    TooFewSeats {
        requested: u8,
    },
    InvalidBlindStructure {
        small_blind: ChipAmount,
        big_blind: ChipAmount,
    },
    InvalidBuyInRange {
        min_buy_in: ChipAmount,
        max_buy_in: ChipAmount,
    },
}

impl fmt::Display for TableConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewSeats { requested } => {
                write!(f, "table needs at least two seats, got {requested}")
            }
            Self::InvalidBlindStructure {
                small_blind,
                big_blind,
            } => write!(
                f,
                "invalid blinds: small blind {small_blind}, big blind {big_blind}"
            ),
            Self::InvalidBuyInRange {
                min_buy_in,
                max_buy_in,
            } => write!(
                f,
                "invalid buy-in range: min {min_buy_in}, max {max_buy_in}"
            ),
        }
    }
}

impl std::error::Error for TableConfigError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandPositions {
    dealer: SeatIndex,
    small_blind: SeatIndex,
    big_blind: SeatIndex,
    first_to_act: SeatIndex,
}

impl HandPositions {
    pub const fn new(
        dealer: SeatIndex,
        small_blind: SeatIndex,
        big_blind: SeatIndex,
        first_to_act: SeatIndex,
    ) -> Self {
        Self {
            dealer,
            small_blind,
            big_blind,
            first_to_act,
        }
    }

    pub fn dealer(&self) -> SeatIndex {
        self.dealer
    }

    pub fn small_blind(&self) -> SeatIndex {
        self.small_blind
    }

    pub fn big_blind(&self) -> SeatIndex {
        self.big_blind
    }

    pub fn first_to_act(&self) -> SeatIndex {
        self.first_to_act
    }
}

#[derive(Debug, Clone)]
pub struct Table {
    config: TableConfig,
    seats: Vec<Option<Player>>,
}

impl Table {
    pub fn new(config: TableConfig) -> Self {
        Self {
            config,
            seats: vec![None; config.max_seats() as usize],
        }
    }

    pub fn config(&self) -> TableConfig {
        self.config
    }

    pub fn seat_count(&self) -> usize {
        self.seats.len()
    }

    pub fn seated_player_count(&self) -> usize {
        self.seats.iter().filter(|seat| seat.is_some()).count()
    }

    pub fn occupied_seats(&self) -> Vec<SeatIndex> {
        self.seats
            .iter()
            .enumerate()
            .filter_map(|(index, seat)| seat.as_ref().map(|_| SeatIndex(index as u8)))
            .collect()
    }

    pub fn playing_seats(&self) -> Vec<SeatIndex> {
        self.seats
            .iter()
            .enumerate()
            .filter_map(|(index, seat)| match seat {
                Some(player) if player.can_play_hand() => Some(SeatIndex(index as u8)),
                _ => None,
            })
            .collect()
    }

    pub fn can_start_hand(&self) -> bool {
        self.playing_seats().len() >= 2
    }

    pub fn hand_positions(
        &self,
        dealer_seat: SeatIndex,
    ) -> Result<Option<HandPositions>, TableError> {
        self.seat_offset(dealer_seat)?;

        let playing_count = self.playing_seats().len();

        if playing_count < 2 {
            return Ok(None);
        }

        if !self
            .player_at(dealer_seat)
            .is_some_and(Player::can_play_hand)
        {
            return Err(TableError::DealerCannotPlay { seat: dealer_seat });
        }

        if playing_count == 2 {
            let big_blind = self.required_next_playing_seat_after(dealer_seat)?;

            return Ok(Some(HandPositions::new(
                dealer_seat,
                dealer_seat,
                big_blind,
                dealer_seat,
            )));
        }

        let small_blind = self.required_next_playing_seat_after(dealer_seat)?;
        let big_blind = self.required_next_playing_seat_after(small_blind)?;
        let first_to_act = self.required_next_playing_seat_after(big_blind)?;

        Ok(Some(HandPositions::new(
            dealer_seat,
            small_blind,
            big_blind,
            first_to_act,
        )))
    }

    pub fn available_seats(&self) -> Vec<SeatIndex> {
        self.seats
            .iter()
            .enumerate()
            .filter_map(|(index, seat)| {
                if seat.is_none() {
                    Some(SeatIndex(index as u8))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn sit_player(
        &mut self,
        id: PlayerId,
        display_name: impl Into<String>,
        seat: SeatIndex,
        buy_in: ChipAmount,
    ) -> Result<(), TableError> {
        let seat_index = self.seat_offset(seat)?;
        self.validate_buy_in(buy_in)?;

        if self.seats[seat_index].is_some() {
            return Err(TableError::SeatOccupied { seat });
        }

        self.seats[seat_index] = Some(Player::new(id, display_name, seat, buy_in));
        Ok(())
    }

    pub fn leave_seat(&mut self, seat: SeatIndex) -> Result<Player, TableError> {
        let seat_index = self.seat_offset(seat)?;

        self.seats[seat_index]
            .take()
            .ok_or(TableError::SeatEmpty { seat })
    }

    pub fn player_at(&self, seat: SeatIndex) -> Option<&Player> {
        let seat_index = self.seat_offset(seat).ok()?;
        self.seats[seat_index].as_ref()
    }

    pub fn player_at_mut(&mut self, seat: SeatIndex) -> Option<&mut Player> {
        let seat_index = self.seat_offset(seat).ok()?;
        self.seats[seat_index].as_mut()
    }

    pub fn next_occupied_seat_after(
        &self,
        seat: SeatIndex,
    ) -> Result<Option<SeatIndex>, TableError> {
        self.next_seat_after(seat, |_| true)
    }

    pub fn next_playing_seat_after(
        &self,
        seat: SeatIndex,
    ) -> Result<Option<SeatIndex>, TableError> {
        self.next_seat_after(seat, Player::can_play_hand)
    }

    fn required_next_playing_seat_after(&self, seat: SeatIndex) -> Result<SeatIndex, TableError> {
        self.next_playing_seat_after(seat)?
            .ok_or_else(|| TableError::NotEnoughPlayingPlayers {
                playing_count: self.playing_seats().len(),
            })
    }

    fn next_seat_after<F>(
        &self,
        seat: SeatIndex,
        mut is_candidate: F,
    ) -> Result<Option<SeatIndex>, TableError>
    where
        F: FnMut(&Player) -> bool,
    {
        let start_index = self.seat_offset(seat)?;

        if self.seats.len() <= 1 {
            return Ok(None);
        }

        for step in 1..self.seats.len() {
            let index = (start_index + step) % self.seats.len();

            if let Some(player) = self.seats[index].as_ref() {
                if is_candidate(player) {
                    return Ok(Some(SeatIndex(index as u8)));
                }
            }
        }

        Ok(None)
    }

    fn seat_offset(&self, seat: SeatIndex) -> Result<usize, TableError> {
        let seat_index = seat.0 as usize;

        if seat_index >= self.seats.len() {
            return Err(TableError::InvalidSeat {
                seat,
                max_seats: self.config.max_seats(),
            });
        }

        Ok(seat_index)
    }

    fn validate_buy_in(&self, buy_in: ChipAmount) -> Result<(), TableError> {
        if buy_in < self.config.min_buy_in() {
            return Err(TableError::BuyInTooSmall {
                buy_in,
                min_buy_in: self.config.min_buy_in(),
            });
        }

        if buy_in > self.config.max_buy_in() {
            return Err(TableError::BuyInTooLarge {
                buy_in,
                max_buy_in: self.config.max_buy_in(),
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableError {
    InvalidSeat {
        seat: SeatIndex,
        max_seats: u8,
    },
    SeatOccupied {
        seat: SeatIndex,
    },
    SeatEmpty {
        seat: SeatIndex,
    },
    BuyInTooSmall {
        buy_in: ChipAmount,
        min_buy_in: ChipAmount,
    },
    BuyInTooLarge {
        buy_in: ChipAmount,
        max_buy_in: ChipAmount,
    },
    DealerCannotPlay {
        seat: SeatIndex,
    },
    NotEnoughPlayingPlayers {
        playing_count: usize,
    },
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSeat { seat, max_seats } => {
                write!(f, "seat {} is outside table size {max_seats}", seat.0)
            }
            Self::SeatOccupied { seat } => write!(f, "seat {} is already occupied", seat.0),
            Self::SeatEmpty { seat } => write!(f, "seat {} is empty", seat.0),
            Self::BuyInTooSmall { buy_in, min_buy_in } => {
                write!(f, "buy-in {buy_in} is below minimum {min_buy_in}")
            }
            Self::BuyInTooLarge { buy_in, max_buy_in } => {
                write!(f, "buy-in {buy_in} is above maximum {max_buy_in}")
            }
            Self::DealerCannotPlay { seat } => {
                write!(f, "dealer seat {} cannot play this hand", seat.0)
            }
            Self::NotEnoughPlayingPlayers { playing_count } => {
                write!(f, "need at least two playing players, got {playing_count}")
            }
        }
    }
}

impl std::error::Error for TableError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> TableConfig {
        TableConfig::new(6, 5, 10, 500, 2_000).expect("config should be valid")
    }

    fn table() -> Table {
        Table::new(config())
    }

    #[test]
    fn table_config_rejects_too_few_seats() {
        let result = TableConfig::new(1, 5, 10, 500, 2_000);

        assert_eq!(result, Err(TableConfigError::TooFewSeats { requested: 1 }));
    }

    #[test]
    fn table_config_rejects_invalid_blinds() {
        let result = TableConfig::new(6, 10, 10, 500, 2_000);

        assert_eq!(
            result,
            Err(TableConfigError::InvalidBlindStructure {
                small_blind: 10,
                big_blind: 10,
            })
        );
    }

    #[test]
    fn table_config_rejects_invalid_buy_in_range() {
        let result = TableConfig::new(6, 5, 10, 2_000, 500);

        assert_eq!(
            result,
            Err(TableConfigError::InvalidBuyInRange {
                min_buy_in: 2_000,
                max_buy_in: 500,
            })
        );
    }

    #[test]
    fn new_table_starts_with_empty_seats() {
        let table = table();

        assert_eq!(table.seat_count(), 6);
        assert_eq!(table.seated_player_count(), 0);
        assert_eq!(
            table.available_seats(),
            vec![
                SeatIndex(0),
                SeatIndex(1),
                SeatIndex(2),
                SeatIndex(3),
                SeatIndex(4),
                SeatIndex(5),
            ]
        );
    }

    #[test]
    fn player_can_sit_at_empty_seat_with_valid_buy_in() {
        let mut table = table();

        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(2), 1_000)
            .expect("seat should be available");

        let player = table
            .player_at(SeatIndex(2))
            .expect("player should be seated");

        assert_eq!(table.seated_player_count(), 1);
        assert_eq!(player.id(), PlayerId(1));
        assert_eq!(player.display_name(), "Ada");
        assert_eq!(player.seat(), SeatIndex(2));
        assert_eq!(player.stack(), 1_000);
    }

    #[test]
    fn occupied_seats_returns_only_seats_with_players() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(2), "Grace", SeatIndex(3), 1_000)
            .expect("player should be seated");

        assert_eq!(table.occupied_seats(), vec![SeatIndex(0), SeatIndex(3)]);
    }

    #[test]
    fn playing_seats_excludes_players_who_cannot_start_hand() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(2), "Grace", SeatIndex(3), 1_000)
            .expect("player should be seated");

        table
            .player_at_mut(SeatIndex(3))
            .expect("player should be seated")
            .sit_out();

        assert_eq!(table.playing_seats(), vec![SeatIndex(0)]);
    }

    #[test]
    fn hand_can_start_with_at_least_two_playing_seats() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");

        assert!(!table.can_start_hand());

        table
            .sit_player(PlayerId(2), "Grace", SeatIndex(3), 1_000)
            .expect("player should be seated");

        assert!(table.can_start_hand());
    }

    #[test]
    fn next_occupied_seat_after_wraps_around_table() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(1), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(2), "Grace", SeatIndex(4), 1_000)
            .expect("player should be seated");

        assert_eq!(
            table.next_occupied_seat_after(SeatIndex(4)),
            Ok(Some(SeatIndex(1)))
        );
    }

    #[test]
    fn next_playing_seat_after_skips_sitting_out_players() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(2), "Grace", SeatIndex(2), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(3), "Linus", SeatIndex(4), 1_000)
            .expect("player should be seated");

        table
            .player_at_mut(SeatIndex(2))
            .expect("player should be seated")
            .sit_out();

        assert_eq!(
            table.next_playing_seat_after(SeatIndex(0)),
            Ok(Some(SeatIndex(4)))
        );
    }

    #[test]
    fn next_playing_seat_after_returns_none_when_there_is_no_other_playing_seat() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");

        assert_eq!(table.next_playing_seat_after(SeatIndex(0)), Ok(None));
    }

    #[test]
    fn hand_positions_returns_none_when_fewer_than_two_players_can_play() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");

        assert_eq!(table.hand_positions(SeatIndex(0)), Ok(None));
    }

    #[test]
    fn heads_up_hand_positions_make_dealer_the_small_blind() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(2), "Grace", SeatIndex(3), 1_000)
            .expect("player should be seated");

        let positions = table
            .hand_positions(SeatIndex(0))
            .expect("dealer seat should be valid")
            .expect("hand should be able to start");

        assert_eq!(positions.dealer(), SeatIndex(0));
        assert_eq!(positions.small_blind(), SeatIndex(0));
        assert_eq!(positions.big_blind(), SeatIndex(3));
        assert_eq!(positions.first_to_act(), SeatIndex(0));
    }

    #[test]
    fn multi_player_hand_positions_start_after_big_blind() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(2), "Grace", SeatIndex(2), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(3), "Linus", SeatIndex(4), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(4), "Margaret", SeatIndex(5), 1_000)
            .expect("player should be seated");

        let positions = table
            .hand_positions(SeatIndex(0))
            .expect("dealer seat should be valid")
            .expect("hand should be able to start");

        assert_eq!(positions.dealer(), SeatIndex(0));
        assert_eq!(positions.small_blind(), SeatIndex(2));
        assert_eq!(positions.big_blind(), SeatIndex(4));
        assert_eq!(positions.first_to_act(), SeatIndex(5));
    }

    #[test]
    fn hand_positions_skip_sitting_out_players() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(2), "Grace", SeatIndex(1), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(3), "Linus", SeatIndex(3), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(4), "Margaret", SeatIndex(5), 1_000)
            .expect("player should be seated");

        table
            .player_at_mut(SeatIndex(1))
            .expect("player should be seated")
            .sit_out();

        let positions = table
            .hand_positions(SeatIndex(0))
            .expect("dealer seat should be valid")
            .expect("hand should be able to start");

        assert_eq!(positions.small_blind(), SeatIndex(3));
        assert_eq!(positions.big_blind(), SeatIndex(5));
        assert_eq!(positions.first_to_act(), SeatIndex(0));
    }

    #[test]
    fn hand_positions_reject_non_playing_dealer_when_hand_can_start() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(2), "Grace", SeatIndex(2), 1_000)
            .expect("player should be seated");
        table
            .sit_player(PlayerId(3), "Linus", SeatIndex(4), 1_000)
            .expect("player should be seated");

        table
            .player_at_mut(SeatIndex(0))
            .expect("player should be seated")
            .sit_out();

        assert_eq!(
            table.hand_positions(SeatIndex(0)),
            Err(TableError::DealerCannotPlay { seat: SeatIndex(0) })
        );
    }

    #[test]
    fn player_cannot_sit_outside_table_size() {
        let mut table = table();

        let result = table.sit_player(PlayerId(1), "Ada", SeatIndex(6), 1_000);

        assert_eq!(
            result,
            Err(TableError::InvalidSeat {
                seat: SeatIndex(6),
                max_seats: 6,
            })
        );
    }

    #[test]
    fn player_cannot_sit_in_occupied_seat() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("first player should be seated");

        let result = table.sit_player(PlayerId(2), "Grace", SeatIndex(0), 1_000);

        assert_eq!(result, Err(TableError::SeatOccupied { seat: SeatIndex(0) }));
    }

    #[test]
    fn player_cannot_buy_in_below_minimum() {
        let mut table = table();

        let result = table.sit_player(PlayerId(1), "Ada", SeatIndex(0), 499);

        assert_eq!(
            result,
            Err(TableError::BuyInTooSmall {
                buy_in: 499,
                min_buy_in: 500,
            })
        );
    }

    #[test]
    fn player_cannot_buy_in_above_maximum() {
        let mut table = table();

        let result = table.sit_player(PlayerId(1), "Ada", SeatIndex(0), 2_001);

        assert_eq!(
            result,
            Err(TableError::BuyInTooLarge {
                buy_in: 2_001,
                max_buy_in: 2_000,
            })
        );
    }

    #[test]
    fn leaving_a_seat_removes_and_returns_player() {
        let mut table = table();
        table
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("player should be seated");

        let player = table
            .leave_seat(SeatIndex(0))
            .expect("seat should contain a player");

        assert_eq!(player.id(), PlayerId(1));
        assert_eq!(table.seated_player_count(), 0);
        assert!(table.player_at(SeatIndex(0)).is_none());
    }

    #[test]
    fn leaving_empty_seat_returns_error() {
        let mut table = table();

        let result = table.leave_seat(SeatIndex(0));

        assert_eq!(result, Err(TableError::SeatEmpty { seat: SeatIndex(0) }));
    }
}
