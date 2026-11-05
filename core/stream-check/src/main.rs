//! Replays a recording through the rolling pipeline on a simulated live clock: audio "arrives"
//! while each pass really runs. Measures caption delay without capture. It does not exercise
//! real capture, an overlay, or real playback.
//!
//! This is the Rust twin of `mac/Sources/StreamCheck`. With `pass=` set, the clock no longer
//! depends on machine speed, so the two print identical output for identical behavior.

use soundveil_core::{
    CaptionUpdate, RollingTranscriber, SpeechGate, UpdateSpeed, Word, update_interval,
};
use soundveil_whisper::Recognizer;
use std::collections::HashMap;
use std::path::Path;
use std::process::exit;
use std::time::Instant;

const USAGE: &str = "Usage: stream-check model.bin audio.wav [reference.txt] [partial=1] [final=5] [step=0.2] [phrase=2.5] [maxphrase=9] [first=0.8] [ctx=640] [tail=1] [show=1] [pass=0.3]";

fn load(path: &str) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    if spec.sample_rate != 16_000 || spec.channels != 1 || spec.bits_per_sample != 16 {
        return Err("expected 16 kHz mono 16-bit audio".into());
    }
    reader
        .samples::<i16>()
        .map(|sample| sample.map(|value| value as f32 / 32768.0))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())
}

fn key(word: &str) -> String {
    word.to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_numeric())
        .collect()
}

fn tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(key)
        .filter(|token| !token.is_empty())
        .collect()
}

fn error_rate(reference: &[String], hypothesis: &[String]) -> f64 {
    let mut row: Vec<usize> = (0..=hypothesis.len()).collect();
    for (i, r) in reference.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, h) in hypothesis.iter().enumerate() {
            let cost = (previous + usize::from(r != h))
                .min(row[j] + 1)
                .min(row[j + 1] + 1);
            previous = row[j + 1];
            row[j + 1] = cost;
        }
    }
    row[hypothesis.len()] as f64 / reference.len().max(1) as f64
}

