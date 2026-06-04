use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;

use crate::{Card, Rank};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HandCategory {
    HighCard,
    OnePair,
    TwoPair,
    ThreeOfAKind,
    Straight,
    Flush,
    FullHouse,
    FourOfAKind,
    StraightFlush,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HandRank {
    category: HandCategory,
    kickers: Vec<Rank>,
}

impl HandRank {
    pub fn category(&self) -> HandCategory {
        self.category
    }

    pub fn kickers(&self) -> &[Rank] {
        &self.kickers
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedHand {
    rank: HandRank,
    cards: [Card; 5],
}

impl EvaluatedHand {
    pub fn rank(&self) -> &HandRank {
        &self.rank
    }

    pub fn cards(&self) -> &[Card; 5] {
        &self.cards
    }
}

impl Ord for EvaluatedHand {
    fn cmp(&self, other: &Self) -> Ordering {
        self.rank.cmp(&other.rank)
    }
}

impl PartialOrd for EvaluatedHand {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandEvaluationError {
    TooFewCards { provided: usize },
    TooManyCards { provided: usize },
}

impl fmt::Display for HandEvaluationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewCards { provided } => {
                write!(
                    f,
                    "need at least 5 cards to evaluate a hand, got {provided}"
                )
            }
            Self::TooManyCards { provided } => {
                write!(f, "can evaluate at most 7 cards, got {provided}")
            }
        }
    }
}

impl std::error::Error for HandEvaluationError {}

pub fn evaluate_best_hand(cards: &[Card]) -> Result<EvaluatedHand, HandEvaluationError> {
    if cards.len() < 5 {
        return Err(HandEvaluationError::TooFewCards {
            provided: cards.len(),
        });
    }

    if cards.len() > 7 {
        return Err(HandEvaluationError::TooManyCards {
            provided: cards.len(),
        });
    }

    let mut best = None;

    for a in 0..cards.len() - 4 {
        for b in a + 1..cards.len() - 3 {
            for c in b + 1..cards.len() - 2 {
                for d in c + 1..cards.len() - 1 {
                    for e in d + 1..cards.len() {
                        let candidate_cards = [cards[a], cards[b], cards[c], cards[d], cards[e]];
                        let candidate = evaluate_five_cards(candidate_cards);

                        if best.as_ref().is_none_or(|current| candidate > *current) {
                            best = Some(candidate);
                        }
                    }
                }
            }
        }
    }

    Ok(best.expect("5 to 7 cards always produces at least one 5-card combination"))
}

fn evaluate_five_cards(cards: [Card; 5]) -> EvaluatedHand {
    let flush = cards.iter().all(|card| card.suit == cards[0].suit);
    let straight_high = straight_high_card(&cards);
    let groups = rank_groups(&cards);

    let rank = if flush {
        if let Some(high_card) = straight_high {
            HandRank {
                category: HandCategory::StraightFlush,
                kickers: vec![high_card],
            }
        } else {
            HandRank {
                category: HandCategory::Flush,
                kickers: sorted_ranks_desc(&cards),
            }
        }
    } else if groups[0].1 == 4 {
        HandRank {
            category: HandCategory::FourOfAKind,
            kickers: vec![groups[0].0, groups[1].0],
        }
    } else if groups[0].1 == 3 && groups[1].1 == 2 {
        HandRank {
            category: HandCategory::FullHouse,
            kickers: vec![groups[0].0, groups[1].0],
        }
    } else if let Some(high_card) = straight_high {
        HandRank {
            category: HandCategory::Straight,
            kickers: vec![high_card],
        }
    } else if groups[0].1 == 3 {
        let mut kickers = vec![groups[0].0];
        kickers.extend(groups.iter().skip(1).map(|(rank, _)| *rank));

        HandRank {
            category: HandCategory::ThreeOfAKind,
            kickers,
        }
    } else if groups[0].1 == 2 && groups[1].1 == 2 {
        HandRank {
            category: HandCategory::TwoPair,
            kickers: vec![groups[0].0, groups[1].0, groups[2].0],
        }
    } else if groups[0].1 == 2 {
        let mut kickers = vec![groups[0].0];
        kickers.extend(groups.iter().skip(1).map(|(rank, _)| *rank));

        HandRank {
            category: HandCategory::OnePair,
            kickers,
        }
    } else {
        HandRank {
            category: HandCategory::HighCard,
            kickers: sorted_ranks_desc(&cards),
        }
    };

    EvaluatedHand { rank, cards }
}

fn rank_groups(cards: &[Card; 5]) -> Vec<(Rank, usize)> {
    let mut counts = HashMap::new();

    for card in cards {
        *counts.entry(card.rank).or_insert(0) += 1;
    }

    let mut groups: Vec<_> = counts.into_iter().collect();
    groups.sort_by(|(left_rank, left_count), (right_rank, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| right_rank.cmp(left_rank))
    });
    groups
}

