//! Keeps the on-screen words in step with the feed. They are one list, shared by the overlay and
//! the preview in the main window. Only rows that changed are touched, so words already on
//! screen stay put and don't replay their fade.

use crate::CaptionWord;
use crate::feed::{Feed, Item, Look};
use slint::{Model as _, ModelRc, Timer, TimerMode, VecModel};
use soundveil_core::CaptionUpdate;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const CLEAR_AFTER: Duration = Duration::from_secs(8);

fn word(item: &Item) -> CaptionWord {
    CaptionWord {
        text: item.text.as_str().into(),
        kind: match item.look {
            Look::Plain => 0,
            Look::Accent => 1,
            Look::Cue => 2,
            Look::LineBreak => 3,
        },
        settled: item.settled,
    }
}

pub struct Captions {
    feed: RefCell<Feed>,
    words: Rc<VecModel<CaptionWord>>,
    pacing: Timer,
    clearing: Timer,
}

impl Captions {
    pub fn new(plain: bool) -> Rc<Self> {
        Rc::new(Self {
            feed: RefCell::new(Feed::new(plain)),
            words: Rc::new(VecModel::default()),
            pacing: Timer::default(),
            clearing: Timer::default(),
        })
    }

    pub fn model(&self) -> ModelRc<CaptionWord> {
        ModelRc::from(self.words.clone())
    }

    fn publish(&self) {
        let items = self.feed.borrow().visible();
        if items.is_empty() {
            self.words.set_vec(Vec::new());
            return;
        }
        let shared = self.words.row_count().min(items.len());
        for (row, item) in items[..shared].iter().enumerate() {
            let word = word(item);
            if self.words.row_data(row).as_ref() != Some(&word) {
                self.words.set_row_data(row, word);
            }
        }
        for item in &items[shared..] {
            self.words.push(word(item));
        }
        while self.words.row_count() > items.len() {
            self.words.remove(self.words.row_count() - 1);
        }
    }

    fn step(self: &Rc<Self>) {
        let wait = self.feed.borrow_mut().reveal();
        let Some(wait) = wait else {
            return;
        };
        self.publish();
        let this = self.clone();
        let wait = Duration::from_secs_f64(wait);
        self.pacing
            .start(TimerMode::SingleShot, wait, move || this.step());
    }

    /// Takes in new words. Returns whether there is now anything to show.
    pub fn apply(self: &Rc<Self>, update: &CaptionUpdate) -> bool {
        self.feed.borrow_mut().apply(update);
        self.publish();
        if !self.pacing.running() {
            self.step();
        }
        let tentative = update.tentative.as_deref().unwrap_or_default();
        if !update.committed.is_empty() || !tentative.is_empty() {
            let this = self.clone();
            self.clearing
                .start(TimerMode::SingleShot, CLEAR_AFTER, move || {
                    this.feed.borrow_mut().go_idle();
                    this.publish();
                });
        }
        !self.feed.borrow().is_empty()
    }

    pub fn reset(&self) {
        self.pacing.stop();
        self.clearing.stop();
        self.feed.borrow_mut().clear();
        self.publish();
    }

    pub fn set_expanded(&self, expanded: bool) {
        self.feed.borrow_mut().expanded = expanded;
        self.publish();
    }

    pub fn set_plain(&self, plain: bool) {
        self.feed.borrow_mut().plain = plain;
        self.publish();
    }
}
