//! Small shared pieces of English, so that one refusal does not disagree with another about how
//! to name a list of things.

/// The values in the order given, without repeats, separated by commas. A value that appears
/// twice is named once, because a message that repeats itself reads as two problems where there
/// is one.
pub(crate) fn distinct(values: &[String]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for value in values {
        if !seen.contains(&value.as_str()) {
            seen.push(value);
        }
    }
    seen.join(", ")
}

/// `singular` when the distinct values are one and `plural` when they are more, followed by them:
/// "criterion C1", or "criteria C1, C2".
pub(crate) fn listed(singular: &str, plural: &str, values: &[String]) -> String {
    let named = distinct(values);
    let one = !named.contains(", ");
    format!("{} {named}", if one { singular } else { plural })
}
