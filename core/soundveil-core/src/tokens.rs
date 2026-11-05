//! Rebuilds words from Whisper's tokens.

use crate::expressive::cue_label;
use crate::pipeline::Word;

const OPENERS: [char; 5] = ['[', '(', '♪', '♫', '*'];
const CLOSERS: [char; 5] = [']', ')', '♪', '♫', '*'];

/// Spaces and tabs, without the line breaks `str::trim` would also remove.
fn is_blank(c: char) -> bool {
    c == '\t' || (c.is_whitespace() && !c.is_control() && !matches!(c, '\u{2028}' | '\u{2029}'))
}

#[derive(Default)]
struct Joiner {
    texts: Vec<String>,
    bytes: Vec<u8>,
    note: String,
}

impl Joiner {
    fn close(&mut self) {
        if self.bytes.is_empty() {
            return;
        }
        let joined = String::from_utf8_lossy(&self.bytes).into_owned();
        self.bytes.clear();
        let mut text = joined.trim_matches(is_blank);
        // Non-speech notes can span tokens, such as "[" "MUSIC" "]". Known ones become a cue.
        if self.note.is_empty() && text.starts_with(OPENERS) {
            self.note.push(' ');
        }
        if !self.note.is_empty() {
            self.note.push_str(text);
            self.note.push(' ');
            if text.ends_with(CLOSERS) {
                if let Some(label) = cue_label(&self.note) {
                    let cue = format!("[{label}]");
                    if self.texts.last() != Some(&cue) {
                        self.texts.push(cue);
                    }
                }
                self.note.clear();
            }
            return;
        }
        while let Some(rest) = text.strip_suffix('…') {
            text = rest;
        }
        while let Some(rest) = text.strip_suffix("...") {
            text = rest;
        }
        if !text.is_empty() {
            self.texts.push(text.to_string());
        }
    }
}

/// Whisper's tokens are fragments of words, and of UTF-8 sequences, so bytes are joined before
/// decoding. A token that opens with a space starts a new word. Times are estimated from each
/// word's share of the text, which is close enough for history timestamps. Each inner list is
/// one Whisper segment's text tokens.
pub fn words_from_tokens(segments: &[Vec<Vec<u8>>], start: f64, end: f64) -> Vec<Word> {
    let mut joiner = Joiner::default();
    for segment in segments {
        for piece in segment {
            if piece.first() == Some(&b' ') {
                joiner.close();
            }
            joiner.bytes.extend_from_slice(piece);
        }
        joiner.close();
    }
    let lengths = |text: &String| text.chars().count() + 1;
    let total = joiner.texts.iter().map(lengths).sum::<usize>().max(1) as f64;
    let span = end - start;
    let mut offset = 0.0;
    joiner
        .texts
        .iter()
        .map(|text| {
            let begin = offset;
            offset += lengths(text) as f64;
            Word::new(
                text.clone(),
                start + begin / total * span,
                start + offset / total * span,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(tokens: &[&str]) -> Vec<Vec<Vec<u8>>> {
        vec![tokens.iter().map(|t| t.as_bytes().to_vec()).collect()]
    }
    fn texts(tokens: &[&str]) -> Vec<String> {
        words_from_tokens(&pieces(tokens), 0.0, 1.0)
            .into_iter()
            .map(|word| word.text)
            .collect()
    }

    #[test]
    fn fragments_join_into_words() {
        assert_eq!(
            texts(&[" And", " so", " my", " fel", "low"]),
            ["And", "so", "my", "fellow"]
        );
    }

    #[test]
    fn split_characters_are_rejoined() {
        let segments = vec![vec![b" caf\xc3".to_vec(), b"\xa9".to_vec()]];
        assert_eq!(words_from_tokens(&segments, 0.0, 1.0)[0].text, "café");
    }

    #[test]
    fn notes_across_tokens_become_a_cue() {
        assert_eq!(
            texts(&[" [", " MUSIC", " ]", " Right"]),
            ["[music]", "Right"]
        );
    }

    #[test]
    fn unknown_notes_are_dropped() {
        assert_eq!(texts(&[" [BLANK_AUDIO]", " Right"]), ["Right"]);
    }

    #[test]
    fn a_cue_is_not_repeated() {
        assert_eq!(texts(&[" [Music]", " [Music]"]), ["[music]"]);
    }

    #[test]
    fn trailing_ellipses_are_removed() {
        assert_eq!(
            texts(&[" Wait...", " and", " then…", " ..."]),
            ["Wait", "and", "then"]
        );
    }

    #[test]
    fn times_follow_each_words_share_of_the_text() {
        let words = words_from_tokens(&pieces(&[" a", " bbb"]), 10.0, 16.0);
        assert_eq!((words[0].start, words[0].end), (10.0, 12.0));
        assert_eq!((words[1].start, words[1].end), (12.0, 16.0));
    }

    #[test]
    fn nothing_decoded_gives_no_words() {
        assert!(words_from_tokens(&[], 0.0, 1.0).is_empty());
    }
}
