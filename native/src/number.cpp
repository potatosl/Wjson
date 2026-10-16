// SPDX-License-Identifier: MIT
// Number conversion for wjson. Freestanding: no C or C++ library headers,
// so it builds with a bare compiler (clang -ffreestanding -nostdlibinc).
//
//   parse   exact: a fast path for short inputs (Clinger), otherwise big
//           integer arithmetic that yields the correctly rounded double
//           (round to nearest, ties to even) for any number of digits.
//   format  shortest round-trip digits (Steele & White / Burger & Dybvig
//           free-format algorithm on big integers), laid out like C++17
//           std::to_chars: fixed or scientific, whichever is shorter.
//   fixed   exact decimal expansion rounded half to even, like printf("%.*f").

#include "wjson_native.h"

namespace {

typedef unsigned int u32;
typedef unsigned long long u64;

const u64 MANT_MASK = (1ull << 52) - 1;
const u64 SIGN_BIT = 1ull << 63;

inline u64 bits_of(double d) {
    u64 b;
    __builtin_memcpy(&b, &d, sizeof b);
    return b;
}

inline double from_bits(u64 b) {
    double d;
    __builtin_memcpy(&d, &b, sizeof d);
    return d;
}

inline bool finite_bits(u64 b) { return ((b >> 52) & 0x7FF) != 0x7FF; }

inline int clz64(u64 v) { return __builtin_clzll(v); }

typedef unsigned __int128 u128;

// (hi:lo) / d for hi < d, i.e. a quotient below 2^64. Uses the CPU's
// 128-by-64 division so no compiler runtime helper (__udivti3) is needed.
inline u64 div128(u64 hi, u64 lo, u64 d, u64 *rem) {
#if defined(__x86_64__) || defined(_M_X64)
    u64 q, r;
    __asm__("divq %4" : "=a"(q), "=d"(r) : "a"(lo), "d"(hi), "rm"(d));
    *rem = r;
    return q;
#else
    u64 q = 0;  // portable shift-subtract
    for (int i = 0; i < 64; ++i) {
        const bool top = (hi >> 63) != 0;
        hi = (hi << 1) | (lo >> 63);
        lo <<= 1;
        q <<= 1;
        if (top || hi >= d) {
            hi -= d;
            q |= 1;
        }
    }
    *rem = hi;
    return q;
#endif
}

const u32 POW10_U32[10] = {1u, 10u, 100u, 1000u, 10000u, 100000u, 1000000u, 10000000u, 100000000u, 1000000000u};

// Exactly representable powers of ten for the fast path.
const double POW10_F64[23] = {1e0,  1e1,  1e2,  1e3,  1e4,  1e5,  1e6,  1e7,  1e8,  1e9,  1e10, 1e11,
                              1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18, 1e19, 1e20, 1e21, 1e22};

// ---------------------------------------------------------------------------
// Big unsigned integer with fixed capacity (little-endian 32-bit limbs).
// The largest value any routine builds is under 4,000 bits (a 781-digit
// significand shifted against 10^1100); 6,400 bits leaves a wide margin.
// ---------------------------------------------------------------------------
struct Big {
    enum { CAP = 200 };
    u32 d[CAP];
    int n;  // limbs in use; d[n-1] != 0 unless n == 0

    void set(u64 v) {
        n = 0;
        while (v) {
            d[n++] = static_cast<u32>(v);
            v >>= 32;
        }
    }

    bool is_zero() const { return n == 0; }

    int bits() const { return n == 0 ? 0 : (n - 1) * 32 + (32 - __builtin_clz(d[n - 1])); }

    void mul_small(u32 m) {
        u64 carry = 0;
        for (int i = 0; i < n; ++i) {
            u64 t = static_cast<u64>(d[i]) * m + carry;
            d[i] = static_cast<u32>(t);
            carry = t >> 32;
        }
        if (carry) d[n++] = static_cast<u32>(carry);
    }

    void add_small(u32 a) {
        u64 carry = a;
        for (int i = 0; i < n && carry; ++i) {
            u64 t = static_cast<u64>(d[i]) + carry;
            d[i] = static_cast<u32>(t);
            carry = t >> 32;
        }
        if (carry) d[n++] = static_cast<u32>(carry);
    }

    void mul_pow10(int k) {
        while (k >= 9) {
            mul_small(1000000000u);
            k -= 9;
        }
        if (k > 0) mul_small(POW10_U32[k]);
    }

