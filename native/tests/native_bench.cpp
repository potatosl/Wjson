// SPDX-License-Identifier: MPL-2.0
// Number conversion and escaping speed of wjson_native, next to the C
// runtime's snprintf/strtod for scale. Freestanding: no library headers;
// the few C runtime and clock functions used are declared here.

#include "wjson_native.h"

extern "C" int printf(const char *format, ...);
extern "C" int snprintf(char *buf, __SIZE_TYPE__ size, const char *format, ...);
extern "C" double strtod(const char *s, char **end);
extern "C" void *malloc(__SIZE_TYPE__ size);
extern "C" void free(void *p);

#ifdef _WIN32
extern "C" __declspec(dllimport) int __stdcall QueryPerformanceCounter(long long *count);
extern "C" __declspec(dllimport) int __stdcall QueryPerformanceFrequency(long long *freq);
static double now() {
    long long c = 0, f = 1;
    QueryPerformanceCounter(&c);
    QueryPerformanceFrequency(&f);
    return static_cast<double>(c) / static_cast<double>(f);
}
#else
struct wj_timespec {
    long tv_sec;
    long tv_nsec;
};
extern "C" int clock_gettime(int clock, wj_timespec *ts);
static double now() {
    wj_timespec ts{0, 0};
    clock_gettime(1 /* CLOCK_MONOTONIC */, &ts);
    return static_cast<double>(ts.tv_sec) + static_cast<double>(ts.tv_nsec) * 1e-9;
}
#endif

template <class F>
static double best(int runs, F f) {
    double b = 1e100;
    for (int r = 0; r < runs; ++r) {
        double t = now();
        f();
        double s = now() - t;
        if (s < b) b = s;
    }
    return b;
}

// In a freestanding build `main` is an ordinary function; give it C linkage.
#if !__STDC_HOSTED__
extern "C"
#endif
int main() {
    const int n = 1000000;
    double *values = static_cast<double *>(malloc(sizeof(double) * n));
    unsigned long long x = 0x9E3779B97F4A7C15ULL;
    for (int i = 0; i < n; ++i) {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        values[i] = static_cast<double>(x % 100000000) / 1000.0 - 50000.0;
    }
    char buf[64];
    volatile long long sink = 0;

    double fmt_ours = best(3, [&] {
        for (int i = 0; i < n; ++i) sink = sink + wjn_format_f64(values[i], buf, sizeof buf);
    });
    double fmt_libc = best(3, [&] {
        for (int i = 0; i < n; ++i) sink = sink + snprintf(buf, sizeof buf, "%.17g", values[i]);
    });

    // Texts as the shortest formatter writes them, 24 bytes per slot.
    char *texts = static_cast<char *>(malloc(static_cast<__SIZE_TYPE__>(n) * 24));
    int *lens = static_cast<int *>(malloc(sizeof(int) * n));
    for (int i = 0; i < n; ++i) lens[i] = static_cast<int>(wjn_format_f64(values[i], texts + 24 * i, 24));
    double parse_ours = best(3, [&] {
        double d;
        for (int i = 0; i < n; ++i) {
            wjn_parse_f64(texts + 24 * i, lens[i], &d);
            sink = sink + static_cast<long long>(d);
        }
    });
    double parse_libc = best(3, [&] {
        for (int i = 0; i < n; ++i) sink = sink + static_cast<long long>(strtod(texts + 24 * i, nullptr));
    });

    const char unit[] = "plain text \"quoted\" \t tab \xED\x95\x9C\xEA\xB8\x80 \xF0\x9F\x98\x80 ";
    const int unit_len = static_cast<int>(sizeof unit - 1);
    const int reps = 20000;
    const int text_len = unit_len * reps;
    char *text = static_cast<char *>(malloc(static_cast<__SIZE_TYPE__>(text_len)));
    for (int r = 0; r < reps; ++r) {
        for (int k = 0; k < unit_len; ++k) text[r * unit_len + k] = unit[k];
    }
    const long long out_cap = static_cast<long long>(text_len) * 13 + 16;
    char *out = static_cast<char *>(malloc(static_cast<__SIZE_TYPE__>(out_cap)));
    double esc = best(5, [&] { sink = sink + wjn_escape(text, text_len, out, out_cap, 0); });
    double esc_ascii = best(5, [&] { sink = sink + wjn_escape(text, text_len, out, out_cap, WJN_ESCAPE_ASCII); });

    printf("format f64   wjson %6.1f ns   C runtime snprintf(%%.17g) %6.1f ns   (shortest round-trip vs 17 digits)\n",
           fmt_ours / n * 1e9, fmt_libc / n * 1e9);
    printf("parse  f64   wjson %6.1f ns   C runtime strtod          %6.1f ns\n", parse_ours / n * 1e9,
           parse_libc / n * 1e9);
    printf("escape       %.0f MB/s (UTF-8 kept)   %.0f MB/s (ASCII-only)\n", text_len / esc / 1048576.0,
           text_len / esc_ascii / 1048576.0);
    free(values);
    free(texts);
    free(lens);
    free(text);
    free(out);
    return sink == 42 ? 1 : 0;
}
