//! The rolling transcriber, on-screen agreement, word placement, and pacing.

use crate::SAMPLE_RATE;
use crate::pipeline::{Caption, SentenceAssembler, SpeechEvent, Word};
use std::collections::VecDeque;

const RATE: f64 = SAMPLE_RATE as f64;

#[derive(Clone, Debug, PartialEq)]
pub struct DecodeJob {
    pub samples: Vec<f32>,
    pub start: f64,
    pub audio_context: usize,
    pub max_tokens: usize,
    pub beam: i32,
    pub is_final: bool,
}

impl DecodeJob {
    pub fn end(&self) -> f64 {
        self.start + self.samples.len() as f64 / RATE
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptionUpdate {
    /// Final text. Replaces the tentative words it covers.
    pub committed: Vec<Word>,
    /// The phrase in progress; `None` means unchanged.
    pub tentative: Option<Vec<Word>>,
    /// Leading tentative words that two passes agreed on.
    pub stable: usize,
    pub sentences: Vec<Caption>,
}

/// Fast English speech is under 5 tokens a second, so a longer result is Whisper looping.
fn token_limit(seconds: f64) -> usize {
    (seconds * 6.0) as usize + 12
}

/// Cuts a hypothesis where a phrase of up to 4 words repeats three times running. Whisper loops
/// like this at the edge of the audio.
fn removing_loops(words: Vec<Word>) -> Vec<Word> {
    let keys: Vec<String> = words.iter().map(Word::key).collect();
    for i in 0..keys.len() {
        for n in 1..=4 {
            if i + 3 * n <= keys.len()
                && keys[i..i + n] == keys[i + n..i + 2 * n]
                && keys[i..i + n] == keys[i + 2 * n..i + 3 * n]
            {
                let mut kept = words;
                kept.truncate(i + n);
                return kept;
            }
        }
    }
    words
}

fn common_prefix(a: &[Word], b: &[Word]) -> usize {
    a.iter()
        .zip(b)
        .take_while(|(x, y)| x.key() == y.key())
        .count()
}

/// Finds the quietest 100 ms of a recording, the safest place to cut a run with no pause.
fn quietest_point(samples: &[f32]) -> usize {
    let (mut best, mut lowest) = (0, f32::INFINITY);
    for start in (0..samples.len().saturating_sub(1600).max(1)).step_by(160) {
        let end = samples.len().min(start + 1600);
        let energy = (start..end)
            .step_by(4)
            .fold(0.0_f32, |sum, i| sum + samples[i] * samples[i]);
        if energy < lowest {
            lowest = energy;
            best = start;
        }
    }
    best + 800
}

/// Each pass keeps the GPU busy, so this trades delay against power. Measured on an M4 Pro,
/// updating every 0.4 s rather than 0.2 s took the GPU from 77% to 46% busy during speech and
/// cut on-screen corrections sixfold, for 0.17 s more delay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpdateSpeed {
    #[default]
    Auto,
    Fastest,
    Balanced,
    Efficient,
    Saver,
}

impl UpdateSpeed {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Fastest,
        Self::Balanced,
        Self::Efficient,
        Self::Saver,
    ];

    pub fn raw_value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Fastest => "fastest",
            Self::Balanced => "balanced",
            Self::Efficient => "efficient",
            Self::Saver => "saver",
        }
    }

    pub fn saved(value: Option<&str>) -> Self {
        Self::ALL
            .into_iter()
            .find(|speed| Some(speed.raw_value()) == value)
            .unwrap_or_default()
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Fastest => "Fastest · every 0.2 s",
            Self::Balanced => "Balanced · every 0.4 s",
            Self::Efficient => "Efficient · every 0.75 s",
            Self::Saver => "Battery saver · every 1.5 s",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::Auto => {
                "Keeps the GPU under about half load on this computer, and slows down when saving power or running hot."
            }
            Self::Fastest => {
                "Lowest delay. Uses the most GPU and battery, and text changes more as it settles."
            }
            Self::Balanced => "A good default on most computers.",
            Self::Efficient => "Much less GPU use, with words arriving a little later.",
            Self::Saver => "Lowest GPU use. Words arrive in small groups.",
        }
    }

    fn seconds(self) -> Option<f64> {
        match self {
            Self::Auto => None,
            Self::Fastest => Some(0.2),
            Self::Balanced => Some(0.4),
            Self::Efficient => Some(0.75),
            Self::Saver => Some(1.5),
        }
    }
}

