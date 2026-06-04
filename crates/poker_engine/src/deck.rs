use rand::seq::SliceRandom;
use rand::thread_rng;

use crate::{Card, Rank, Suit};

#[derive(Debug, Clone)]
pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    pub const CARD_COUNT: usize = 52;

    pub fn new_ordered() -> Self {
        let mut cards = Vec::with_capacity(Self::CARD_COUNT);

        for suit in Suit::ALL {
            for rank in Rank::ALL {
                cards.push(Card::new(rank, suit));
            }
        }

        Self { cards }
    }

    pub fn new_shuffled() -> Self {
        let mut deck = Self::new_ordered();
        deck.shuffle();
        deck
    }

    pub fn shuffle(&mut self) {
        self.cards.shuffle(&mut thread_rng());
    }

    pub fn deal_one(&mut self) -> Option<Card> {
        self.cards.pop()
    }

    pub fn deal_many(&mut self, count: usize) -> Option<Vec<Card>> {
        if count > self.cards.len() {
            return None;
        }

        Some((0..count).filter_map(|_| self.deal_one()).collect())
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }
}

impl Default for Deck {
    fn default() -> Self {
        Self::new_ordered()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn new_ordered_deck_has_52_cards() {
        let deck = Deck::new_ordered();

        assert_eq!(deck.len(), Deck::CARD_COUNT);
    }

    #[test]
    fn new_ordered_deck_has_52_unique_cards() {
        let deck = Deck::new_ordered();
        let unique_cards: HashSet<Card> = deck.cards().iter().copied().collect();

        assert_eq!(unique_cards.len(), Deck::CARD_COUNT);
    }

    #[test]
    fn new_shuffled_deck_preserves_52_unique_cards() {
        let deck = Deck::new_shuffled();
        let unique_cards: HashSet<Card> = deck.cards().iter().copied().collect();

        assert_eq!(deck.len(), Deck::CARD_COUNT);
        assert_eq!(unique_cards.len(), Deck::CARD_COUNT);
    }

    #[test]
    fn deal_one_removes_a_card_from_the_deck() {
        let mut deck = Deck::new_ordered();
        let dealt_card = deck.deal_one();

        assert!(dealt_card.is_some());
        assert_eq!(deck.len(), Deck::CARD_COUNT - 1);
    }

    #[test]
    fn deal_many_removes_requested_number_of_cards() {
        let mut deck = Deck::new_ordered();
        let dealt_cards = deck.deal_many(5).expect("deck should have enough cards");

        assert_eq!(dealt_cards.len(), 5);
        assert_eq!(deck.len(), Deck::CARD_COUNT - 5);
    }

    #[test]
    fn deal_many_does_not_mutate_when_request_exceeds_deck_size() {
        let mut deck = Deck::new_ordered();
        let dealt_cards = deck.deal_many(Deck::CARD_COUNT + 1);

        assert!(dealt_cards.is_none());
        assert_eq!(deck.len(), Deck::CARD_COUNT);
    }

    #[test]
    fn dealing_all_cards_eventually_empties_the_deck() {
        let mut deck = Deck::new_ordered();
        let dealt_cards = deck
            .deal_many(Deck::CARD_COUNT)
            .expect("deck should have exactly enough cards");

        assert_eq!(dealt_cards.len(), Deck::CARD_COUNT);
        assert!(deck.is_empty());
        assert!(deck.deal_one().is_none());
    }
}