    void shl(int s) {
        if (n == 0 || s <= 0) return;
        int words = s / 32;
        int bits = s % 32;
        if (bits) {
            u32 carry = 0;
            for (int i = 0; i < n; ++i) {
                u32 v = d[i];
                d[i] = (v << bits) | carry;
                carry = v >> (32 - bits);
            }
            if (carry) d[n++] = carry;
        }
        if (words) {
            for (int i = n - 1; i >= 0; --i) d[i + words] = d[i];
            for (int i = 0; i < words; ++i) d[i] = 0;
            n += words;
        }
    }

    void shr1() {
        for (int i = 0; i < n; ++i) {
            d[i] = (d[i] >> 1) | (i + 1 < n ? d[i + 1] << 31 : 0);
        }
        if (n && d[n - 1] == 0) --n;
    }

    void shr(int s) {
        if (s <= 0 || n == 0) return;
        int words = s / 32;
        int bits = s % 32;
        if (words >= n) {
            n = 0;
            return;
        }
        for (int i = 0; i + words < n; ++i) d[i] = d[i + words];
        n -= words;
        if (bits) {
            for (int i = 0; i < n; ++i) {
                d[i] = (d[i] >> bits) | (i + 1 < n ? d[i + 1] << (32 - bits) : 0);
            }
        }
        while (n && d[n - 1] == 0) --n;
    }

    // Value of bit `i`.
    bool bit(int i) const {
        int w = i / 32;
        return w < n && ((d[w] >> (i % 32)) & 1u);
    }

    // True when any bit below `i` is set.
    bool any_below(int i) const {
        int w = i / 32;
        for (int k = 0; k < w && k < n; ++k) {
            if (d[k]) return true;
        }
        return w < n && (i % 32) != 0 && (d[w] & ((1u << (i % 32)) - 1)) != 0;
    }