/// A fixed choice is always honored. Automatic rests as long as each pass took, so the GPU is
/// busy about half the time at most, and doubles that when saving power or under heat.
pub fn update_interval(speed: UpdateSpeed, pass_time: f64, constrained: bool) -> f64 {
    if let Some(seconds) = speed.seconds() {
        return seconds;
    }
    let paced = (pass_time * 2.0).max(0.4);
    if constrained { paced * 2.0 } else { paced }.min(3.0)
}

#[derive(Clone, Debug)]
struct Phrase {
    samples: Vec<f32>,
    start: f64,
}

/// Whisper has no native streaming, and its word timestamps are too unreliable to cut audio on.
/// So partial passes over the current phrase are for display only, and the transcript comes from
/// one final pass over each phrase, cut at a pause. This is a pure state machine, so the policy
/// is testable without audio or a model. The caller ingests events from capture and pulls one
/// job at a time for inference, and must serialize the two.
#[derive(Clone, Debug)]
pub struct RollingTranscriber {
    /// Seconds of speech before the first pass.
    pub first_decode: f64,
    /// New audio required between passes. See [`UpdateSpeed`].
    pub step: f64,
    /// Shorter phrases carry on through a pause.
    pub min_phrase: f64,
    /// Longer phrases are cut at their quietest point.
    pub max_phrase: f64,
    /// Seconds kept. Past this, inference has stalled and audio is dropped.
    pub capacity: f64,
    /// Encoder frames, at 50 a second. Cost scales with this, so it is far below Whisper's 1500,
    /// but it must outrun the longest phrase or words are lost. It must also be the same for
    /// every pass: mixing sizes in one Whisper context corrupted the passes that followed.
    pub audio_context: usize,
    /// Words shown beyond what two passes agreed on.
    pub tail_words: usize,
    /// Phrases with less speech are not decoded; Whisper invents text from silence.
    pub min_voiced: f64,
    pub partial_beam: i32,
    pub final_beam: i32,
    skipped: f64,
    buffer: Vec<f32>,
    buffer_start: f64,
    decoded_end: f64,
    closing: VecDeque<Phrase>,
    busy: bool,
    previous: Vec<Word>,
    shown: Vec<Word>,
    voiced: f64,
    empty_passes: u32,
    assembler: SentenceAssembler,
}

impl Default for RollingTranscriber {
    fn default() -> Self {
        Self {
            first_decode: 0.8,
            step: 0.4,
            min_phrase: 2.5,
            max_phrase: 9.0,
            capacity: 24.0,
            audio_context: 640,
            tail_words: 1,
            min_voiced: 0.25,
            partial_beam: 1,
            final_beam: 5,
            skipped: 0.0,
            buffer: Vec::new(),
            buffer_start: 0.0,
            decoded_end: 0.0,
            closing: VecDeque::new(),
            busy: false,
            previous: Vec::new(),
            shown: Vec::new(),
            voiced: 0.0,
            empty_passes: 0,
            assembler: SentenceAssembler::default(),
        }
    }
}

impl RollingTranscriber {
    /// Seconds of audio lost because inference stalled.
    pub fn skipped(&self) -> f64 {
        self.skipped
    }

    pub fn audio_end(&self) -> f64 {
        self.buffer_start + self.length()
    }

    fn length(&self) -> f64 {
        self.buffer.len() as f64 / RATE
    }

