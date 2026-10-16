// SPDX-License-Identifier: MIT
// JSON string escaping and UTF-8 utilities for wjson. Freestanding: needs
// no C or C++ library headers.

#include "wjson_native.h"

namespace {

// Decodes one UTF-8 sequence at p (< e). Returns its length (1..4) and the
// code point, or 0 for an invalid/overlong/surrogate/truncated sequence.
int decode_utf8(const unsigned char *p, const unsigned char *e, uint32_t *cp) {
    unsigned char b0 = p[0];
    if (b0 < 0x80) {
        *cp = b0;
        return 1;
    }
    int n;
    uint32_t min;
    uint32_t c;
    if ((b0 & 0xE0) == 0xC0) {
        n = 2; min = 0x80; c = b0 & 0x1F;
    } else if ((b0 & 0xF0) == 0xE0) {
        n = 3; min = 0x800; c = b0 & 0x0F;
    } else if ((b0 & 0xF8) == 0xF0) {
        n = 4; min = 0x10000; c = b0 & 0x07;
    } else {
        return 0;
    }
    if (e - p < n) return 0;
    for (int i = 1; i < n; ++i) {
        if ((p[i] & 0xC0) != 0x80) return 0;
        c = (c << 6) | (p[i] & 0x3F);
    }
    if (c < min || c > 0x10FFFF || (c >= 0xD800 && c <= 0xDFFF)) return 0;
    *cp = c;
    return n;
}

// Output sink that counts everything but only writes while it fits, so one
// pass both sizes and fills the destination.
class Sink {
public:
    Sink(char *dst, int64_t cap) : dst_(dst), cap_(dst ? cap : 0) {}

    void put(char c) {
        if (len_ < cap_) dst_[len_] = c;
        ++len_;
    }

    void put(const char *s, int64_t n) {
        for (int64_t i = 0; i < n; ++i) put(s[i]);
    }

    void unicode_escape(uint32_t unit) {
        static const char hex[] = "0123456789abcdef";
        char buf[6] = {'\\', 'u', hex[(unit >> 12) & 0xF], hex[(unit >> 8) & 0xF],
                       hex[(unit >> 4) & 0xF], hex[unit & 0xF]};
        put(buf, 6);
    }

    int64_t finish() {
        if (len_ < cap_) dst_[len_] = '\0';
        return len_;
    }

private:
    char *dst_;
    int64_t cap_;
    int64_t len_ = 0;
};

}  // namespace

extern "C" int64_t wjn_escape(const char *src, int64_t len, char *dst, int64_t cap,
                              uint32_t flags) noexcept {
    if (len < 0 || (len > 0 && src == nullptr) || cap < 0) return WJN_ERR_INVALID;
    const bool ascii = (flags & WJN_ESCAPE_ASCII) != 0;
    const bool html = (flags & WJN_ESCAPE_HTML) != 0;
    const bool quotes = (flags & WJN_ESCAPE_QUOTES) != 0;

    Sink out(dst, cap);
    if (quotes) out.put('"');

    const auto *p = reinterpret_cast<const unsigned char *>(src);
    const auto *e = p + len;
    while (p < e) {
        unsigned char b = *p;
        if (b >= 0x80) {
            uint32_t cp = 0;
            int n = decode_utf8(p, e, &cp);
            if (n == 0) return WJN_ERR_INVALID;
            if (!ascii) {
                out.put(reinterpret_cast<const char *>(p), n);
            } else if (cp < 0x10000) {
                out.unicode_escape(cp);
            } else {
                uint32_t v = cp - 0x10000;
                out.unicode_escape(0xD800 + (v >> 10));
                out.unicode_escape(0xDC00 + (v & 0x3FF));
            }
            p += n;
            continue;
        }
        switch (b) {
            case '"': out.put("\\\"", 2); break;
            case '\\': out.put("\\\\", 2); break;
            case '\b': out.put("\\b", 2); break;
            case '\f': out.put("\\f", 2); break;
            case '\n': out.put("\\n", 2); break;
            case '\r': out.put("\\r", 2); break;
            case '\t': out.put("\\t", 2); break;
            case '<': case '>': case '&': case '\'':
                if (html) {
                    out.unicode_escape(b);
                } else {
                    out.put(static_cast<char>(b));
                }
                break;
            default:
                if (b < 0x20 || b == 0x7F) {
                    out.unicode_escape(b);
                } else {
                    out.put(static_cast<char>(b));
                }
        }
        ++p;
    }
    if (quotes) out.put('"');
    return out.finish();
}

extern "C" int64_t wjn_utf8_check(const char *src, int64_t len) noexcept {
    if (len <= 0 || src == nullptr) return -1;
    const auto *base = reinterpret_cast<const unsigned char *>(src);
    const auto *p = base;
    const auto *e = base + len;
    while (p < e) {
        uint32_t cp;
        int n = decode_utf8(p, e, &cp);
        if (n == 0) return p - base;
        p += n;
    }
    return -1;
}

extern "C" int64_t wjn_utf8_count(const char *src, int64_t len) noexcept {
    if (len < 0 || (len > 0 && src == nullptr)) return WJN_ERR_INVALID;
    const auto *p = reinterpret_cast<const unsigned char *>(src);
    const auto *e = p + len;
    int64_t count = 0;
    while (p < e) {
        uint32_t cp;
        int n = decode_utf8(p, e, &cp);
        if (n == 0) return WJN_ERR_INVALID;
        p += n;
        ++count;
    }
    return count;
}