    // Bits [from, from + 64) as an integer (bits past the top read as 0).
    u64 extract64(int from) const {
        u64 v = 0;
        for (int k = 0; k < 64; ++k) {
            if (bit(from + k)) v |= 1ull << k;
        }
        return v;
    }
};

// Value of a Big known to be below 2^128.
inline u128 to_u128(const Big &b) {
    u128 v = 0;
    for (int i = b.n - 1; i >= 0; --i) v = (v << 32) | b.d[i];
    return v;
}

int cmp(const Big &a, const Big &b) {
    if (a.n != b.n) return a.n < b.n ? -1 : 1;
    for (int i = a.n - 1; i >= 0; --i) {
        if (a.d[i] != b.d[i]) return a.d[i] < b.d[i] ? -1 : 1;
    }
    return 0;
}

// a -= b, requires a >= b.
void sub(Big &a, const Big &b) {
    long long borrow = 0;
    for (int i = 0; i < a.n; ++i) {
        long long t = static_cast<long long>(a.d[i]) - (i < b.n ? b.d[i] : 0) - borrow;
        borrow = t < 0;
        a.d[i] = static_cast<u32>(t);
    }
    while (a.n && a.d[a.n - 1] == 0) --a.n;
}

// Compares a + b with c.
int cmp_sum(const Big &a, const Big &b, const Big &c) {
    const Big &x = a.n >= b.n ? a : b;
    const Big &y = a.n >= b.n ? b : a;
    Big s;
    u64 carry = 0;
    for (int i = 0; i < x.n; ++i) {
        u64 t = static_cast<u64>(x.d[i]) + (i < y.n ? y.d[i] : 0) + carry;
        s.d[i] = static_cast<u32>(t);
        carry = t >> 32;
    }
    s.n = x.n;
    if (carry) s.d[s.n++] = static_cast<u32>(carry);
    return cmp(s, c);
}

// ---------------------------------------------------------------------------
// Grammar and small helpers
// ---------------------------------------------------------------------------

inline bool is_digit(char c) { return c >= '0' && c <= '9'; }

// RFC 8259 number grammar; reports whether the lexeme is a plain integer.
bool is_json_number(const char *s, const char *e, bool *integer) {
    const char *p = s;
    *integer = true;
    if (p < e && *p == '-') ++p;
    if (p == e) return false;
    if (*p == '0') {
        ++p;
    } else if (*p >= '1' && *p <= '9') {
        while (p < e && is_digit(*p)) ++p;
    } else {
        return false;
    }
    if (p < e && *p == '.') {
        *integer = false;
        ++p;
        const char *d = p;
        while (p < e && is_digit(*p)) ++p;
        if (p == d) return false;
    }
    if (p < e && (*p == 'e' || *p == 'E')) {
        *integer = false;
        ++p;
        if (p < e && (*p == '+' || *p == '-')) ++p;
        const char *d = p;
        while (p < e && is_digit(*p)) ++p;
        if (p == d) return false;
    }
    return p == e;
}

// Copies [src, src+len) into dst honoring the "needed length" contract.
int64_t emit(const char *src, int64_t len, char *dst, int64_t cap) {
    if (dst != nullptr && cap > len) {
        for (int64_t i = 0; i < len; ++i) dst[i] = src[i];
        dst[len] = '\0';
    }
    return len;
}

// Writes the decimal digits of v (no sign) and returns their count.
int u64_digits(u64 v, char *out) {
    char tmp[20];
    int n = 0;
    do {
        tmp[n++] = static_cast<char>('0' + v % 10);
        v /= 10;
    } while (v);
    for (int i = 0; i < n; ++i) out[i] = tmp[n - 1 - i];
    return n;
}

// Decimal digits of a nonzero big integer (most significant first).
int big_digits(Big v, char *out) {
    char rev[800];
    int n = 0;
    while (!v.is_zero()) {
        u64 rem = 0;  // v /= 10^9
        for (int i = v.n - 1; i >= 0; --i) {
            u64 cur = (rem << 32) | v.d[i];
            v.d[i] = static_cast<u32>(cur / 1000000000u);
            rem = cur % 1000000000u;
        }
        while (v.n && v.d[v.n - 1] == 0) --v.n;
        for (int k = 0; k < 9; ++k) {
            rev[n++] = static_cast<char>('0' + rem % 10);
            rem /= 10;
        }
    }
    while (n > 1 && rev[n - 1] == '0') --n;
    for (int i = 0; i < n; ++i) out[i] = rev[n - 1 - i];
    return n;
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

enum { MAX_DIGITS = 780 };  // >= 768 keeps rounding exact (see parse_double)

// Rounds q * 2^exp2 (+ a nonzero tail when `sticky`) to the nearest double,
// ties to even. Returns WJN_OK, or WJN_ERR_RANGE for +-inf / +-0 results.
int32_t round_to_double(u64 q, int exp2, bool sticky, bool negative, double *out) {
    const u64 sign = negative ? SIGN_BIT : 0;
    const u64 inf = sign | (0x7FFull << 52);
    int lz = clz64(q);
    q <<= lz;
    exp2 -= lz;
    int e = exp2 + 63;  // exponent of the leading bit
    if (e > 1023) {
        *out = from_bits(inf);
        return WJN_ERR_RANGE;
    }
    const int keep = e >= -1022 ? 53 : e + 1075;  // significant bits that survive
    if (keep < 0) {
        *out = from_bits(sign);
        return WJN_ERR_RANGE;
    }
    u64 m;
    if (keep == 0) {
        // Value in [2^-1075, 2^-1074): at or above half the smallest subnormal.
        m = (q == (1ull << 63) && !sticky) ? 0 : 1;
    } else {
        const int shift = 64 - keep;  // 11 for normal numbers
        m = q >> shift;
        const u64 rem = q & ((1ull << shift) - 1);
        const u64 half = 1ull << (shift - 1);
        if (rem > half || (rem == half && (sticky || (m & 1)))) ++m;
    }
    u64 bits;
    if (keep == 53) {
        if (m == (1ull << 53)) {
            m >>= 1;
            ++e;
            if (e > 1023) {
                *out = from_bits(inf);
                return WJN_ERR_RANGE;
            }
        }
        bits = (static_cast<u64>(e + 1023) << 52) | (m & MANT_MASK);
    } else {
        bits = m;  // subnormal; m == 2^52 encodes the smallest normal
    }
    *out = from_bits(sign | bits);
    return bits == 0 ? WJN_ERR_RANGE : WJN_OK;
}

int32_t parse_double(const char *s, const char *end, double *out) {
    bool negative = false;
    const char *p = s;
    if (*p == '-') {
        negative = true;
        ++p;
    }
    char dig[MAX_DIGITS + 1];
    int nd = 0;
    long long exp10 = 0;  // value = DIGITS * 10^exp10
    bool dropped = false;
    for (; p < end && is_digit(*p); ++p) {
        if (nd == 0 && *p == '0') continue;
        if (nd < MAX_DIGITS) {
            dig[nd++] = *p;
        } else {
            dropped |= *p != '0';
            ++exp10;
        }
    }
    if (p < end && *p == '.') {
        for (++p; p < end && is_digit(*p); ++p) {
            if (nd == 0 && *p == '0') {
                --exp10;
            } else if (nd < MAX_DIGITS) {
                dig[nd++] = *p;
                --exp10;
            } else {
                dropped |= *p != '0';
            }
        }
    }
    if (p < end && (*p == 'e' || *p == 'E')) {
        ++p;
        bool eneg = false;
        if (*p == '+' || *p == '-') eneg = *p++ == '-';
        long long x = 0;
        for (; p < end; ++p) {
            if (x < 100000000) x = x * 10 + (*p - '0');
        }
        exp10 += eneg ? -x : x;
    }
    if (nd == 0) {
        *out = from_bits(negative ? SIGN_BIT : 0);
        return WJN_OK;
    }
    while (nd > 1 && dig[nd - 1] == '0') {
        --nd;
        ++exp10;
    }
    // Digits past the 780th matter only as "something nonzero follows": any
    // value strictly between D*10^E and (D+1)*10^E rounds like
    // D*10^E + 10^(E-1), since points halfway between doubles have at most
    // 767 significant digits and cannot fall strictly inside that interval.
    if (dropped) {
        dig[nd++] = '1';
        --exp10;
    }
    const long long magnitude = nd + exp10;  // 10^(magnitude-1) <= value < 10^magnitude
    if (magnitude > 310) {
        *out = from_bits((negative ? SIGN_BIT : 0) | (0x7FFull << 52));
        return WJN_ERR_RANGE;
    }
    if (magnitude < -324) {  // below 10^-324: under half the smallest subnormal
        *out = from_bits(negative ? SIGN_BIT : 0);
        return WJN_ERR_RANGE;
    }
    const int e10 = static_cast<int>(exp10);

    // Fast path: the significand and the power of ten are exact doubles, so
    // one IEEE multiplication or division rounds correctly.
    if (nd <= 15) {
        u64 w = 0;
        for (int i = 0; i < nd; ++i) w = w * 10 + static_cast<u64>(dig[i] - '0');
        double v = -1;
        if (e10 >= 0 && e10 <= 22) {
            v = static_cast<double>(w) * POW10_F64[e10];
        } else if (e10 < 0 && e10 >= -22) {
            v = static_cast<double>(w) / POW10_F64[-e10];
        } else if (e10 > 22 && e10 <= 22 + 15 - nd) {
            v = static_cast<double>(w * static_cast<u64>(POW10_F64[e10 - 22])) * POW10_F64[22];
        }
        if (v >= 0) {
            *out = negative ? -v : v;
            return WJN_OK;
        }
    }

    // Medium path: up to 19 digits and a power of ten below 2^64, exact in
    // 128-bit arithmetic.
    if (nd <= 19 && e10 >= -19 && e10 <= 19) {
        u64 w = 0;
        for (int i = 0; i < nd; ++i) w = w * 10 + static_cast<u64>(dig[i] - '0');
        u64 p10 = 1;
        for (int i = 0; i < (e10 < 0 ? -e10 : e10); ++i) p10 *= 10;
        if (e10 >= 0) {
            const u128 prod = static_cast<u128>(w) * p10;
            const u64 hi = static_cast<u64>(prod >> 64);
            const u64 lo = static_cast<u64>(prod);
            if (hi == 0) return round_to_double(lo, 0, false, negative, out);
            const int shift = 64 - clz64(hi);  // drop this many low bits
            const u64 q = (hi << (64 - shift)) | (lo >> shift);
            const bool sticky = (lo & ((1ull << shift) - 1)) != 0;
            return round_to_double(q, shift, sticky, negative, out);
        }
        // q = floor(w * 2^s / 10^k) with 63 or 64 bits.
        const int s = 63 + (64 - clz64(p10)) - (64 - clz64(w));
        const u128 num = static_cast<u128>(w) << s;
        u64 rem = 0;
        const u64 q = div128(static_cast<u64>(num >> 64), static_cast<u64>(num), p10, &rem);
        return round_to_double(q, -s, rem != 0, negative, out);
    }

    // Exact path.
    Big D;
    D.set(0);
    for (int i = 0; i < nd;) {
        const int chunk = nd - i < 9 ? nd - i : 9;
        u32 part = 0;
        for (int k = 0; k < chunk; ++k) part = part * 10 + static_cast<u32>(dig[i + k] - '0');
        D.mul_small(POW10_U32[chunk]);
        D.add_small(part);
        i += chunk;
    }
    u64 q;
    int exp2;
    bool sticky;
    if (e10 >= 0) {
        D.mul_pow10(e10);
        const int b = D.bits();
        if (b <= 64) {
            q = D.extract64(0);
            exp2 = 0;
            sticky = false;
        } else {
            q = D.extract64(b - 64);
            exp2 = b - 64;
            sticky = D.any_below(b - 64);
        }
    } else {
        // q = floor(D * 2^s / 10^-e10), with s chosen so q has 63 or 64 bits;
        // the remainder only matters as sticky.
        Big Q;
        Q.set(1);
        Q.mul_pow10(-e10);
        const int s = 63 + Q.bits() - D.bits();
        if (s >= 0) {
            D.shl(s);
        } else {
            Q.shl(-s);
        }
        Big B = Q;  // binary long division; the quotient is below 2^64
        B.shl(63);
        q = 0;
        for (int k = 63; k >= 0; --k) {
            if (cmp(D, B) >= 0) {
                sub(D, B);
                q |= 1ull << k;
            }
            B.shr1();
        }
        exp2 = -s;
        sticky = !D.is_zero();
    }
    return round_to_double(q, exp2, sticky, negative, out);
}

// ---------------------------------------------------------------------------
// Shortest formatting
// ---------------------------------------------------------------------------

// Shortest digits that round-trip to v (> 0, finite); among those, the ones
// closest to v, ties to even. Returns the count; v ~ 0.DIGITS * 10^*k_out.
int shortest_digits(u64 bits, char *digits, int *k_out) {
    const u64 mant = bits & MANT_MASK;
    const int be = static_cast<int>((bits >> 52) & 0x7FF);
    const u64 f = be == 0 ? mant : (mant | (1ull << 52));
    const int e = be == 0 ? -1074 : be - 1075;
    const bool even = (f & 1) == 0;
    const bool lower_closer = mant == 0 && be > 1;  // gap below is half the gap above

    // v = r / s; the rounding interval is (v - mm/s, v + mp/s).
    Big r, s, mp, mm;
    r.set(f);
    mp.set(1);
    mm.set(1);
    if (e >= 0) {
        if (!lower_closer) {
            r.shl(e + 1);
            s.set(2);
            mp.shl(e);
            mm.shl(e);
        } else {
            r.shl(e + 2);
            s.set(4);
            mp.shl(e + 1);
            mm.shl(e);
        }
    } else {
        s.set(1);
        if (!lower_closer) {
            r.shl(1);
            s.shl(1 - e);
        } else {
            r.shl(2);
            s.shl(2 - e);
            mp.set(2);
        }
    }

    // k = ceil(log10(v)), estimated from below and fixed up.
    const int flen = 64 - clz64(f);
    const double est = (e + flen - 1) * 0.30102999566398114 - 1e-10;
    int k = static_cast<int>(est);
    if (k < est) ++k;
    if (k >= 0) {
        s.mul_pow10(k);
    } else {
        r.mul_pow10(-k);
        mp.mul_pow10(-k);
        mm.mul_pow10(-k);
    }
    const int top = cmp_sum(r, mp, s);
    if (even ? top >= 0 : top > 0) {
        s.mul_small(10);
        ++k;
    }

    int n = 0;
    // Common case: all four values fit comfortably in 128 bits (s < 2^120
    // and r, m+ < s), so the digit loop can run on native integers.
    if (s.bits() <= 120) {
        const u128 S = to_u128(s);
        u128 R = to_u128(r), MP = to_u128(mp), MM = to_u128(mm);
        for (;;) {
            R *= 10;
            MP *= 10;
            MM *= 10;
            int d = 0;
            while (R >= S) {
                R -= S;
                ++d;
            }
            const bool tc1 = even ? R <= MM : R < MM;
            const bool tc2 = even ? R + MP >= S : R + MP > S;
            if (!tc1 && !tc2) {
                digits[n++] = static_cast<char>('0' + d);
                continue;
            }
            if (tc1 && tc2) {
                const u128 twice = R * 2;
                if (twice > S || (twice == S && (d & 1))) ++d;
            } else if (tc2) {
                ++d;
            }
            while (d == 10) {
                if (n == 0) {
                    d = 1;
                    ++k;
                    break;
                }
                d = digits[--n] - '0' + 1;
            }
            digits[n++] = static_cast<char>('0' + d);
            *k_out = k;
            return n;
        }
    }
    for (;;) {
        r.mul_small(10);
        mp.mul_small(10);
        mm.mul_small(10);
        int d = 0;
        while (cmp(r, s) >= 0) {
            sub(r, s);
            ++d;
        }
        const int lo = cmp(r, mm);
        const bool tc1 = even ? lo <= 0 : lo < 0;  // rounding down stays inside
        const int hi = cmp_sum(r, mp, s);
        const bool tc2 = even ? hi >= 0 : hi > 0;  // rounding up stays inside
        if (!tc1 && !tc2) {
            digits[n++] = static_cast<char>('0' + d);
            continue;
        }
        if (tc1 && tc2) {
            Big twice = r;
            twice.shl(1);
            const int c = cmp(twice, s);
            if (c > 0 || (c == 0 && (d & 1))) ++d;  // nearest, ties to even
        } else if (tc2) {
            ++d;
        }
        while (d == 10) {  // a final 10 carries into earlier digits
            if (n == 0) {
                d = 1;
                ++k;
                break;
            }
            d = digits[--n] - '0' + 1;
        }
        digits[n++] = static_cast<char>('0' + d);
        break;
    }
    *k_out = k;
    return n;
}

// Lays out digits like std::to_chars: fixed or scientific, whichever is
// shorter (fixed on a tie). Returns the length written to `out`.
// `exact` (optional) holds the full integer digits of the value; when fixed
// notation is chosen for an integer at or above 2^53, those are printed
// instead of zero padding, as std::to_chars does.
int layout(bool negative, const char *digits, int n, int k, const char *exact, int exact_n, char *out) {
    int len = 0;
    if (negative) out[len++] = '-';
    const int x = k - 1;  // scientific exponent
    const int ax = x < 0 ? -x : x;
    const int exp_digits = ax >= 100 ? 3 : 2;
    const int sci_len = n + (n > 1 ? 1 : 0) + 2 + exp_digits;
    int fixed_len;
    if (k <= 0) {
        fixed_len = 2 - k + n;
    } else if (k < n) {
        fixed_len = n + 1;
    } else {
        fixed_len = k;
    }
    if (fixed_len <= sci_len) {
        if (k <= 0) {
            out[len++] = '0';
            out[len++] = '.';
            for (int i = 0; i < -k; ++i) out[len++] = '0';
            for (int i = 0; i < n; ++i) out[len++] = digits[i];
        } else if (k < n) {
            for (int i = 0; i < k; ++i) out[len++] = digits[i];
            out[len++] = '.';
            for (int i = k; i < n; ++i) out[len++] = digits[i];
        } else if (exact != nullptr && exact_n == k) {
            for (int i = 0; i < k; ++i) out[len++] = exact[i];
        } else {
            for (int i = 0; i < n; ++i) out[len++] = digits[i];
            for (int i = n; i < k; ++i) out[len++] = '0';
        }
    } else {
        out[len++] = digits[0];
        if (n > 1) {
            out[len++] = '.';
            for (int i = 1; i < n; ++i) out[len++] = digits[i];
        }
        out[len++] = 'e';
        out[len++] = x < 0 ? '-' : '+';
        if (exp_digits == 3) out[len++] = static_cast<char>('0' + ax / 100);
        out[len++] = static_cast<char>('0' + ax / 10 % 10);
        out[len++] = static_cast<char>('0' + ax % 10);
    }
    return len;
}

}  // namespace

extern "C" int32_t wjn_parse_f64(const char *src, int64_t len, double *out) noexcept {
    if (out == nullptr || len <= 0 || src == nullptr) return WJN_ERR_INVALID;
    const char *end = src + len;
    bool integer = false;
    if (!is_json_number(src, end, &integer)) return WJN_ERR_INVALID;
    return parse_double(src, end, out);
}

extern "C" int32_t wjn_parse_i64(const char *src, int64_t len, int64_t *out) noexcept {
    if (out == nullptr || len <= 0 || src == nullptr) return WJN_ERR_INVALID;
    const char *end = src + len;
    bool integer = false;
    if (!is_json_number(src, end, &integer) || !integer) return WJN_ERR_INVALID;
    const char *p = src;
    const bool negative = *p == '-';
    if (negative) ++p;
    const u64 limit = negative ? (1ull << 63) : (1ull << 63) - 1;
    u64 v = 0;
    for (; p < end; ++p) {
        const u64 digit = static_cast<u64>(*p - '0');
        if (v > (limit - digit) / 10) return WJN_ERR_RANGE;
        v = v * 10 + digit;
    }
    *out = negative ? static_cast<int64_t>(0 - v) : static_cast<int64_t>(v);
    return WJN_OK;
}

extern "C" int64_t wjn_format_f64(double value, char *dst, int64_t cap) noexcept {
    const u64 bits = bits_of(value);
    if (!finite_bits(bits)) return WJN_ERR_NONFINITE;
    if (cap < 0) return WJN_ERR_INVALID;
    const bool negative = (bits & SIGN_BIT) != 0;
    char tmp[64];
    int len = 0;
    if ((bits & ~SIGN_BIT) == 0) {
        if (negative) tmp[len++] = '-';
        tmp[len++] = '0';
    } else {
        char digits[24];
        int k = 0;
        const int n = shortest_digits(bits & ~SIGN_BIT, digits, &k);
        char exact[24];
        int exact_n = 0;
        const int be = static_cast<int>((bits >> 52) & 0x7FF);
        if (k > n && k <= 22 && be >= 1075) {  // integer >= 2^52 that may print in fixed form
            Big v;
            v.set((bits & MANT_MASK) | (1ull << 52));
            v.shl(be - 1075);
            exact_n = big_digits(v, exact);
        }
        len = layout(negative, digits, n, k, exact_n ? exact : nullptr, exact_n, tmp);
    }
    return emit(tmp, len, dst, cap);
}

extern "C" int64_t wjn_format_f64_fixed(double value, int32_t precision, char *dst, int64_t cap) noexcept {
    const u64 bits = bits_of(value);
    if (!finite_bits(bits)) return WJN_ERR_NONFINITE;
    if (cap < 0 || precision < 0 || precision > 17) return WJN_ERR_INVALID;
    const bool negative = (bits & SIGN_BIT) != 0;
    const u64 mant = bits & MANT_MASK;
    const int be = static_cast<int>((bits >> 52) & 0x7FF);
    const u64 f = be == 0 ? mant : (mant | (1ull << 52));
    const int e = be == 0 ? -1074 : be - 1075;

    // N = round_half_even(|value| * 10^precision), exactly.
    Big N;
    N.set(f);
    N.mul_pow10(precision);
    if (e >= 0) {
        N.shl(e);
    } else {
        const int s = -e;
        const bool half = N.bit(s - 1);
        const bool below = N.any_below(s - 1);
        N.shr(s);
        if (half && (below || N.bit(0))) N.add_small(1);
    }

    char digits[400];  // DBL_MAX has 309 integer digits, plus 17 decimals
    const int n = N.is_zero() ? 0 : big_digits(N, digits);
    char tmp[420];
    int len = 0;
    if (negative) tmp[len++] = '-';
    const int total = n > precision ? n : precision + 1;  // >= 1 integer digit
    const int pad = total - n;
    for (int i = 0; i < total; ++i) {
        if (i == total - precision) tmp[len++] = '.';
        tmp[len++] = i < pad ? '0' : digits[i - pad];
    }
    return emit(tmp, len, dst, cap);
}

extern "C" int64_t wjn_format_i64(int64_t value, char *dst, int64_t cap) noexcept {
    if (cap < 0) return WJN_ERR_INVALID;
    char tmp[24];
    int len = 0;
    u64 magnitude = static_cast<u64>(value);
    if (value < 0) {
        tmp[len++] = '-';
        magnitude = 0 - magnitude;
    }
    len += u64_digits(magnitude, tmp + len);
    return emit(tmp, len, dst, cap);
}
