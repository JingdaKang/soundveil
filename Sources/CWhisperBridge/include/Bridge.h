#pragma once
#include <stdint.h>
typedef void * FCContext;
FCContext fc_create(const char *path);
void fc_destroy(FCContext ctx);
int fc_transcribe(FCContext ctx, const float *samples, int count, const char *prompt);
int fc_count(FCContext ctx);
const char *fc_text(FCContext ctx, int index);
int64_t fc_start(FCContext ctx, int index);
int64_t fc_end(FCContext ctx, int index);