    pub fn ingest(&mut self, event: SpeechEvent) {
        match event {
            SpeechEvent::Audio { samples, start } => {
                if self.buffer.is_empty() {
                    self.buffer_start = start;
                    self.decoded_end = start;
                }
                self.buffer.extend(samples);
                if self.length() >= self.max_phrase {
                    let tail = self.buffer.len().saturating_sub(64_000);
                    let end = tail.max(self.buffer.len().saturating_sub(4800));
                    self.close(tail + quietest_point(&self.buffer[tail..end]), true);
                }
            }
            SpeechEvent::Pause { at, voiced } => {
                self.voiced += voiced;
                if self.length() >= self.min_phrase && at > self.buffer_start {
                    let count = ((at - self.buffer_start) * RATE) as usize;
                    self.close(count.min(self.buffer.len()), false);
                }
            }
            SpeechEvent::Endpoint { voiced } => {
                self.voiced += voiced;
                self.close(self.buffer.len(), false);
            }
            SpeechEvent::Gap => self.close(self.buffer.len(), true),
            SpeechEvent::Discard => {
                self.buffer.clear();
                self.voiced = 0.0;
                self.closing.push_back(Phrase {
                    samples: Vec::new(),
                    start: self.buffer_start,
                });
            }
        }
    }

    fn close(&mut self, count: usize, force: bool) {
        if count == 0 {
            return;
        }
        // A phrase that is not speech closes empty, which still clears anything shown for it.
        let keep = self.voiced >= self.min_voiced || force;
        self.closing.push_back(Phrase {
            samples: if keep {
                self.buffer[..count].to_vec()
            } else {
                Vec::new()
            },
            start: self.buffer_start,
        });
        self.voiced = 0.0;
        self.buffer.drain(..count);
        self.buffer_start += count as f64 / RATE;
        self.decoded_end = self.buffer_start;
        let held = self
            .closing
            .iter()
            .fold(0.0, |sum, phrase| sum + phrase.samples.len() as f64 / RATE);
        if self.closing.len() > 1
            && held > self.capacity
            && let Some(dropped) = self.closing.pop_front()
        {
            self.skipped += dropped.samples.len() as f64 / RATE;
        }
    }

    /// Phrases are finalized in order and first, so a slow machine falls behind on the words
    /// still being refined rather than on the transcript.
    pub fn next_job(&mut self) -> Option<DecodeJob> {
        if self.busy {
            return None;
        }
        if let Some(phrase) = self.closing.front() {
            let seconds = phrase.samples.len() as f64 / RATE;
            let job = DecodeJob {
                samples: phrase.samples.clone(),
                start: phrase.start,
                audio_context: self.audio_context,
                max_tokens: token_limit(seconds),
                beam: self.final_beam,
                is_final: true,
            };
            self.busy = true;
            return Some(job);
        }
        // Music or noise passes the gate but gives no words, so each empty pass waits longer.
        let backoff = self.step * 2_f64.powi(self.empty_passes.min(4) as i32);
        let wait = self.step.max(2.0).min(backoff);
        if self.length() < self.first_decode || self.audio_end() - self.decoded_end < wait {
            return None;
        }
        self.busy = true;
        self.decoded_end = self.audio_end();
        Some(DecodeJob {
            samples: self.buffer.clone(),
            start: self.buffer_start,
            audio_context: self.audio_context,
            max_tokens: token_limit(self.length()),
            beam: self.partial_beam,
            is_final: false,
        })
    }

    pub fn complete(&mut self, job: &DecodeJob, words: Vec<Word>) -> CaptionUpdate {
        self.busy = false;
        let mut update = CaptionUpdate::default();
        let pass = removing_loops(words);
        if job.is_final {
            self.closing.pop_front();
            self.previous.clear();
            self.shown.clear();
            self.empty_passes = 0;
            update.sentences = self.assembler.add(&pass);
            update.committed = pass;
            update.tentative = Some(Vec::new());
        } else if self.closing.is_empty() && !self.buffer.is_empty() {
            // Whisper's guess at a cut-off phrase can swing wildly from one pass to the next, so
            // what is on screen only changes when two passes in a row agree. A lone outlier is
            // ignored. New words at the end are the exception: one is shown ahead of agreement,
            // to keep up.
            self.empty_passes = if pass.is_empty() {
                self.empty_passes + 1
            } else {
                0
            };
            let agreed = common_prefix(&self.previous, &pass);
            let kept = common_prefix(&self.shown, &pass);
            if kept == self.shown.len() {
                self.shown = prefix(&pass, kept.max(agreed + self.tail_words));
            } else if agreed > kept {
                self.shown = prefix(&pass, agreed + self.tail_words);
            }
            update.tentative = Some(self.shown.clone());
            update.stable = agreed.min(self.shown.len());
            self.previous = pass;
        }
        update
    }

