//! Runs decode jobs on whisper.cpp through a narrow C bridge.

use soundveil_core::{DecodeJob, Word, words_from_tokens};
use std::ffi::{CStr, CString, c_char, c_float, c_int, c_void};
use std::path::Path;

unsafe extern "C" {
    fn fc_create_device(path: *const c_char, use_gpu: c_int) -> *mut c_void;
    fn fc_destroy(context: *mut c_void);
    fn fc_stream(
        context: *mut c_void,
        samples: *const c_float,
        count: c_int,
        audio_ctx: c_int,
        beam: c_int,
        max_tokens: c_int,
        cues: c_int,
    ) -> c_int;
    fn fc_count(context: *mut c_void) -> c_int;
    fn fc_tokens(context: *mut c_void, segment: c_int) -> c_int;
    fn fc_token_text(context: *mut c_void, segment: c_int, token: c_int) -> *const c_char;
    fn fc_token_is_text(context: *mut c_void, segment: c_int, token: c_int) -> c_int;
}

/// A loaded model. It decodes one job at a time, so `decode` takes `&mut self`.
pub struct Recognizer {
    context: *mut c_void,
    pub allow_cues: bool,
}

// The context is only ever used through `&mut self`, so it moves between threads but is never
// shared by them.
unsafe impl Send for Recognizer {}

impl Recognizer {
    pub fn new(path: &Path, use_gpu: bool) -> Option<Self> {
        let path = CString::new(path.to_str()?).ok()?;
        let context = unsafe { fc_create_device(path.as_ptr(), use_gpu as c_int) };
        (!context.is_null()).then_some(Self {
            context,
            allow_cues: true,
        })
    }

    /// Returns `None` if Whisper failed. An empty job needs no pass and gives no words.
    pub fn decode(&mut self, job: &DecodeJob) -> Option<Vec<Word>> {
        if job.samples.is_empty() {
            return Some(Vec::new());
        }
        let code = unsafe {
            fc_stream(
                self.context,
                job.samples.as_ptr(),
                job.samples.len() as c_int,
                job.audio_context as c_int,
                job.beam,
                job.max_tokens as c_int,
                (job.is_final && self.allow_cues) as c_int,
            )
        };
        (code == 0).then(|| words_from_tokens(&self.tokens(), job.start, job.end()))
    }

    fn tokens(&self) -> Vec<Vec<Vec<u8>>> {
        let segments = unsafe { fc_count(self.context) };
        (0..segments)
            .map(|segment| {
                let tokens = unsafe { fc_tokens(self.context, segment) };
                (0..tokens)
                    .filter_map(|token| unsafe {
                        if fc_token_is_text(self.context, segment, token) == 0 {
                            return None;
                        }
                        let text = fc_token_text(self.context, segment, token);
                        (!text.is_null()).then(|| CStr::from_ptr(text).to_bytes().to_vec())
                    })
                    .collect()
            })
            .collect()
    }
}

impl Drop for Recognizer {
    fn drop(&mut self) {
        unsafe { fc_destroy(self.context) }
    }
}
