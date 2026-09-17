//! Small shared pieces of English, so that one refusal does not disagree with another about how
//! to name a list of things.

/// The values in the order given, without repeats. A value that appears twice is kept once,
/// because a message that repeats itself reads as two problems where there is one.
fn without_repeats(values: &[String]) -> Vec<&str> {
    let mut seen: Vec<&str> = Vec::new();
    for value in values {
        if !seen.contains(&value.as_str()) {
            seen.push(value);
        }
    }
    seen
}

/// The distinct values in the order given, separated by commas.
pub(crate) fn distinct(values: &[String]) -> String {
    without_repeats(values).join(", ")
}

/// `singular` when the distinct values are one and `plural` when they are more, followed by them:
/// "criterion C1", or "criteria C1, C2". How many values there are decides, not what is inside
/// them: an id a caller wrote with a comma in it is still one id.
pub(crate) fn listed(singular: &str, plural: &str, values: &[String]) -> String {
    let named = without_repeats(values);
    let word = if named.len() == 1 { singular } else { plural };
    format!("{word} {}", named.join(", "))
}
