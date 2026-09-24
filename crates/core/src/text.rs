//! Small shared pieces of English, so that one refusal does not disagree with another about how
//! to name a list of things, and the one way Farik counts a text's tokens.

/// The tokens of `text` as Farik counts them without a tokenizer: a quarter of its characters,
/// rounded up. A notebook's cap and the channel's summary are both measured with it, so that
/// neither disagrees with the other about how long a text is.
#[must_use]
pub fn tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

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

#[cfg(test)]
mod tests {
    use super::tokens;

    #[test]
    fn counts_tokens_as_a_quarter_of_the_characters() {
        // Characters, not bytes: an accented letter is one character and two bytes.
        assert_eq!(tokens(""), 0);
        assert_eq!(tokens("é"), 1);
        assert_eq!(tokens("abcdefgh"), 2);
        assert_eq!(tokens("abcdefghé"), 3);
        // Eight bytes, which counted as bytes would be two tokens.
        assert_eq!(tokens("éééé"), 1);
    }
}
