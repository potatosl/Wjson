/* SPDX-License-Identifier: MIT
 *
 * wjson core C ABI (implemented in Rust, core/src/lib.rs).
 *
 * A parse always returns a document handle, even on failure; check wj_ok()
 * and read the error from the handle, then release it with wj_free().
 * Values are addressed by int32 node ids; -1 means "no such value".
 * Every accessor tolerates a NULL handle and out-of-range ids.
 */
#ifndef WJSON_H
#define WJSON_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct wj_doc wj_doc;

enum {
    WJ_NULL = 0,
    WJ_BOOL = 1,
    WJ_NUMBER = 2,
    WJ_STRING = 3,
    WJ_ARRAY = 4,
    WJ_OBJECT = 5,
    WJ_INVALID = -1
};

enum {
    WJ_ALLOW_TRAILING_COMMAS = 1u,
    WJ_ALLOW_COMMENTS = 2u
};

enum {
    WJ_OK = 0,
    WJ_ERR_EMPTY = 1,
    WJ_ERR_UNEXPECTED_CHAR = 2,
    WJ_ERR_UNEXPECTED_END = 3,
    WJ_ERR_INVALID_NUMBER = 4,
    WJ_ERR_INVALID_STRING = 5,
    WJ_ERR_INVALID_ESCAPE = 6,
    WJ_ERR_INVALID_UNICODE = 7,
    WJ_ERR_CONTROL_CHAR = 8,
    WJ_ERR_INVALID_UTF8 = 9,
    WJ_ERR_TRAILING_DATA = 10,
    WJ_ERR_TOO_DEEP = 11,
    WJ_ERR_TOO_LARGE = 12,
    WJ_ERR_INVALID_ARGUMENT = 13
};

const uint8_t *wj_version(void);

wj_doc *wj_parse(const uint8_t *src, int64_t len);
wj_doc *wj_parse_ex(const uint8_t *src, int64_t len, int32_t max_depth, uint32_t flags);
wj_doc *wj_parse_cstr(const uint8_t *src, int32_t max_depth, uint32_t flags);
void wj_free(wj_doc *doc);

int32_t wj_ok(const wj_doc *doc);
int32_t wj_error_code(const wj_doc *doc);
const uint8_t *wj_error_message(const wj_doc *doc);
int64_t wj_error_offset(const wj_doc *doc);
int64_t wj_error_line(const wj_doc *doc);
int64_t wj_error_column(const wj_doc *doc);

int32_t wj_root(const wj_doc *doc);
int64_t wj_node_count(const wj_doc *doc);
int32_t wj_type(const wj_doc *doc, int32_t node);
int32_t wj_bool(const wj_doc *doc, int32_t node);
int64_t wj_len(const wj_doc *doc, int32_t node);
int32_t wj_at(const wj_doc *doc, int32_t node, int64_t index);
const uint8_t *wj_key(const wj_doc *doc, int32_t node, int64_t index, int64_t *out_len);
int32_t wj_get(const wj_doc *doc, int32_t node, const uint8_t *key, int64_t key_len);
int32_t wj_pointer(const wj_doc *doc, int32_t node, const uint8_t *pointer, int64_t len);
const uint8_t *wj_str(const wj_doc *doc, int32_t node, int64_t *out_len);
const uint8_t *wj_num(const wj_doc *doc, int32_t node, int64_t *out_len);
int32_t wj_num_is_int(const wj_doc *doc, int32_t node);

/* ---- editing ------------------------------------------------------------
 * New values are created detached; attaching a value that already has a
 * parent is refused (copy it with wj_copy). Creation returns a node id or a
 * negative WJ_EDIT_* code; mutations return 0 or a negative WJ_EDIT_* code. */
enum {
    WJ_EDIT_OK = 0,
    WJ_EDIT_INVALID = -1,
    WJ_EDIT_ATTACHED = -2,
    WJ_EDIT_CYCLE = -3,
    WJ_EDIT_NOT_FOUND = -4,
    WJ_EDIT_WRONG_TYPE = -5,
    WJ_EDIT_BAD_VALUE = -6
};

wj_doc *wj_new_doc(void);
int32_t wj_new_null(wj_doc *doc);
int32_t wj_new_bool(wj_doc *doc, int32_t value);
int32_t wj_new_number(wj_doc *doc, const uint8_t *lexeme, int64_t len);
int32_t wj_new_string(wj_doc *doc, const uint8_t *text, int64_t len);
int32_t wj_new_array(wj_doc *doc);
int32_t wj_new_object(wj_doc *doc);
int32_t wj_set_root(wj_doc *doc, int32_t node);
int32_t wj_parent(const wj_doc *doc, int32_t node);
int32_t wj_arr_push(wj_doc *doc, int32_t arr, int32_t value);
int32_t wj_arr_insert(wj_doc *doc, int32_t arr, int64_t index, int32_t value);
int32_t wj_arr_set(wj_doc *doc, int32_t arr, int64_t index, int32_t value);
int32_t wj_arr_remove(wj_doc *doc, int32_t arr, int64_t index);
int32_t wj_obj_set(wj_doc *doc, int32_t obj, const uint8_t *key, int64_t key_len, int32_t value);
int32_t wj_obj_remove(wj_doc *doc, int32_t obj, const uint8_t *key, int64_t key_len);
int32_t wj_copy(wj_doc *doc, const wj_doc *src, int32_t src_node);

/* ---- streaming (pull parser) ------------------------------------------- */
typedef struct wj_reader wj_reader;

enum {
    WJ_EV_ERROR = -1,
    WJ_EV_NEED_MORE = 0,
    WJ_EV_OBJECT_START = 1,
    WJ_EV_OBJECT_END = 2,
    WJ_EV_ARRAY_START = 3,
    WJ_EV_ARRAY_END = 4,
    WJ_EV_KEY = 5,
    WJ_EV_STRING = 6,
    WJ_EV_NUMBER = 7,
    WJ_EV_TRUE = 8,
    WJ_EV_FALSE = 9,
    WJ_EV_NULL = 10,
    WJ_EV_END = 11
};

#define WJ_STREAM_MULTI 4u /* accept many top-level values (JSON Lines) */

wj_reader *wj_reader_new(int32_t max_depth, uint32_t flags);
void wj_reader_free(wj_reader *r);
int32_t wj_reader_feed(wj_reader *r, const uint8_t *data, int64_t len);
void wj_reader_finish(wj_reader *r);
int32_t wj_reader_next(wj_reader *r);
const uint8_t *wj_reader_text(const wj_reader *r, int64_t *out_len);
int32_t wj_reader_depth(const wj_reader *r);
int64_t wj_reader_documents(const wj_reader *r);
int64_t wj_reader_buffered(const wj_reader *r);
int32_t wj_reader_error_code(const wj_reader *r);
const uint8_t *wj_reader_error_message(const wj_reader *r);
int64_t wj_reader_error_offset(const wj_reader *r);
int64_t wj_reader_error_line(const wj_reader *r);
int64_t wj_reader_error_column(const wj_reader *r);

#ifdef __cplusplus
}
#endif

#endif /* WJSON_H */
