use std::{cmp::Ordering, fmt::{Formatter, Display}, time::Duration};

use cardy::{face::Face, hand::Hand, holder::Holder};
use colored::Colorize;
use prediput::select::Select;

/// Value for a player to bust at.
pub const BUST_THRESHOLD: usize = 21;
/// Value for the dealer to stand at.
pub const DEALER_STAND_THRESHOLD: usize = 18;
/// Value to multiply bet by when the player wins.
pub const WIN_MULTIPLIER: f64 = 0.6; // 3/5 or 3:2
/// Value to multiply bet by when doubling down.
pub const DOUBLE_DOWN_MULTIPLIER: f64 = 2.;

/// Time to "simulate" a card being dealt, so that the player can see what's happening without printing excess lines.
pub const DEALING_SIMULATION_TIME: Duration = Duration::from_millis(800);
/// Percent of deck that must be used in order for a new one to be used instead.
pub const DECK_REPLACEMENT_THRESHOLD: f64 = 0.5;
pub const WINNINGS_UNIT_STR: &str = "$";
pub const STANDARD_NUM_DECKS: usize = 4;

pub const PLAYER_COLOR: (u8, u8, u8) = (110, 157, 211);
pub const DEALER_COLOR: (u8, u8, u8) = (113, 110, 211);
pub const SUM_COLOR: (u8, u8, u8) = (110, 208, 211);
pub const WINNINGS_COLOR: (u8, u8, u8) = (206, 148, 22);
pub const LIGHT_TEXT: (u8, u8, u8) = (200, 200, 200);
pub const FG_TEXT_COLOR: (u8, u8, u8) = (160, 160, 160);
pub const BG_TEXT_COLOR: (u8, u8, u8) = (120, 120, 120);

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Decision {
    Hit,
    Stand
}


#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Outcome {
    Holding(usize), Bust
}

impl Ord for Outcome {
    fn cmp(&self, other: &Self) -> Ordering {
        match (*self, other) {
            (Outcome::Bust, Outcome::Bust) => Ordering::Equal,
            (Outcome::Holding(_), Outcome::Bust) => Ordering::Greater,
            (Outcome::Bust, Outcome::Holding(_)) => Ordering::Less,
            (Outcome::Holding(v1), Outcome::Holding(v2)) => v1.cmp(v2)
        }
    }
}

impl PartialOrd for Outcome {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for Outcome {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            Outcome::Holding(sum) => write!(f, "{}", sum.to_string().as_str().truecolor(SUM_COLOR.0, SUM_COLOR.1, SUM_COLOR.2)),
            Outcome::Bust => write!(f, "{}", "BUST".bright_red())
        }
    }
}

pub fn get_outcome(hand: &Hand) -> Outcome {
    let sum = hand_val(hand);
    if sum > BUST_THRESHOLD {
        return Outcome::Bust;
    }
    Outcome::Holding(sum)
}

pub fn face_val(sum: usize, face: Face) -> usize {
    match face {
        Face::Ace if sum > 10 => 1,
        Face::Ace => 11,
        Face::King | Face::Queen | Face::Jack | Face::Ten => 10,
        Face::Nine => 9,
        Face::Eight => 8,
        Face::Seven => 7,
        Face::Six => 6,
        Face::Five => 5,
        Face::Four => 4,
        Face::Three => 3,
        Face::Two => 2,
    }
}

pub fn hand_val(hand: &Hand) -> usize {
    hand.cards().iter().fold(0, |acc, card| acc + face_val(acc, card.face))
}

pub fn hand_as_str(hand: &Hand) -> String {
    format!("✋{}🤚 {}", hand, get_outcome(hand).to_string().truecolor(SUM_COLOR.0, SUM_COLOR.1, SUM_COLOR.2))
}

pub fn round_decimal(decimal: f64, places: usize) -> f64 {
    (decimal * 10f64.powi(places as i32)).round() / 10f64.powi(places as i32)
}

pub fn prompt_player() -> Decision {
    let (br, bg, bb) = BG_TEXT_COLOR;

    let prefix = "➜ ".yellow().bold().to_string();
    let hit_opt_string = "Hit".truecolor(br, bg, bb).to_string();
    let stand_opt_string = "Stand".truecolor(br, bg, bb).to_string();
    let hit_selected_string = format!(" {}{}", "Hit".yellow(), ": Request to add another card".truecolor(br, bg, bb));
    let stand_selected_string = format!(" {}{}", "Stand".yellow(), ": End turn as is".truecolor(br, bg, bb));

    'prompting: loop
    {
        let sel = Select::new(&prefix, vec![(&hit_opt_string, Some(&hit_selected_string), Decision::Hit), (&stand_opt_string, Some(&stand_selected_string), Decision::Stand)])
            .padding(1).override_prefix_len(3).aligned().clear_after();

        match sel.prompt("Make a decision:")
        {
            Ok((_, _, first_decision)) => {
                return first_decision;
            }
            Err(e) => {
                println!("Something went wrong: {}", e.to_string());
                continue 'prompting;
            }
        }
    }
}

pub fn prompt_dealer(hand: &Hand, score_to_beat: usize) -> Decision {
    let sum = hand_val(hand);
    if sum >= DEALER_STAND_THRESHOLD || sum > score_to_beat {
        return Decision::Stand;
    }
    Decision::Hit
}
