use std::cmp::Ordering;
use std::thread;

use blackjack::{
    face_val, get_outcome, hand_as_str, hand_val, prompt_dealer, prompt_player, round_decimal,
    Decision, Outcome, BG_TEXT_COLOR, DEALER_COLOR, DEALER_STAND_THRESHOLD,
    DEALING_SIMULATION_TIME, DECK_REPLACEMENT_THRESHOLD, DOUBLE_DOWN_MULTIPLIER, FG_TEXT_COLOR,
    LIGHT_TEXT, PLAYER_COLOR, STANDARD_NUM_DECKS, SUM_COLOR, WINNINGS_COLOR, WINNINGS_UNIT_STR,
    WIN_MULTIPLIER,
};
use cardy::{deck::Deck, hand::Hand, holder::Holder};
use colored::*;
use console::Term;
use prediput::prompting::{Predicate, Prompter};
use prediput::{any_key_continue, confirm};

/*
* treat as a 1-player game

Terms:
    Blackjack: Dealt 21 on the first hand
    Stand: A decision where the player stops hitting
    Push: Both parties get their bet back
    Double down: After being initially dealt two cards, the player can "double down" to hit once and then stand after. They will gain or lose double their original bet depending on the game's outcome.
    Soft Ace: An ace is normally valued at 11, but if that makes a hand exceed 21, it instead is valued at 1.
*   Splitting: If dealt two cards, treat each card as separate hands (the bet is applied at the same value for both hands). Cannot split after doubling down or splitting once.

BEFORE GAME
    1. State 3:2 +(50%) payout for blackjack
    2. State dealer hits on soft 17 (stands on x >= 18) or hard 17 (stands on x >= 17)
    3. Prompt for a bet

DURING GAME
    1. Create 4 decks, shuffled into one
    2. Deal 2 cards to house, reveal one
    3. Deal 2 cards to player, reveal both
    4. Check for a blackjack between the player and house.
        - If both have a blackjack, immediately end the game with no gain/loss for either party
        - If one has a blackjack, immediately end the game in their favor
    5. Let the player make a decision (hit, stand, double down)
        - If they double down, they must hit once and stand immediately after.
        - If the player busts, immediately end the game (dealer wins)
    6. Reveal the house's second card
    7. Let the house make a decision (hit, stand)
        - The house will continue hitting until their sum exceeds a threshold
            - Hard: stand on 17 or above
            - Soft: stand on 18 or above
        - If the house busts, the player wins (given they didn't bust first)
    8. Compare the player and house's sums; whoever has the greater sum wins.
    9. Provide winnings at a 3:2 rate to the player if they win, or take the entire bid if they lose.

AFTER GAME
    * Use a new shuffled deck if 50% of the existing deck is consumed
*/
fn main() {
    #[cfg(target_os = "windows")]
    control::set_virtual_terminal(true)
        .expect("failed to set virtual terminal after recognizing windows operating system");

    let mut winnings: f64 = 100.;
    let soft_terms: (&str, usize) = if DEALER_STAND_THRESHOLD == 18 {
        ("soft", 18)
    } else {
        ("hard", 17)
    };
    let (sr, sg, sb) = SUM_COLOR;
    let (fr, fg, fb) = FG_TEXT_COLOR;
    let (wr, wg, wb) = WINNINGS_COLOR;
    let term = Term::stdout();

    term.show_cursor().unwrap();

    let num_decks: usize = 4;
    let mut deck = Deck::make_decks(num_decks).shuffled();

    loop {
        // 1 - Announce required rules
        term.clear_screen().unwrap();
        println!(
            "Your balance: {}",
            format!("{}{}", WINNINGS_UNIT_STR, winnings)
                .as_str()
                .truecolor(wr, wg, wb)
        );
        println!();
        println!(
            "{}",
            format!(
                "The dealer rewards you at {} of your bet as winnings.",
                format!("+{:.0}%", (WIN_MULTIPLIER * 100.))
                    .to_string()
                    .as_str()
                    .truecolor(wr, wg, wb)
            )
            .as_str()
            .truecolor(fr, fg, fb)
        );
        println!(
            "{}",
            format!(
                "{} decks are shuffled together, which refreshes when {} of the deck is used.",
                STANDARD_NUM_DECKS.to_string().as_str().white(),
                format!("{:.0}%", (DECK_REPLACEMENT_THRESHOLD * 100.)).white()
            )
            .truecolor(fr, fg, fb)
        );
        println!("{}", format!("The dealer stands at {} 17 (when their sum is {} or above) or if their sum exceeds the player.", soft_terms.0.to_string().as_str().truecolor(sr, sg, sb), soft_terms.1.to_string().as_str().truecolor(sr, sg, sb)).as_str().truecolor(fr, fg, fb));
        println!();

        if deck.dealt_count() as f64
            >= DECK_REPLACEMENT_THRESHOLD * (deck.undealt_count() + deck.dealt_count()) as f64
        {
            deck.reset();
            deck.shuffle();
            println!("{}", "Reset and shuffled the deck.".truecolor(fr, fg, fb));
        }

        // Prompt for bet
        let winnings_pred: Predicate<f64> = Predicate::new(
            "Your bid must be less than your balance!",
            Box::new(move |uinput| *uinput <= winnings),
        );
        let cent_pred: Predicate<f64> = Predicate::new(
            "You must enter at least a cent!",
            Box::new(|uinput| *uinput >= 0.01),
        );
        let bid_prompter = Prompter::new("Please enter a decimal!")
            .pred(cent_pred)
            .pred(winnings_pred);
        let bet = round_decimal(
            bid_prompter.prompt(
                format!("What is your bet? {}", WINNINGS_UNIT_STR.white())
                    .truecolor(wr, wg, wb)
                    .to_string()
                    .as_str(),
            ),
            2,
        );

        let change_in_winnings = play(bet, &mut deck);
        println!(
            "{}\n",
            report_earnings_progression(winnings, change_in_winnings)
        );

        if winnings + change_in_winnings < 0.01 {
            println!(
                "{}",
                "You were donated a cent from charity.".truecolor(wr, wg, wb)
            );
        }
        winnings = round_decimal((winnings + change_in_winnings).max(0.01), 2);
        any_key_continue().unwrap();
    }
}

