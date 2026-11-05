//! The words on screen, on their own so the rules can be tested without a window.
//!
//! Confirmed words stay where they are and tentative ones follow them and may change. Words appear
//! one at a time at a pace set by the backlog, because a pass delivers several at once. Old
//! captions leave the overlay after a pause, but they are kept for reading back when it's expanded.

use soundveil_core::{CaptionUpdate, WordStyle, cue_label, reveal_interval, word_styles};

/// Beyond this, half of the oldest words go. Doing it in one step keeps the lines below from
/// rewrapping with every new word.
const KEEP: usize = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Plain,
    /// A sound word or slang.
    Accent,
    /// A sound Whisper noted, shown as a small label.
    Cue,
    /// Ends the line, so what follows a pause starts on a fresh one.
    LineBreak,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub text: String,
    pub look: Look,
    /// Tentative words are drawn fainter until two passes agree on them.
    pub settled: bool,
}

#[derive(Default)]
pub struct Feed {
    settled: Vec<Item>,
    pending: Vec<Item>,
    /// Every settled item so far, including those already trimmed away.
    total: usize,
    /// How many items have been shown, counted the same way.
    revealed: usize,
    idle: bool,
    pub expanded: bool,
    pub plain: bool,
}

fn item(raw: &str, style: WordStyle, settled: bool) -> Item {
    let (text, look) = match style {
        WordStyle::Cue => {
            let inner = raw.trim_matches(['[', ']', '(', ')']);
            let label = cue_label(raw).unwrap_or(inner);
            let text = if label == "music" {
                "♪ music".to_string()
            } else {
                label.to_string()
            };
            (text, Look::Cue)
        }
        WordStyle::Plain => (raw.to_string(), Look::Plain),
        WordStyle::Sound | WordStyle::Slang => (raw.to_string(), Look::Accent),
    };
    Item {
        text,
        look,
        settled,
    }
}

impl Feed {
    pub fn new(plain: bool) -> Self {
        Self {
            plain,
            ..Self::default()
        }
    }

    /// Clears the words, keeping how they're shown.
    pub fn clear(&mut self) {
        *self = Self {
            plain: self.plain,
            expanded: self.expanded,
            ..Self::default()
        };
    }

    pub fn apply(&mut self, update: &CaptionUpdate) {
        let shown = self.revealed;
        let tentative = update.tentative.as_deref();
        if !update.committed.is_empty() || tentative.is_some_and(|words| !words.is_empty()) {
            self.idle = false;
        }
        let texts: Vec<&str> = update.committed.iter().map(|w| w.text.as_str()).collect();
        for (text, style) in texts.iter().zip(word_styles(&texts)) {
            self.settled.push(item(text, style, true));
            self.total += 1;
        }
        if self.settled.len() > KEEP {
            self.settled.drain(..KEEP / 2);
        }
        if let Some(words) = tentative {
            let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
            self.pending = texts
                .iter()
                .zip(word_styles(&texts))
                .enumerate()
                .map(|(index, (text, style))| item(text, style, index < update.stable))
                .collect();
        }
        // A final pass rewrites words the viewer already saw, so it must not restart their reveal.
        let waiting = if update.committed.is_empty() {
            0
        } else {
            self.pending.len()
        };
        let end = self.total + self.pending.len();
        self.revealed = shown.max(self.total.saturating_sub(waiting)).min(end);
    }

    pub fn is_empty(&self) -> bool {
        self.settled.is_empty() && self.pending.is_empty()
    }

    /// Shows one more word, and says how long to wait before the next.
    pub fn reveal(&mut self) -> Option<f64> {
        let end = self.total + self.pending.len();
        if self.revealed >= end {
            return None;
        }
        let backlog = end - self.revealed;
        self.revealed += 1;
        Some(reveal_interval(backlog))
    }

    /// Clears the overlay after a pause. What comes next starts on a fresh line.
    pub fn go_idle(&mut self) {
        self.idle = true;
        let ended = self.settled.last().is_none_or(|last| last.look == Look::LineBreak);
        if self.pending.is_empty() && !ended {
            self.settled.push(Item {
                text: String::new(),
                look: Look::LineBreak,
                settled: true,
            });
            self.total += 1;
            self.revealed += 1;
        }
    }

