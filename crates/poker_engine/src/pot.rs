use std::collections::BTreeSet;

use crate::{ChipAmount, SeatIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PotContribution {
    seat: SeatIndex,
    amount: ChipAmount,
    eligible: bool,
}

impl PotContribution {
    pub const fn new(seat: SeatIndex, amount: ChipAmount, eligible: bool) -> Self {
        Self {
            seat,
            amount,
            eligible,
        }
    }

    pub fn seat(&self) -> SeatIndex {
        self.seat
    }

    pub fn amount(&self) -> ChipAmount {
        self.amount
    }

    pub fn eligible(&self) -> bool {
        self.eligible
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidePot {
    amount: ChipAmount,
    eligible_seats: Vec<SeatIndex>,
}

impl SidePot {
    pub fn amount(&self) -> ChipAmount {
        self.amount
    }

    pub fn eligible_seats(&self) -> &[SeatIndex] {
        &self.eligible_seats
    }
}

pub fn calculate_side_pots(contributions: &[PotContribution]) -> Vec<SidePot> {
    let thresholds: BTreeSet<_> = contributions
        .iter()
        .filter_map(|contribution| (contribution.amount > 0).then_some(contribution.amount))
        .collect();
    let mut side_pots = Vec::new();
    let mut previous_threshold = 0;

    for threshold in thresholds {
        let contributors: Vec<_> = contributions
            .iter()
            .filter(|contribution| contribution.amount >= threshold)
            .collect();
        let amount = (threshold - previous_threshold) * contributors.len() as ChipAmount;
        let mut eligible_seats = contributors
            .iter()
            .filter_map(|contribution| contribution.eligible.then_some(contribution.seat))
            .collect::<Vec<_>>();
        eligible_seats.sort_by_key(|seat| seat.0);

        if amount > 0 && !eligible_seats.is_empty() {
            side_pots.push(SidePot {
                amount,
                eligible_seats,
            });
        }

        previous_threshold = threshold;
    }

    side_pots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_contributions_create_one_pot_for_all_eligible_players() {
        let side_pots = calculate_side_pots(&[
            PotContribution::new(SeatIndex(0), 100, true),
            PotContribution::new(SeatIndex(3), 100, true),
            PotContribution::new(SeatIndex(5), 100, true),
        ]);

        assert_eq!(side_pots.len(), 1);
        assert_eq!(side_pots[0].amount(), 300);
        assert_eq!(
            side_pots[0].eligible_seats(),
            &[SeatIndex(0), SeatIndex(3), SeatIndex(5)]
        );
    }

    #[test]
    fn all_in_contributions_create_main_and_side_pots() {
        let side_pots = calculate_side_pots(&[
            PotContribution::new(SeatIndex(0), 40, true),
            PotContribution::new(SeatIndex(3), 100, true),
            PotContribution::new(SeatIndex(5), 100, true),
        ]);

        assert_eq!(side_pots.len(), 2);
        assert_eq!(side_pots[0].amount(), 120);
        assert_eq!(
            side_pots[0].eligible_seats(),
            &[SeatIndex(0), SeatIndex(3), SeatIndex(5)]
        );
        assert_eq!(side_pots[1].amount(), 120);
        assert_eq!(side_pots[1].eligible_seats(), &[SeatIndex(3), SeatIndex(5)]);
    }

    #[test]
    fn folded_players_contribute_dead_chips_but_are_not_eligible() {
        let side_pots = calculate_side_pots(&[
            PotContribution::new(SeatIndex(0), 100, true),
            PotContribution::new(SeatIndex(3), 100, true),
            PotContribution::new(SeatIndex(5), 100, false),
        ]);

        assert_eq!(side_pots.len(), 1);
        assert_eq!(side_pots[0].amount(), 300);
        assert_eq!(side_pots[0].eligible_seats(), &[SeatIndex(0), SeatIndex(3)]);
    }
}