/// Returns the change (gain or loss) in winnings from the bet
fn play(bet: f64, deck: &mut Deck) -> f64 {
    let term = Term::stdout();
    let mut player_hand = Hand::new();
    let mut dealer_hand = Hand::new();

    let (dr, dg, db) = DEALER_COLOR;
    let (pr, pg, pb) = PLAYER_COLOR;
    let (sr, sg, sb) = SUM_COLOR;
    let (fr, fg, fb) = FG_TEXT_COLOR;
    let (br, bg, bb) = BG_TEXT_COLOR;

    // 2 - Deal to dealer
    println!("\n{}", "Dealing...".truecolor(fr, fg, fb).reversed());
    println!();

    for i in 0..2 {
        // dealer
        let card_dealt = deck
            .deal_one()
            .expect("unexpectedly no cards are remaining in the deck");
        let hand_sum = hand_val(&dealer_hand);
        let is_blackjack = hand_sum + face_val(hand_sum, card_dealt.face) == 21;

        let card_dealt = if i == 1 && !is_blackjack {
            card_dealt.hidden()
        } else {
            card_dealt
        }; // deal the second card face down unless it's a blackjack
        dealer_hand.push_card(card_dealt);

        term.clear_last_lines(1).unwrap();
        let hand_str = match (i, is_blackjack) {
            (1, false) => "?".truecolor(sr, sg, sb).to_string(),
            (1, true) => "BJ".black().to_string(),
            _ => hand_val(&dealer_hand).to_string(),
        };

        println!(
            " {} ✋{}🤚 {}",
            "Dealer".truecolor(dr, dg, db),
            dealer_hand,
            hand_str.as_str().truecolor(sr, sg, sb)
        );
        thread::sleep(DEALING_SIMULATION_TIME);
    }

    // 3 - Deal to player
    println!();
    for i in 0..2 {
        // player
        let card_dealt = deck
            .deal_one()
            .expect("unexpectedly no cards are remaining in the deck");
        let hand_sum = hand_val(&player_hand);
        let is_blackjack = hand_sum + face_val(hand_sum, card_dealt.face) == 21;
        player_hand.push_card(card_dealt);

        term.clear_last_lines(1).unwrap();
        let hand_str = match (i, is_blackjack) {
            (1, true) => "BJ".black().to_string(),
            _ => hand_val(&player_hand).to_string(),
        };

        println!(
            "    {} ✋{}🤚 {}",
            "You".truecolor(pr, pg, pb),
            player_hand,
            hand_str.as_str().truecolor(sr, sg, sb)
        );
        thread::sleep(DEALING_SIMULATION_TIME);
    }

    // 4 - Check for blackjacks
    match (hand_val(&player_hand), hand_val(&dealer_hand)) {
        (21, 21) => {
            println!(
                "\n{}",
                "Both players had blackjacks, so the game is a draw. No bets are recognized."
                    .truecolor(fr, fg, fb)
            );
            return 0.;
        }
        (21, _) => {
            println!("\n{}", "You got a blackjack and won the game!".green());
            return round_decimal(bet * WIN_MULTIPLIER, 2);
        }
        (_, 21) => {
            println!(
                "\n{}",
                "The dealer got a blackjack, so you lost the game.".red()
            );
            return round_decimal(-bet, 2);
        }
        _ => {}
    }

    println!("\n{}", "Your turn.".truecolor(pr, pg, pb).reversed());
    let is_doubling_down = confirm(
        &*format!(
            "Double down? This doubles the wager but forces you to hit then stand. {}",
            "(y/n)".truecolor(br, bg, bb)
        ),
        true,
    )
    .expect("failed to read from terminal");

    //     5. Let the player make decisions (hit, stand, double down)
    let player_outcome = if is_doubling_down {
        //         - If they double down, they must hit once and stand immediately after.
        println!("{}", "You doubled your wager!".bright_red().bold());
        thread::sleep(DEALING_SIMULATION_TIME);
        let first_turn_outcome = simulate_turn(deck, &mut player_hand, Decision::Hit);
        thread::sleep(DEALING_SIMULATION_TIME);

        if first_turn_outcome == Outcome::Bust {
            first_turn_outcome
        } else {
            // don't play the second turn if the first one is a bust
            let second_turn_outcome = simulate_turn(deck, &mut player_hand, Decision::Stand);
            thread::sleep(DEALING_SIMULATION_TIME);
            second_turn_outcome
        }
    } else {
        'hitting: loop {
            let resp = prompt_player();
            let outcome = simulate_turn(deck, &mut player_hand, resp);

            if resp == Decision::Stand || outcome == Outcome::Bust {
                break 'hitting outcome;
            }
        }
    };

    //         - If the player busts, immediately end the game (dealer wins)
    if player_outcome == Outcome::Bust {
        println!("\n{}", "Your hand busted. You lost.".red());
        return round_decimal(
            -bet * if is_doubling_down {
                DOUBLE_DOWN_MULTIPLIER
            } else {
                1.
            },
            2,
        );
    }

    println!("\n{}", "Dealer's turn.".truecolor(dr, dg, db).reversed());

    //     6. Reveal the house's second card
    thread::sleep(DEALING_SIMULATION_TIME);
    println!(
        " {} ✋{}🤚 {}",
        "Dealer".truecolor(dr, dg, db),
        dealer_hand,
        "?".truecolor(sr, sg, sb)
    );

    let c = dealer_hand
        .cards
        .pop()
        .expect("dealer unexpectedly has no cards after being dealt two");
    let c = c.revealed();
    dealer_hand.push_card(c);

    thread::sleep(DEALING_SIMULATION_TIME);
    term.clear_last_lines(1).unwrap();
    println!(
        " {} ✋{}🤚 {}",
        "Dealer".truecolor(dr, dg, db),
        dealer_hand,
        hand_val(&dealer_hand)
            .to_string()
            .as_str()
            .truecolor(sr, sg, sb)
    );
    thread::sleep(DEALING_SIMULATION_TIME);
    //     7. Let the house make a decision (hit, stand)
    let dealer_outcome = loop {
        let score_to_beat = hand_val(&player_hand);
        let resp = prompt_dealer(&dealer_hand, score_to_beat);
        let outcome = simulate_turn(deck, &mut dealer_hand, resp);

        if resp == Decision::Stand || outcome == Outcome::Bust {
            break outcome;
        }
        thread::sleep(DEALING_SIMULATION_TIME);
    };

    //         - If the house busts, the player wins (given they didn't bust first)
    if dealer_outcome == Outcome::Bust {
        println!("\n{}", "The dealer's hand busted. You won!".green());
        return round_decimal(
            bet * WIN_MULTIPLIER
                * if is_doubling_down {
                    DOUBLE_DOWN_MULTIPLIER
                } else {
                    1.
                },
            2,
        );
    }

    //     8. Compare the player and house's sums; whoever has the greater sum wins.
    println!("\n{}", "Results".bold());
    println!(
        " {} {} {}",
        "Dealer".truecolor(dr, dg, db),
        dealer_hand,
        dealer_outcome
    );
    println!(
        "    {} {} {}",
        "You".truecolor(pr, pg, pb),
        player_hand,
        player_outcome
    );
    println!();

    //     8. Compare the player and house's sums; whoever has the greater sum wins.
    //     9. Provide winnings at a 3:2 (3/5) rate to the player if they win, or take the entire bid if they lose.
    let change = match player_outcome.cmp(&dealer_outcome) {
        Ordering::Equal => {
            println!("Draw!");
            0.
        }
        Ordering::Greater => {
            println!("{}", "You won!".green());
            bet * WIN_MULTIPLIER
                * if is_doubling_down {
                    DOUBLE_DOWN_MULTIPLIER
                } else {
                    1.
                }
        }
        Ordering::Less => {
            println!("{}", "You lost!".red());
            -bet * if is_doubling_down {
                DOUBLE_DOWN_MULTIPLIER
            } else {
                1.
            }
        }
    };
    round_decimal(change, 2)
}

