//! Prints everything the caption logic does on seeded random input, with a scripted decoder.
//! `mac/Sources/CoreTrace` is the same program in Swift, so identical output shows the two
//! implementations agree. `core/scripts/compare-with-swift.sh` runs both.

use soundveil_core::{
    Caption, CaptionUpdate, DecodeJob, RollingTranscriber, SpeechEvent, SpeechGate, UpdateSpeed,
    Word, WordPlacer, WordStyle, cue_label, reveal_interval, update_interval, word_styles,
};
use std::io::{BufWriter, Write};

struct Rng {
    state: u64,
}

impl Rng {
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / 9007199254740992.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick(&mut self) -> String {
        VOCABULARY[self.below(VOCABULARY.len())].to_string()
    }
}

const VOCABULARY: [&str; 50] = [
    "Hello",
    "hello,",
    "there.",
    "What",
    "what",
    "your",
    "your,",
    "no",
    "No",
    "way!",
    "way",
    "cap",
    "cap,",
    "go",
    "let's",
    "Let's",
    "oh",
    "my",
    "god!",
    "ha",
    "Ha",
    "haha",
    "Hahaha",
    "sooooo",
    "1000",
    "5th",
    "boom",
    "Boom.",
    "boom!",
    "based",
    "Based.",
    "sus",
    "bruh",
    "[laughter]",
    "[music]",
    "um,",
    "?",
    "sure?",
    "café",
    "ÉCOLE",
    "école",
    "Straße",
    "١٢٣",
    "½",
    "it's",
    "giving",
    "tick",
    "tock",
    "ding",
    "dong",
];
const NOTES: [&str; 12] = [
    "[MUSIC PLAYING]",
    "(audience laughing)",
    "♪♪",
    "[Applause]",
    "[BLANK_AUDIO]",
    "(speaking foreign language)",
    "*cheering*",
    "(Chuckles)",
    "♫ singing ♫",
    "[clapping]",
    "",
    "plain",
];

fn checksum(samples: &[f32]) -> u64 {
    samples.iter().fold(14695981039346656037, |hash: u64, s| {
        (hash ^ s.to_bits() as u64).wrapping_mul(1099511628211)
    })
}
fn show(words: &[Word]) -> String {
    let parts: Vec<String> = words
        .iter()
        .map(|w| format!("{}@{:.9}-{:.9}", w.text, w.start, w.end))
        .collect();
    parts.join(" ")
}
fn show_lines(lines: &[Caption]) -> String {
    let parts: Vec<String> = lines
        .iter()
        .map(|c| format!("{}@{:.9}-{:.9}", c.text, c.start, c.end))
        .collect();
    parts.join(" | ")
}
fn describe_event(event: &SpeechEvent) -> String {
    match event {
        SpeechEvent::Audio { samples, start } => {
            format!("audio {} {start:.9} {}", samples.len(), checksum(samples))
        }
        SpeechEvent::Pause { at, voiced } => format!("pause {at:.9} {voiced:.9}"),
        SpeechEvent::Endpoint { voiced } => format!("endpoint {voiced:.9}"),
        SpeechEvent::Discard => "discard".into(),
        SpeechEvent::Gap => "gap".into(),
    }
}
fn describe_job(job: &DecodeJob) -> String {
    format!(
        "job {} {:.9} {:.9} {} {} {} {} {}",
        job.samples.len(),
        job.start,
        job.end(),
        job.audio_context,
        job.max_tokens,
        job.beam,
        job.is_final,
        checksum(&job.samples)
    )
}
fn describe_update(update: &CaptionUpdate) -> String {
    let tentative = update
        .tentative
        .as_ref()
        .map_or("nil".to_string(), |words| show(words));
    format!(
        "update [{}] [{tentative}] {} [{}]",
        show(&update.committed),
        update.stable,
        show_lines(&update.sentences)
    )
}
fn style_name(style: WordStyle) -> &'static str {
    match style {
        WordStyle::Plain => "plain",
        WordStyle::Sound => "sound",
        WordStyle::Slang => "slang",
        WordStyle::Cue => "cue",
    }
}
fn decode(job: &DecodeJob, rng: &mut Rng, last: &mut Vec<String>) -> Vec<Word> {
    let mut texts: Vec<String> = Vec::new();
    match rng.below(6) {
        0 => {}
        1 | 2 => {
            texts = last.clone();
            texts.push(rng.pick());
        }
        3 => {
            let keep = rng.below(last.len() + 1);
            texts = last[..keep].to_vec();
            texts.push(rng.pick());
            texts.push(rng.pick());
        }
        4 => {
            let length = 1 + rng.below(4);
            let phrase: Vec<String> = (0..length).map(|_| rng.pick()).collect();
            texts.push(rng.pick());
            for _ in 0..2 + rng.below(3) {
                texts.extend(phrase.iter().cloned());
            }
        }
        _ => {
            let length = rng.below(13);
            texts = (0..length).map(|_| rng.pick()).collect();
        }
    }
    *last = texts.clone();
    if job.samples.is_empty() {
        return Vec::new();
    }
    texts
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let i = i as f64;
            Word::new(text, job.start + i * 0.3, job.start + (i + 1.0) * 0.3)
        })
        .collect()
}

