/* SPDX-License-Identifier: MIT
 *
 * wjson_native: the C++ half of wjson — number conversion and string
 * escaping, exposed through a plain C ABI so Wave can call it directly.
 *
 * Conventions:
 *   - Input byte ranges are (pointer, length); they need not be NUL-terminated.
 *   - Writers return the number of bytes the full result needs. When that is
 *     less than `cap` the output is also NUL-terminated. Call once with
 *     cap == 0 to size a buffer, then again to fill it.
 *   - Negative returns are WJN_ERR_* codes.
 */
#ifndef WJSON_NATIVE_H
#define WJSON_NATIVE_H

#include <stdint.h>

#ifdef __cplusplus
#define WJN_NOEXCEPT noexcept
extern "C" {
#else
#define WJN_NOEXCEPT
#endif

#define WJN_OK 0
#define WJN_ERR_INVALID (-1)  /* not a valid number / bad argument / bad UTF-8 */
#define WJN_ERR_RANGE (-2)    /* value does not fit the target type */
#define WJN_ERR_NONFINITE (-3) /* NaN or infinity has no JSON spelling */

/* Escape flags. */
#define WJN_ESCAPE_ASCII 1u   /* emit non-ASCII as \uXXXX (surrogate pairs above U+FFFF) */
#define WJN_ESCAPE_HTML 2u    /* also escape < > & ' as < etc. */
#define WJN_ESCAPE_QUOTES 4u  /* wrap the result in double quotes */

/* Parses a JSON number lexeme to double (round-to-nearest, correctly rounded).
 * Out-of-range magnitudes store +-inf or +-0 and return WJN_ERR_RANGE. */
int32_t wjn_parse_f64(const char *src, int64_t len, double *out) WJN_NOEXCEPT;

/* Parses a JSON integer lexeme (no fraction/exponent) to int64. */
int32_t wjn_parse_i64(const char *src, int64_t len, int64_t *out) WJN_NOEXCEPT;

/* Shortest round-trip decimal form of `value`, always valid JSON. */
int64_t wjn_format_f64(double value, char *dst, int64_t cap) WJN_NOEXCEPT;

/* Fixed notation with `precision` digits after the point (0..17). */
int64_t wjn_format_f64_fixed(double value, int32_t precision, char *dst, int64_t cap) WJN_NOEXCEPT;

int64_t wjn_format_i64(int64_t value, char *dst, int64_t cap) WJN_NOEXCEPT;

/* JSON-escapes UTF-8 `src`. Returns needed length or WJN_ERR_INVALID. */
int64_t wjn_escape(const char *src, int64_t len, char *dst, int64_t cap, uint32_t flags) WJN_NOEXCEPT;

/* Returns -1 when `src` is valid UTF-8, else the offset of the first bad byte. */
int64_t wjn_utf8_check(const char *src, int64_t len) WJN_NOEXCEPT;

/* Number of Unicode code points in valid UTF-8 `src`, or WJN_ERR_INVALID. */
int64_t wjn_utf8_count(const char *src, int64_t len) WJN_NOEXCEPT;

/* Switches a Windows console to UTF-8 output (returns the previous code
 * page). No-op returning 0 on other platforms. */
int32_t wjn_console_utf8(void) WJN_NOEXCEPT;

#ifdef __cplusplus
}
#endif

#endif /* WJSON_NATIVE_H */