fn report_earnings_progression(balance: f64, change: f64) -> String {
    let (wr, wg, wb) = WINNINGS_COLOR;
    let (fr, fg, fb) = FG_TEXT_COLOR;

    let change_str = if change > 0. {
        format!("+ {} ", round_decimal(change.abs(), 2))
            .as_str()
            .green()
            .to_string()
    } else if change < 0. {
        format!("- {} ", round_decimal(change.abs(), 2))
            .as_str()
            .red()
            .to_string()
    } else {
        String::new()
    };

    format!(
        "{} {}➜ {}",
        format!("{}{}", WINNINGS_UNIT_STR, balance)
            .as_str()
            .truecolor(wr, wg, wb),
        change_str,
        format!(
            "{}{}",
            WINNINGS_UNIT_STR,
            round_decimal((balance + change).max(0.), 2)
        )
        .as_str()
        .truecolor(wr, wg, wb)
    )
    .as_str()
    .truecolor(fr, fg, fb)
    .to_string()
}

fn simulate_turn(deck: &mut Deck, hand: &mut Hand, decision: Decision) -> Outcome {
    match decision {
        Decision::Hit => {
            let card_to_deal = deck
                .deal_one()
                .expect("unexpectedly no cards were left in the deck");
            hand.push_card(card_to_deal);
            let outcome = get_outcome(hand);
            println!("    {} {}", "HIT".yellow(), hand_as_str(hand));
            outcome
        }
        Decision::Stand => {
            let outcome = get_outcome(hand);
            let (r, g, b) = LIGHT_TEXT;
            println!("  {} {}", "STAND".truecolor(r, g, b), hand_as_str(hand));
            outcome
        }
    }
}