    /// What should be on screen now.
    pub fn visible(&self) -> Vec<Item> {
        if self.idle && !self.expanded {
            return Vec::new();
        }
        let first = self.total - self.settled.len();
        let count = self.revealed.saturating_sub(first);
        self.settled
            .iter()
            .chain(&self.pending)
            .take(count)
            .map(|item| {
                let mut item = item.clone();
                if self.plain && item.look == Look::Accent {
                    item.look = Look::Plain;
                }
                item
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soundveil_core::Word;

    fn words(texts: &[&str]) -> Vec<Word> {
        texts.iter().map(|text| Word::new(*text, 0.0, 0.0)).collect()
    }

    fn update(committed: &[&str], tentative: Option<&[&str]>, stable: usize) -> CaptionUpdate {
        CaptionUpdate {
            committed: words(committed),
            tentative: tentative.map(words),
            stable,
            sentences: Vec::new(),
        }
    }

    fn reveal_all(feed: &mut Feed) {
        while feed.reveal().is_some() {}
    }

    fn texts(feed: &Feed) -> Vec<String> {
        feed.visible().into_iter().map(|item| item.text).collect()
    }

    #[test]
    fn tentative_words_appear_one_at_a_time() {
        let mut feed = Feed::default();
        feed.apply(&update(&[], Some(&["one", "two", "three"]), 0));
        assert!(feed.visible().is_empty());
        feed.reveal();
        assert_eq!(texts(&feed), ["one"]);
        feed.reveal();
        feed.reveal();
        assert_eq!(texts(&feed), ["one", "two", "three"]);
        assert_eq!(feed.reveal(), None);
    }

    #[test]
    fn a_bigger_backlog_reveals_faster() {
        let mut feed = Feed::default();
        feed.apply(&update(&[], Some(&["a"; 12]), 0));
        let first = feed.reveal().unwrap();
        while feed.reveal().is_some_and(|gap| gap < 0.09) {}
        assert!(first < 0.05, "{first}");
    }

    #[test]
    fn stable_words_are_drawn_settled() {
        let mut feed = Feed::default();
        feed.apply(&update(&[], Some(&["one", "two"]), 1));
        reveal_all(&mut feed);
        let settled: Vec<bool> = feed.visible().iter().map(|item| item.settled).collect();
        assert_eq!(settled, [true, false]);
    }

    #[test]
    fn a_final_pass_does_not_restart_the_reveal() {
        let mut feed = Feed::default();
        feed.apply(&update(&[], Some(&["one", "two", "three"]), 0));
        reveal_all(&mut feed);
        feed.apply(&update(&["one", "two", "three"], Some(&[]), 0));
        assert_eq!(texts(&feed), ["one", "two", "three"]);
        assert_eq!(feed.reveal(), None);
    }

    #[test]
    fn committed_words_follow_earlier_ones() {
        let mut feed = Feed::default();
        feed.apply(&update(&["one"], Some(&["two"]), 0));
        reveal_all(&mut feed);
        feed.apply(&update(&["two"], Some(&["three"]), 0));
        reveal_all(&mut feed);
        assert_eq!(texts(&feed), ["one", "two", "three"]);
    }

    #[test]
    fn sound_words_are_accented_unless_turned_off() {
        let mut feed = Feed::default();
        feed.apply(&update(&["Haha!"], None, 0));
        reveal_all(&mut feed);
        assert_eq!(feed.visible()[0].look, Look::Accent);
        feed.plain = true;
        assert_eq!(feed.visible()[0].look, Look::Plain);
    }

    #[test]
    fn cues_stay_labels_when_highlighting_is_off() {
        let mut feed = Feed::new(true);
        feed.apply(&update(&["[music]", "[laughter]"], None, 0));
        reveal_all(&mut feed);
        let shown = feed.visible();
        assert_eq!(shown[0].text, "♪ music");
        assert_eq!(shown[0].look, Look::Cue);
        assert_eq!(shown[1].text, "laughter");
    }

    #[test]
    fn idle_clears_the_overlay_but_expanding_brings_it_back() {
        let mut feed = Feed::default();
        feed.apply(&update(&["one"], Some(&[]), 0));
        reveal_all(&mut feed);
        feed.go_idle();
        assert!(feed.visible().is_empty());
        feed.expanded = true;
        assert_eq!(texts(&feed)[0], "one");
    }

    #[test]
    fn speech_after_a_pause_starts_a_fresh_line() {
        let mut feed = Feed::default();
        feed.apply(&update(&["one"], Some(&[]), 0));
        reveal_all(&mut feed);
        feed.go_idle();
        feed.go_idle();
        feed.apply(&update(&["two"], Some(&[]), 0));
        reveal_all(&mut feed);
        let looks: Vec<Look> = feed.visible().iter().map(|item| item.look).collect();
        assert_eq!(looks, [Look::Plain, Look::LineBreak, Look::Plain]);
    }

    #[test]
    fn long_sessions_stay_bounded_and_keep_the_newest_words() {
        let mut feed = Feed::default();
        for index in 0..5_000 {
            let word = format!("w{index}");
            feed.apply(&update(&[&word], None, 0));
            reveal_all(&mut feed);
        }
        let shown = texts(&feed);
        assert!(shown.len() <= KEEP);
        assert_eq!(shown.last().unwrap(), "w4999");
    }
}
