//! Decides which words stand out, on the principle that when everything stands out nothing does.
//! The lists are short, and everyday fillers ("um", "oh", "yeah") are left plain on purpose.

use crate::pipeline::word_key;
use regex_lite::Regex;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordStyle {
    Plain,
    /// Onomatopoeia and interjections: "haha", "whoa", "kaboom".
    Sound,
    /// Internet slang and memes: "bruh", "no cap".
    Slang,
    /// A non-speech sound Whisper reported: "[laughter]".
    Cue,
}

/// Maps Whisper's non-speech notes to a short label. Anything else, such as `[BLANK_AUDIO]` or
/// `(speaking foreign language)`, is dropped, because those notes are mostly noise.
pub fn cue_label(note: &str) -> Option<&'static str> {
    let text: String = note
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic() || matches!(c, ' ' | '♪' | '♫'))
        .collect();
    let has = |parts: &[&str]| parts.iter().any(|part| text.contains(part));
    if has(&["♪", "♫", "music", "singing"]) {
        Some("music")
    } else if has(&["laugh", "chuckl", "giggl"]) {
        Some("laughter")
    } else if has(&["applau", "clapping"]) {
        Some("applause")
    } else if has(&["cheer"]) {
        Some("cheering")
    } else {
        None
    }
}

const SOUNDS: &[&str] = &[
    "aha",
    "ahem",
    "argh",
    "aww",
    "bam",
    "beep",
    "blah",
    "boing",
    "boop",
    "brr",
    "chaching",
    "clang",
    "clank",
    "cowabunga",
    "eek",
    "eureka",
    "eww",
    "gah",
    "grr",
    "haha",
    "hehe",
    "hooray",
    "kaboom",
    "kablam",
    "kachow",
    "kapow",
    "kerplunk",
    "meow",
    "mwahaha",
    "oink",
    "oof",
    "ooh",
    "oops",
    "ouch",
    "ow",
    "phew",
    "pow",
    "psst",
    "ribbit",
    "shh",
    "splat",
    "swoosh",
    "tada",
    "thud",
    "uhoh",
    "ugh",
    "vroom",
    "wham",
    "whee",
    "whoa",
    "whoops",
    "whoosh",
    "woah",
    "woof",
    "woohoo",
    "wow",
    "yay",
    "yikes",
    "yippee",
    "zing",
];
const SLANG: &[&str] = &[
    "bruh", "bussin", "delulu", "goated", "gyatt", "iykyk", "lmao", "lol", "omg", "pog", "poggers",
    "rizz", "rizzler", "rofl", "sheesh", "skibidi", "smh", "sus", "wtf", "yeet",
];
// Everyday words that only count when shouted: "Boom!" but not "the baby boom".
const SHOUTED_SOUNDS: &[&str] = &[
    "bang", "boom", "buzz", "clap", "crash", "crack", "ding", "hiss", "pop", "roar", "slam",
    "smack", "snap", "splash", "thump", "whack", "zap",
];
const SHOUTED_SLANG: &[&str] = &[
    "based", "bet", "bro", "cap", "cringe", "facts", "fire", "lit", "mid", "periodt", "slay",
    "sigma",
];
const SLANG_PHRASES: &[&[&str]] = &[
    &["big", "mood"],
    &["its", "giving"],
    &["lets", "go"],
    &["main", "character"],
    &["mic", "drop"],
    &["no", "cap"],
    &["plot", "twist"],
    &["say", "less"],
    &["skill", "issue"],
    &["touch", "grass"],
];
const SHOUTED_PHRASES: &[&[&str]] = &[&["no", "way"], &["oh", "my", "god"], &["oh", "no"]];
const SOUND_PHRASES: &[&[&str]] = &[
    &["blah", "blah"],
    &["ding", "dong"],
    &["la", "la"],
    &["na", "na"],
    &["tick", "tock"],
    &["yada", "yada"],
];

fn is_laugh(key: &str) -> bool {
    static LAUGH: LazyLock<Regex> =
        LazyLock::new(|| Regex::new("^(a*h+a+|h+e+|h+i+){2,}h*$").expect("valid pattern"));
    LAUGH.is_match(key)
}

/// "Sooooo" and "nooo" are spoken for effect. Numbers and short words are not.
fn is_stretched(key: &str) -> bool {
    let letters: Vec<char> = key.chars().collect();
    letters.len() >= 4
        && letters.iter().all(|c| c.is_alphabetic())
        && letters.windows(3).any(|w| w[0] == w[1] && w[1] == w[2])
}

fn closes_sentence(word: &str) -> bool {
    word.ends_with(['.', '?', '!'])
}

