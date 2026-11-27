// bench.c - time transcribe_run on one clip with the model already loaded.
//
// Loads the model once, runs the clip `warmup` times untimed, then `iters`
// times timed, each with the given language hint (and vocabulary, if any).
// Prints one JSON line per timed run and a summary line with the median wall
// time. Wall time is measured around transcribe_run only, so it covers mel,
// encoder, prefill and decode, as one dictation would in Skald.
//
// Build against a transcribe.cpp build tree (Metal, static):
//
//     clang -O2 -I <transcribe.cpp>/include bench.c \
//         <build>/src/libtranscribe.a <build>/ggml/src/libggml.a \
//         <build>/ggml/src/ggml-metal/libggml-metal.a \
//         <build>/ggml/src/libggml-cpu.a <build>/ggml/src/libggml-base.a \
//         -framework Metal -framework Foundation -framework Accelerate \
//         -lc++ -o bokfink-bench
//
// Run:
//
//     bokfink-bench model.gguf clip.wav nb 10 3 ["keywords"]

#include "transcribe.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

// 16 kHz mono 16-bit PCM WAV, the layout the clip fetcher writes.
static float * read_wav_pcm16(const char * path, int * out_n) {
    FILE * f = fopen(path, "rb");
    if (f == NULL) {
        return NULL;
    }
    fseek(f, 0, SEEK_END);
    const long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    unsigned char * data = malloc((size_t) size);
    if (data == NULL || fread(data, 1, (size_t) size, f) != (size_t) size) {
        fclose(f);
        free(data);
        return NULL;
    }
    fclose(f);
    if (size < 12 || memcmp(data, "RIFF", 4) != 0 || memcmp(data + 8, "WAVE", 4) != 0) {
        free(data);
        return NULL;
    }
    uint16_t channels = 0, bits = 0, tag = 0;
    uint32_t rate = 0;
    float *  pcm  = NULL;
    for (long pos = 12; pos + 8 <= size;) {
        uint32_t chunk_size;
        memcpy(&chunk_size, data + pos + 4, 4);
        const unsigned char * body = data + pos + 8;
        if (memcmp(data + pos, "fmt ", 4) == 0) {
            memcpy(&tag, body, 2);
            memcpy(&channels, body + 2, 2);
            memcpy(&rate, body + 4, 4);
            memcpy(&bits, body + 14, 2);
        } else if (memcmp(data + pos, "data", 4) == 0) {
            if (tag != 1 || channels != 1 || rate != 16000 || bits != 16) {
                break;
            }
            const int n = (int) (chunk_size / 2);
            pcm         = malloc((size_t) n * sizeof(float));
            for (int i = 0; i < n; ++i) {
                int16_t s;
                memcpy(&s, body + 2 * i, 2);
                pcm[i] = (float) s / 32768.0f;
            }
            *out_n = n;
            break;
        }
        pos += 8 + chunk_size + (chunk_size & 1);
    }
    free(data);
    return pcm;
}

static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000.0 + ts.tv_nsec / 1e6;
}

static int cmp_double(const void * a, const void * b) {
    const double x = *(const double *) a, y = *(const double *) b;
    return (x > y) - (x < y);
}

int main(int argc, char ** argv) {
    if (argc < 6) {
        fprintf(stderr, "usage: %s model.gguf clip.wav language iters warmup [keywords]\n", argv[0]);
        return 2;
    }
    const char * model_path = argv[1];
    const char * wav_path   = argv[2];
    const char * language   = argv[3];
    const int    iters      = atoi(argv[4]);
    const int    warmup     = atoi(argv[5]);
    const char * keywords   = argc > 6 ? argv[6] : NULL;

    int     n_samples = 0;
    float * pcm       = read_wav_pcm16(wav_path, &n_samples);
    if (pcm == NULL) {
        fprintf(stderr, "error: %s is not a 16 kHz mono 16-bit WAV\n", wav_path);
        return 1;
    }

    const double                t_load0 = now_ms();
    struct transcribe_session * session = NULL;
    transcribe_status           st      = transcribe_open(model_path, NULL, NULL, &session);
    if (st != TRANSCRIBE_OK) {
        fprintf(stderr, "error: transcribe_open: %s\n", transcribe_status_string(st));
        return 1;
    }
    const double load_ms = now_ms() - t_load0;

    struct transcribe_run_params rp;
    transcribe_run_params_init(&rp);
    rp.language = language;
    if (keywords != NULL) {
        rp.vocabulary   = &keywords;
        rp.n_vocabulary = 1;
    }

    double * wall = calloc((size_t) (iters > 0 ? iters : 1), sizeof(double));
    for (int i = 0; i < warmup + iters; ++i) {
        const double t0 = now_ms();
        st              = transcribe_run(session, pcm, n_samples, &rp);
        const double ms = now_ms() - t0;
        if (st != TRANSCRIBE_OK) {
            fprintf(stderr, "error: transcribe_run: %s\n", transcribe_status_string(st));
            return 1;
        }
        if (i < warmup) {
            continue;
        }
        struct transcribe_timings t;
        transcribe_timings_init(&t);
        transcribe_get_timings(session, &t);
        wall[i - warmup] = ms;
        printf("{\"run\":%d,\"wall_ms\":%.1f,\"mel_ms\":%.1f,\"encode_ms\":%.1f,\"decode_ms\":%.1f}\n", i - warmup + 1,
               ms, t.mel_ms, t.encode_ms, t.decode_ms);
    }

    qsort(wall, (size_t) iters, sizeof(double), cmp_double);
    const double median = iters % 2 ? wall[iters / 2] : (wall[iters / 2 - 1] + wall[iters / 2]) / 2.0;
    const double audio_s = n_samples / 16000.0;
    printf("{\"summary\":true,\"audio_s\":%.2f,\"load_ms\":%.1f,\"iters\":%d,\"median_wall_ms\":%.1f,"
           "\"min_wall_ms\":%.1f,\"max_wall_ms\":%.1f,\"realtime_x\":%.1f}\n",
           audio_s, load_ms, iters, median, wall[0], wall[iters - 1], audio_s * 1000.0 / median);
    printf("text: %s\n", transcribe_full_text(session));

    free(wall);
    free(pcm);
    transcribe_close(session);
    return 0;
}
