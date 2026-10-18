# wjson — JSON for the Wave language

**English** | [한국어](README.ko.md)

A JSON library for [Wave](https://wave-lang.dev) in which Rust, C++ and Wave
each do the part they are best at.

| Layer | Language | Responsibility | Location |
| --- | --- | --- | --- |
| API | **Wave** | the parser, document model, edit operations, JSON Pointer and incremental (pull) parser (`core_*.wave`, served through `ffi.wave`); navigation, path queries, document editing (incl. by path), writer, serialization, deep equality, file streaming; number conversion, string escaping and UTF-8 checks (`number.wave`, `escape.wave`) | `src/*.wave` |
| Core | **Rust** | the same RFC 8259 parser, index-based DOM, edit operations, hash index, pull parser, JSON Pointer and error positions for C and C++ users, and as the reference the Wave code is tested against (`src/ffi_rust.wave`) | `core/` |
| Native | **C++20** (freestanding) | the same number conversion (correctly rounded parsing, shortest round-trip output), string escaping and UTF-8 validation for C and C++ users, and as the reference the Wave code is tested against; compiles without any standard-library or SDK headers | `native/` |
| Toolchain | **Rust** | `wjlink`: the Windows link driver used instead of MSVC — generates import libraries from system DLLs, emits the startup object, archives objects, drives rust-lld | `chain/` |

Every boundary between Wave and Rust is a C ABI (`extern(c)`). C and C++
programs can use the same core through `include/wjson.h` and
`native/include/wjson.hpp`.

**Zero external dependencies.** No crates, no C++ libraries
(`core/Cargo.lock` lists only `wjson_core`); even the reference parser used
for differential fuzzing is written inside the tests. You only need build
tools (wavec, cargo, a C++ compiler) — and on Windows **no MSVC or Visual
Studio**.

> Status: 0.x. Wave itself is pre-beta and the API may still change; see
> [Known limitations](#known-limitations).

## How to use

wjson is used from source: build its two static libraries once, then compile
your own program against them.

**Quick install (Linux, macOS).** One command downloads the sources, builds the
library and writes a wrapper that compiles a Wave program against it (it
needs `wavec` and a C++ compiler on your `PATH`, see step 1 below; it
tells you if one is missing and never installs them for you):

```sh
curl -fsSL https://raw.githubusercontent.com/potatosl/wjson/main/install.sh | sh
~/.wjson/bin/wjson-wavec hello.wave --run     # hello.wave: see step 3
```

The script is short, [read it](install.sh) before running it. It installs into
`~/.wjson` (remove that folder to uninstall). `WJSON_REF=<tag or commit>` picks
a version instead of `main`, `WJSON_DIR` the folder, and `WJSON_SHA256` makes
it refuse a download whose SHA-256 differs (the actual value is always
printed). The steps below do the same by hand; on Windows use them or
`setup-windows.cmd`.

**1. Install the tools** — the Wave compiler `wavec` (0.2.1-pre-beta, the
newest release) and a C++20 compiler; Rust 1.99 or newer is only needed to run the
full test suite (the Rust core is the reference the Wave code is tested against)
and, with `install.sh`, when programs are built with `-O0` (macOS arm64), where
the installer uses the faster Rust backend (`WJSON_BACKEND=wave|rust|auto`). Wave's official
installer (see its [install page](https://wave-lang.dev/docs/en/getting-started/install)):

```sh
curl -fsSL https://wave-lang.dev/install.sh | bash     # Linux, macOS, FreeBSD
```

On Windows see [Windows (no MSVC)](#windows-no-msvc); `setup-windows.cmd`
installs everything.

**2. Get wjson and build the libraries**

```sh
git clone https://github.com/potatosl/wjson.git
cd wjson
WJSON_TASK=libs wavec run build.wave
```

The last lines print the two library paths and a `link :` line with the
linker flags for your machine. Keep that line for step 4.

**3. Write your program** (`hello.wave`, anywhere — for example next to the
`wjson` folder):

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

**4. Compile and run it**

```sh
wavec build hello.wave \
      wjson/build/native/libwjson_native.a wjson/build/cargo/release/libwjson_core.a \
      --dep=wjson=wjson \
      --link=gcc_s --link=util --link=rt --link=pthread --link=m --link=dl --link=c \
      -L /usr/lib/gcc/x86_64-linux-gnu/15 --run
```

wavec compiles at `-O0` unless told otherwise. wjson's native code is already
optimized, but your own Wave code runs several times faster with `-O2`
(`wavec -O2 build ...`). Exception: on macOS arm64 optimized builds currently
break inside Wave's `std::mem::alloc` (Wave issue
[#933](https://github.com/wavefnd/Wave/issues/933)); use `-O0` there until that is fixed.

This prints `0.0.0.0:8080`. `--dep=wjson=wjson` points at the cloned folder.
Replace the `--link=...` and `-L ...` flags with the `link :` line from step 2
(the gcc directory differs from machine to machine). If `wavec` cannot find its
standard library (`wavec print std-path` shows where it looks; the installer
puts it in `~/.wave/bin/std`), add `--std-root=<folder containing Wave's std>`
or run `wavec install std`, as Wave's troubleshooting page suggests.

**From C or C++** (no Wave needed) link the same two libraries and include the
headers. `wjson.hpp` is a header-only C++ view; `wjson.h` is the plain C API
(it only needs `libwjson_core.a`):

```sh
c++ -std=c++20 -Iwjson/include -Iwjson/native/include app.cpp \
    wjson/build/native/libwjson_native.a wjson/build/cargo/release/libwjson_core.a \
    -lpthread -ldl -o app
```

```cpp
#include "wjson.hpp"
#include <cstdio>

int main() {
    auto doc = wjson::Document::parse(R"({"server": {"host": "0.0.0.0", "ports": [8080, 8443]}})");
    if (!doc) return 1;
    auto server = doc.root()["server"];
    std::printf("%lld\n", (long long)server["ports"][0].as_int().value_or(80));   // 8080
}
```

Next: the examples below, the [API overview](#api-overview-wave), and
`examples/` for longer programs.

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

## Building and testing

The build driver is itself written in Wave (`build.wave`). Requirements:
[wavec](https://wave-lang.dev/docs/en/getting-started/install) 0.2.1-pre-beta
(LLVM 21), Rust 1.99 or newer (cargo), and a C++20 compiler (clang on Windows).

```sh
wavec run build.wave                                   # Rust + C++ + Wave tests, all examples
WJSON_RUN=examples/show_data.wave wavec run build.wave # build and run one program
WJSON_TASK=libs wavec run build.wave                   # only the static libraries (+ link flags)
WJSON_TASK=bench wavec run build.wave                  # benchmarks (Rust core + C++ layer)
WJSON_DRY_RUN=1 wavec run build.wave                   # print the commands only
```

`build.wave` compiles every Wave program (tests and examples) with `-O2`;
wavec's default is `-O0`, which was about 4x slower on the same code. On macOS
arm64 it uses `-O0` instead, because Wave's std makes its raw syscalls without
declaring a register the kernel overwrites, which breaks optimized builds
([#933](https://github.com/wavefnd/Wave/issues/933)). `WJSON_OPT=-O2` (or any
other level) overrides the choice.

Wave's `main` takes no arguments, so tasks are chosen with environment
variables. Other knobs: `WAVEC`, `WAVE_STD` (explicit std root), `CARGO`,
`RUSTC`, `CXX`. Because `std::process` cannot spawn processes on Windows yet,
the driver runs commands through the C library's `system()`.

Tests:

| Test | Covers |
| --- | --- |
| `core/src` unit tests | parser, editing, key index, streaming (down to 1-byte chunks) |
| `core/tests/fuzz.rs` | differential against an in-house reference parser, streaming vs DOM (same error code and offset), random edits vs a model, large-object index |
| `native/tests/native_test.cpp` | C++ conversions against an exact in-house oracle (decimal expansions): parse rounding, halfway cases, shortest output, fixed notation, escape → parse round trip |
| `chain/src/tests.rs` | wjlink: import library and archive layout, runtime object, command-line quoting |
| `tests/*.wave` | Wave API end to end: basics, editing, streaming (incl. 100k-line JSON Lines) |
| `tests/number_test.wave`, `tests/escape_test.wave` | the pure-Wave ports (below) against the C++ layer: same status code and the same bits or bytes for every input, including too-small buffers (defaults: 5,000 / 20,000 iterations) |

Increase the randomized iterations with `WJSON_FUZZ_ITERS` (defaults: 5,000
Rust / 50,000 C++). During development every randomized test was run for
1,000,000 iterations. `WJSON_TASK=sanitize` runs the C++ tests under
AddressSanitizer + UBSan and valgrind and the Rust unit tests under valgrind;
a run with 1,000,000 sanitized iterations and 30,000 valgrind iterations
(plus 200,000 Rust fuzz iterations) found nothing.

**Number and string code in Wave.** `src/number.wave` (parse, shortest and
fixed formatting of numbers) and `src/escape.wave` (string escaping, UTF-8
check and count) re-implement the C++ layer in Wave, and the Wave API uses them
(through `src/ffi.wave`); the C++ code stays for C and C++ users. The tests above
compare the Wave code bit for bit with the C++ code. Built
with `-O2` they run at about the speed of the C++ code (300,000 values, Wave vs
C++: formatting short decimals such as prices 6 ms vs 6 ms, parsing them 3 ms vs
4 ms, formatting random doubles 461 ms vs 439 ms); built with `-O0` they are
about 3x slower on short decimals (21 ms vs 7 ms formatting) and about 4x
slower on random doubles.

Performance (Linux, Xeon 2.8 GHz, one core, `WJSON_TASK=bench`):

| Operation | Result |
| --- | --- |
| DOM parsing | records 150–176 MB/s, number-heavy 145–197 MB/s, string-heavy 300–350 MB/s |
| Streaming parsing | 180–310 MB/s; memory fixed at chunk + one token |
| Key lookup (100k-key object) | hash index ~330 ns, linear scan ~150 µs |
| Array append (two arrays, 1M interleaved) | ~42 ns |
| f64 → text | ~175 ns (snprintf `%.17g` ~340 ns) |
| text → f64 | ~48 ns (strtod ~88 ns) |

### Windows (no MSVC)

The Windows build uses no MSVC, Visual Studio or Windows SDK:

- **Linking** is done by `wjlink` (`chain/`, Rust). `build.wave` first builds
  wjlink with rust-lld — wjlink itself is `no_std` and uses raw-dylib, so it
  links with no libraries at all — then `wjlink init` generates import
  libraries (kernel32.lib, msvcrt.lib, …) from the DLLs in
  `C:\Windows\System32` and writes the startup object `wjrt.obj`. cargo,
  wavec and the C++ test all use wjlink as their linker. The C runtime is the
  `msvcrt.dll` that ships with Windows.
- **C++** is compiled by clang in freestanding mode
  (`-ffreestanding -nostdlibinc`), without any standard-library or SDK headers.
- **Static libraries** are archived by `wjlink lib` instead of lib.exe.

Easiest: **double-click `setup-windows.cmd`**. It installs whatever is
missing (Rust, LLVM/clang, Wave) and runs `wavec run build.wave`; output goes
to `setup-log.txt` and `run-log.txt`. Accept the UAC prompt when LLVM installs.

Manual setup (once):

```powershell
# 1) Wave compiler (official installer) — open a new terminal afterwards
irm https://wave-lang.dev/install.ps1 -OutFile install.ps1; powershell -ExecutionPolicy Bypass -File .\install.ps1
# 2) Rust (includes rust-lld)
winget install Rustlang.Rustup
# 3) clang
winget install LLVM.LLVM
```

Wave's own Windows installer finishes with a smoke compile that, according to
its documentation, needs the Windows SDK and MSVC/UCRT libraries to be visible
(on GitHub's `windows-2025` runner it passes). That check belongs to the Wave
installer, not to wjson's build. If you have no SDK, download the
`wave-<version>-x86_64-pc-windows-msvc.zip` (`aarch64-pc-windows-msvc` on
Windows on ARM) from
[Wave's releases](https://github.com/wavefnd/Wave/releases), extract it keeping
its layout (`wavec.exe`, `std`, `llvm`), add that folder to `PATH` and pass
`--std-root` as below.

A normal PowerShell is enough (no Developer PowerShell). `build.wave` is
itself a Wave program, so wjlink has to exist before wavec can link it —
build it once, then pass it to wavec. Also pass the std location: the
Windows installer puts std next to `wavec.exe`, where wavec does not look on
its own. (`setup-windows.cmd` does all of this.)

```powershell
# once: build wjlink with rust-lld and generate the import libraries
cargo build --release --manifest-path chain/Cargo.toml --target-dir build/chain `
  --config "target.x86_64-pc-windows-msvc.linker='rust-lld'" `
  --config "target.x86_64-pc-windows-msvc.rustflags=['-Clink-arg=/NODEFAULTLIB']"
build\chain\release\wjlink.exe init --lld "$(rustc --print sysroot)\lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe"
# (on Windows on ARM use aarch64-pc-windows-msvc in all three places)

# then
$wj = "-Clinker=build\chain\release\wjlink.exe", "-L", "build\chain\release\lib"
wavec --std-root="$env:LOCALAPPDATA\Wave\bin\std" @wj run build.wave
$env:WJSON_RUN="examples/show_data.wave"; wavec --std-root="$env:LOCALAPPDATA\Wave\bin\std" @wj run build.wave
```

Verified on Windows 11 with wavec 0.2.1-pre-beta, Rust 1.99.1 and clang
23.1.3, with all MSVC paths removed from the environment.

### Using the libraries directly

Once built, programs can be compiled with wavec alone; `WJSON_TASK=libs`
prints the right link flags for the machine. The examples read their data
relative to `examples/`, so run them from there:

```sh
cd examples
wavec build show_data.wave ../build/native/libwjson_native.a ../build/cargo/release/libwjson_core.a \
      --dep=wjson=.. --link=gcc_s --link=pthread --link=m --link=dl --link=c \
      -L /usr/lib/gcc/x86_64-linux-gnu/15 --run   # the gcc directory is machine specific:
                                                  # use the flags `WJSON_TASK=libs` printed
```

### Vex

`vex.ws` follows the Vex manifest format (format 2, `lib = true`). Vex
(as of 2026-10) has no manifest field for linking native libraries and
rejects unknown fields, so wjson — which links Rust and C++ libraries —
cannot be built with `vex build` yet. Use `build.wave`.

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

## Layout

```
core/          Rust core: parser/dom/edit/stream (staticlib + unit tests, tests/fuzz.rs, examples/bench.rs)
native/        C++ conversion layer, wjson.hpp (C++ wrapper), C++ tests and benchmark
chain/         wjlink: MSVC-free Windows link driver (Rust)
include/       C header for the Rust core
src/           Wave API (lib.wave is the entry point); number.wave and escape.wave are the pure-Wave number and string code
tests/         Wave end-to-end tests and the port comparison tests
examples/      Wave examples and sample data
build.wave     build driver (Wave)
```

## Known limitations

- Wave is pre-beta; `src/` may need updates when the language changes.
- Declaring `extern(c) fun name(...)` without an explicit symbol in another
  module currently gets the name mangled by wavec and breaks linking, so
  `src/ffi.wave` always uses the `extern(c, "symbol")` form.
- Verified on Linux x86-64 and Windows 11 x64 (without MSVC). CI builds and
  runs the full test suite on every push on Linux x64 and ARM64
  (`ubuntu-26.04`, `ubuntu-26.04-arm`), Windows x64 and ARM64 (`windows-2025`,
  `windows-11-arm`) and macOS Apple Silicon and Intel (`macos-26`,
  `macos-26-intel`). On Windows the build targets the machine it runs on;
  wjlink produces x86-64 or ARM64 code (`WJSON_TARGET` overrides the choice,
  `wjlink init --arch x64|arm64` the architecture of the generated files).
- Before 1.0 the API may change; changes are listed in `CHANGELOG.md`.
- The Windows build uses the system `msvcrt.dll` as its C runtime (as MinGW
  does), which Microsoft does not recommend for applications. Only a few C99
  math functions (`fma`) come from `ucrtbase.dll`.
- `wjlink` is verified for what wjson needs (x64 and ARM64 console programs);
  it is not yet a general-purpose linker.

## Contributing and security

See `CONTRIBUTING.md`; report vulnerabilities as described in `SECURITY.md`.

## License

Mozilla Public License 2.0 (MPL-2.0) — see `LICENSE`. Copyright (c) 2026 potatosl and the
wjson contributors. Modified copies of the files in this repository must stay under the
MPL-2.0; programs that merely use wjson may use any license.