pub fn word_styles<S: AsRef<str>>(words: &[S]) -> Vec<WordStyle> {
    let keys: Vec<String> = words.iter().map(|word| word_key(word.as_ref())).collect();
    let mut styles = vec![WordStyle::Plain; words.len()];
    // A word is shouted if it has an exclamation mark or stands alone as a short sentence.
    let shouted = |i: usize| {
        let word = words[i].as_ref();
        word.ends_with('!')
            || ((i == 0 || closes_sentence(words[i - 1].as_ref())) && closes_sentence(word))
    };
    for (i, key) in keys.iter().enumerate() {
        if words[i].as_ref().starts_with('[') {
            styles[i] = WordStyle::Cue;
        } else if key.is_empty() {
            continue;
        } else if SOUNDS.contains(&key.as_str()) || is_laugh(key) || is_stretched(key) {
            styles[i] = WordStyle::Sound;
        } else if SLANG.contains(&key.as_str()) {
            styles[i] = WordStyle::Slang;
        } else if shouted(i) {
            if SHOUTED_SOUNDS.contains(&key.as_str()) {
                styles[i] = WordStyle::Sound;
            } else if SHOUTED_SLANG.contains(&key.as_str()) {
                styles[i] = WordStyle::Slang;
            }
        }
    }
    let mut mark = |phrases: &[&[&str]], style: WordStyle, shouted_only: bool| {
        for phrase in phrases {
            if phrase.len() > keys.len() {
                continue;
            }
            for start in 0..=keys.len() - phrase.len() {
                let found = keys[start..]
                    .iter()
                    .zip(*phrase)
                    .all(|(key, part)| key == part);
                if found && (!shouted_only || shouted(start + phrase.len() - 1)) {
                    styles[start..start + phrase.len()].fill(style);
                }
            }
        }
    };
    mark(SOUND_PHRASES, WordStyle::Sound, false);
    mark(SLANG_PHRASES, WordStyle::Slang, false);
    mark(SHOUTED_PHRASES, WordStyle::Sound, true);
    // "ha ha ha": a laugh spoken as separate words.
    for i in 0..keys.len() {
        if ["ha", "he", "ho", "hah"].contains(&keys[i].as_str())
            && ((i > 0 && keys[i - 1] == keys[i]) || (i + 1 < keys.len() && keys[i + 1] == keys[i]))
        {
            styles[i] = WordStyle::Sound;
        }
    }
    styles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(text: &str) -> Vec<&str> {
        let words: Vec<&str> = text.split(' ').collect();
        words
            .iter()
            .zip(word_styles(&words))
            .filter(|(_, style)| *style != WordStyle::Plain)
            .map(|(word, _)| *word)
            .collect()
    }
    const NONE: [&str; 0] = [];

    #[test]
    fn sound_words_stand_out() {
        assert_eq!(styled("Haha that was great"), ["Haha"]);
        assert_eq!(
            styled("and then whoa it just went kaboom, right"),
            ["whoa", "kaboom,"]
        );
        assert_eq!(
            word_styles(&["Ugh,", "fine."]),
            [WordStyle::Sound, WordStyle::Plain]
        );
    }

    #[test]
    fn spoken_laughs() {
        assert_eq!(styled("He said ha ha ha and left"), ["ha", "ha", "ha"]);
        assert_eq!(styled("Hahahaha stop"), ["Hahahaha"]);
        assert_eq!(styled("Hehehe okay"), ["Hehehe"]);
        assert_eq!(styled("Ha, I see"), NONE);
    }

    #[test]
    fn stretched_words() {
        assert_eq!(styled("It was sooooo good"), ["sooooo"]);
        assert_eq!(styled("Nooo don't"), ["Nooo"]);
        assert_eq!(styled("A 1000 people"), NONE);
    }

    #[test]
    fn slang_and_memes() {
        assert_eq!(word_styles(&["bruh"]), [WordStyle::Slang]);
        assert_eq!(styled("That is so sus honestly"), ["sus"]);
        assert_eq!(styled("No cap, that was good"), ["No", "cap,"]);
        assert_eq!(styled("OK let's go everyone"), ["let's", "go"]);
    }

    #[test]
    fn everyday_words_stay_plain_unless_shouted() {
        assert_eq!(styled("The baby boom was based on data"), NONE);
        assert_eq!(styled("And then boom! It fell"), ["boom!"]);
        assert_eq!(styled("It was over. Boom. Gone"), ["Boom."]);
        assert_eq!(styled("Nice shot. Based."), ["Based."]);
        assert_eq!(styled("There is no way to know"), NONE);
        assert_eq!(styled("No way!"), ["No", "way!"]);
    }

    #[test]
    fn fillers_stay_plain() {
        assert_eq!(styled("Um, oh, yeah, uh, okay, hmm, well"), NONE);
    }

    #[test]
    fn ordinary_dialogue_is_untouched() {
        assert_eq!(
            styled(
                "I dropped out of Reed College after the first six months, but then stayed around."
            ),
            NONE
        );
    }

    #[test]
    fn cues() {
        assert_eq!(
            word_styles(&["[laughter]", "Right"]),
            [WordStyle::Cue, WordStyle::Plain]
        );
        assert_eq!(cue_label("[MUSIC PLAYING]"), Some("music"));
        assert_eq!(cue_label("(audience laughing)"), Some("laughter"));
        assert_eq!(cue_label("♪♪"), Some("music"));
        assert_eq!(cue_label("[Applause]"), Some("applause"));
        assert_eq!(cue_label("[BLANK_AUDIO]"), None);
        assert_eq!(cue_label("(speaking foreign language)"), None);
    }
}
