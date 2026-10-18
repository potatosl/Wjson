# Guide

Parsing, writing, editing and streaming by example, and the memory rules. Back to the [README](../README.md).

## Quick start

```wave
import("wjson")::{JsonDoc, Json, json_parse, json_ok, json_root, json_path_str,
                  json_path_i64, json_free};

fun main() {
    var doc: JsonDoc = json_parse("{\"server\": {\"host\": \"0.0.0.0\", \"ports\": [8080, 8443]}}");
    if (!json_ok(doc)) { return; }
    var cfg: Json = json_root(doc);
    println("{}:{}", json_path_str(cfg, "server.host", "localhost"),
                     json_path_i64(cfg, "server.ports[0]", 80));
    json_free(&doc);
}
```

Writing JSON:

```wave
var w: JsonWriter = json_writer_new(2);      // indent 2; 0 = one line
json_begin_object(&w);
json_field_str(&w, "name", "wave");
json_key(&w, "tags");
json_begin_array(&w);
json_write_i64(&w, 1);
json_write_f64(&w, 2.5);
json_end_array(&w);
json_end_object(&w);
println("{}", json_writer_finish(&w));       // null string if the structure is wrong
json_writer_free(&w);
```

Editing a document:

```wave
var doc: JsonDoc = json_parse("{\"name\": \"wave\", \"tags\": []}");
var root: Json = json_root(doc);
json_set_str(root, "name", "wjson");               // replace a value
json_set_path_i64(root, "limits.depth", 512);      // intermediate objects are created
json_push_str(json_get(root, "tags"), "json");     // append to an array
json_remove(root, "obsolete");                     // delete a key
```

Streaming a large file (constant memory regardless of file size):

```wave
var s: JsonStream = json_stream_open("big.jsonl", JSON_STREAM_MULTI);
var ev: i32 = json_stream_next(&s);
while (ev != JSON_EV_END && ev != JSON_EV_ERROR) {
    if (ev == JSON_EV_KEY && json_stream_text_eq(&s, "id")) { ... }
    ev = json_stream_next(&s);
}
json_stream_close(&s);
```

Longer examples in `examples/`:

| Example | Shows |
| --- | --- |
| `config.wave` | reading a config file with comments and trailing commas, typed path queries |
| `transform.wave` | filtering and producing new JSON |
| `edit_config.wave` | editing a config document |
| `stream_rows.wave` | streaming search in 4 KB chunks |
| `csv2json.wave` | statistics-style CSV (3 header rows, quoted fields) to nested JSON, then 1,449 cells checked back against the CSV |
| `survey_report.wave` | the same CSV built into a document with the edit API only, then copy, sort, rank, rename, summarize and save |
| `show_data.wave` | printing the converted data as a table |

The example data is **synthetic** — generated for this repository
(see `examples/data/README.md`).

## Memory rules

- `Json` values and strings returned by `json_as_str`, `json_key_at` and
  `json_number_text` live inside the document: do not use them after
  `json_free`. Until then they **stay valid even while the document is
  edited** — parsed text is never rewritten, and text created by edits is
  allocated separately and never moves. Passing a string from the same
  document (`json_set_str(v, "new", json_as_str(json_get(v, "old")))`) is safe.
- **Embedded NULs.** JSON strings may contain `\u0000`, and a `str` ends at the
  first NUL, so `"admin\u0000.txt"` reads as `admin` through `json_as_str` or
  `json_key_at`. For untrusted documents use `json_as_str_checked` (null when
  the text has a NUL), `json_str_has_nul`, or the exact-length
  `json_as_bytes` / `json_key_at_bytes` / `json_str_eq`.
- **Threads.** Reading a document nobody is editing (`json_get`, `json_at`,
  `json_path`, ...) is safe from several threads. Edits need exclusive access.
  A reader/stream belongs to one thread.
- **Streaming limits.** A reader buffers at most `json_reader_set_max_token`
  bytes (default 256 MiB) while one string or comment is incomplete, and
  rejects numbers longer than 64 KiB, with `JSON_ERR_TOO_LARGE`. Set a small
  cap when the input comes from an untrusted peer.
- `json_writer_finish` returns writer-owned text (valid until the next write,
  reset or free).
- `json_to_string*` results belong to the caller → `json_string_free`.
- old child-list slots left behind by edits are released with the document.
- streaming `*_text` results are valid until the next `next` call.
