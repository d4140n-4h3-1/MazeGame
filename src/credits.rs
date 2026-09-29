//! Credits, the maze's money: whole credits and hundredths of one, as dollars and cents are, kept
//! as a whole number of hundredths so that they add up exactly. Written `1,234.56 CR`.
//!
//! Every computer carries some, whatever else it has on it - a random amount, most of them a few
//! credits and now and then a good deal more (see [`Credits::random`]) - which clearing its hack
//! transfers to the player (see [`crate::computer`]).

use std::{fmt, ops::AddAssign};

/// How many credits a computer can carry at most, in whole credits.
const MOST: f64 = 500.0;

/// An amount of credits, in hundredths of a credit.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Credits(pub u64);

impl Credits {
    /// `whole` credits and `cents` hundredths of one.
    pub fn new(whole: u64, cents: u64) -> Self {
        Self(whole * 100 + cents)
    }

    /// The whole credits in it, and the hundredths left over.
    pub fn whole(self) -> u64 {
        self.0 / 100
    }

    pub fn cents(self) -> u64 {
        self.0 % 100
    }

    /// A computer's credits, from `below`, which gives a number below the one it is given: never
    /// none, mostly a few credits, now and then up to [`MOST`] - the whole credits go as the cube
    /// of an even chance, so a quarter of computers carry under 8, half under 63, and one in ten
    /// over 364.
    pub fn random(mut below: impl FnMut(usize) -> usize) -> Self {
        let chance = below(1001) as f64 / 1000.0;
        let whole = (chance.powi(3) * MOST) as u64;
        let cents = below(100) as u64;
        Self::new(whole, cents).max(Self(1))
    }
}

impl AddAssign for Credits {
    fn add_assign(&mut self, other: Self) {
        self.0 = self.0.saturating_add(other.0);
    }
}

/// As `1,234.56 CR`: the whole credits in threes, and always both hundredths.
impl fmt::Display for Credits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = self.whole().to_string();
        let mut whole = String::new();
        for (n, digit) in digits.chars().enumerate() {
            if n > 0 && (digits.len() - n) % 3 == 0 {
                whole.push(',');
            }
            whole.push(digit);
        }
        write!(f, "{whole}.{:02} CR", self.cents())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_are_written_as_dollars_and_cents_are() {
        assert_eq!(Credits(0).to_string(), "0.00 CR");
        assert_eq!(Credits(5).to_string(), "0.05 CR");
        assert_eq!(Credits(1_250).to_string(), "12.50 CR");
        assert_eq!(Credits::new(1_234, 56).to_string(), "1,234.56 CR");
        assert_eq!(Credits::new(1_000_000, 7).to_string(), "1,000,000.07 CR");
    }

    #[test]
    fn hundredths_add_up_exactly_into_whole_credits() {
        let mut wallet = Credits::default();
        for _ in 0..10 {
            wallet += Credits(10);
        }
        assert_eq!((wallet.whole(), wallet.cents()), (1, 0));
        wallet += Credits::new(2, 95);
        assert_eq!(wallet, Credits::new(3, 95));
    }

    #[test]
    fn every_computer_carries_some_and_mostly_a_little() {
        // Every chance and every hundredth, as the dice could give them.
        let (mut least, mut most, mut small) = (Credits(u64::MAX), Credits(0), 0);
        for chance in 0..=1000 {
            for cents in [0, 99] {
                let mut rolls = [chance, cents].into_iter();
                let credits = Credits::random(|_| rolls.next().unwrap());
                least = least.min(credits);
                most = most.max(credits);
                small += usize::from(credits.whole() < 63);
            }
        }
        assert_eq!(least, Credits(1), "never none");
        assert_eq!(most, Credits::new(500, 99));
        assert!(small > 900 && small < 1100, "half under 63: {small} of 2002");
    }
}
