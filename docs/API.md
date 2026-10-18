# API overview

The Wave API by area; every function is also commented in `src/`. Back to the [README](../README.md).

## API overview (Wave)

Everything is imported with `import("wjson")::{...}`.

**Parsing** — `json_parse(text)`, `json_parse_with(text, max_depth, flags)`,
`json_parse_bytes(ptr, len, max_depth, flags)`, `json_ok(doc)`,
`json_error(doc) -> JsonError{code, message, offset, line, column}`,
`json_root(doc)`, `json_free(&doc)`

- flags: `JSON_STRICT` (default), `JSON_ALLOW_TRAILING_COMMAS`,
  `JSON_ALLOW_COMMENTS`, `JSON_RELAXED` (both)
- default maximum nesting depth 512, and no `max_depth` can exceed
  `JSON_MAX_DEPTH_LIMIT` (2048) because parsing recurses; a UTF-8 BOM is
  accepted; invalid UTF-8 and lone surrogates are rejected

**Navigation** — `json_get(v, key)`, `json_at(v, i)`, `json_key_at(v, i)`,
`json_len(v)`, `json_has(v, key)`, `json_pointer(v, "/a/0/b")`,
`json_path(v, "a.b[0][\"x.y\"]")`

- missing keys, out-of-range indices and type mismatches return an "invalid"
  value instead of failing, so chains are always safe; check with
  `json_is_valid()` at the end
- with duplicate keys the last one wins (like JavaScript's `JSON.parse`)

**Reading values** — `json_type`, `json_is_*`, `json_as_bool`, `json_as_str`,
`json_as_bytes`, `json_as_str_checked`, `json_str_has_nul`, `json_as_i64`,
`json_as_f64` (+ `_checked` variants),
`json_number_text` (the source lexeme), with defaults `json_i64_or` /
`json_f64_or` / `json_bool_or` / `json_str_or`, path + default
`json_path_i64` etc., `json_str_eq`

**Writing** — `json_writer_new(indent)`, `json_begin_object/array`,
`json_end_object/array`, `json_key`,
`json_write_null/bool/i64/f64/f64_fixed/str/raw`, `json_field_*`,
`json_writer_finish`, `json_writer_status`, `json_writer_reset`,
`json_writer_free`, `json_writer_set_escape(JSON_ESCAPE_ASCII | JSON_ESCAPE_HTML)`

- the writer manages commas, colons and nesting; out-of-order calls (a value
  without a key, mismatched closers, two top-level values, …) put it in an
  error state
- NaN and infinity have no JSON spelling: `JSON_WRITER_ERR_NUMBER`

**Trees** — `json_write_value(&w, v)`, `json_to_string(v, indent)`,
`json_to_string_with(v, indent, escape_flags)`, `json_string_free(s)`,
`json_equal(a, b)` (ignores member order and number spelling), `json_depth(v)`

**Editing** — new values: `json_new_document()`,
`json_new_null/bool/i64/f64/str/array/object(doc, …)`,
`json_new_number_text(doc, "12.50")`, `json_new_number_bytes(doc, p, n)`,
`json_new_f64_fixed(doc, 82.349, 1)` (→ `82.3`), `json_copy(doc, v)` (also
across documents), `json_set_root(doc, v)`.
Objects: `json_set(obj, key, v)`, `json_set_i64/f64/str/bool/null`,
`json_set_object/array` (insert a new container and return it),
`json_remove(obj, key)`.
Arrays: `json_push`, `json_push_i64/…/object/array`, `json_insert(arr, i, v)`,
`json_replace(arr, i, v)`, `json_remove_at(arr, i)`, `json_clear(v)`.
Paths: `json_set_path(root, "a.b[0].c", v)` (creates intermediate objects),
`json_set_path_i64/f64/str/bool`, `json_remove_path`.
Structure: `json_parent(v)`, `json_is_attached(v)`, `json_edit_error(v)`.

- results are `JSON_EDIT_*` codes. A value can be attached in one place only
  (`JSON_EDIT_ATTACHED` → use `json_copy`); inserting a value into itself is
  `JSON_EDIT_CYCLE`; NaN, infinity or a malformed number text is
  `JSON_EDIT_BAD_VALUE`
- replaced or removed values become detached and can be attached again; a
  document that failed to parse cannot be edited
- objects with 16 or more keys build a hash index on the first lookup and
  keep it up to date on insert and remove

**Streaming** — low level: `json_reader_new(flags)`,
`json_reader_feed/feed_str/finish/next`, `json_reader_text/text_eq/i64/f64`,
`json_reader_depth/documents/buffered/error`, `json_reader_set_max_token`,
`json_reader_free`.
Files: `json_stream_open(path, flags)`, `json_stream_open_with(path, flags, chunk)`,
`json_stream_next(&s)`, `json_stream_skip(&s)` (skip the current container),
`json_stream_text/text_eq/i64/f64`,
`json_stream_depth/documents/bytes_read/buffered/error/io_error`,
`json_stream_set_max_token`,
`json_stream_close(&s)`.

- events: `JSON_EV_OBJECT_START/END`, `ARRAY_START/END`, `KEY`, `STRING`,
  `NUMBER`, `TRUE`, `FALSE`, `NULL`, `END`, `ERROR` (and `NEED_MORE` for the
  reader)
- flags: `JSON_ALLOW_TRAILING_COMMAS`, `JSON_ALLOW_COMMENTS`,
  `JSON_STREAM_MULTI` (several top-level values, JSON Lines)
- same grammar, error codes and error positions as the DOM parser (checked by
  fuzzing)

**Null strings** — Wave's `str` cannot be null, so "no string" is returned as
a `str` whose pointer is null. Test it with `json_str_is_null(s)`.
