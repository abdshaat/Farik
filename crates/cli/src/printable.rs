//! Text as a terminal may be given it. Questions, reasons, notes, titles, and diffs are written
//! by agents, and an escape sequence among them would be obeyed by the terminal that prints it:
//! it could clear the screen, rewrite a line already printed, or set the window's title.

use std::borrow::Cow;

/// `text` with every control character but a line break and a tab written as `\u` and four hex
/// digits, `\u001b` for an escape, so that the terminal shows it rather than obeys it. Every
/// control character is one such escape, and it is JSON's own, so a JSON line stays the same JSON:
/// a control character is found only inside its strings, and there it is the same character.
#[must_use]
pub fn printable(text: &str) -> Cow<'_, str> {
    let is_escaped = |character: char| character.is_control() && !matches!(character, '\n' | '\t');
    if !text.chars().any(is_escaped) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .map(|character| {
                if is_escaped(character) {
                    format!("\\u{:04x}", u32::from(character))
                } else {
                    character.to_string()
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::printable;

    #[test]
    fn escapes_every_control_character_but_a_line_break_and_a_tab() {
        assert_eq!(
            printable("a\u{1b}[2Jb\u{7}c\rd\u{9b}e\u{7f}\n\tf"),
            "a\\u001b[2Jb\\u0007c\\u000dd\\u009be\\u007f\n\tf"
        );
        assert_eq!(printable("plain, with é"), "plain, with é");
    }

    #[test]
    fn leaves_a_json_line_the_same_json() {
        // serde_json writes a C1 control character and DEL as they are.
        let line = json!({ "question": "a\u{9b}b\u{7f}\u{1b}" }).to_string();
        let read: Value = serde_json::from_str(&printable(&line)).expect("still JSON");
        assert_eq!(read["question"], "a\u{9b}b\u{7f}\u{1b}");
    }
}