    /// Called on Stop: nothing more will be decoded, so keep what was heard.
    pub fn finish(&mut self) -> CaptionUpdate {
        let mut update = CaptionUpdate {
            committed: if self.previous.len() > self.shown.len() {
                self.previous.clone()
            } else {
                self.shown.clone()
            },
            tentative: Some(Vec::new()),
            ..CaptionUpdate::default()
        };
        update.sentences = self.assembler.add(&update.committed);
        update.sentences.extend(self.assembler.take());
        update
    }
}

fn prefix(words: &[Word], count: usize) -> Vec<Word> {
    words[..count.min(words.len())].to_vec()
}

/// One word's place in the overlay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedWord {
    pub line: usize,
    pub x: f64,
}

/// Greedy wrapping. Each word's position depends only on the words before it, so appended words
/// never move text that is already on screen.
#[derive(Clone, Copy, Debug)]
pub struct WordPlacer {
    pub line: usize,
    pub x: f64,
    pub space: f64,
    pub max_width: f64,
}

impl WordPlacer {
    pub fn new(space: f64, max_width: f64) -> Self {
        Self {
            line: 0,
            x: 0.0,
            space,
            max_width,
        }
    }

    pub fn place(&mut self, width: f64) -> PlacedWord {
        if self.x > 0.0 && self.x + width > self.max_width {
            self.line += 1;
            self.x = 0.0;
        }
        let placed = PlacedWord {
            line: self.line,
            x: self.x,
        };
        self.x += width + self.space;
        placed
    }
}