fn percentile(values: &[f64], p: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[(sorted.len() - 1).min((sorted.len() as f64 * p) as usize)]
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn joined(words: &[Word]) -> String {
    let texts: Vec<&str> = words.iter().map(|word| word.text.as_str()).collect();
    texts.join(" ")
}

#[derive(Default)]
struct Screen {
    committed: usize,
    changes: usize,
    showing: Vec<String>,
    first_seen: Vec<(String, usize, f64)>,
    first_shown: Vec<f64>,
    settled: Vec<f64>,
    transcript: Vec<String>,
    print: bool,
}

impl Screen {
    /// Words the viewer saw replaced: anything on screen after the first word that changed.
    fn show(&mut self, words: Vec<String>) {
        let same = self
            .showing
            .iter()
            .zip(&words)
            .take_while(|(a, b)| key(a) == key(b))
            .count();
        self.changes += self.showing.len() - same;
        self.showing = words;
    }

    fn record(&mut self, update: &CaptionUpdate, now: f64) {
        for (i, word) in update.committed.iter().enumerate() {
            self.settled.push((now - word.end).max(0.0));
            self.transcript.push(word.text.clone());
            // A word counts as shown once it was on screen near its final position in the phrase.
            let seen = self
                .first_seen
                .iter()
                .find(|(seen, position, _)| *seen == key(&word.text) && position.abs_diff(i) <= 2)
                .map_or(now, |entry| entry.2);
            self.first_shown.push((seen - word.end).max(0.0));
        }
        self.committed += update.committed.len();
        if !update.committed.is_empty() {
            self.show(update.committed.iter().map(|w| w.text.clone()).collect());
            self.showing.clear();
            self.first_seen.clear();
        }
        if let Some(tentative) = &update.tentative {
            if update.committed.is_empty() {
                self.show(tentative.iter().map(|w| w.text.clone()).collect());
            }
            for (i, word) in tentative.iter().enumerate() {
                self.first_seen.push((key(&word.text), i, now));
            }
        }
        if self.print && (update.tentative.is_some() || !update.committed.is_empty()) {
            let tentative = update.tentative.as_deref().unwrap_or_default();
            println!(
                "{now:6.2}  {} | {}",
                joined(&update.committed),
                joined(tentative)
            );
        }
    }
}

fn main() {
    let (mut paths, mut options) = (Vec::new(), HashMap::new());
    for argument in std::env::args().skip(1) {
        let parts: Vec<&str> = argument.split('=').collect();
        match parts[..] {
            [name, value] if value.parse::<f64>().is_ok() => {
                options.insert(name.to_string(), value.parse::<f64>().unwrap());
            }
            _ => paths.push(argument),
        }
    }
    if paths.len() < 2 {
        eprintln!("{USAGE}");
        exit(2);
    }
    let option = |name: &str| options.get(name).copied();
    let mut samples = load(&paths[1]).unwrap_or_else(|error| {
        eprintln!("{error}");
        exit(1);
    });
    samples.extend(std::iter::repeat_n(0.0, 24_000));
    let mut recognizer =
        Recognizer::new(Path::new(&paths[0]), option("cpu").is_none()).expect("Model load failed");
    if option("cues") == Some(0.0) {
        recognizer.allow_cues = false;
    }
    let mut gate = SpeechGate::default();
    let mut transcriber = RollingTranscriber::default();
    if let Some(v) = option("step") {
        transcriber.step = v;
    }
    if let Some(v) = option("phrase") {
        transcriber.min_phrase = v;
    }
    if let Some(v) = option("maxphrase") {
        transcriber.max_phrase = v;
    }
    if let Some(v) = option("partial") {
        transcriber.partial_beam = v as i32;
    }
    if let Some(v) = option("final") {
        transcriber.final_beam = v as i32;
    }
    if let Some(v) = option("first") {
        transcriber.first_decode = v;
    }
    if let Some(v) = option("tail") {
        transcriber.tail_words = v as usize;
    }
    if let Some(v) = option("ctx") {
        transcriber.audio_context = v as usize;
    }
    let mut screen = Screen {
        print: option("show").is_some(),
        ..Screen::default()
    };
    let (mut fed, mut now) = (0_usize, 0.0_f64);
    let (mut passes, mut lengths, mut finals) = (Vec::new(), Vec::new(), Vec::new());
    let feed = |until: f64, fed: &mut usize, gate: &mut SpeechGate, t: &mut RollingTranscriber| {
        let target = samples.len().min((until * 16_000.0) as usize);
        while *fed < target {
            let end = target.min(*fed + 1600);
            for event in gate.append(&samples[*fed..end]) {
                t.ingest(event);
            }
            *fed = end;
        }
    };
    loop {
        feed(now, &mut fed, &mut gate, &mut transcriber);
        let Some(job) = transcriber.next_job() else {
            if fed >= samples.len() {
                break;
            }
            now += 0.01;
            continue;
        };
        let began = Instant::now();
        let words = recognizer.decode(&job).unwrap_or_default();
        // slow=3 simulates a machine three times slower. pass= fixes the time of every pass.
        let took = option("pass")
            .unwrap_or_else(|| began.elapsed().as_secs_f64() * option("slow").unwrap_or(1.0));
        if let Some(auto) = option("auto")
            && !job.is_final
        {
            transcriber.step = update_interval(UpdateSpeed::Auto, took, auto == 2.0);
        }
        passes.push(took);
        lengths.push(job.samples.len() as f64 / 16_000.0);
        if job.is_final {
            finals.push(took);
        }
        now += took;
        feed(now, &mut fed, &mut gate, &mut transcriber);
        let update = transcriber.complete(&job, words);
        screen.record(&update, now);
    }
    let update = transcriber.finish();
    screen.record(&update, now);
    println!("{}", screen.transcript.join(" "));
    println!(
        "audio {:.1}s · passes {} · pass mean {:.0} ms, p95 {:.0} ms · buffer mean {:.1}s, max {:.1}s · skipped {:.1}s",
        samples.len() as f64 / 16_000.0,
        passes.len(),
        mean(&passes) * 1000.0,
        percentile(&passes, 0.95) * 1000.0,
        mean(&lengths),
        lengths.iter().copied().fold(0.0, f64::max),
        transcriber.skipped()
    );
    println!(
        "final passes {} · mean {:.0} ms, p95 {:.0} ms",
        finals.len(),
        mean(&finals) * 1000.0,
        percentile(&finals, 0.95) * 1000.0
    );
    println!(
        "first shown after word ends: mean {:.2}s, p95 {:.2}s",
        mean(&screen.first_shown),
        percentile(&screen.first_shown, 0.95)
    );
    println!(
        "final text after word ends:  mean {:.2}s, p95 {:.2}s",
        mean(&screen.settled),
        percentile(&screen.settled, 0.95)
    );
    println!(
        "on-screen words replaced: {} across {} words ({:.1}%)",
        screen.changes,
        screen.committed,
        screen.changes as f64 / screen.committed.max(1) as f64 * 100.0
    );
    if let Some(path) = paths.get(2) {
        let reference = std::fs::read_to_string(path).expect("reference file");
        println!(
            "word error rate: {:.1}%",
            error_rate(&tokens(&reference), &tokens(&screen.transcript.join(" "))) * 100.0
        );
    }
}
