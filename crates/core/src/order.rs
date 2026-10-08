//! A purchase order's lines and total (`docs/SPEC.md` 6.10, ADR 0039): the sums Farik writes into
//! an order's workbook and records, exact in hundredths. Pure: the caller parses the amounts.

use std::fmt;

use crate::marketing::Amount;

/// The most an order may total, in hundredths: 10,000,000.00.
const MOST_TOTAL: u64 = 1_000_000_000;

/// One line of a purchase order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderLine {
    /// What is bought, on one line.
    pub item: String,
    /// How many, from 1 to 1,000,000.
    pub quantity: u32,
    /// The price of one, in hundredths of the order's currency.
    pub unit_price: Amount,
    /// What one is counted in (`kg`, `box`), possibly empty.
    pub unit: String,
}

/// How often an order's lines are paid: a subscription's are per period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderPeriod {
    /// Once.
    Once,
    /// Every month.
    Month,
    /// Every year.
    Year,
}

/// Why an order has no total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderError {
    /// The lines add to more than 10,000,000.00; `total` is the sum, saturating.
    TooLarge {
        /// What the lines add to.
        total: Amount,
    },
}

impl fmt::Display for OrderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { total } => write!(
                formatter,
                "the order totals {total}, and an order is at most {}",
                Amount(MOST_TOTAL)
            ),
        }
    }
}

impl std::error::Error for OrderError {}

/// What one line comes to: its quantity times its unit price. `None` when that passes what a
/// `u64` of hundredths holds.
#[must_use]
pub fn line_total(line: &OrderLine) -> Option<Amount> {
    u64::from(line.quantity)
        .checked_mul(line.unit_price.0)
        .map(Amount)
}

/// What the lines add to, exact in hundredths.
///
/// # Errors
///
/// [`OrderError::TooLarge`] when the sum passes 10,000,000.00.
pub fn order_total(lines: &[OrderLine]) -> Result<Amount, OrderError> {
    let total = lines.iter().fold(0_u64, |sum, line| {
        sum.saturating_add(line_total(line).map_or(u64::MAX, |amount| amount.0))
    });
    if total > MOST_TOTAL {
        return Err(OrderError::TooLarge {
            total: Amount(total),
        });
    }
    Ok(Amount(total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketing::{Amount, parse_amount};

    fn a_line(quantity: u32, unit_price: &str) -> OrderLine {
        OrderLine {
            item: "Baby car mirror".to_string(),
            quantity,
            unit_price: parse_amount(unit_price).expect("an amount"),
            unit: String::new(),
        }
    }

    #[test]
    fn order_total_adds_exactly() {
        let lines = [a_line(3, "19.99"), a_line(1, "0.01")];
        assert_eq!(line_total(&lines[0]), Some(Amount(5997)));
        assert_eq!(order_total(&lines), Ok(Amount(5998)));
        assert_eq!(order_total(&lines).expect("a total").to_string(), "59.98");

        let at_the_limit = [a_line(1_000_000, "10.00")];
        assert_eq!(
            order_total(&at_the_limit).expect("a total").to_string(),
            "10000000.00"
        );
        let past_it = [a_line(1_000_000, "10.00"), a_line(1, "0.01")];
        assert_eq!(
            order_total(&past_it),
            Err(OrderError::TooLarge {
                total: Amount(1_000_000_001)
            })
        );
        let message = order_total(&past_it).expect_err("too large").to_string();
        assert!(message.contains("10000000.01"), "{message}");
    }

    #[test]
    fn a_line_total_past_a_machine_word_is_none() {
        let huge = OrderLine {
            item: "x".to_string(),
            quantity: u32::MAX,
            unit_price: Amount(u64::MAX),
            unit: String::new(),
        };
        assert_eq!(line_total(&huge), None);
        assert!(matches!(
            order_total(&[huge]),
            Err(OrderError::TooLarge { .. })
        ));
    }
}