/// Staggers a batch of words so it reads as a flow. A batch always drains within about 0.3 s, the
/// time between passes, so pacing cannot build up a backlog of delay.
pub fn reveal_interval(backlog: usize) -> f64 {
    (0.28 / backlog.max(1) as f64).max(0.02).min(0.09)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast() -> RollingTranscriber {
        RollingTranscriber {
            step: 0.2,
            ..RollingTranscriber::default()
        }
    }
    fn words_from(text: &str, start: f64) -> Vec<Word> {
        text.split(' ')
            .enumerate()
            .map(|(i, word)| {
                let i = i as f64;
                Word::new(word, start + i * 0.4, start + (i + 1.0) * 0.4)
            })
            .collect()
    }
    fn words(text: &str) -> Vec<Word> {
        words_from(text, 0.0)
    }
    fn texts(words: &[Word]) -> String {
        words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn tentative(update: &CaptionUpdate) -> String {
        texts(update.tentative.as_deref().unwrap_or_default())
    }
    fn audio_level(seconds: f64, start: f64, level: f32) -> SpeechEvent {
        SpeechEvent::Audio {
            samples: vec![level; (seconds * 16_000.0) as usize],
            start,
        }
    }
    fn audio(seconds: f64, start: f64) -> SpeechEvent {
        audio_level(seconds, start, 0.1)
    }
    fn pause(at: f64, voiced: f64) -> SpeechEvent {
        SpeechEvent::Pause { at, voiced }
    }
    fn endpoint(voiced: f64) -> SpeechEvent {
        SpeechEvent::Endpoint { voiced }
    }
    fn pass(t: &mut RollingTranscriber, text: &str, time: &mut f64) -> CaptionUpdate {
        let job = t.next_job().unwrap();
        let update = t.complete(&job, words(text));
        t.ingest(audio(0.25, *time));
        *time += 0.25;
        update
    }
    fn near(a: f64, b: f64, accuracy: f64) -> bool {
        (a - b).abs() <= accuracy
    }

    #[test]
    fn audio_context_outruns_the_longest_phrase() {
        let t = fast();
        assert!(t.audio_context as f64 > t.max_phrase * 50.0 + 100.0);
    }

    #[test]
    fn every_pass_uses_the_same_audio_context() {
        let mut t = fast();
        t.ingest(audio(3.0, 0.0));
        let partial = t.next_job().unwrap();
        t.complete(&partial, vec![]);
        t.ingest(pause(2.9, 2.5));
        assert_eq!(t.next_job().unwrap().audio_context, partial.audio_context);
    }

    #[test]
    fn token_limit_allows_fast_speech() {
        assert!(token_limit(10.0) > 60);
    }

    #[test]
    fn loops_are_cut() {
        assert_eq!(
            texts(&removing_loops(words("what your your your your"))),
            "what your"
        );
        assert_eq!(
            texts(&removing_loops(words("so you are, you are, you are"))),
            "so you are,"
        );
        assert_eq!(
            texts(&removing_loops(words("no no we agree"))),
            "no no we agree"
        );
    }

    #[test]
    fn quietest_point_finds_the_dip() {
        let mut samples = vec![0.2_f32; 16_000];
        samples[9000..10_000].fill(0.0);
        assert!(near(quietest_point(&samples) as f64, 9500.0, 300.0));
    }

    #[test]
    fn no_pass_before_first_decode() {
        let mut t = fast();
        t.ingest(audio(0.5, 0.0));
        assert!(t.next_job().is_none());
        t.ingest(audio(0.5, 0.5));
        assert!(t.next_job().is_some());
    }

    #[test]
    fn partials_use_greedy_and_finals_use_beam() {
        let mut t = fast();
        t.ingest(audio(3.0, 0.0));
        let partial = t.next_job().unwrap();
        assert_eq!(partial.beam, 1);
        t.complete(&partial, vec![]);
        t.ingest(pause(2.9, 2.5));
        let last = t.next_job().unwrap();
        assert!(last.is_final);
        assert_eq!(last.beam, 5);
    }

    #[test]
    fn only_one_pass_runs_at_a_time() {
        let mut t = fast();
        t.ingest(audio(2.0, 0.0));
        let job = t.next_job().unwrap();
        assert!(t.next_job().is_none());
        t.ingest(audio(0.5, 2.0));
        assert!(t.next_job().is_none());
        t.complete(&job, vec![]);
        assert!(t.next_job().is_some());
    }

    #[test]
    fn no_pass_without_enough_new_audio() {
        let mut t = fast();
        t.ingest(audio(2.0, 0.0));
        let job = t.next_job().unwrap();
        t.complete(&job, words("One"));
        assert!(t.next_job().is_none());
        t.ingest(audio(0.25, 2.0));
        assert!(t.next_job().is_some());
    }

    #[test]
    fn only_one_word_is_shown_ahead_of_agreement() {
        let (mut t, mut time) = (fast(), 2.0);
        t.ingest(audio(2.0, 0.0));
        assert_eq!(tentative(&pass(&mut t, "one two three", &mut time)), "one");
        let update = pass(&mut t, "one two three four", &mut time);
        assert_eq!(tentative(&update), "one two three four");
        assert_eq!(update.stable, 3);
    }

    #[test]
    fn one_outlier_pass_cannot_change_the_screen() {
        let (mut t, mut time) = (fast(), 2.0);
        t.ingest(audio(2.0, 0.0));
        pass(&mut t, "ask what you can", &mut time);
        pass(&mut t, "ask what you can", &mut time);
        assert_eq!(
            tentative(&pass(&mut t, "to the to", &mut time)),
            "ask what you can"
        );
    }

    #[test]
    fn two_passes_can_change_the_screen() {
        let (mut t, mut time) = (fast(), 2.0);
        t.ingest(audio(2.0, 0.0));
        pass(&mut t, "one two three", &mut time);
        pass(&mut t, "one two three", &mut time);
        pass(&mut t, "one to three four", &mut time);
        assert_eq!(
            tentative(&pass(&mut t, "one to three four", &mut time)),
            "one to three four"
        );
    }

    #[test]
    fn words_are_not_taken_back_when_a_pass_is_shorter() {
        let (mut t, mut time) = (fast(), 2.0);
        t.ingest(audio(2.0, 0.0));
        pass(&mut t, "one two three", &mut time);
        pass(&mut t, "one two three", &mut time);
        assert_eq!(tentative(&pass(&mut t, "one", &mut time)), "one two three");
    }

    #[test]
    fn pause_cuts_the_phrase_exactly_there() {
        let mut t = fast();
        t.ingest(audio(3.0, 0.0));
        t.ingest(pause(2.5, 2.4));
        let job = t.next_job().unwrap();
        assert!(job.is_final);
        assert_eq!(job.samples.len(), 40_000);
        assert!(near(t.audio_end(), 3.0, 0.001));
    }

    #[test]
    fn short_phrase_carries_on_through_a_pause() {
        let mut t = fast();
        t.ingest(audio(1.5, 0.0));
        t.ingest(pause(1.4, 1.2));
        assert!(!t.next_job().unwrap().is_final);
    }

    #[test]
    fn final_pass_commits_and_replaces_tentative() {
        let mut t = fast();
        t.ingest(audio(3.0, 0.0));
        let job = t.next_job().unwrap();
        t.complete(&job, words("Hello there"));
        t.ingest(endpoint(2.5));
        let last = t.next_job().unwrap();
        assert!(last.is_final);
        let update = t.complete(&last, words("Hello there, friend."));
        assert_eq!(texts(&update.committed), "Hello there, friend.");
        assert_eq!(update.tentative.as_ref().unwrap().len(), 0);
        assert_eq!(update.sentences.len(), 1);
        assert_eq!(update.sentences[0].text, "Hello there, friend.");
        assert!(t.next_job().is_none());
    }

    #[test]
    fn partial_finishing_after_a_cut_is_ignored() {
        let mut t = fast();
        t.ingest(audio(3.0, 0.0));
        let partial = t.next_job().unwrap();
        t.ingest(pause(2.9, 2.5));
        assert!(
            t.complete(&partial, words("Stale words"))
                .tentative
                .is_none()
        );
        assert!(t.next_job().unwrap().is_final);
    }

    #[test]
    fn tail_of_a_pause_is_not_decoded() {
        let mut t = fast();
        t.ingest(audio(3.0, 0.0));
        t.ingest(pause(2.9, 2.5));
        let job = t.next_job().unwrap();
        t.complete(&job, words("Done."));
        t.ingest(audio_level(0.4, 3.0, 0.0));
        t.ingest(endpoint(0.0));
        assert_eq!(t.next_job().unwrap().samples.len(), 0);
    }

    #[test]
    fn next_phrase_waits_for_the_final_pass() {
        let mut t = fast();
        t.ingest(audio(3.0, 0.0));
        t.ingest(endpoint(2.5));
        let last = t.next_job().unwrap();
        t.ingest(audio(2.0, 5.0));
        assert!(t.next_job().is_none());
        t.complete(&last, words("Done."));
        let next = t.next_job().unwrap();
        assert!(!next.is_final);
        assert_eq!(next.start, 5.0);
    }

    #[test]
    fn long_run_is_cut_at_its_quietest_point() {
        let mut t = fast();
        let mut samples = vec![0.2_f32; 9 * 16_000];
        samples[7 * 16_000..7 * 16_000 + 2400].fill(0.0);
        t.ingest(SpeechEvent::Audio {
            samples,
            start: 0.0,
        });
        let job = t.next_job().unwrap();
        assert!(job.is_final);
        assert!(near(job.end(), 7.07, 0.1));
    }

    #[test]
    fn silent_phrase_is_not_decoded_and_clears_the_screen() {
        let mut t = fast();
        t.ingest(audio(3.0, 0.0));
        let job = t.next_job().unwrap();
        t.complete(&job, words("Invented"));
        t.ingest(endpoint(0.1));
        let job = t.next_job().unwrap();
        assert!(job.samples.is_empty());
        assert_eq!(t.complete(&job, vec![]).tentative.unwrap().len(), 0);
    }

    #[test]
    fn final_phrases_are_bounded_and_report_lost_coverage() {
        let mut t = fast();
        t.capacity = 10.0;
        for i in 0..6 {
            t.ingest(audio(3.0, i as f64 * 3.0));
            t.ingest(endpoint(2.5));
        }
        assert!(t.skipped() > 0.0);
    }

    #[test]
    fn stop_keeps_tentative_words() {
        let mut t = fast();
        t.ingest(audio(2.0, 0.0));
        let job = t.next_job().unwrap();
        t.complete(&job, words("Almost done"));
        assert_eq!(texts(&t.finish().committed), "Almost done");
    }

    #[test]
    fn discard_during_a_pass_clears_its_words() {
        let mut t = fast();
        t.ingest(audio(1.0, 0.0));
        let job = t.next_job().unwrap();
        t.ingest(SpeechEvent::Discard);
        t.complete(&job, words("Noise"));
        let clear = t.next_job().unwrap();
        assert!(clear.samples.is_empty());
        assert_eq!(t.complete(&clear, vec![]).tentative.unwrap().len(), 0);
    }

    #[test]
    fn automatic_keeps_the_gpu_under_half_load() {
        assert_eq!(update_interval(UpdateSpeed::Auto, 0.1, false), 0.4);
        assert_eq!(update_interval(UpdateSpeed::Auto, 0.6, false), 1.2);
        assert_eq!(update_interval(UpdateSpeed::Auto, 9.0, false), 3.0);
    }

    #[test]
    fn automatic_slows_down_under_pressure() {
        assert_eq!(update_interval(UpdateSpeed::Auto, 0.1, true), 0.8);
        assert_eq!(update_interval(UpdateSpeed::Auto, 2.0, true), 3.0);
    }

    #[test]
    fn fixed_speeds_are_always_honored() {
        assert_eq!(update_interval(UpdateSpeed::Fastest, 2.0, true), 0.2);
        assert_eq!(update_interval(UpdateSpeed::Saver, 0.0, false), 1.5);
    }

    #[test]
    fn saved_speeds_survive_and_unknown_ones_fall_back() {
        for speed in UpdateSpeed::ALL {
            assert_eq!(UpdateSpeed::saved(Some(speed.raw_value())), speed);
        }
        assert_eq!(UpdateSpeed::saved(Some("warp")), UpdateSpeed::Auto);
        assert_eq!(UpdateSpeed::saved(None), UpdateSpeed::Auto);
    }

    #[test]
    fn empty_passes_back_off() {
        let mut t = fast();
        t.ingest(audio(2.0, 0.0));
        let job = t.next_job().unwrap();
        t.complete(&job, vec![]);
        t.ingest(audio(0.25, 2.0));
        assert!(t.next_job().is_none());
        t.ingest(audio(0.2, 2.25));
        assert!(t.next_job().is_some());
    }

    #[test]
    fn words_end_the_back_off() {
        let mut t = fast();
        t.ingest(audio(2.0, 0.0));
        let job = t.next_job().unwrap();
        t.complete(&job, vec![]);
        t.ingest(audio(0.5, 2.0));
        let job = t.next_job().unwrap();
        t.complete(&job, words("Back"));
        t.ingest(audio(0.25, 2.5));
        assert!(t.next_job().is_some());
    }

    fn place_words(widths: &[f64], space: f64, max_width: f64) -> Vec<PlacedWord> {
        let mut placer = WordPlacer::new(space, max_width);
        widths.iter().map(|width| placer.place(*width)).collect()
    }

    #[test]
    fn wrapping_breaks_on_width() {
        let placed = place_words(&[60.0, 60.0, 60.0], 10.0, 135.0);
        assert_eq!(placed.iter().map(|p| p.line).collect::<Vec<_>>(), [0, 0, 1]);
        assert_eq!(placed[1].x, 70.0);
        assert_eq!(placed[2].x, 0.0);
    }

    #[test]
    fn appending_never_moves_earlier_words() {
        let widths: Vec<f64> = (0..40).map(|i| (20 + i * 7 % 50) as f64).collect();
        let all = place_words(&widths, 6.0, 300.0);
        for n in 1..widths.len() {
            assert_eq!(place_words(&widths[..n], 6.0, 300.0), all[..n]);
        }
    }

    #[test]
    fn oversized_word_keeps_its_own_line() {
        let placed = place_words(&[500.0, 20.0], 5.0, 300.0);
        assert_eq!(placed.iter().map(|p| p.line).collect::<Vec<_>>(), [0, 1]);
    }

    #[test]
    fn reveal_drains_a_batch_quickly() {
        assert!(reveal_interval(2) * 2.0 <= 0.3);
        assert!(reveal_interval(30) * 30.0 <= 0.7);
        assert_eq!(reveal_interval(1), 0.09);
        assert!(reveal_interval(1000) > 0.0);
    }
}
