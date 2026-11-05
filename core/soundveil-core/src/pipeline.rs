//! The speech gate, words, and sentence grouping.

use crate::SAMPLE_RATE;

const RATE: f64 = SAMPLE_RATE as f64;
const FRAME: usize = 320; // 20 ms.
const PRE_ROLL: usize = 4800; // 300 ms.
const PAUSE: usize = 3200; // 200 ms.
const ENDPOINT: usize = 9600; // 600 ms.
const MIN_SPEECH: usize = 1600; // 100 ms.

#[derive(Clone, Debug, PartialEq)]
pub enum SpeechEvent {
    Audio {
        samples: Vec<f32>,
        start: f64,
    },
    /// A gap between phrases, and a safe place to cut. `voiced` is seconds of speech since the
    /// last pause.
    Pause {
        at: f64,
        voiced: f64,
    },
    /// Speech ended; finalize what has been heard.
    Endpoint {
        voiced: f64,
    },
    /// The burst was too short to be speech.
    Discard,
    /// Capture was interrupted; do not join audio across it.
    Gap,
}

/// Frame-sized voice detection. Keeps 300 ms of pre-roll and closes an utterance after 600 ms
/// of silence. Bursts under 100 ms are discarded. Pauses are measured against the speaker's own
/// recent level, so they are still found over a noise floor or music.
#[derive(Clone, Debug)]
pub struct SpeechGate {
    pub threshold: f32,
    pending: Vec<f32>,
    pre: Vec<f32>,
    clock: i64,
    silence: usize,
    voiced: usize,
    quiet: usize,
    phrase: usize,
    level: f32,
    active: bool,
}

impl Default for SpeechGate {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl SpeechGate {
    pub fn new(start: f64) -> Self {
        Self {
            threshold: 0.003,
            pending: Vec::new(),
            pre: Vec::new(),
            clock: (start * RATE) as i64,
            silence: 0,
            voiced: 0,
            quiet: 0,
            phrase: 0,
            level: 0.0,
            active: false,
        }
    }