struct Run {
    rng: Rng,
    transcriber: RollingTranscriber,
    running: Option<DecodeJob>,
    hold: usize,
    last: Vec<String>,
}

impl Run {
    fn pump(&mut self, out: &mut impl Write) -> std::io::Result<()> {
        if let Some(job) = self.running.take() {
            if self.hold > 0 {
                self.hold -= 1;
                self.running = Some(job);
                return Ok(());
            }
            let words = decode(&job, &mut self.rng, &mut self.last);
            let update = self.transcriber.complete(&job, words);
            writeln!(out, "{}", describe_update(&update))?;
        }
        if let Some(job) = self.transcriber.next_job() {
            writeln!(out, "{}", describe_job(&job))?;
            self.running = Some(job);
            self.hold = self.rng.below(4);
        }
        Ok(())
    }
}

fn main() -> std::io::Result<()> {
    let seeds: u64 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(300);
    let mut out = BufWriter::new(std::io::stdout().lock());
    for seed in 0..seeds {
        let mut rng = Rng { state: seed };
        writeln!(out, "seed {seed}")?;
        let mut gate = SpeechGate::new([0.0, 12.5][rng.below(2)]);
        let mut transcriber = RollingTranscriber::default();
        transcriber.step = [0.2, 0.4, 0.75, 1.5][rng.below(4)];
        transcriber.capacity = [24.0, 6.0][rng.below(2)];
        transcriber.max_phrase = [9.0, 5.0][rng.below(2)];
        let mut run = Run {
            rng,
            transcriber,
            running: None,
            hold: 0,
            last: Vec::new(),
        };
        for _ in 0..80 {
            let kind = run.rng.below(10);
            let seconds = 0.02 + run.rng.unit() * if kind < 6 { 2.5 } else { 1.0 };
            let level: f32 = if kind < 6 {
                0.02 + run.rng.unit() as f32 * 0.28
            } else if kind < 8 {
                0.0
            } else {
                0.001 + run.rng.unit() as f32 * 0.009
            };
            let samples: Vec<f32> = (0..(seconds * 16_000.0) as usize)
                .map(|_| (run.rng.unit() * 2.0 - 1.0) as f32 * level)
                .collect();
            let mut offset = 0;
            while offset < samples.len() {
                let end = samples.len().min(offset + 1 + run.rng.below(3000));
                for event in gate.append(&samples[offset..end]) {
                    writeln!(out, "{}", describe_event(&event))?;
                    run.transcriber.ingest(event);
                }
                offset = end;
                run.pump(&mut out)?;
            }
            if run.rng.below(20) == 0 {
                writeln!(out, "{}", describe_event(&SpeechEvent::Gap))?;
                run.transcriber.ingest(SpeechEvent::Gap);
                run.pump(&mut out)?;
            }
            if run.rng.below(8) == 0 {
                let speed = UpdateSpeed::ALL[run.rng.below(UpdateSpeed::ALL.len())];
                let pass_time = run.rng.unit();
                let constrained = run.rng.below(2) == 0;
                run.transcriber.step = update_interval(speed, pass_time, constrained);
                writeln!(out, "step {:.9}", run.transcriber.step)?;
            }
        }
        if let Some(job) = run.running.take() {
            let words = decode(&job, &mut run.rng, &mut run.last);
            let update = run.transcriber.complete(&job, words);
            writeln!(out, "{}", describe_update(&update))?;
        }
        writeln!(out, "{}", describe_update(&run.transcriber.finish()))?;
        writeln!(
            out,
            "skipped {:.9} end {:.9}",
            run.transcriber.skipped(),
            run.transcriber.audio_end()
        )?;
        let mut rng = run.rng;
        for _ in 0..40 {
            let length = rng.below(10);
            let words: Vec<String> = (0..length).map(|_| rng.pick()).collect();
            let styles: Vec<&str> = word_styles(&words).into_iter().map(style_name).collect();
            writeln!(out, "styles {} = {}", words.join(" "), styles.join(" "))?;
        }
        for note in NOTES {
            writeln!(out, "cue {note} = {}", cue_label(note).unwrap_or("nil"))?;
        }
        let mut placer = WordPlacer::new(6.0, 200.0 + rng.unit() * 400.0);
        for _ in 0..30 {
            let placed = placer.place(10.0 + rng.unit() * 120.0);
            writeln!(out, "place {} {:.9}", placed.line, placed.x)?;
        }
        for _ in 0..5 {
            writeln!(out, "reveal {:.9}", reveal_interval(rng.below(60)))?;
        }
    }
    out.flush()
}
