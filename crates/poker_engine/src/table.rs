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
