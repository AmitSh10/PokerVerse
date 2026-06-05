use std::fmt;

use crate::{
    BountyPaymentEvent, Card, ChipAmount, ContributionSnapshot, Deck, EvaluatedHand, GameEvent,
    GamePhase, GameSnapshot, HandEvaluationError, HandSnapshot, HandState, HandStateError,
    PayoutEvent, Player, PlayerAction, PlayerError, PlayerId, PlayerSnapshot, PlayerStatus,
    PotContribution, Rank, SeatIndex, Table, TableConfig, TableError, calculate_side_pots,
    evaluate_best_hand,
};

const TWO_SEVEN_BOUNTY_AMOUNT: ChipAmount = 10;

#[derive(Debug, Clone)]
pub struct GameEngine {
    table: Table,
    current_hand: Option<HandState>,
    deck: Option<Deck>,
    events: Vec<GameEvent>,
}

impl GameEngine {
    pub fn new(config: TableConfig) -> Self {
        Self {
            table: Table::new(config),
            current_hand: None,
            deck: None,
            events: Vec::new(),
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

    pub fn events(&self) -> &[GameEvent] {
        &self.events
    }

    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        self.events.drain(..).collect()
    }

    pub fn public_snapshot(&self) -> GameSnapshot {
        self.snapshot_for_viewer(None)
    }

    pub fn private_snapshot_for(&self, seat: SeatIndex) -> Result<GameSnapshot, GameEngineError> {
        if !self.table.occupied_seats().contains(&seat) {
            return Err(TableError::SeatEmpty { seat }.into());
        }

        Ok(self.snapshot_for_viewer(Some(seat)))
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
        self.events.push(GameEvent::HandStarted {
            dealer_seat,
            playing_seats: playing_seats.clone(),
        });
        self.events.push(GameEvent::HoleCardsDealt {
            seats: playing_seats,
            cards_per_player: Player::MAX_HOLE_CARDS,
        });

        Ok(self
            .current_hand
            .as_ref()
            .expect("current_hand was just inserted"))
    }

    pub fn advance_hand_phase(&mut self) -> Result<GamePhase, GameEngineError> {
        let phase = {
            let hand = self
                .current_hand
                .as_mut()
                .ok_or(GameEngineError::NoActiveHand)?;

            hand.advance_phase().map_err(GameEngineError::HandState)?
        };
        self.events.push(GameEvent::PhaseAdvanced { phase });

        Ok(phase)
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
        self.events.push(GameEvent::BlindsPosted {
            small_blind_seat,
            small_blind: small_blind_committed,
            big_blind_seat,
            big_blind: big_blind_committed,
        });

        Ok(())
    }

    pub fn deal_flop(&mut self) -> Result<(), GameEngineError> {
        self.ensure_hand_phase(GamePhase::Flop)?;

        let cards = [
            self.deal_one_from_deck()?,
            self.deal_one_from_deck()?,
            self.deal_one_from_deck()?,
        ];

        self.current_hand_mut()?.reveal_flop(cards)?;
        self.events.push(GameEvent::BoardRevealed {
            phase: GamePhase::Flop,
            cards: cards.to_vec(),
        });
        Ok(())
    }

    pub fn deal_turn(&mut self) -> Result<(), GameEngineError> {
        self.ensure_hand_phase(GamePhase::Turn)?;
        let card = self.deal_one_from_deck()?;

        self.current_hand_mut()?.reveal_turn(card)?;
        self.events.push(GameEvent::BoardRevealed {
            phase: GamePhase::Turn,
            cards: vec![card],
        });
        Ok(())
    }

    pub fn deal_river(&mut self) -> Result<(), GameEngineError> {
        self.ensure_hand_phase(GamePhase::River)?;
        let card = self.deal_one_from_deck()?;

        self.current_hand_mut()?.reveal_river(card)?;
        self.events.push(GameEvent::BoardRevealed {
            phase: GamePhase::River,
            cards: vec![card],
        });
        Ok(())
    }

    pub fn finish_hand(&mut self) -> Result<HandState, GameEngineError> {
        let hand = self
            .current_hand
            .take()
            .ok_or(GameEngineError::NoActiveHand)?;

        if hand.phase() != GamePhase::HandComplete {
            let actual = hand.phase();
            self.current_hand = Some(hand);

            return Err(GameEngineError::InvalidPhaseForFinishingHand { actual });
        }

        self.deck = None;

        for seat in self.table.occupied_seats() {
            if let Some(player) = self.table.player_at_mut(seat) {
                player.clear_hole_cards();
                // Folded players are ready for the next hand.
                if player.status() == PlayerStatus::Folded {
                    player.sit_in();
                }
            }
        }
        self.events.push(GameEvent::HandFinished);

        Ok(hand)
    }

    pub fn showdown(&self) -> Result<ShowdownResult, GameEngineError> {
        let hand = self
            .current_hand
            .as_ref()
            .ok_or(GameEngineError::NoActiveHand)?;

        if hand.phase() != GamePhase::Showdown {
            return Err(GameEngineError::InvalidPhaseForShowdown {
                actual: hand.phase(),
            });
        }

        let mut player_hands = Vec::new();

        for seat in self.contesting_seats() {
            let player = self
                .table
                .player_at(seat)
                .ok_or(TableError::SeatEmpty { seat })?;

            if player.hole_cards().len() != Player::MAX_HOLE_CARDS {
                return Err(GameEngineError::PlayerMissingHoleCards {
                    seat,
                    card_count: player.hole_cards().len(),
                });
            }

            let mut cards = Vec::with_capacity(Player::MAX_HOLE_CARDS + HandState::MAX_BOARD_CARDS);
            cards.extend_from_slice(player.hole_cards());
            cards.extend_from_slice(hand.board());

            player_hands.push(PlayerShowdownHand {
                seat,
                player_id: player.id(),
                hand: evaluate_best_hand(&cards)?,
            });
        }

        let best_hand = player_hands
            .iter()
            .map(|player_hand| &player_hand.hand)
            .max()
            .ok_or(GameEngineError::NoShowdownPlayers)?;
        let winner_seats = player_hands
            .iter()
            .filter(|player_hand| player_hand.hand == *best_hand)
            .map(|player_hand| player_hand.seat)
            .collect();

        Ok(ShowdownResult {
            winner_seats,
            player_hands,
        })
    }

    pub fn award_showdown_pot(&mut self) -> Result<PayoutResult, GameEngineError> {
        let showdown = self.showdown()?;
        let two_seven_winner_seats = self.two_seven_bounty_winner_seats(showdown.winner_seats());
        let (total_pot, contributions) = {
            let hand = self
                .current_hand
                .as_ref()
                .ok_or(GameEngineError::NoActiveHand)?;
            let contesting_seats = self.contesting_seats();
            let mut contributions = hand
                .contributions()
                .map(|(seat, amount)| {
                    PotContribution::new(seat, amount, contesting_seats.contains(&seat))
                })
                .collect::<Vec<_>>();
            contributions.sort_by_key(|contribution| contribution.seat().0);

            (hand.pot(), contributions)
        };
        let side_pots = calculate_side_pots(&contributions);
        let mut awarded_amounts = Vec::new();

        for side_pot in &side_pots {
            let eligible_hands = showdown
                .player_hands()
                .iter()
                .filter(|player_hand| side_pot.eligible_seats().contains(&player_hand.seat()))
                .collect::<Vec<_>>();
            let best_hand = eligible_hands
                .iter()
                .map(|player_hand| player_hand.hand())
                .max()
                .ok_or(GameEngineError::NoShowdownPlayers)?;
            let mut winner_seats = eligible_hands
                .into_iter()
                .filter_map(|player_hand| {
                    (player_hand.hand() == best_hand).then_some(player_hand.seat())
                })
                .collect::<Vec<_>>();
            winner_seats.sort_by_key(|seat| seat.0);

            Self::record_split_payouts(&mut awarded_amounts, side_pot.amount(), &winner_seats);
        }

        self.current_hand_mut()?.take_pot();
        let mut payouts = Vec::with_capacity(awarded_amounts.len());

        for (seat, amount) in awarded_amounts {
            let player = self
                .table
                .player_at_mut(seat)
                .ok_or(TableError::SeatEmpty { seat })?;

            player.credit_chips(amount);
            payouts.push(PlayerPayout {
                seat,
                player_id: player.id(),
                amount,
            });
        }

        let bounty_events = self.award_two_seven_bounties(&two_seven_winner_seats)?;

        self.advance_hand_phase()?;
        self.events.push(GameEvent::PotAwarded {
            total_pot,
            payouts: payouts
                .iter()
                .map(|payout| PayoutEvent::new(payout.seat(), payout.amount()))
                .collect(),
        });
        self.events.extend(bounty_events);

        Ok(PayoutResult { total_pot, payouts })
    }

    /// Awards the pot to the single remaining player when all others have folded.
    /// Does not evaluate hands, but private hole cards still count for table bounties.
    pub fn award_uncontested_pot(&mut self) -> Result<(), GameEngineError> {
        let contesting = self.contesting_seats();

        if contesting.len() != 1 {
            return Err(GameEngineError::NoShowdownPlayers);
        }

        let winner_seat = contesting[0];
        let hand = self
            .current_hand
            .as_mut()
            .ok_or(GameEngineError::NoActiveHand)?;
        let total_pot = hand.take_pot();

        let player = self
            .table
            .player_at_mut(winner_seat)
            .ok_or(TableError::SeatEmpty { seat: winner_seat })?;
        player.credit_chips(total_pot);

        self.events.push(GameEvent::PotAwarded {
            total_pot,
            payouts: vec![PayoutEvent::new(winner_seat, total_pot)],
        });
        let bounty_events = if self
            .two_seven_bounty_winner_seats(&[winner_seat])
            .is_empty()
        {
            Vec::new()
        } else {
            self.award_two_seven_bounties(&[winner_seat])?
        };
        self.events.extend(bounty_events);

        // Advance through remaining phases to HandComplete
        loop {
            let phase = self
                .current_hand
                .as_mut()
                .ok_or(GameEngineError::NoActiveHand)?
                .advance_phase()
                .map_err(GameEngineError::HandState)?;
            self.events.push(GameEvent::PhaseAdvanced { phase });
            if phase == GamePhase::HandComplete {
                break;
            }
        }

        self.finish_hand()?;
        Ok(())
    }

    fn record_split_payouts(
        awarded_amounts: &mut Vec<(SeatIndex, ChipAmount)>,
        amount: ChipAmount,
        winner_seats: &[SeatIndex],
    ) {
        let winner_count = winner_seats.len() as ChipAmount;
        let base_share = amount / winner_count;
        let extra_chips = amount % winner_count;

        for (index, seat) in winner_seats.iter().copied().enumerate() {
            let payout_amount = base_share + u64::from((index as ChipAmount) < extra_chips);

            if let Some((_, existing_amount)) = awarded_amounts
                .iter_mut()
                .find(|(existing_seat, _)| *existing_seat == seat)
            {
                *existing_amount += payout_amount;
            } else {
                awarded_amounts.push((seat, payout_amount));
            }
        }

        awarded_amounts.sort_by_key(|(seat, _)| seat.0);
    }

    fn two_seven_bounty_winner_seats(&self, winner_seats: &[SeatIndex]) -> Vec<SeatIndex> {
        winner_seats
            .iter()
            .copied()
            .filter(|seat| {
                self.table
                    .player_at(*seat)
                    .is_some_and(|player| Self::hole_cards_are_two_seven(player.hole_cards()))
            })
            .collect()
    }

    fn award_two_seven_bounties(
        &mut self,
        winner_seats: &[SeatIndex],
    ) -> Result<Vec<GameEvent>, GameEngineError> {
        let payer_seats = self.dealt_in_seats();
        let mut events = Vec::new();

        for winner_seat in winner_seats {
            let mut payments = Vec::new();
            let mut total_awarded = 0;

            for payer_seat in payer_seats.iter().copied() {
                if payer_seat == *winner_seat {
                    continue;
                }

                let amount = self
                    .table
                    .player_at(payer_seat)
                    .ok_or(TableError::SeatEmpty { seat: payer_seat })?
                    .stack()
                    .min(TWO_SEVEN_BOUNTY_AMOUNT);

                if amount == 0 {
                    continue;
                }

                self.table
                    .player_at_mut(payer_seat)
                    .ok_or(TableError::SeatEmpty { seat: payer_seat })?
                    .debit_chips(amount)?;
                self.table
                    .player_at_mut(*winner_seat)
                    .ok_or(TableError::SeatEmpty { seat: *winner_seat })?
                    .credit_chips(amount);

                payments.push(BountyPaymentEvent::new(payer_seat, amount));
                total_awarded += amount;
            }

            if total_awarded > 0 {
                events.push(GameEvent::TwoSevenBountyAwarded {
                    winner_seat: *winner_seat,
                    bounty_per_player: TWO_SEVEN_BOUNTY_AMOUNT,
                    total_awarded,
                    payments,
                });
            }
        }

        Ok(events)
    }

    fn dealt_in_seats(&self) -> Vec<SeatIndex> {
        self.table
            .occupied_seats()
            .into_iter()
            .filter(|seat| {
                self.table
                    .player_at(*seat)
                    .is_some_and(|player| player.hole_cards().len() == Player::MAX_HOLE_CARDS)
            })
            .collect()
    }

    fn hole_cards_are_two_seven(cards: &[Card]) -> bool {
        if cards.len() != Player::MAX_HOLE_CARDS {
            return false;
        }

        matches!(
            (cards[0].rank, cards[1].rank),
            (Rank::Two, Rank::Seven) | (Rank::Seven, Rank::Two)
        )
    }

    fn snapshot_for_viewer(&self, viewer_seat: Option<SeatIndex>) -> GameSnapshot {
        let players = self
            .table
            .occupied_seats()
            .into_iter()
            .filter_map(|seat| {
                self.table.player_at(seat).map(|player| {
                    let visible_hole_cards =
                        (viewer_seat == Some(seat)).then(|| player.hole_cards().to_vec());

                    PlayerSnapshot::new(
                        player.id(),
                        player.display_name(),
                        seat,
                        player.stack(),
                        player.status(),
                        player.hole_cards().len(),
                        visible_hole_cards,
                    )
                })
            })
            .collect();
        let hand = self.current_hand.as_ref().map(|hand| {
            let mut contributions = hand
                .contributions()
                .map(|(seat, total)| {
                    ContributionSnapshot::new(seat, total, hand.round_contribution_for(seat))
                })
                .collect::<Vec<_>>();
            contributions.sort_by_key(|contribution| contribution.seat().0);

            HandSnapshot::new(
                hand.phase(),
                hand.acting_seat(),
                hand.board().to_vec(),
                hand.pot(),
                hand.current_bet(),
                hand.dealer_seat(),
                hand.small_blind_seat(),
                hand.big_blind_seat(),
                hand.first_to_act_seat(),
                contributions,
            )
        });

        GameSnapshot::new(players, hand)
    }

    pub fn apply_player_action(
        &mut self,
        seat: SeatIndex,
        action: PlayerAction,
    ) -> Result<(), GameEngineError> {
        self.validate_player_action_turn(seat)?;

        match action {
            PlayerAction::Fold => {
                self.table
                    .player_at_mut(seat)
                    .ok_or(TableError::SeatEmpty { seat })?
                    .fold();
                self.current_hand_mut()?.mark_player_acted(seat);
                self.complete_round_or_advance_action_after(seat)?;
            }
            PlayerAction::Check => {
                let amount_to_call = self
                    .current_hand
                    .as_ref()
                    .expect("turn validation guarantees an active hand")
                    .amount_to_call(seat);

                if amount_to_call > 0 {
                    return Err(GameEngineError::CannotCheckFacingBet { amount_to_call });
                }

                self.current_hand_mut()?.mark_player_acted(seat);
                self.complete_round_or_advance_action_after(seat)?;
            }
            PlayerAction::Call => {
                let amount_to_call = self
                    .current_hand
                    .as_ref()
                    .expect("turn validation guarantees an active hand")
                    .amount_to_call(seat);

                if amount_to_call == 0 {
                    return Err(GameEngineError::CannotCallWithoutBet);
                }

                let committed = self
                    .table
                    .player_at_mut(seat)
                    .ok_or(TableError::SeatEmpty { seat })?
                    .debit_chips(amount_to_call)?;

                self.current_hand_mut()?
                    .record_contribution(seat, committed);
                self.current_hand_mut()?.mark_player_acted(seat);
                self.complete_round_or_advance_action_after(seat)?;
            }
            PlayerAction::Bet { amount } => {
                let current_bet = self
                    .current_hand
                    .as_ref()
                    .expect("turn validation guarantees an active hand")
                    .current_bet();

                if current_bet > 0 {
                    return Err(GameEngineError::CannotBetFacingBet { current_bet });
                }

                let minimum_bet = self.table.config().big_blind();

                if amount < minimum_bet {
                    return Err(GameEngineError::BetTooSmall {
                        amount,
                        minimum: minimum_bet,
                    });
                }

                let committed = self
                    .table
                    .player_at_mut(seat)
                    .ok_or(TableError::SeatEmpty { seat })?
                    .debit_chips(amount)?;

                self.current_hand_mut()?
                    .record_contribution(seat, committed);
                self.current_hand_mut()?.reset_round_actions();
                self.current_hand_mut()?.mark_player_acted(seat);
                self.complete_round_or_advance_action_after(seat)?;
            }
            PlayerAction::Raise { amount } => {
                let (current_bet, round_contribution) = {
                    let hand = self
                        .current_hand
                        .as_ref()
                        .expect("turn validation guarantees an active hand");

                    (hand.current_bet(), hand.round_contribution_for(seat))
                };

                if current_bet == 0 {
                    return Err(GameEngineError::CannotRaiseWithoutBet);
                }

                let minimum_raise = current_bet + self.table.config().big_blind();

                if amount < minimum_raise {
                    return Err(GameEngineError::RaiseTooSmall {
                        amount,
                        minimum: minimum_raise,
                    });
                }

                let amount_to_commit = amount - round_contribution;
                let committed = self
                    .table
                    .player_at_mut(seat)
                    .ok_or(TableError::SeatEmpty { seat })?
                    .debit_chips(amount_to_commit)?;

                self.current_hand_mut()?
                    .record_contribution(seat, committed);
                self.current_hand_mut()?.reset_round_actions();
                self.current_hand_mut()?.mark_player_acted(seat);
                self.complete_round_or_advance_action_after(seat)?;
            }
            PlayerAction::AllIn => {
                let current_bet = self
                    .current_hand
                    .as_ref()
                    .expect("turn validation guarantees an active hand")
                    .current_bet();
                let committed = self
                    .table
                    .player_at_mut(seat)
                    .ok_or(TableError::SeatEmpty { seat })?
                    .move_all_in();

                self.current_hand_mut()?
                    .record_contribution(seat, committed);

                if self
                    .current_hand
                    .as_ref()
                    .expect("active hand still exists after all-in")
                    .current_bet()
                    > current_bet
                {
                    self.current_hand_mut()?.reset_round_actions();
                }

                self.current_hand_mut()?.mark_player_acted(seat);
                self.complete_round_or_advance_action_after(seat)?;
            }
        }

        self.events.push(GameEvent::PlayerActed { seat, action });

        Ok(())
    }

    fn validate_player_action_turn(&self, seat: SeatIndex) -> Result<(), GameEngineError> {
        let hand = self
            .current_hand
            .as_ref()
            .ok_or(GameEngineError::NoActiveHand)?;

        if !hand.phase().is_betting_phase() {
            return Err(GameEngineError::InvalidPhaseForPlayerAction {
                actual: hand.phase(),
            });
        }

        let acting_seat = hand.acting_seat().ok_or(GameEngineError::NoActingPlayer)?;

        if seat != acting_seat {
            return Err(GameEngineError::NotPlayersTurn {
                expected: acting_seat,
                actual: seat,
            });
        }

        Ok(())
    }

    fn complete_round_or_advance_action_after(
        &mut self,
        seat: SeatIndex,
    ) -> Result<(), GameEngineError> {
        if self.remaining_contesting_player_count() <= 1 {
            self.current_hand_mut()?.set_acting_seat(None);
            return Ok(());
        }

        if self.is_betting_round_complete()? {
            let next_phase = self.advance_hand_phase()?;
            self.deal_board_cards_for_current_phase(next_phase)?;

            if matches!(
                next_phase,
                GamePhase::Flop | GamePhase::Turn | GamePhase::River
            ) {
                let next_acting_seat = self.first_postflop_acting_seat()?;
                self.current_hand_mut()?.set_acting_seat(next_acting_seat);
            } else {
                self.current_hand_mut()?.set_acting_seat(None);
            }

            return Ok(());
        }

        let next_acting_seat = if self.table.playing_seats().len() > 1 {
            self.table.next_playing_seat_after(seat)?
        } else {
            None
        };

        self.current_hand_mut()?.set_acting_seat(next_acting_seat);
        Ok(())
    }

    fn deal_board_cards_for_current_phase(
        &mut self,
        phase: GamePhase,
    ) -> Result<(), GameEngineError> {
        match phase {
            GamePhase::Flop => self.deal_flop(),
            GamePhase::Turn => self.deal_turn(),
            GamePhase::River => self.deal_river(),
            _ => Ok(()),
        }
    }

    fn is_betting_round_complete(&self) -> Result<bool, GameEngineError> {
        let hand = self
            .current_hand
            .as_ref()
            .ok_or(GameEngineError::NoActiveHand)?;
        let acting_seats = self.table.playing_seats();

        if acting_seats.is_empty() {
            return Ok(true);
        }

        Ok(acting_seats
            .into_iter()
            .all(|seat| hand.amount_to_call(seat) == 0 && hand.has_player_acted_this_round(seat)))
    }

    fn remaining_contesting_player_count(&self) -> usize {
        self.contesting_seats().len()
    }

    pub fn contesting_seat_count(&self) -> usize {
        self.contesting_seats().len()
    }

    fn contesting_seats(&self) -> Vec<SeatIndex> {
        self.table
            .occupied_seats()
            .into_iter()
            .filter(|seat| {
                self.table.player_at(*seat).is_some_and(|player| {
                    matches!(player.status(), PlayerStatus::Active | PlayerStatus::AllIn)
                })
            })
            .collect()
    }

    fn first_postflop_acting_seat(&self) -> Result<Option<SeatIndex>, GameEngineError> {
        let dealer_seat = self
            .current_hand
            .as_ref()
            .ok_or(GameEngineError::NoActiveHand)?
            .dealer_seat();

        self.table
            .next_playing_seat_after(dealer_seat)
            .map_err(GameEngineError::Table)
    }

    fn ensure_hand_phase(&self, expected: GamePhase) -> Result<(), GameEngineError> {
        let hand = self
            .current_hand
            .as_ref()
            .ok_or(GameEngineError::NoActiveHand)?;

        if hand.phase() != expected {
            return Err(GameEngineError::InvalidPhaseForCommunityCards {
                expected,
                actual: hand.phase(),
            });
        }

        Ok(())
    }

    fn current_hand_mut(&mut self) -> Result<&mut HandState, GameEngineError> {
        self.current_hand
            .as_mut()
            .ok_or(GameEngineError::NoActiveHand)
    }

    fn deal_one_from_deck(&mut self) -> Result<Card, GameEngineError> {
        self.deck
            .as_mut()
            .ok_or(GameEngineError::NoActiveDeck)?
            .deal_one()
            .ok_or(GameEngineError::DeckExhausted)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowdownResult {
    winner_seats: Vec<SeatIndex>,
    player_hands: Vec<PlayerShowdownHand>,
}

impl ShowdownResult {
    pub fn winner_seats(&self) -> &[SeatIndex] {
        &self.winner_seats
    }

    pub fn player_hands(&self) -> &[PlayerShowdownHand] {
        &self.player_hands
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerShowdownHand {
    seat: SeatIndex,
    player_id: PlayerId,
    hand: EvaluatedHand,
}

impl PlayerShowdownHand {
    pub fn seat(&self) -> SeatIndex {
        self.seat
    }

    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    pub fn hand(&self) -> &EvaluatedHand {
        &self.hand
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayoutResult {
    total_pot: ChipAmount,
    payouts: Vec<PlayerPayout>,
}

impl PayoutResult {
    pub fn total_pot(&self) -> ChipAmount {
        self.total_pot
    }

    pub fn payouts(&self) -> &[PlayerPayout] {
        &self.payouts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerPayout {
    seat: SeatIndex,
    player_id: PlayerId,
    amount: ChipAmount,
}

impl PlayerPayout {
    pub fn seat(&self) -> SeatIndex {
        self.seat
    }

    pub fn player_id(&self) -> PlayerId {
        self.player_id
    }

    pub fn amount(&self) -> ChipAmount {
        self.amount
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEngineError {
    NoActiveHand,
    NoActiveDeck,
    NoActingPlayer,
    HandAlreadyInProgress,
    NotEnoughPlayersToStartHand,
    DeckExhausted,
    InvalidPhaseForPostingBlinds {
        actual: GamePhase,
    },
    InvalidPhaseForCommunityCards {
        expected: GamePhase,
        actual: GamePhase,
    },
    InvalidPhaseForFinishingHand {
        actual: GamePhase,
    },
    InvalidPhaseForShowdown {
        actual: GamePhase,
    },
    InvalidPhaseForPlayerAction {
        actual: GamePhase,
    },
    NotPlayersTurn {
        expected: SeatIndex,
        actual: SeatIndex,
    },
    CannotCheckFacingBet {
        amount_to_call: ChipAmount,
    },
    CannotCallWithoutBet,
    CannotBetFacingBet {
        current_bet: ChipAmount,
    },
    BetTooSmall {
        amount: ChipAmount,
        minimum: ChipAmount,
    },
    CannotRaiseWithoutBet,
    RaiseTooSmall {
        amount: ChipAmount,
        minimum: ChipAmount,
    },
    UnsupportedPlayerAction {
        action: PlayerAction,
    },
    PlayerMissingHoleCards {
        seat: SeatIndex,
        card_count: usize,
    },
    NoShowdownPlayers,
    Table(TableError),
    Player(PlayerError),
    HandState(HandStateError),
    HandEvaluation(HandEvaluationError),
}

impl fmt::Display for GameEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoActiveHand => write!(f, "no active hand"),
            Self::NoActiveDeck => write!(f, "no active deck"),
            Self::NoActingPlayer => write!(f, "no acting player"),
            Self::HandAlreadyInProgress => write!(f, "a hand is already in progress"),
            Self::NotEnoughPlayersToStartHand => write!(f, "not enough players to start hand"),
            Self::DeckExhausted => write!(f, "deck does not have enough cards"),
            Self::InvalidPhaseForPostingBlinds { actual } => {
                write!(f, "cannot post blinds in phase {actual:?}")
            }
            Self::InvalidPhaseForCommunityCards { expected, actual } => {
                write!(
                    f,
                    "cannot deal community cards in phase {actual:?}; expected {expected:?}"
                )
            }
            Self::InvalidPhaseForFinishingHand { actual } => {
                write!(f, "cannot finish hand in phase {actual:?}")
            }
            Self::InvalidPhaseForShowdown { actual } => {
                write!(f, "cannot evaluate showdown in phase {actual:?}")
            }
            Self::InvalidPhaseForPlayerAction { actual } => {
                write!(f, "cannot apply player action in phase {actual:?}")
            }
            Self::NotPlayersTurn { expected, actual } => {
                write!(
                    f,
                    "not player {}'s turn; expected seat {}",
                    actual.0, expected.0
                )
            }
            Self::CannotCheckFacingBet { amount_to_call } => {
                write!(f, "cannot check while facing bet of {amount_to_call}")
            }
            Self::CannotCallWithoutBet => write!(f, "cannot call when there is no bet to call"),
            Self::CannotBetFacingBet { current_bet } => {
                write!(f, "cannot bet while current bet is {current_bet}")
            }
            Self::BetTooSmall { amount, minimum } => {
                write!(f, "bet {amount} is below minimum {minimum}")
            }
            Self::CannotRaiseWithoutBet => write!(f, "cannot raise when there is no bet to raise"),
            Self::RaiseTooSmall { amount, minimum } => {
                write!(f, "raise {amount} is below minimum {minimum}")
            }
            Self::UnsupportedPlayerAction { action } => {
                write!(f, "player action {action:?} is not supported yet")
            }
            Self::PlayerMissingHoleCards { seat, card_count } => {
                write!(
                    f,
                    "player in seat {} has {card_count} hole cards; expected 2",
                    seat.0
                )
            }
            Self::NoShowdownPlayers => write!(f, "no players available for showdown"),
            Self::Table(error) => write!(f, "{error}"),
            Self::Player(error) => write!(f, "{error}"),
            Self::HandState(error) => write!(f, "{error}"),
            Self::HandEvaluation(error) => write!(f, "{error}"),
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

impl From<HandStateError> for GameEngineError {
    fn from(error: HandStateError) -> Self {
        Self::HandState(error)
    }
}

impl From<HandEvaluationError> for GameEngineError {
    fn from(error: HandEvaluationError) -> Self {
        Self::HandEvaluation(error)
    }
}

#[cfg(test)]
mod tests {
    use crate::{HandCategory, Player, PlayerId, Rank, Suit, TableConfig};

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

    fn engine_with_started_hand() -> GameEngine {
        let mut engine = engine_with_two_players();
        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with two players");
        engine
    }

    fn engine_with_three_players_started_hand() -> GameEngine {
        let mut engine = engine_with_two_players();
        engine
            .table_mut()
            .sit_player(PlayerId(3), "Linus", SeatIndex(5), 1_000)
            .expect("third player should sit");
        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with three players");
        engine
    }

    fn advance_to(engine: &mut GameEngine, phase: GamePhase) {
        while engine
            .current_hand()
            .expect("hand should be active")
            .phase()
            != phase
        {
            engine
                .advance_hand_phase()
                .expect("phase should advance before hand is complete");
        }
    }

    fn card(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    fn force_hole_cards(engine: &mut GameEngine, seat: SeatIndex, cards: [Card; 2]) {
        let player = engine
            .table_mut()
            .player_at_mut(seat)
            .expect("player should be seated");

        player.clear_hole_cards();
        player
            .receive_card(cards[0])
            .expect("first hole card should be accepted");
        player
            .receive_card(cards[1])
            .expect("second hole card should be accepted");
    }

    fn force_commit_chips(engine: &mut GameEngine, seat: SeatIndex, amount: ChipAmount) {
        let committed = engine
            .table_mut()
            .player_at_mut(seat)
            .expect("player should be seated")
            .debit_chips(amount)
            .expect("player should have enough chips");

        engine
            .current_hand
            .as_mut()
            .expect("hand should be active")
            .record_contribution(seat, committed);
    }

    fn force_board_and_showdown(engine: &mut GameEngine, flop: [Card; 3], turn: Card, river: Card) {
        advance_to(engine, GamePhase::Flop);
        engine
            .current_hand
            .as_mut()
            .expect("hand should be active")
            .reveal_flop(flop)
            .expect("flop should reveal");

        advance_to(engine, GamePhase::Turn);
        engine
            .current_hand
            .as_mut()
            .expect("hand should be active")
            .reveal_turn(turn)
            .expect("turn should reveal");

        advance_to(engine, GamePhase::River);
        engine
            .current_hand
            .as_mut()
            .expect("hand should be active")
            .reveal_river(river)
            .expect("river should reveal");

        advance_to(engine, GamePhase::Showdown);
    }

    #[test]
    fn new_engine_starts_without_active_hand() {
        let engine = engine();

        assert!(engine.current_hand().is_none());
        assert!(engine.deck().is_none());
        assert_eq!(engine.table().seat_count(), 6);
        assert!(engine.events().is_empty());
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
        assert_eq!(
            engine.events(),
            &[
                GameEvent::HandStarted {
                    dealer_seat: SeatIndex(0),
                    playing_seats: vec![SeatIndex(0), SeatIndex(3)],
                },
                GameEvent::HoleCardsDealt {
                    seats: vec![SeatIndex(0), SeatIndex(3)],
                    cards_per_player: Player::MAX_HOLE_CARDS,
                },
            ]
        );
    }

    #[test]
    fn drain_events_returns_and_clears_accumulated_events() {
        let mut engine = engine_with_two_players();
        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with two players");

        let events = engine.drain_events();

        assert_eq!(events.len(), 2);
        assert!(engine.events().is_empty());
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
    fn public_snapshot_hides_hole_cards_but_reports_counts() {
        let mut engine = engine_with_two_players();
        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with two players");

        let snapshot = engine.public_snapshot();

        assert_eq!(snapshot.players().len(), 2);
        assert!(
            snapshot
                .players()
                .iter()
                .all(|player| player.hole_card_count() == Player::MAX_HOLE_CARDS)
        );
        assert!(
            snapshot
                .players()
                .iter()
                .all(|player| player.visible_hole_cards().is_none())
        );
        assert_eq!(
            snapshot.hand().expect("hand should be active").phase(),
            GamePhase::StartingHand
        );
    }

    #[test]
    fn private_snapshot_reveals_only_requested_players_hole_cards() {
        let mut engine = engine_with_two_players();
        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with two players");

        let snapshot = engine
            .private_snapshot_for(SeatIndex(0))
            .expect("seated player should have a private snapshot");
        let own_player = snapshot
            .players()
            .iter()
            .find(|player| player.seat() == SeatIndex(0))
            .expect("own player should be present");
        let other_player = snapshot
            .players()
            .iter()
            .find(|player| player.seat() == SeatIndex(3))
            .expect("other player should be present");

        assert_eq!(
            own_player.visible_hole_cards().map(<[Card]>::len),
            Some(Player::MAX_HOLE_CARDS)
        );
        assert!(other_player.visible_hole_cards().is_none());
    }

    #[test]
    fn private_snapshot_requires_occupied_seat() {
        let engine = engine_with_two_players();

        assert_eq!(
            engine.private_snapshot_for(SeatIndex(2)),
            Err(GameEngineError::Table(TableError::SeatEmpty {
                seat: SeatIndex(2),
            }))
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
        engine.drain_events();

        engine.post_blinds().expect("blinds should post");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.contribution_for(SeatIndex(0)), 5);
        assert_eq!(hand.contribution_for(SeatIndex(3)), 10);
        assert_eq!(hand.round_contribution_for(SeatIndex(0)), 5);
        assert_eq!(hand.round_contribution_for(SeatIndex(3)), 10);
        assert_eq!(hand.current_bet(), 10);
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
        assert_eq!(
            engine.events(),
            &[GameEvent::BlindsPosted {
                small_blind_seat: SeatIndex(0),
                small_blind: 5,
                big_blind_seat: SeatIndex(3),
                big_blind: 10,
            }]
        );
    }

    #[test]
    fn snapshot_reports_hand_pot_current_bet_and_contributions() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");

        let snapshot = engine.public_snapshot();
        let hand = snapshot.hand().expect("hand should be active");

        assert_eq!(hand.phase(), GamePhase::PostingBlinds);
        assert_eq!(hand.pot(), 15);
        assert_eq!(hand.current_bet(), 10);
        assert_eq!(hand.contributions().len(), 2);
        assert_eq!(hand.contributions()[0].seat(), SeatIndex(0));
        assert_eq!(hand.contributions()[0].total(), 5);
        assert_eq!(hand.contributions()[0].round(), 5);
        assert_eq!(hand.contributions()[1].seat(), SeatIndex(3));
        assert_eq!(hand.contributions()[1].total(), 10);
        assert_eq!(hand.contributions()[1].round(), 10);
    }

    #[test]
    fn deal_flop_requires_active_hand() {
        let mut engine = engine();

        assert_eq!(engine.deal_flop(), Err(GameEngineError::NoActiveHand));
    }

    #[test]
    fn deal_flop_requires_flop_phase_and_does_not_burn_cards() {
        let mut engine = engine_with_started_hand();
        let deck_len_before = engine.deck().map(Deck::len);

        assert_eq!(
            engine.deal_flop(),
            Err(GameEngineError::InvalidPhaseForCommunityCards {
                expected: GamePhase::Flop,
                actual: GamePhase::StartingHand,
            })
        );
        assert_eq!(engine.deck().map(Deck::len), deck_len_before);
        assert_eq!(
            engine
                .current_hand()
                .expect("hand should be active")
                .board()
                .len(),
            0
        );
    }

    #[test]
    fn deal_flop_places_three_cards_on_board() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::Flop);
        let deck_len_before = engine.deck().map(Deck::len).expect("deck should exist");
        engine.drain_events();

        engine.deal_flop().expect("flop should deal");

        assert_eq!(
            engine
                .current_hand()
                .expect("hand should be active")
                .board()
                .len(),
            3
        );
        assert_eq!(engine.deck().map(Deck::len), Some(deck_len_before - 3));
        assert!(matches!(
            engine.events(),
            [GameEvent::BoardRevealed {
                phase: GamePhase::Flop,
                cards,
            }] if cards.len() == 3
        ));
    }

    #[test]
    fn deal_turn_and_river_complete_the_board() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::Flop);
        engine.deal_flop().expect("flop should deal");

        advance_to(&mut engine, GamePhase::Turn);
        engine.deal_turn().expect("turn should deal");

        advance_to(&mut engine, GamePhase::River);
        engine.deal_river().expect("river should deal");

        assert_eq!(
            engine
                .current_hand()
                .expect("hand should be active")
                .board()
                .len(),
            HandState::MAX_BOARD_CARDS
        );
    }

    #[test]
    fn finish_hand_requires_active_hand() {
        let mut engine = engine();

        assert_eq!(engine.finish_hand(), Err(GameEngineError::NoActiveHand));
    }

    #[test]
    fn finish_hand_requires_hand_complete_phase_and_keeps_hand_active_on_error() {
        let mut engine = engine_with_started_hand();

        assert_eq!(
            engine.finish_hand(),
            Err(GameEngineError::InvalidPhaseForFinishingHand {
                actual: GamePhase::StartingHand,
            })
        );
        assert!(engine.current_hand().is_some());
        assert!(engine.deck().is_some());
    }

    #[test]
    fn finish_hand_clears_active_hand_deck_and_hole_cards() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::HandComplete);
        engine.drain_events();

        let finished_hand = engine.finish_hand().expect("hand should finish");

        assert_eq!(finished_hand.phase(), GamePhase::HandComplete);
        assert!(engine.current_hand().is_none());
        assert!(engine.deck().is_none());
        assert!(
            engine
                .table()
                .player_at(SeatIndex(0))
                .expect("player should be seated")
                .hole_cards()
                .is_empty()
        );
        assert!(
            engine
                .table()
                .player_at(SeatIndex(3))
                .expect("player should be seated")
                .hole_cards()
                .is_empty()
        );
        assert_eq!(engine.events(), &[GameEvent::HandFinished]);
    }

    #[test]
    fn showdown_requires_showdown_phase() {
        let engine = engine_with_started_hand();

        assert_eq!(
            engine.showdown(),
            Err(GameEngineError::InvalidPhaseForShowdown {
                actual: GamePhase::StartingHand,
            })
        );
    }

    #[test]
    fn showdown_selects_best_remaining_player() {
        let mut engine = engine_with_started_hand();
        force_hole_cards(
            &mut engine,
            SeatIndex(0),
            [
                card(Rank::Ten, Suit::Clubs),
                card(Rank::Three, Suit::Diamonds),
            ],
        );
        force_hole_cards(
            &mut engine,
            SeatIndex(3),
            [
                card(Rank::Nine, Suit::Clubs),
                card(Rank::Eight, Suit::Diamonds),
            ],
        );
        force_board_and_showdown(
            &mut engine,
            [
                card(Rank::Ace, Suit::Spades),
                card(Rank::King, Suit::Hearts),
                card(Rank::Queen, Suit::Clubs),
            ],
            card(Rank::Jack, Suit::Diamonds),
            card(Rank::Two, Suit::Spades),
        );

        let result = engine.showdown().expect("showdown should evaluate");

        assert_eq!(result.winner_seats(), &[SeatIndex(0)]);
        assert_eq!(result.player_hands().len(), 2);
        assert_eq!(
            result
                .player_hands()
                .iter()
                .find(|player_hand| player_hand.seat() == SeatIndex(0))
                .expect("winner should have a hand")
                .hand()
                .rank()
                .category(),
            HandCategory::Straight
        );
    }

    #[test]
    fn showdown_supports_tied_winners() {
        let mut engine = engine_with_started_hand();
        force_hole_cards(
            &mut engine,
            SeatIndex(0),
            [
                card(Rank::Three, Suit::Clubs),
                card(Rank::Two, Suit::Diamonds),
            ],
        );
        force_hole_cards(
            &mut engine,
            SeatIndex(3),
            [card(Rank::Four, Suit::Clubs), card(Rank::Two, Suit::Hearts)],
        );
        force_board_and_showdown(
            &mut engine,
            [
                card(Rank::Ace, Suit::Spades),
                card(Rank::King, Suit::Hearts),
                card(Rank::Queen, Suit::Clubs),
            ],
            card(Rank::Jack, Suit::Diamonds),
            card(Rank::Ten, Suit::Spades),
        );

        let result = engine.showdown().expect("showdown should evaluate");

        assert_eq!(result.winner_seats(), &[SeatIndex(0), SeatIndex(3)]);
        assert!(
            result
                .player_hands()
                .iter()
                .all(|player_hand| player_hand.hand().rank().category() == HandCategory::Straight)
        );
    }

    #[test]
    fn award_showdown_pot_credits_single_winner_and_completes_hand() {
        let mut engine = engine_with_started_hand();
        force_hole_cards(
            &mut engine,
            SeatIndex(0),
            [
                card(Rank::Ten, Suit::Clubs),
                card(Rank::Three, Suit::Diamonds),
            ],
        );
        force_hole_cards(
            &mut engine,
            SeatIndex(3),
            [
                card(Rank::Nine, Suit::Clubs),
                card(Rank::Eight, Suit::Diamonds),
            ],
        );
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");
        force_commit_chips(&mut engine, SeatIndex(0), 5);
        force_board_and_showdown(
            &mut engine,
            [
                card(Rank::Ace, Suit::Spades),
                card(Rank::King, Suit::Hearts),
                card(Rank::Queen, Suit::Clubs),
            ],
            card(Rank::Jack, Suit::Diamonds),
            card(Rank::Two, Suit::Spades),
        );
        engine.drain_events();

        let payout = engine
            .award_showdown_pot()
            .expect("showdown pot should be awarded");

        assert_eq!(payout.total_pot(), 20);
        assert_eq!(payout.payouts().len(), 1);
        assert_eq!(payout.payouts()[0].seat(), SeatIndex(0));
        assert_eq!(payout.payouts()[0].amount(), 20);
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(0))
                .expect("winner should be seated")
                .stack(),
            1_010
        );
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(3))
                .expect("loser should be seated")
                .stack(),
            990
        );

        let hand = engine.current_hand().expect("hand should remain active");
        assert_eq!(hand.pot(), 0);
        assert_eq!(hand.phase(), GamePhase::HandComplete);
        assert_eq!(
            engine.events(),
            &[
                GameEvent::PhaseAdvanced {
                    phase: GamePhase::HandComplete,
                },
                GameEvent::PotAwarded {
                    total_pot: 20,
                    payouts: vec![PayoutEvent::new(SeatIndex(0), 20)],
                },
            ]
        );
    }

    #[test]
    fn two_seven_bounty_accepts_suited_or_unsuited_hole_cards() {
        assert!(GameEngine::hole_cards_are_two_seven(&[
            card(Rank::Two, Suit::Clubs),
            card(Rank::Seven, Suit::Clubs),
        ]));
        assert!(GameEngine::hole_cards_are_two_seven(&[
            card(Rank::Seven, Suit::Hearts),
            card(Rank::Two, Suit::Spades),
        ]));
        assert!(!GameEngine::hole_cards_are_two_seven(&[
            card(Rank::Two, Suit::Clubs),
            card(Rank::Eight, Suit::Clubs),
        ]));
    }

    #[test]
    fn award_showdown_pot_awards_two_seven_bounty_from_each_other_dealt_player() {
        let mut engine = engine_with_three_players_started_hand();
        force_hole_cards(
            &mut engine,
            SeatIndex(0),
            [
                card(Rank::Two, Suit::Clubs),
                card(Rank::Seven, Suit::Diamonds),
            ],
        );
        force_hole_cards(
            &mut engine,
            SeatIndex(3),
            [
                card(Rank::Nine, Suit::Clubs),
                card(Rank::Eight, Suit::Diamonds),
            ],
        );
        force_hole_cards(
            &mut engine,
            SeatIndex(5),
            [
                card(Rank::Four, Suit::Clubs),
                card(Rank::Three, Suit::Diamonds),
            ],
        );
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");
        force_commit_chips(&mut engine, SeatIndex(0), 10);
        force_board_and_showdown(
            &mut engine,
            [
                card(Rank::Seven, Suit::Spades),
                card(Rank::Seven, Suit::Hearts),
                card(Rank::Ace, Suit::Clubs),
            ],
            card(Rank::King, Suit::Diamonds),
            card(Rank::Queen, Suit::Spades),
        );
        engine.drain_events();

        let payout = engine
            .award_showdown_pot()
            .expect("showdown pot should be awarded");

        assert_eq!(payout.total_pot(), 25);
        assert_eq!(payout.payouts().len(), 1);
        assert_eq!(payout.payouts()[0].seat(), SeatIndex(0));
        assert_eq!(payout.payouts()[0].amount(), 25);
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(0))
                .expect("bounty winner should be seated")
                .stack(),
            1_035
        );
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(3))
                .expect("first payer should be seated")
                .stack(),
            985
        );
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(5))
                .expect("second payer should be seated")
                .stack(),
            980
        );
        assert_eq!(
            engine.events(),
            &[
                GameEvent::PhaseAdvanced {
                    phase: GamePhase::HandComplete,
                },
                GameEvent::PotAwarded {
                    total_pot: 25,
                    payouts: vec![PayoutEvent::new(SeatIndex(0), 25)],
                },
                GameEvent::TwoSevenBountyAwarded {
                    winner_seat: SeatIndex(0),
                    bounty_per_player: 10,
                    total_awarded: 20,
                    payments: vec![
                        BountyPaymentEvent::new(SeatIndex(3), 10),
                        BountyPaymentEvent::new(SeatIndex(5), 10),
                    ],
                },
            ]
        );
    }

    #[test]
    fn award_uncontested_pot_awards_two_seven_bounty_when_everyone_folds() {
        let mut engine = engine_with_three_players_started_hand();
        force_hole_cards(
            &mut engine,
            SeatIndex(0),
            [
                card(Rank::Seven, Suit::Clubs),
                card(Rank::Two, Suit::Diamonds),
            ],
        );
        force_hole_cards(
            &mut engine,
            SeatIndex(3),
            [
                card(Rank::Ace, Suit::Clubs),
                card(Rank::King, Suit::Diamonds),
            ],
        );
        force_hole_cards(
            &mut engine,
            SeatIndex(5),
            [
                card(Rank::Queen, Suit::Clubs),
                card(Rank::Jack, Suit::Diamonds),
            ],
        );
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");
        engine
            .table_mut()
            .player_at_mut(SeatIndex(3))
            .expect("small blind should be seated")
            .fold();
        engine
            .table_mut()
            .player_at_mut(SeatIndex(5))
            .expect("big blind should be seated")
            .fold();
        engine.drain_events();

        engine
            .award_uncontested_pot()
            .expect("uncontested pot should be awarded");

        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(0))
                .expect("bounty winner should be seated")
                .stack(),
            1_035
        );
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(3))
                .expect("first folded payer should be seated")
                .stack(),
            985
        );
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(5))
                .expect("second folded payer should be seated")
                .stack(),
            980
        );
        assert!(engine.events().contains(&GameEvent::TwoSevenBountyAwarded {
            winner_seat: SeatIndex(0),
            bounty_per_player: 10,
            total_awarded: 20,
            payments: vec![
                BountyPaymentEvent::new(SeatIndex(3), 10),
                BountyPaymentEvent::new(SeatIndex(5), 10),
            ],
        }));
    }

    #[test]
    fn award_showdown_pot_splits_tied_winners() {
        let mut engine = engine_with_started_hand();
        force_hole_cards(
            &mut engine,
            SeatIndex(0),
            [
                card(Rank::Three, Suit::Clubs),
                card(Rank::Two, Suit::Diamonds),
            ],
        );
        force_hole_cards(
            &mut engine,
            SeatIndex(3),
            [card(Rank::Four, Suit::Clubs), card(Rank::Two, Suit::Hearts)],
        );
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");
        force_commit_chips(&mut engine, SeatIndex(0), 5);
        force_board_and_showdown(
            &mut engine,
            [
                card(Rank::Ace, Suit::Spades),
                card(Rank::King, Suit::Hearts),
                card(Rank::Queen, Suit::Clubs),
            ],
            card(Rank::Jack, Suit::Diamonds),
            card(Rank::Ten, Suit::Spades),
        );

        let payout = engine
            .award_showdown_pot()
            .expect("showdown pot should be awarded");

        assert_eq!(payout.total_pot(), 20);
        assert_eq!(payout.payouts().len(), 2);
        assert_eq!(payout.payouts()[0].seat(), SeatIndex(0));
        assert_eq!(payout.payouts()[0].amount(), 10);
        assert_eq!(payout.payouts()[1].seat(), SeatIndex(3));
        assert_eq!(payout.payouts()[1].amount(), 10);
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(0))
                .expect("first winner should be seated")
                .stack(),
            1_000
        );
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(3))
                .expect("second winner should be seated")
                .stack(),
            1_000
        );

        let hand = engine.current_hand().expect("hand should remain active");
        assert_eq!(hand.pot(), 0);
        assert_eq!(hand.phase(), GamePhase::HandComplete);
    }

    #[test]
    fn award_showdown_pot_awards_side_pots_by_eligibility() {
        let mut engine =
            GameEngine::new(TableConfig::new(6, 5, 10, 1, 2_000).expect("config should be valid"));
        engine
            .table_mut()
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 40)
            .expect("short-stacked player should sit");
        engine
            .table_mut()
            .sit_player(PlayerId(2), "Grace", SeatIndex(3), 100)
            .expect("second player should sit");
        engine
            .table_mut()
            .sit_player(PlayerId(3), "Linus", SeatIndex(5), 100)
            .expect("third player should sit");
        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with three players");
        force_hole_cards(
            &mut engine,
            SeatIndex(0),
            [
                card(Rank::Ten, Suit::Clubs),
                card(Rank::Three, Suit::Diamonds),
            ],
        );
        force_hole_cards(
            &mut engine,
            SeatIndex(3),
            [
                card(Rank::King, Suit::Clubs),
                card(Rank::Nine, Suit::Diamonds),
            ],
        );
        force_hole_cards(
            &mut engine,
            SeatIndex(5),
            [
                card(Rank::Eight, Suit::Clubs),
                card(Rank::Seven, Suit::Diamonds),
            ],
        );
        force_commit_chips(&mut engine, SeatIndex(0), 40);
        force_commit_chips(&mut engine, SeatIndex(3), 100);
        force_commit_chips(&mut engine, SeatIndex(5), 100);
        force_board_and_showdown(
            &mut engine,
            [
                card(Rank::Ace, Suit::Spades),
                card(Rank::King, Suit::Hearts),
                card(Rank::Queen, Suit::Clubs),
            ],
            card(Rank::Jack, Suit::Diamonds),
            card(Rank::Two, Suit::Spades),
        );

        let payout = engine
            .award_showdown_pot()
            .expect("showdown pot should be awarded");

        assert_eq!(payout.total_pot(), 240);
        assert_eq!(payout.payouts().len(), 2);
        assert_eq!(payout.payouts()[0].seat(), SeatIndex(0));
        assert_eq!(payout.payouts()[0].amount(), 120);
        assert_eq!(payout.payouts()[1].seat(), SeatIndex(3));
        assert_eq!(payout.payouts()[1].amount(), 120);
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(0))
                .expect("main pot winner should be seated")
                .stack(),
            120
        );
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(3))
                .expect("side pot winner should be seated")
                .stack(),
            120
        );
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(5))
                .expect("loser should be seated")
                .stack(),
            0
        );

        let hand = engine.current_hand().expect("hand should remain active");
        assert_eq!(hand.pot(), 0);
        assert_eq!(hand.phase(), GamePhase::HandComplete);
    }

    #[test]
    fn player_action_requires_active_hand() {
        let mut engine = engine();

        assert_eq!(
            engine.apply_player_action(SeatIndex(0), PlayerAction::Check),
            Err(GameEngineError::NoActiveHand)
        );
    }

    #[test]
    fn player_action_requires_betting_phase() {
        let mut engine = engine_with_started_hand();

        assert_eq!(
            engine.apply_player_action(SeatIndex(0), PlayerAction::Check),
            Err(GameEngineError::InvalidPhaseForPlayerAction {
                actual: GamePhase::StartingHand,
            })
        );
    }

    #[test]
    fn player_action_rejects_non_acting_seat() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::PreFlop);

        assert_eq!(
            engine.apply_player_action(SeatIndex(3), PlayerAction::Check),
            Err(GameEngineError::NotPlayersTurn {
                expected: SeatIndex(0),
                actual: SeatIndex(3),
            })
        );
    }

    #[test]
    fn acting_player_can_apply_action_in_betting_phase() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::PreFlop);

        assert_eq!(
            engine.apply_player_action(SeatIndex(0), PlayerAction::Check),
            Ok(())
        );
    }

    #[test]
    fn check_advances_action_to_next_playing_seat() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::PreFlop);
        engine.drain_events();

        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Check)
            .expect("acting player should be able to check in placeholder betting logic");

        assert_eq!(
            engine.current_hand().and_then(HandState::acting_seat),
            Some(SeatIndex(3))
        );
        assert_eq!(
            engine.events(),
            &[GameEvent::PlayerActed {
                seat: SeatIndex(0),
                action: PlayerAction::Check,
            }]
        );
    }

    #[test]
    fn fold_marks_player_folded_and_advances_action() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::PreFlop);

        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Fold)
            .expect("acting player should be able to fold");

        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(0))
                .expect("player should be seated")
                .status(),
            crate::PlayerStatus::Folded
        );
        assert_eq!(
            engine.current_hand().and_then(HandState::acting_seat),
            Some(SeatIndex(3))
        );
    }

    #[test]
    fn folding_heads_up_clears_acting_seat_when_only_one_player_remains() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::PreFlop);

        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Fold)
            .expect("acting player should be able to fold");

        assert_eq!(engine.current_hand().and_then(HandState::acting_seat), None);
    }

    #[test]
    fn check_fails_when_player_is_facing_a_bet() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");
        advance_to(&mut engine, GamePhase::PreFlop);

        assert_eq!(
            engine.apply_player_action(SeatIndex(0), PlayerAction::Check),
            Err(GameEngineError::CannotCheckFacingBet { amount_to_call: 5 })
        );
    }

    #[test]
    fn call_matches_current_bet_and_advances_action() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");
        advance_to(&mut engine, GamePhase::PreFlop);

        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Call)
            .expect("small blind should be able to call big blind");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.contribution_for(SeatIndex(0)), 10);
        assert_eq!(hand.round_contribution_for(SeatIndex(0)), 10);
        assert_eq!(hand.current_bet(), 10);
        assert_eq!(hand.pot(), 20);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(0))
                .expect("player should be seated")
                .stack(),
            990
        );
    }

    #[test]
    fn call_fails_when_there_is_no_bet_to_call() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::PreFlop);

        assert_eq!(
            engine.apply_player_action(SeatIndex(0), PlayerAction::Call),
            Err(GameEngineError::CannotCallWithoutBet)
        );
    }

    #[test]
    fn big_blind_can_check_after_small_blind_calls() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");
        advance_to(&mut engine, GamePhase::PreFlop);
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Call)
            .expect("small blind should be able to call");

        assert_eq!(
            engine.apply_player_action(SeatIndex(3), PlayerAction::Check),
            Ok(())
        );
    }

    #[test]
    fn preflop_betting_round_completion_advances_to_flop() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");
        advance_to(&mut engine, GamePhase::PreFlop);
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Call)
            .expect("small blind should call");

        engine
            .apply_player_action(SeatIndex(3), PlayerAction::Check)
            .expect("big blind should check");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.phase(), GamePhase::Flop);
        assert_eq!(hand.board().len(), 3);
        assert_eq!(hand.current_bet(), 0);
        assert_eq!(hand.round_contribution_for(SeatIndex(0)), 0);
        assert_eq!(hand.round_contribution_for(SeatIndex(3)), 0);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));
    }

    #[test]
    fn completed_betting_rounds_deal_board_cards_through_showdown() {
        let mut engine = engine_with_started_hand();
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");
        advance_to(&mut engine, GamePhase::PreFlop);

        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Call)
            .expect("small blind should call");
        engine
            .apply_player_action(SeatIndex(3), PlayerAction::Check)
            .expect("big blind should check");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.phase(), GamePhase::Flop);
        assert_eq!(hand.board().len(), 3);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));

        engine
            .apply_player_action(SeatIndex(3), PlayerAction::Check)
            .expect("first postflop player should check");
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Check)
            .expect("second postflop player should check");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.phase(), GamePhase::Turn);
        assert_eq!(hand.board().len(), 4);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));

        engine
            .apply_player_action(SeatIndex(3), PlayerAction::Check)
            .expect("turn first player should check");
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Check)
            .expect("turn second player should check");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.phase(), GamePhase::River);
        assert_eq!(hand.board().len(), HandState::MAX_BOARD_CARDS);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));

        engine
            .apply_player_action(SeatIndex(3), PlayerAction::Check)
            .expect("river first player should check");
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Check)
            .expect("river second player should check");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.phase(), GamePhase::Showdown);
        assert_eq!(hand.board().len(), HandState::MAX_BOARD_CARDS);
        assert_eq!(hand.acting_seat(), None);
    }

    #[test]
    fn bet_starts_new_betting_round_and_advances_action() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);

        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Bet { amount: 25 })
            .expect("acting player should be able to bet when no bet exists");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.contribution_for(SeatIndex(0)), 25);
        assert_eq!(hand.round_contribution_for(SeatIndex(0)), 25);
        assert_eq!(hand.current_bet(), 25);
        assert_eq!(hand.pot(), 25);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(0))
                .expect("player should be seated")
                .stack(),
            975
        );
    }

    #[test]
    fn checking_around_completes_postflop_betting_round() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);
        engine.deal_flop().expect("flop should deal");

        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Check)
            .expect("first player should check");
        engine
            .apply_player_action(SeatIndex(3), PlayerAction::Check)
            .expect("second player should check");
        engine
            .apply_player_action(SeatIndex(5), PlayerAction::Check)
            .expect("third player should check");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.phase(), GamePhase::Turn);
        assert_eq!(hand.board().len(), 4);
        assert_eq!(hand.current_bet(), 0);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));
    }

    #[test]
    fn bet_resets_prior_checks_so_everyone_can_respond() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);
        engine.deal_flop().expect("flop should deal");
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Check)
            .expect("first player should check");
        engine
            .apply_player_action(SeatIndex(3), PlayerAction::Bet { amount: 25 })
            .expect("second player should bet");

        engine
            .apply_player_action(SeatIndex(5), PlayerAction::Call)
            .expect("third player should call");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.phase(), GamePhase::Flop);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(0)));

        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Call)
            .expect("first player should get a chance to answer the bet");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.phase(), GamePhase::Turn);
        assert_eq!(hand.board().len(), 4);
        assert_eq!(hand.current_bet(), 0);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));
    }

    #[test]
    fn bet_fails_when_current_bet_already_exists() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Bet { amount: 25 })
            .expect("first bet should succeed");

        assert_eq!(
            engine.apply_player_action(SeatIndex(3), PlayerAction::Bet { amount: 50 }),
            Err(GameEngineError::CannotBetFacingBet { current_bet: 25 })
        );
    }

    #[test]
    fn bet_must_meet_minimum_bet() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);

        assert_eq!(
            engine.apply_player_action(SeatIndex(0), PlayerAction::Bet { amount: 9 }),
            Err(GameEngineError::BetTooSmall {
                amount: 9,
                minimum: 10,
            })
        );
    }

    #[test]
    fn bet_fails_when_amount_exceeds_stack() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);

        assert_eq!(
            engine.apply_player_action(SeatIndex(0), PlayerAction::Bet { amount: 1_001 }),
            Err(GameEngineError::Player(PlayerError::NotEnoughChips {
                requested: 1_001,
                available: 1_000,
            }))
        );
    }

    #[test]
    fn raise_increases_current_bet_and_advances_action() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Bet { amount: 25 })
            .expect("first bet should succeed");

        engine
            .apply_player_action(SeatIndex(3), PlayerAction::Raise { amount: 50 })
            .expect("next player should be able to raise");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.contribution_for(SeatIndex(3)), 50);
        assert_eq!(hand.round_contribution_for(SeatIndex(3)), 50);
        assert_eq!(hand.current_bet(), 50);
        assert_eq!(hand.pot(), 75);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(5)));
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(3))
                .expect("player should be seated")
                .stack(),
            950
        );
    }

    #[test]
    fn raise_fails_when_there_is_no_bet_to_raise() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);

        assert_eq!(
            engine.apply_player_action(SeatIndex(0), PlayerAction::Raise { amount: 25 }),
            Err(GameEngineError::CannotRaiseWithoutBet)
        );
    }

    #[test]
    fn raise_must_increase_current_bet_by_at_least_big_blind() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Bet { amount: 25 })
            .expect("first bet should succeed");

        assert_eq!(
            engine.apply_player_action(SeatIndex(3), PlayerAction::Raise { amount: 34 }),
            Err(GameEngineError::RaiseTooSmall {
                amount: 34,
                minimum: 35,
            })
        );
    }

    #[test]
    fn raise_only_debits_amount_needed_above_existing_round_contribution() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::PostingBlinds);
        engine.post_blinds().expect("blinds should post");
        advance_to(&mut engine, GamePhase::PreFlop);
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Call)
            .expect("small blind should call to the current bet");

        engine
            .apply_player_action(SeatIndex(3), PlayerAction::Raise { amount: 30 })
            .expect("big blind should raise from their existing contribution");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.contribution_for(SeatIndex(3)), 30);
        assert_eq!(hand.round_contribution_for(SeatIndex(3)), 30);
        assert_eq!(hand.current_bet(), 30);
        assert_eq!(hand.pot(), 50);
        assert_eq!(
            engine
                .table()
                .player_at(SeatIndex(3))
                .expect("player should be seated")
                .stack(),
            970
        );
    }

    #[test]
    fn raise_fails_when_amount_needed_exceeds_stack() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Bet { amount: 25 })
            .expect("first bet should succeed");

        assert_eq!(
            engine.apply_player_action(SeatIndex(3), PlayerAction::Raise { amount: 1_001 }),
            Err(GameEngineError::Player(PlayerError::NotEnoughChips {
                requested: 1_001,
                available: 1_000,
            }))
        );
    }

    #[test]
    fn all_in_commits_remaining_stack_and_advances_action() {
        let mut engine = engine_with_three_players_started_hand();
        advance_to(&mut engine, GamePhase::Flop);

        engine
            .apply_player_action(SeatIndex(0), PlayerAction::AllIn)
            .expect("acting player should be able to move all in");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.contribution_for(SeatIndex(0)), 1_000);
        assert_eq!(hand.round_contribution_for(SeatIndex(0)), 1_000);
        assert_eq!(hand.current_bet(), 1_000);
        assert_eq!(hand.pot(), 1_000);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(3)));

        let player = engine
            .table()
            .player_at(SeatIndex(0))
            .expect("player should be seated");
        assert_eq!(player.stack(), 0);
        assert_eq!(player.status(), crate::PlayerStatus::AllIn);
    }

    #[test]
    fn all_in_for_less_than_current_bet_does_not_raise_current_bet() {
        let mut engine =
            GameEngine::new(TableConfig::new(6, 5, 10, 1, 2_000).expect("config should be valid"));
        engine
            .table_mut()
            .sit_player(PlayerId(1), "Ada", SeatIndex(0), 1_000)
            .expect("first player should sit");
        engine
            .table_mut()
            .sit_player(PlayerId(2), "Grace", SeatIndex(3), 40)
            .expect("short-stacked player should sit");
        engine
            .table_mut()
            .sit_player(PlayerId(3), "Linus", SeatIndex(5), 1_000)
            .expect("third player should sit");
        engine
            .start_hand(SeatIndex(0))
            .expect("hand should start with three players");
        advance_to(&mut engine, GamePhase::Flop);
        engine
            .apply_player_action(SeatIndex(0), PlayerAction::Bet { amount: 100 })
            .expect("first bet should succeed");

        engine
            .apply_player_action(SeatIndex(3), PlayerAction::AllIn)
            .expect("short-stacked player should be able to move all in");

        let hand = engine.current_hand().expect("hand should be active");
        assert_eq!(hand.contribution_for(SeatIndex(3)), 40);
        assert_eq!(hand.round_contribution_for(SeatIndex(3)), 40);
        assert_eq!(hand.current_bet(), 100);
        assert_eq!(hand.pot(), 140);
        assert_eq!(hand.acting_seat(), Some(SeatIndex(5)));

        let player = engine
            .table()
            .player_at(SeatIndex(3))
            .expect("player should be seated");
        assert_eq!(player.stack(), 0);
        assert_eq!(player.status(), crate::PlayerStatus::AllIn);
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