    pub fn append(&mut self, samples: &[f32]) -> Vec<SpeechEvent> {
        self.pending.extend_from_slice(samples);
        let mut events = Vec::new();
        let mut heard: Vec<f32> = Vec::new();
        let mut heard_start = 0_i64;
        let mut offset = 0;
        while self.pending.len() - offset >= FRAME {
            let frame = &self.pending[offset..offset + FRAME];
            offset += FRAME;
            let rms = (frame.iter().fold(0.0_f32, |sum, s| sum + s * s) / FRAME as f32).sqrt();
            let speech = rms > self.threshold;
            if !self.active && speech {
                self.active = true;
                heard_start = self.clock - self.pre.len() as i64;
                heard = std::mem::take(&mut self.pre);
                self.silence = 0;
                self.voiced = 0;
                self.quiet = 0;
            }
            if self.active {
                if heard.is_empty() {
                    heard_start = self.clock;
                }
                heard.extend_from_slice(frame);
                if speech {
                    self.silence = 0;
                    self.voiced += FRAME;
                } else {
                    self.silence += FRAME;
                }
                if !speech || rms < self.level * 0.2 {
                    self.quiet += FRAME;
                } else {
                    self.quiet = 0;
                    self.phrase += FRAME;
                    self.level = if self.level == 0.0 {
                        rms
                    } else {
                        self.level * 0.98 + rms * 0.02
                    };
                }
                if self.quiet == PAUSE {
                    flush(&mut events, &mut heard, heard_start);
                    events.push(SpeechEvent::Pause {
                        at: (self.clock + FRAME as i64 - MIN_SPEECH as i64) as f64 / RATE,
                        voiced: self.phrase as f64 / RATE,
                    });
                    self.phrase = 0;
                }
                if self.silence >= ENDPOINT {
                    flush(&mut events, &mut heard, heard_start);
                    events.push(if self.voiced >= MIN_SPEECH {
                        SpeechEvent::Endpoint {
                            voiced: self.phrase as f64 / RATE,
                        }
                    } else {
                        SpeechEvent::Discard
                    });
                    self.active = false;
                    self.silence = 0;
                    self.voiced = 0;
                    self.quiet = 0;
                    self.phrase = 0;
                }
            } else {
                self.pre.extend_from_slice(frame);
                if self.pre.len() > PRE_ROLL {
                    self.pre.drain(..self.pre.len() - PRE_ROLL);
                }
            }
            self.clock += FRAME as i64;
        }
        flush(&mut events, &mut heard, heard_start);
        self.pending.drain(..offset);
        events
    }
}

fn flush(events: &mut Vec<SpeechEvent>, heard: &mut Vec<f32>, start: i64) {
    if !heard.is_empty() {
        events.push(SpeechEvent::Audio {
            samples: std::mem::take(heard),
            start: start as f64 / RATE,
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

impl Word {
    pub fn new(text: impl Into<String>, start: f64, end: f64) -> Self {
        Self {
            text: text.into(),
            start,
            end,
        }
    }

    pub(crate) fn key(&self) -> String {
        word_key(&self.text)
    }

    fn ends_sentence(&self) -> bool {
        self.text.ends_with(['.', '?', '!'])
    }
}

/// Words compare by letters and digits alone, so "Hello," and "hello" are the same word.
pub(crate) fn word_key(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_numeric())
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Caption {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Groups confirmed words into history lines at sentence ends, or after 24 words without one.
#[derive(Clone, Debug, Default)]
pub(crate) struct SentenceAssembler {
    words: Vec<Word>,
}

impl SentenceAssembler {
    pub(crate) fn add(&mut self, new: &[Word]) -> Vec<Caption> {
        let mut result = Vec::new();
        for word in new {
            self.words.push(word.clone());
            if word.ends_sentence() || self.words.len() >= 24 {
                result.extend(self.take());
            }
        }
        result
    }

    pub(crate) fn take(&mut self) -> Option<Caption> {
        let first = self.words.first()?;
        let last = self.words.last()?;
        let caption = Caption {
            start: first.start,
            end: last.end,
            text: self
                .words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        };
        self.words.clear();
        Some(caption)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech(count: usize) -> Vec<f32> {
        vec![0.1; count]
    }
    fn silence(count: usize) -> Vec<f32> {
        vec![0.0; count]
    }
    fn heard(events: &[SpeechEvent]) -> (usize, Vec<f64>, usize) {
        let (mut samples, mut starts, mut endpoints) = (0, Vec::new(), 0);
        for event in events {
            match event {
                SpeechEvent::Audio { samples: s, start } => {
                    samples += s.len();
                    starts.push(*start);
                }
                SpeechEvent::Endpoint { .. } => endpoints += 1,
                _ => {}
            }
        }
        (samples, starts, endpoints)
    }
    fn pauses(events: &[SpeechEvent]) -> Vec<f64> {
        events
            .iter()
            .filter_map(|event| match event {
                SpeechEvent::Pause { at, .. } => Some(*at),
                _ => None,
            })
            .collect()
    }
    fn close(a: f64, b: f64, accuracy: f64) -> bool {
        (a - b).abs() <= accuracy
    }

    #[test]
    fn silence_emits_nothing() {
        assert!(SpeechGate::default().append(&silence(320_000)).is_empty());
    }

    #[test]
    fn speech_includes_pre_roll() {
        let mut gate = SpeechGate::default();
        gate.append(&silence(16_000));
        let (samples, starts, _) = heard(&gate.append(&speech(16_000)));
        assert!(close(starts[0], 0.7, 0.001));
        assert_eq!(samples, 20_800);
    }

    #[test]
    fn silence_closes_utterance() {
        let mut gate = SpeechGate::default();
        gate.append(&speech(16_000));
        assert_eq!(heard(&gate.append(&silence(9600))).2, 1);
    }

    #[test]
    fn short_pause_is_reported() {
        let mut gate = SpeechGate::default();
        gate.append(&speech(16_000));
        let found = pauses(&gate.append(&silence(4000)));
        assert_eq!(found.len(), 1);
        assert!(close(found[0], 1.1, 0.03));
    }

    #[test]
    fn brief_dip_is_not_a_pause() {
        let mut gate = SpeechGate::default();
        gate.append(&speech(16_000));
        let dip = [silence(1600), speech(1600)].concat();
        assert!(pauses(&gate.append(&dip)).is_empty());
    }

    #[test]
    fn pause_found_over_a_noise_floor() {
        let mut gate = SpeechGate::default();
        gate.append(&speech(32_000));
        assert_eq!(pauses(&gate.append(&vec![0.01; 4000])).len(), 1);
    }

    #[test]
    fn short_noise_is_discarded() {
        let mut gate = SpeechGate::default();
        gate.append(&speech(640));
        let events = gate.append(&silence(16_000));
        assert_eq!(heard(&events).2, 0);
        assert!(events.contains(&SpeechEvent::Discard));
    }

    #[test]
    fn splitting_the_audio_changes_nothing() {
        let samples = [speech(32_000), silence(12_800)].concat();
        let expected = heard(&SpeechGate::default().append(&samples));
        let mut gate = SpeechGate::default();
        let mut events = Vec::new();
        for chunk in samples.chunks(137) {
            events.extend(gate.append(chunk));
        }
        assert_eq!(heard(&events).0, expected.0);
        assert_eq!(heard(&events).2, 1);
    }

    #[test]
    fn timestamps_continue_after_a_gap() {
        let mut gate = SpeechGate::new(12.5);
        assert!(close(
            heard(&gate.append(&speech(16_000))).1[0],
            12.5,
            0.001
        ));
    }

    #[test]
    fn sentences_group_into_history_lines() {
        let mut assembler = SentenceAssembler::default();
        let lines = assembler.add(&[
            Word::new("Hello", 0.0, 1.0),
            Word::new("there.", 1.0, 2.0),
            Word::new("Next", 2.0, 3.0),
        ]);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello there.");
        assert_eq!(assembler.take().unwrap().text, "Next");
    }

    #[test]
    fn long_unpunctuated_run_still_reaches_history() {
        let words: Vec<Word> = (0..24)
            .map(|i| Word::new(format!("w{i}"), i as f64, i as f64 + 1.0))
            .collect();
        assert_eq!(SentenceAssembler::default().add(&words).len(), 1);
    }
}
