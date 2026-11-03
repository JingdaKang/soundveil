#include "Bridge.h"
#include "whisper.h"
FCContext fc_create(const char *path) {
 struct whisper_context_params p = whisper_context_default_params();
 p.use_gpu = true;
 return whisper_init_from_file_with_params(path, p);
}
void fc_destroy(FCContext c) { if(c) whisper_free(c); }
int fc_transcribe(FCContext c, const float *s, int n, const char *prompt) {
 struct whisper_full_params p = whisper_full_default_params(WHISPER_SAMPLING_BEAM_SEARCH);
 p.n_threads = 4; p.language = "en"; p.translate = false;
 p.no_context = true; p.initial_prompt = prompt;
 p.print_realtime = false; p.print_progress = false; p.print_timestamps = false;
 p.suppress_blank = true; p.suppress_nst = true;
 p.beam_search.beam_size = 5;
 return whisper_full(c, p, s, n);
}
int fc_count(FCContext c) { return whisper_full_n_segments(c); }
const char *fc_text(FCContext c, int i) { return whisper_full_get_segment_text(c,i); }
int64_t fc_start(FCContext c,int i) { return whisper_full_get_segment_t0(c,i); }
int64_t fc_end(FCContext c,int i) { return whisper_full_get_segment_t1(c,i); }
