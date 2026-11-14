#include "bridge.h"
#include "whisper.h"
#include <stdlib.h>
// The model is loaded once and each audio context size decodes in a state of its own. Changing
// the size inside one state corrupted the passes that followed, so a state only ever sees one.
#define FC_STATES 4
struct FC { struct whisper_context *model; struct whisper_state *states[FC_STATES]; int sizes[FC_STATES]; struct whisper_state *last; };
FCContext fc_create_device(const char *path, int use_gpu) {
 struct whisper_context_params p = whisper_context_default_params();
 p.use_gpu = use_gpu;
 struct whisper_context *model = whisper_init_from_file_with_params_no_state(path, p);
 if(!model) return NULL;
 struct FC *c = calloc(1, sizeof(struct FC));
 c->model = model;
 return c;
}
void fc_destroy(FCContext context) {
 struct FC *c = context;
 if(!c) return;
 for(int i = 0; i < FC_STATES; i++) if(c->states[i]) whisper_free_state(c->states[i]);
 whisper_free(c->model); free(c);
}
// Finds the state for one audio context size, making it on first use. Returns 0 if it can't.
int fc_prepare(FCContext context, int audio_ctx) {
 struct FC *c = context;
 for(int i = 0; i < FC_STATES; i++) {
  if(c->states[i] && c->sizes[i] == audio_ctx) { c->last = c->states[i]; return 1; }
  if(!c->states[i]) {
   c->states[i] = whisper_init_state(c->model); c->sizes[i] = audio_ctx; c->last = c->states[i];
   return c->last != NULL;
  }
 }
 return 0;
}
// One rolling pass. The encoder dominates the cost and scales with the audio context, so the
// passes that only refresh the screen use far less than Whisper's default. A bad pass is replaced
// by the next, so there are no retries: on a final pass they were tried and changed nothing.
// With timestamps off, Whisper writes text alone. With them on and a small audio context, it can
// end a phrase early by its own clock and then say the phrase again, so a pass that has both must
// be one whose text is checked against another pass.
// There is no prompt: Whisper echoed it into the transcript and invented text. There are no word
// timestamps either: they were too unreliable here, so the pipeline cuts audio at pauses.
int fc_stream(FCContext context, const float *s, int n, int audio_ctx, int beam, int max_tokens, int cues, int timestamps) {
 struct FC *c = context;
 if(!fc_prepare(c, audio_ctx)) return -1;
 struct whisper_full_params p = whisper_full_default_params(beam > 1 ? WHISPER_SAMPLING_BEAM_SEARCH : WHISPER_SAMPLING_GREEDY);
 p.n_threads = 4; p.language = "en"; p.translate = false;
 p.no_context = true; p.audio_ctx = audio_ctx;
 p.print_realtime = false; p.print_progress = false; p.print_timestamps = false; p.print_special = false;
 p.suppress_blank = true; p.suppress_nst = !cues; p.no_timestamps = !timestamps;
 p.single_segment = true; p.max_tokens = max_tokens; p.temperature_inc = 0.0f;
 p.beam_search.beam_size = beam; p.greedy.best_of = 1;
 return whisper_full_with_state(c->model, c->last, p, s, n);
}
// These read the pass that ran last.
int fc_count(FCContext c) { return whisper_full_n_segments_from_state(((struct FC *)c)->last); }
int fc_tokens(FCContext c, int i) { return whisper_full_n_tokens_from_state(((struct FC *)c)->last, i); }
const char *fc_token_text(FCContext c, int i, int t) { struct FC *f = c; return whisper_full_get_token_text_from_state(f->model, f->last, i, t); }
int fc_token_is_text(FCContext c, int i, int t) { struct FC *f = c; return whisper_full_get_token_id_from_state(f->last, i, t) < whisper_token_eot(f->model); }