fn sorted_ranks_desc(cards: &[Card; 5]) -> Vec<Rank> {
    let mut ranks: Vec<_> = cards.iter().map(|card| card.rank).collect();
    ranks.sort_by(|left, right| right.cmp(left));
    ranks
}

fn straight_high_card(cards: &[Card; 5]) -> Option<Rank> {
    let mut values: Vec<_> = cards.iter().map(|card| rank_value(card.rank)).collect();
    values.sort_unstable();
    values.dedup();
    values.sort_by(|left, right| right.cmp(left));

    if values.len() != 5 {
        return None;
    }

    if values == [14, 5, 4, 3, 2] {
        return Some(Rank::Five);
    }

    if values.windows(2).all(|window| window[0] == window[1] + 1) {
        value_rank(values[0])
    } else {
        None
    }
}

fn rank_value(rank: Rank) -> u8 {
    match rank {
        Rank::Two => 2,
        Rank::Three => 3,
        Rank::Four => 4,
        Rank::Five => 5,
        Rank::Six => 6,
        Rank::Seven => 7,
        Rank::Eight => 8,
        Rank::Nine => 9,
        Rank::Ten => 10,
        Rank::Jack => 11,
        Rank::Queen => 12,
        Rank::King => 13,
        Rank::Ace => 14,
    }
}

