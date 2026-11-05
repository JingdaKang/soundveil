#pragma once
#include <stdint.h>
typedef void *FCContext;
FCContext fc_create_device(const char *path, int use_gpu);
void fc_destroy(FCContext ctx);
int fc_stream(FCContext ctx, const float *samples, int count, int audio_ctx, int beam, int max_tokens, int cues);
int fc_count(FCContext ctx);
int fc_tokens(FCContext ctx, int segment);
const char *fc_token_text(FCContext ctx, int segment, int token);
int fc_token_is_text(FCContext ctx, int segment, int token);
