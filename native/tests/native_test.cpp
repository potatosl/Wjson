// SPDX-License-Identifier: MIT
// Tests for wjson_native and the wjson.hpp wrapper over the Rust core.
//
// Builds hosted (with a C++ standard library) or freestanding (no library
// headers at all; only printf/getenv/malloc are linked from the platform C
// runtime). Number conversions are checked against an exact in-house oracle
// built on decimal expansions, never against the C library's strtod/printf,
// whose rounding differs between runtimes.

#include "wjson.hpp"

#if WJSON_STL
#include <cstdio>
#include <cstdlib>
#else
extern "C" int printf(const char *format, ...);
extern "C" char *getenv(const char *name);
#endif

typedef unsigned int u32;
typedef unsigned long long u64;

static int failures = 0;
static int checks = 0;

#define CHECK(cond)                                                        \
    do {                                                                   \
        ++checks;                                                          \
        if (!(cond)) {                                                     \
            ++failures;                                                    \
            printf("FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);         \
        }                                                                  \
    } while (0)

static u64 bits_of(double d) {
    u64 b;
    __builtin_memcpy(&b, &d, sizeof b);
    return b;
}

static double from_bits(u64 b) {
    double d;
    __builtin_memcpy(&d, &b, sizeof d);
    return d;
}

static int text_len(const char *s) {
    int n = 0;
    while (s[n]) ++n;
    return n;
}

static bool same(const char *a, const char *b) {
    while (*a && *a == *b) {
        ++a;
        ++b;
    }
    return *a == *b;
}

static bool is_finite(double d) { return ((bits_of(d) >> 52) & 0x7FF) != 0x7FF; }

const double INF = from_bits(0x7FF0000000000000ull);

// Formats with wjn_format_f64 into a small owned buffer.
struct Text {
    char s[512];
    int n;
};

static Text fmt(double v) {
    Text t;
    int64_t n = wjn_format_f64(v, t.s, sizeof t.s);
    t.n = n < 0 ? 0 : static_cast<int>(n);
    if (n < 0) t.s[0] = '\0';
    return t;
}

static double parse_f(const char *s, int32_t *rc = nullptr) {
    double v = -12345;
    int32_t r = wjn_parse_f64(s, text_len(s), &v);
    if (rc) *rc = r;
    return v;
}

// ---- exact oracle --------------------------------------------------------------

// Big unsigned integer (independent of the one in number.cpp).
struct BigT {
    u32 d[180];
    int n;
    void set(u64 v) {
        n = 0;
        while (v) {
            d[n++] = static_cast<u32>(v);
            v >>= 32;
        }
    }
    void mul(u32 m) {
        u64 c = 0;
        for (int i = 0; i < n; ++i) {
            u64 t = static_cast<u64>(d[i]) * m + c;
            d[i] = static_cast<u32>(t);
            c = t >> 32;
        }
        if (c) d[n++] = static_cast<u32>(c);
    }
    void shl1() {
        u32 c = 0;
        for (int i = 0; i < n; ++i) {
            u32 v = d[i];
            d[i] = (v << 1) | c;
            c = v >> 31;
        }
        if (c) d[n++] = c;
    }
    // Decimal digits, most significant first; returns the count (0 for zero).
    int digits(char *out) const {
        BigT v = *this;
        char rev[1600];
        int k = 0;
        while (v.n) {
            u64 rem = 0;  // v /= 10^9
            for (int i = v.n - 1; i >= 0; --i) {
                u64 cur = (rem << 32) | v.d[i];
                v.d[i] = static_cast<u32>(cur / 1000000000u);
                rem = cur % 1000000000u;
            }
            while (v.n && v.d[v.n - 1] == 0) --v.n;
            for (int j = 0; j < 9; ++j) {
                rev[k++] = static_cast<char>('0' + rem % 10);
                rem /= 10;
            }
        }
        while (k && rev[k - 1] == '0') --k;
        for (int i = 0; i < k; ++i) out[i] = rev[k - 1 - i];
        return k;
    }
};

// Exact nonnegative decimal: value = 0.D1 D2 ... Dn * 10^point (D1 != 0),
// no trailing zeros; n == 0 means zero.
struct Dec {
    char d[1600];
    int n;
    int point;
};

static void normalize(Dec &x) {
    int lead = 0;
    while (lead < x.n && x.d[lead] == '0') ++lead;
    if (lead) {
        for (int i = lead; i < x.n; ++i) x.d[i - lead] = x.d[i];
        x.n -= lead;
        x.point -= lead;
    }
    while (x.n && x.d[x.n - 1] == '0') --x.n;
    if (x.n == 0) x.point = 0;
}

// Exact value of m * 2^q.
static Dec exact(u64 m, int q) {
    BigT b;
    b.set(m);
    if (q >= 0) {
        for (int i = 0; i < q % 31; ++i) b.shl1();
        for (int i = 0; i < q / 31; ++i) b.mul(1u << 31);
    } else {
        int k = -q;  // m * 2^q = m * 5^-q / 10^-q
        for (; k >= 13; k -= 13) b.mul(1220703125u);  // 5^13
        for (; k > 0; --k) b.mul(5);
    }
    Dec x;
    x.n = b.digits(x.d);
    x.point = q >= 0 ? x.n : x.n + q;
    normalize(x);
    return x;
}

// Exact value of a (valid) JSON number lexeme, ignoring the sign.
static Dec exact_text(const char *s, int len) {
    Dec x;
    x.n = 0;
    long long e = 0;
    int i = 0;
    if (s[i] == '-') ++i;
    for (; i < len && s[i] >= '0' && s[i] <= '9'; ++i) x.d[x.n++] = s[i];
    if (i < len && s[i] == '.') {
        for (++i; i < len && s[i] >= '0' && s[i] <= '9'; ++i) {
            x.d[x.n++] = s[i];
            --e;
        }
    }
    if (i < len && (s[i] == 'e' || s[i] == 'E')) {
        ++i;
        bool neg = false;
        if (s[i] == '+' || s[i] == '-') neg = s[i++] == '-';
        long long v = 0;
        for (; i < len; ++i) v = v * 10 + (s[i] - '0');
        e += neg ? -v : v;
    }
    x.point = static_cast<int>(x.n + e);
    normalize(x);
    return x;
}

static int compare(const Dec &a, const Dec &b) {
    if (a.n == 0 || b.n == 0) return (a.n != 0) - (b.n != 0);
    if (a.point != b.point) return a.point < b.point ? -1 : 1;
    int m = a.n > b.n ? a.n : b.n;
    for (int i = 0; i < m; ++i) {
        char x = i < a.n ? a.d[i] : '0';
        char y = i < b.n ? b.d[i] : '0';
        if (x != y) return x < y ? -1 : 1;
    }
    return 0;
}

// Keeps the first `keep` digits (relative to D1), rounding half to even.
static Dec round_keep(const Dec &x, int keep) {
    Dec r;
    r.point = x.point;
    if (x.n == 0 || keep >= x.n) {
        r = x;
        return r;
    }
    bool up;
    if (keep < 0) {
        up = false;
    } else {
        char first = x.d[keep];
        bool rest = false;
        for (int i = keep + 1; i < x.n; ++i) rest |= x.d[i] != '0';
        char last = keep > 0 ? x.d[keep - 1] : '0';
        up = first > '5' || (first == '5' && (rest || ((last - '0') & 1)));
    }
    r.n = keep < 0 ? 0 : keep;
    for (int i = 0; i < r.n; ++i) r.d[i] = x.d[i];
    if (up) {
        int i = r.n - 1;
        if (keep == 0) {
            r.d[0] = '1';
            r.n = 1;
            r.point = x.point + 1;
        } else {
            while (i >= 0 && r.d[i] == '9') r.d[i--] = '0';
            if (i >= 0) {
                ++r.d[i];
            } else {
                for (int k = r.n; k > 0; --k) r.d[k] = r.d[k - 1];
                r.d[0] = '1';
                ++r.n;
                ++r.point;
            }
        }
    }
    normalize(r);
    return r;
}

// x plus or minus one unit in digit position `keep` (x has <= keep digits).
static Dec step(const Dec &x, int keep, int dir) {
    Dec r = x;
    for (int i = r.n; i < keep; ++i) r.d[i] = '0';
    r.n = keep;
    int i = keep - 1;
    if (dir > 0) {
        while (i >= 0 && r.d[i] == '9') r.d[i--] = '0';
        if (i >= 0) {
            ++r.d[i];
        } else {
            for (int k = r.n; k > 0; --k) r.d[k] = r.d[k - 1];
            r.d[0] = '1';
            ++r.n;
            ++r.point;
        }
    } else {
        while (i >= 0 && r.d[i] == '0') r.d[i--] = '9';
        if (i >= 0) --r.d[i];
    }
    normalize(r);
    return r;
}

// Writes x as "0.DIGITSe+POINT", a valid JSON lexeme for the same value.
static int dec_text(const Dec &x, char *out) {
    int len = 0;
    out[len++] = '0';
    if (x.n == 0) {
        out[len] = '\0';
        return len;
    }
    out[len++] = '.';
    for (int i = 0; i < x.n; ++i) out[len++] = x.d[i];
    out[len++] = 'e';
    int p = x.point;
    if (p < 0) {
        out[len++] = '-';
        p = -p;
    }
    char tmp[12];
    int k = 0;
    do {
        tmp[k++] = static_cast<char>('0' + p % 10);
        p /= 10;
    } while (p);
    while (k) out[len++] = tmp[--k];
    out[len] = '\0';
    return len;
}

static void decompose(u64 bits, u64 *m, int *q) {
    u64 mant = bits & ((1ull << 52) - 1);
    int be = static_cast<int>((bits >> 52) & 0x7FF);
    *m = be == 0 ? mant : (mant | (1ull << 52));
    *q = be == 0 ? -1074 : be - 1075;
}

// Is `x` (with status rc) the correctly rounded value of lexeme s?
static bool parse_is_exact(const char *s, int len, double x, int32_t rc) {
    const bool negative = s[0] == '-';
    const u64 bits = bits_of(x);
    if (((bits >> 63) != 0) != negative) return false;
    const u64 ax = bits & ~(1ull << 63);
    Dec v = exact_text(s, len);
    if (v.n == 0) return rc == WJN_OK && ax == 0;
    if (rc == WJN_ERR_RANGE) {
        if (ax == 0x7FF0000000000000ull) return compare(v, exact((1ull << 54) - 1, 970)) >= 0;
        if (ax == 0) return compare(v, exact(1, -1075)) <= 0;
        return false;
    }
    if (rc != WJN_OK || ax == 0 || ax >= 0x7FF0000000000000ull) return false;
    u64 m;
    int q;
    decompose(ax, &m, &q);
    const bool even = (m & 1) == 0;
    Dec hi = exact(2 * m + 1, q - 1);
    Dec lo = ((ax & ((1ull << 52) - 1)) == 0 && (ax >> 52) > 1) ? exact(4 * m - 1, q - 2) : exact(2 * m - 1, q - 1);
    int cl = compare(v, lo);
    int ch = compare(v, hi);
    return (cl > 0 || (cl == 0 && even)) && (ch < 0 || (ch == 0 && even));
}

static bool round_trips(const Dec &c, u64 ax) {
    char buf[1700];
    int len = dec_text(c, buf);
    double back = 0;
    return wjn_parse_f64(buf, len, &back) == WJN_OK && bits_of(back) == ax;
}

// Is s the shortest round-trip form of x, closest to x, ties to even?
static bool shortest_is_exact(const char *s, int len, double x) {
    const u64 ax = bits_of(x) & ~(1ull << 63);
    u64 m;
    int q;
    decompose(ax, &m, &q);
    Dec e = exact(m, q);
    Dec got = exact_text(s, len);
    // Integers from 2^53 up that print in fixed form show all their digits
    // (as std::to_chars does), so they equal the exact value.
    bool plain = true;
    for (int i = 0; i < len; ++i) plain &= s[i] != '.' && s[i] != 'e';
    if (plain && ax >= 0x4340000000000000ull) return compare(got, e) == 0;
    for (int n = 1; n <= 17; ++n) {
        Dec c = round_keep(e, n);
        if (round_trips(c, ax)) return compare(got, c) == 0;
        int side = compare(c, e) < 0 ? 1 : -1;
        Dec other = step(c, n + (c.point - e.point), side);
        if (round_trips(other, ax)) return compare(got, other) == 0;
    }
    return false;
}

// Expected printf("%.*f") text from the exact expansion.
static int fixed_expected(double v, int prec, char *out) {
    const u64 bits = bits_of(v);
    u64 m;
    int q;
    decompose(bits & ~(1ull << 63), &m, &q);
    Dec e = exact(m, q);
    Dec r = round_keep(e, e.point + prec);
    int len = 0;
    if (bits >> 63) out[len++] = '-';
    if (r.n == 0 || r.point <= 0) {
        out[len++] = '0';
    } else {
        for (int i = 0; i < r.point; ++i) out[len++] = i < r.n ? r.d[i] : '0';
    }
    if (prec > 0) {
        out[len++] = '.';
        for (int i = 0; i < prec; ++i) {
            int idx = (r.n == 0 ? 0 : r.point) + i;
            out[len++] = (r.n != 0 && idx >= 0 && idx < r.n) ? r.d[idx] : '0';
        }
    }
    out[len] = '\0';
    return len;
}

// ---- fixed cases ---------------------------------------------------------------

static void test_numbers() {
    CHECK(same(fmt(0.1).s, "0.1"));
    CHECK(same(fmt(100.0).s, "100"));
    CHECK(same(fmt(-0.0).s, "-0"));
    CHECK(same(fmt(1e21).s, "1e+21"));
    CHECK(same(fmt(1e5).s, "1e+05"));
    CHECK(same(fmt(123000.0).s, "123000"));
    CHECK(same(fmt(5e-324).s, "5e-324"));
    CHECK(same(fmt(1.0 / 3.0).s, "0.3333333333333333"));
    CHECK(same(fmt(9007199254740993.0 * 2).s, "18014398509481984"));
    CHECK(same(fmt(1.7976931348623157e308).s, "1.7976931348623157e+308"));
    CHECK(wjn_format_f64(from_bits(0x7FF8000000000000ull), nullptr, 0) == WJN_ERR_NONFINITE);
    CHECK(wjn_format_f64(INF, nullptr, 0) == WJN_ERR_NONFINITE);

    // Sizing call then fill call.
    CHECK(wjn_format_f64(3.25, nullptr, 0) == 4);
    char small[4];
    CHECK(wjn_format_f64(3.25, small, sizeof small) == 4);  // does not fit with NUL

    char buf[512];
    CHECK(wjn_format_f64_fixed(2.0 / 3.0, 3, buf, sizeof buf) == 5 && same(buf, "0.667"));
    CHECK(wjn_format_f64_fixed(2.5, 0, buf, sizeof buf) == 1 && same(buf, "2"));
    CHECK(wjn_format_f64_fixed(-0.001, 2, buf, sizeof buf) == 5 && same(buf, "-0.00"));
    CHECK(wjn_format_f64_fixed(0.125, 2, buf, sizeof buf) == 4 && same(buf, "0.12"));
    CHECK(wjn_format_f64_fixed(1e20, 1, buf, sizeof buf) == 23 && same(buf, "100000000000000000000.0"));
    CHECK(wjn_format_i64(INT64_MIN, buf, sizeof buf) == 20 && same(buf, "-9223372036854775808"));

    int32_t rc = 0;
    CHECK(parse_f("0.1") == 0.1);
    CHECK(parse_f("-2.5e3") == -2500.0);
    CHECK(parse_f("1e400", &rc) == INF && rc == WJN_ERR_RANGE);
    CHECK(parse_f("-1e400", &rc) == -INF && rc == WJN_ERR_RANGE);
    CHECK(parse_f("1e-400", &rc) == 0.0 && rc == WJN_ERR_RANGE);
    CHECK(parse_f("0.000001e-400", &rc) == 0.0 && rc == WJN_ERR_RANGE);
    CHECK(parse_f("2.4703282292062327e-324", &rc) == 0.0 && rc == WJN_ERR_RANGE);  // just below half
    CHECK(parse_f("2.4703282292062328e-324", &rc) == 5e-324 && rc == WJN_OK);    // just above half
    CHECK(parse_f("1.7976931348623158e308", &rc) == 1.7976931348623157e308 && rc == WJN_OK);
    CHECK(parse_f("1.7976931348623159e308", &rc) == INF && rc == WJN_ERR_RANGE);
    CHECK(bits_of(parse_f("-0.0e9", &rc)) == (1ull << 63) && rc == WJN_OK);
    parse_f("01", &rc);
    CHECK(rc == WJN_ERR_INVALID);
    parse_f("1.", &rc);
    CHECK(rc == WJN_ERR_INVALID);
    parse_f("+1", &rc);
    CHECK(rc == WJN_ERR_INVALID);

    // Round trip: shortest output must parse back to the same bits.
    const double samples[] = {0.1, 1.0 / 3.0, 123456789.125, 2.2250738585072014e-308, 1.7976931348623157e308,
                              -7.5e-10};
    for (double d : samples) {
        Text t = fmt(d);
        double back = parse_f(t.s, &rc);
        CHECK(rc == WJN_OK && back == d);
    }

    int64_t i = 0;
    CHECK(wjn_parse_i64("9223372036854775807", 19, &i) == WJN_OK && i == INT64_MAX);
    CHECK(wjn_parse_i64("9223372036854775808", 19, &i) == WJN_ERR_RANGE);
    CHECK(wjn_parse_i64("-9223372036854775808", 20, &i) == WJN_OK && i == INT64_MIN);
    CHECK(wjn_parse_i64("-42", 3, &i) == WJN_OK && i == -42);
    CHECK(wjn_parse_i64("1.0", 3, &i) == WJN_ERR_INVALID);
    CHECK(wjn_parse_i64("1e2", 3, &i) == WJN_ERR_INVALID);
}

static void test_escape() {
    using wjson::escape;
    using wjson::Str;
    CHECK(escape("plain") == "\"plain\"");
    CHECK(escape("a\"b\\c") == "\"a\\\"b\\\\c\"");
    CHECK(escape("\n\t\r\b\f") == "\"\\n\\t\\r\\b\\f\"");
    CHECK(escape(Str("\x01\x1f\x7f", 3)) == "\"\\u0001\\u001f\\u007f\"");
    CHECK(escape("é😀") == "\"é😀\"");
    CHECK(escape("é😀", WJN_ESCAPE_QUOTES | WJN_ESCAPE_ASCII) == "\"\\u00e9\\ud83d\\ude00\"");
    CHECK(escape("<a&'>", WJN_ESCAPE_HTML) == "\\u003ca\\u0026\\u0027\\u003e");
    CHECK(escape(Str("nul\0x", 5), 0) == "nul\\u0000x");
    CHECK(wjn_escape("\xff", 1, nullptr, 0, 0) == WJN_ERR_INVALID);
    CHECK(wjn_escape("\xed\xa0\x80", 3, nullptr, 0, 0) == WJN_ERR_INVALID);  // surrogate
    CHECK(wjn_escape("\xc0\xaf", 2, nullptr, 0, 0) == WJN_ERR_INVALID);      // overlong

    CHECK(wjn_utf8_check("ok", 2) == -1);
    CHECK(wjn_utf8_check("ab\xe2\x82", 4) == 2);
    CHECK(wjn_utf8_count("héllo😀", text_len("héllo😀")) == 6);
}

static void test_document() {
    auto doc = wjson::Document::parse(R"({
        "name": "wave",
        "tags": ["rust", "c++", "wave"],
        "ver": {"major": 0, "minor": 2.5},
        "ok": true, "none": null
    })");
    CHECK(doc.ok());
    wjson::Value root = doc.root();
    CHECK(root.is_object() && root.size() == 5);
    CHECK(root["name"].as_string() == wjson::Str("wave"));
    CHECK(root["tags"].size() == 3);
    CHECK(root["tags"][1].as_string() == wjson::Str("c++"));
    CHECK(root["ver"]["major"].as_int() == int64_t{0});
    CHECK(root["ver"]["minor"].as_double() == 2.5);
    CHECK(!root["ver"]["minor"].as_int().has_value());
    CHECK(root["ok"].as_bool() == true);
    CHECK(root["none"].is_null());
    CHECK(!root["missing"]["deeper"][7].valid());
    CHECK(root.at_pointer("/tags/2").as_string() == wjson::Str("wave"));
    CHECK(root.key_at(0) == "name");

    char joined[64];
    int len = 0;
    for (wjson::Value tag : root["tags"]) {
        wjson::Str s = *tag.as_string();
        for (char c : s) joined[len++] = c;
        joined[len++] = ';';
    }
    joined[len] = '\0';
    CHECK(same(joined, "rust;c++;wave;"));

    auto bad = wjson::Document::parse("[1, 2,\n 3 4]");
    CHECK(!bad);
    CHECK(bad.error().code == WJ_ERR_UNEXPECTED_CHAR);
    CHECK(bad.error().line == 2 && bad.error().column == 4);
    CHECK(bad.error().message == "unexpected character");
    CHECK(!bad.root().valid());

    wjson::ParseOptions relaxed;
    relaxed.comments = true;
    relaxed.trailing_commas = true;
    auto loose = wjson::Document::parse("/* cfg */ {\"a\": [1,],}", relaxed);
    CHECK(loose.ok() && loose.root()["a"].size() == 1);

    wjson::Document moved = static_cast<wjson::Document &&>(doc);
    CHECK(moved.ok() && moved.root()["name"].valid());

    CHECK(wjson::format_double(0.5) == "0.5");
}

// ---- randomized checks against the exact oracle --------------------------------

struct Rng {
    u64 s;
    u64 next() {
        u64 x = s;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        s = x;
        return x * 0x2545F4914F6CDD1DULL;
    }
    u64 below(u64 n) { return n ? next() % n : 0; }
};

static long fuzz_iterations() {
    const char *env = getenv("WJSON_FUZZ_ITERS");
    long n = 0;
    for (; env && *env >= '0' && *env <= '9'; ++env) n = n * 10 + (*env - '0');
    return n > 0 ? n : 50000;
}

static int append_int(char *out, int v) {
    int len = 0;
    if (v < 0) {
        out[len++] = '-';
        v = -v;
    }
    char tmp[12];
    int k = 0;
    do {
        tmp[k++] = static_cast<char>('0' + v % 10);
        v /= 10;
    } while (v);
    while (k) out[len++] = tmp[--k];
    return len;
}

static void fuzz_numbers(long iters) {
    Rng rng{0x853C49E6748FEA9BULL};
    long shortest_fail = 0, parse_fail = 0, tie_fail = 0, fixed_fail = 0, int_fail = 0;
    static char buf[2000], ref[2000];
    for (long i = 0; i < iters; ++i) {
        // 1) Shortest formatting: round-trips, no shorter form does, and it is
        //    the closest such form (ties to even). Mix in subnormals and
        //    integers.
        u64 bits = rng.next();
        if (i % 4 == 1) bits &= 0x800FFFFFFFFFFFFFull;
        if (i % 4 == 2) bits = bits_of(static_cast<double>(static_cast<long long>(rng.next() >> rng.below(64))));
        double d = from_bits(bits);
        if (is_finite(d) && (bits & ~(1ull << 63)) != 0) {
            int64_t n = wjn_format_f64(d, buf, sizeof buf);
            if (n <= 0 || !shortest_is_exact(buf, static_cast<int>(n), d)) {
                if (++shortest_fail < 5) printf("shortest: %s\n", buf);
            }
        }

        // 2) Parsing random decimals is correctly rounded.
        int len = 0;
        if (rng.below(2)) buf[len++] = '-';
        int digits = 1 + static_cast<int>(rng.below(i % 64 == 0 ? 900 : 25));
        buf[len++] = static_cast<char>('1' + rng.below(9));
        for (int k = 1; k < digits; ++k) buf[len++] = static_cast<char>('0' + rng.below(10));
        if (rng.below(2)) {
            buf[len++] = '.';
            int frac = 1 + static_cast<int>(rng.below(20));
            for (int k = 0; k < frac; ++k) buf[len++] = static_cast<char>('0' + rng.below(10));
        }
        if (rng.below(3)) {
            buf[len++] = 'e';
            len += append_int(buf + len, static_cast<int>(rng.below(720)) - 360);
        }
        buf[len] = '\0';
        double ours = 0;
        int32_t rc = wjn_parse_f64(buf, len, &ours);
        if (!parse_is_exact(buf, len, ours, rc)) {
            if (++parse_fail < 5) printf("parse: %s\n", buf);
        }

        // 3) Exact halfway points between neighbouring doubles, and values a
        //    hair above or below them.
        u64 hb = rng.next() & 0x7FEFFFFFFFFFFFFFull;
        if (i % 3 == 0) hb >>= rng.below(12);  // reach subnormals
        u64 m;
        int q;
        decompose(hb, &m, &q);
        Dec mid = exact(2 * m + 1, q - 1);
        int mode = static_cast<int>(rng.below(3));
        if (mode == 1) {
            mid.d[mid.n++] = '1';
        } else if (mode == 2) {
            mid = step(mid, mid.n, -1);
            mid.d[mid.n++] = '9';
            mid.d[mid.n++] = '9';
        }
        len = dec_text(mid, buf);
        rc = wjn_parse_f64(buf, len, &ours);
        if (!parse_is_exact(buf, len, ours, rc)) {
            if (++tie_fail < 5) printf("halfway: %s\n", buf);
        }

        // 4) Fixed notation is the exact expansion rounded half to even.
        double v = static_cast<double>(static_cast<long long>(rng.next() >> (rng.below(50) + 10))) /
                   static_cast<double>(1 + rng.below(1000));
        if (i % 8 == 0) v = from_bits(rng.next() & 0x3FFFFFFFFFFFFFFFull);  // tiny to huge
        if (rng.below(2)) v = -v;
        int prec = static_cast<int>(rng.below(18));
        int64_t n = wjn_format_f64_fixed(v, prec, buf, sizeof buf);
        fixed_expected(v, prec, ref);
        if (n < 0 || !same(buf, ref)) {
            if (++fixed_fail < 5) printf("fixed: %s vs %s\n", buf, ref);
        }

        // 5) Integers round-trip.
        int64_t iv = static_cast<int64_t>(rng.next());
        int64_t back = 0;
        n = wjn_format_i64(iv, buf, sizeof buf);
        if (wjn_parse_i64(buf, n, &back) != WJN_OK || back != iv) ++int_fail;
    }
    CHECK(shortest_fail == 0);
    CHECK(parse_fail == 0);
    CHECK(tie_fail == 0);
    CHECK(fixed_fail == 0);
    CHECK(int_fail == 0);
}

static int append_utf8(char *out, u32 cp) {
    if (cp < 0x80) {
        out[0] = static_cast<char>(cp);
        return 1;
    }
    if (cp < 0x800) {
        out[0] = static_cast<char>(0xC0 | (cp >> 6));
        out[1] = static_cast<char>(0x80 | (cp & 0x3F));
        return 2;
    }
    if (cp < 0x10000) {
        out[0] = static_cast<char>(0xE0 | (cp >> 12));
        out[1] = static_cast<char>(0x80 | ((cp >> 6) & 0x3F));
        out[2] = static_cast<char>(0x80 | (cp & 0x3F));
        return 3;
    }
    out[0] = static_cast<char>(0xF0 | (cp >> 18));
    out[1] = static_cast<char>(0x80 | ((cp >> 12) & 0x3F));
    out[2] = static_cast<char>(0x80 | ((cp >> 6) & 0x3F));
    out[3] = static_cast<char>(0x80 | (cp & 0x3F));
    return 4;
}

static void fuzz_escape(long iters) {
    Rng rng{0xDA942042E4DD58B5ULL};
    long roundtrip_fail = 0, validity_fail = 0;
    char text[64];
    char bytes[16];
    for (long i = 0; i < iters; ++i) {
        // Random valid UTF-8 text from every plane (no surrogates).
        int tlen = 0;
        int chars = static_cast<int>(rng.below(12));
        for (int k = 0; k < chars; ++k) {
            u32 cp;
            switch (rng.below(4)) {
                case 0: cp = static_cast<u32>(rng.below(0x80)); break;
                case 1: cp = 0x80 + static_cast<u32>(rng.below(0x780)); break;
                case 2: cp = 0x800 + static_cast<u32>(rng.below(0xF800)); break;
                default: cp = 0x10000 + static_cast<u32>(rng.below(0x100000)); break;
            }
            if (cp >= 0xD800 && cp <= 0xDFFF) cp = 0xFFFD;
            tlen += append_utf8(text + tlen, cp);
        }
        wjson::Str original(text, static_cast<wjson::Str::size_type>(tlen));
        u32 flags = static_cast<u32>(rng.below(4)) | WJN_ESCAPE_QUOTES;
        wjson::String quoted = wjson::escape(original, flags);
        // The escaped form must be valid JSON that decodes back to the text.
        auto doc = wjson::Document::parse(quoted);
        auto decoded = doc.root().as_string();
        if (!doc || !decoded || *decoded != original) {
            if (++roundtrip_fail < 5) printf("escape roundtrip failed (%d bytes)\n", tlen);
        }
        if ((flags & WJN_ESCAPE_ASCII) != 0) {
            for (char c : wjson::Str(quoted)) {
                if (static_cast<unsigned char>(c) >= 0x80) {
                    ++roundtrip_fail;
                    break;
                }
            }
        }

        // Random bytes: escape refuses exactly the inputs utf8_check flags.
        int n = static_cast<int>(rng.below(10));
        for (int k = 0; k < n; ++k) bytes[k] = static_cast<char>(rng.below(256));
        int64_t esc = wjn_escape(bytes, n, nullptr, 0, 0);
        int64_t bad_at = wjn_utf8_check(bytes, n);
        if ((esc < 0) != (bad_at >= 0)) ++validity_fail;
    }
    CHECK(roundtrip_fail == 0);
    CHECK(validity_fail == 0);
}

// In a freestanding build `main` is an ordinary function, so give it C
// linkage for the runtime's startup code to find.
#if !__STDC_HOSTED__
extern "C"
#endif
int main() {
    test_numbers();
    test_escape();
    test_document();
    long iters = fuzz_iterations();
    fuzz_numbers(iters);
    fuzz_escape(iters);
    printf("native_test: %s build, randomized checks over %ld iterations\n",
           WJSON_STL ? "hosted" : "freestanding", iters);
    printf("native_test: %d/%d checks passed\n", checks - failures, checks);
    return failures == 0 ? 0 : 1;
}