fn value_rank(value: u8) -> Option<Rank> {
    match value {
        2 => Some(Rank::Two),
        3 => Some(Rank::Three),
        4 => Some(Rank::Four),
        5 => Some(Rank::Five),
        6 => Some(Rank::Six),
        7 => Some(Rank::Seven),
        8 => Some(Rank::Eight),
        9 => Some(Rank::Nine),
        10 => Some(Rank::Ten),
        11 => Some(Rank::Jack),
        12 => Some(Rank::Queen),
        13 => Some(Rank::King),
        14 => Some(Rank::Ace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::Suit;

    use super::*;

    fn card(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    fn category(cards: &[Card]) -> HandCategory {
        evaluate_best_hand(cards)
            .expect("hand should evaluate")
            .rank()
            .category()
    }

    #[test]
    fn evaluates_high_card() {
        assert_eq!(
            category(&[
                card(Rank::Ace, Suit::Clubs),
                card(Rank::King, Suit::Diamonds),
                card(Rank::Nine, Suit::Hearts),
                card(Rank::Seven, Suit::Spades),
                card(Rank::Three, Suit::Clubs),
            ]),
            HandCategory::HighCard
        );
    }

    #[test]
    fn evaluates_one_pair() {
        assert_eq!(
            category(&[
                card(Rank::Ace, Suit::Clubs),
                card(Rank::Ace, Suit::Diamonds),
                card(Rank::Nine, Suit::Hearts),
                card(Rank::Seven, Suit::Spades),
                card(Rank::Three, Suit::Clubs),
            ]),
            HandCategory::OnePair
        );
    }

    #[test]
    fn evaluates_two_pair() {
        assert_eq!(
            category(&[
                card(Rank::Ace, Suit::Clubs),
                card(Rank::Ace, Suit::Diamonds),
                card(Rank::Nine, Suit::Hearts),
                card(Rank::Nine, Suit::Spades),
                card(Rank::Three, Suit::Clubs),
            ]),
            HandCategory::TwoPair
        );
    }

    #[test]
    fn evaluates_three_of_a_kind() {
        assert_eq!(
            category(&[
                card(Rank::Queen, Suit::Clubs),
                card(Rank::Queen, Suit::Diamonds),
                card(Rank::Queen, Suit::Hearts),
                card(Rank::Nine, Suit::Spades),
                card(Rank::Three, Suit::Clubs),
            ]),
            HandCategory::ThreeOfAKind
        );
    }

    #[test]
    fn evaluates_straight_with_wheel() {
        let hand = evaluate_best_hand(&[
            card(Rank::Ace, Suit::Clubs),
            card(Rank::Five, Suit::Diamonds),
            card(Rank::Four, Suit::Hearts),
            card(Rank::Three, Suit::Spades),
            card(Rank::Two, Suit::Clubs),
        ])
        .expect("hand should evaluate");

        assert_eq!(hand.rank().category(), HandCategory::Straight);
        assert_eq!(hand.rank().kickers(), &[Rank::Five]);
    }

    #[test]
    fn evaluates_flush() {
        assert_eq!(
            category(&[
                card(Rank::Ace, Suit::Spades),
                card(Rank::Jack, Suit::Spades),
                card(Rank::Nine, Suit::Spades),
                card(Rank::Seven, Suit::Spades),
                card(Rank::Three, Suit::Spades),
            ]),
            HandCategory::Flush
        );
    }

    #[test]
    fn evaluates_full_house() {
        assert_eq!(
            category(&[
                card(Rank::King, Suit::Clubs),
                card(Rank::King, Suit::Diamonds),
                card(Rank::King, Suit::Hearts),
                card(Rank::Two, Suit::Spades),
                card(Rank::Two, Suit::Clubs),
            ]),
            HandCategory::FullHouse
        );
    }

    #[test]
    fn evaluates_four_of_a_kind() {
        assert_eq!(
            category(&[
                card(Rank::Ten, Suit::Clubs),
                card(Rank::Ten, Suit::Diamonds),
                card(Rank::Ten, Suit::Hearts),
                card(Rank::Ten, Suit::Spades),
                card(Rank::Two, Suit::Clubs),
            ]),
            HandCategory::FourOfAKind
        );
    }

    #[test]
    fn evaluates_straight_flush() {
        assert_eq!(
            category(&[
                card(Rank::Nine, Suit::Hearts),
                card(Rank::Eight, Suit::Hearts),
                card(Rank::Seven, Suit::Hearts),
                card(Rank::Six, Suit::Hearts),
                card(Rank::Five, Suit::Hearts),
            ]),
            HandCategory::StraightFlush
        );
    }

    #[test]
    fn picks_best_five_cards_from_seven() {
        let hand = evaluate_best_hand(&[
            card(Rank::Ace, Suit::Hearts),
            card(Rank::King, Suit::Hearts),
            card(Rank::Queen, Suit::Hearts),
            card(Rank::Jack, Suit::Hearts),
            card(Rank::Ten, Suit::Hearts),
            card(Rank::Two, Suit::Clubs),
            card(Rank::Two, Suit::Diamonds),
        ])
        .expect("hand should evaluate");

        assert_eq!(hand.rank().category(), HandCategory::StraightFlush);
        assert_eq!(hand.rank().kickers(), &[Rank::Ace]);
    }

    #[test]
    fn higher_pair_beats_lower_pair() {
        let aces = evaluate_best_hand(&[
            card(Rank::Ace, Suit::Clubs),
            card(Rank::Ace, Suit::Diamonds),
            card(Rank::Nine, Suit::Hearts),
            card(Rank::Seven, Suit::Spades),
            card(Rank::Three, Suit::Clubs),
        ])
        .expect("hand should evaluate");
        let kings = evaluate_best_hand(&[
            card(Rank::King, Suit::Clubs),
            card(Rank::King, Suit::Diamonds),
            card(Rank::Nine, Suit::Hearts),
            card(Rank::Seven, Suit::Spades),
            card(Rank::Three, Suit::Clubs),
        ])
        .expect("hand should evaluate");

        assert!(aces > kings);
    }

    #[test]
    fn equal_rank_hands_tie_even_with_different_suits() {
        let spades = evaluate_best_hand(&[
            card(Rank::Ace, Suit::Spades),
            card(Rank::King, Suit::Spades),
            card(Rank::Queen, Suit::Spades),
            card(Rank::Jack, Suit::Spades),
            card(Rank::Nine, Suit::Spades),
        ])
        .expect("hand should evaluate");
        let hearts = evaluate_best_hand(&[
            card(Rank::Ace, Suit::Hearts),
            card(Rank::King, Suit::Hearts),
            card(Rank::Queen, Suit::Hearts),
            card(Rank::Jack, Suit::Hearts),
            card(Rank::Nine, Suit::Hearts),
        ])
        .expect("hand should evaluate");

        assert_eq!(spades.cmp(&hearts), Ordering::Equal);
    }

    #[test]
    fn rejects_too_few_cards() {
        assert_eq!(
            evaluate_best_hand(&[
                card(Rank::Ace, Suit::Clubs),
                card(Rank::King, Suit::Diamonds),
                card(Rank::Queen, Suit::Hearts),
                card(Rank::Jack, Suit::Spades),
            ]),
            Err(HandEvaluationError::TooFewCards { provided: 4 })
        );
    }
}
