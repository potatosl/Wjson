# Changelog

All notable changes are listed here. The project follows
[Semantic Versioning](https://semver.org); before 1.0 the API may change in
minor releases.

## Unreleased

### Wave
- `build.wave` now compiles every Wave program with `-O2` (wavec defaults to
  `-O0`, about 4x slower on the same code). On macOS arm64 it uses `-O0`: Wave's
  std makes raw syscalls without declaring that XNU overwrites `x1`, so optimized
  builds fail inside `std::mem::alloc` (Wave issue #933; found with `-O2` in this
  repository's CI). `WJSON_OPT` overrides the level.
- New `src/number.wave` and `src/escape.wave`: the number conversion and the
  string escaping / UTF-8 checks written in Wave. The Wave API now uses them
  (through `src/ffi.wave`) instead of the C++ functions, which remain for C and
  C++ users. `tests/number_test.wave` and `tests/escape_test.wave` compare them
  with the C++ layer (same status code, same bits or bytes). Without `-O2` the
  Wave code is about 4x slower than the C++ code; with `-O2` it is level.
- New `src/core_dom.wave` and `src/core_parse.wave`: the document model and
  the RFC 8259 parser (with the comment and trailing-comma options) written in
  Wave, a port of `core/src/dom.rs` and `parser.rs`. Not used by the Wave API
  yet, which still goes through the Rust core. `tests/core_parse_test.wave`
  compares them with the Rust parser on valid, relaxed and damaged inputs
  (same ok/error, error code, offset, line and column, and the same tree).
  Built with `-O2` the Wave parser runs at the Rust parser's speed (about
  310 MB/s on a 30 MB record file); with `-O0` it is 2.5x slower. It allocates
  through the C library, because Wave's std allocator maps fresh pages for every
  allocation.
- `num_format_f64` has the same fast path for short decimals as the C++ code
  (a decimal of at most 15 digits is unique for its double, so `m / 10^k`
  dividing back to the value exactly is also the shortest form), and the powers
  of ten come from a table. Formatting 300,000 prices-like values went from
  29 ms to 6 ms with `-O2` (C++: 6 ms) and from 55 ms to 21 ms with `-O0`.
  `tests/number_test.wave` compares short decimals, their neighbouring doubles
  and values with trailing zeros with the C++ code.
- `src/core_dom.wave` also has object lookup (`core_get`, the last duplicate key
  wins) and RFC 6901 JSON Pointer (`core_pointer`), ports of `dom.rs`.
  Objects with 16 or more members get a hash index that is built when the
  document is parsed, not on first lookup as in the Rust core: Wave's std has no
  mutex or atomics, and building the index up front means lookups never modify
  the document, so reading from several threads stays safe. The price is a
  somewhat slower parse of documents with large objects (4.8 ms vs 3.7 ms for a
  100,000-key object with `-O2`); lookups are faster (1M lookups: 35 ms vs 72 ms).
  `tests/core_lookup_test.wave` compares every key and many pointers (including
  `~0`/`~1`, leading zeros and out-of-range indexes) with the Rust core.
- New `install.sh`: `curl -fsSL .../install.sh | sh` downloads the sources
  (`WJSON_REF` picks a tag, branch or commit), builds the libraries and writes
  `~/.wjson/bin/wjson-wavec`, a wrapper that compiles a Wave program against
  them (`-O2`, or `-O0` on macOS arm64). It checks for `wavec`, Rust and a C++
  compiler and says how to install what is missing; it prints the SHA-256 of
  the download and can verify it (`WJSON_SHA256`). CI installs the commit under
  test with it on Linux and macOS and runs a program through the wrapper.
- New `src/core_edit.wave`: the editing operations (new values, `set_root`,
  array insert/set/remove/push, object set/remove/clear, deep copy including
  copying a document into itself), a port of `edit.rs`. It keeps the key index
  of large objects current as members are added and removed. Not used by the
  Wave API yet. `tests/core_edit_test.wave` applies the same random sequences of
  edits (including invalid ones) to a Rust document and a Wave document and
  compares every returned code, the whole tree and the answers of key lookups
  and JSON Pointers (1.2 million checks at 300 sequences).
- New `src/core_stream.wave`: the incremental pull parser (chunked input,
  `NEED_MORE`, trailing-comma / comment / multi-document options, depth and
  token-size limits), a port of `stream.rs`. Not used by the Wave API yet.
  `tests/core_stream_test.wave` steps the Rust reader and the Wave reader in
  lockstep on the same input, fed whole, byte by byte, in fixed sizes and in
  random chunks, and compares every event, the text that goes with it, depth,
  buffered bytes, document count and the error code, offset, line and column
  (3 million checks at 3000 random documents, valid and damaged).
- The Wave API now runs on the Wave document model, parser, editor and pull
  parser: `src/ffi.wave` serves the same functions from `core_dom`, `core_parse`,
  `core_edit` and `core_stream` instead of calling the Rust core. The Rust
  bindings moved to `src/ffi_rust.wave` and stay as the reference the
  differential tests compare against; the Rust core is still built and exported
  for C and C++ users. `tests/ffi_wave_test.wave` compares the handle layer with
  the Rust C ABI on null handles, failed parses, bad ids and lengths, error
  messages and readers (3,000 checks); the other tests run unchanged.
- Programs that use the Wave library no longer link the Rust core: the examples,
  the wrapper written by `install.sh`, `WJSON_RUN` and `WJSON_TASK=libs` build
  and link the C++ layer only (`--link=m --link=c`), so installing on Linux and
  macOS needs `wavec` and a C++ compiler, not Rust. The tests that compare with
  the Rust core still link it; `WJSON_CORE=1` adds it to `libs` and `WJSON_RUN`.
  Windows is unchanged (its link flags come with the Rust build). New
  `WJSON_TASK=apibench` / `tests/api_bench.wave` times the Wave document model
  and pull parser against the Rust core on a 24.6 MB records file (best of 5,
  `-O2`): parse 61 ms vs 94 ms, 600k key lookups 1 ms vs 4 ms, pull parser
  61 ms vs 62 ms; with `-O0` Wave is 2x to 5x slower (189 / 8 / 312 ms).
- Backend choice for unoptimized builds. The document, edit and reader
  functions of `src/ffi.wave` now live in `src/ffi_backend.wave` (Wave code);
  `src/ffi_rust.wave` is the same set on the Rust core. Wave is as fast as Rust
  with `-O2` and 2x to 5x slower with `-O0` (macOS arm64 builds with `-O0`), so
  `install.sh` copies the Rust version over `ffi_backend.wave`, builds the core
  and links it when the programs will be built with `-O0` and cargo is
  installed; otherwise it keeps the Wave backend. `WJSON_BACKEND=wave|rust|auto`
  overrides the choice. The installer's CI job checks both backends.
  `build.wave` notices a Rust `ffi_backend.wave` and links the core into every
  program. Checked: the whole suite and all examples on both backends, at
  `-O2` and `-O0`; the randomized tests at 100,000 documents (pull parser: 100
  million checks); the Wave tests under valgrind without errors or leaks; clippy.
- Fixes found by checking the Wave code against the Rust core from more
  angles: a document copied into itself no longer keeps half a copy when the
  copy fails (nesting over 2048 levels, or out of memory); the input size limit
  is 2 GiB exactly as in the Rust core (an input of 2,147,483,647 bytes was
  refused). Tests added: the nesting limit of copies, the 2 GiB boundary,
  `\u` escapes at the 1/2/3/4-byte encoding and surrogate boundaries, a long
  string fed byte by byte (no rescanning), and `WJSON_SEED=n` to run the random
  tests on other inputs. Eight seeds at 200,000 documents, out-of-memory runs
  (`ulimit -v`: every failure is a clean error or refusal, no crash), the
  examples' output identical on both backends, and mutation checks of limits,
  surrogate ranges and the parser's resume logic all came out clean.
- Parsing strings is about 20% cheaper in instructions: the UTF-8 check is
  skipped for strings that are plain ASCII (the scan for the closing quote
  notes whether any byte is 128 or above, escaped ones included), ASCII bytes
  inside a string that has other characters skip the decoder, and the decoder
  reserves room once per string and copies runs without a capacity check per
  byte. New `tests/api_bench_shapes.wave` (part of `WJSON_TASK=apibench`) times
  other document shapes, Wave against Rust, `-O2`, best of 5: float pairs 16 MB
  54 ms vs 47 ms, strings with escapes and UTF-8 23 MB 36 ms vs 23 ms, one object
  with 300,000 keys 12 ms vs 8 ms, a million pushes plus 200,000 `obj_set`
  37 ms vs 53 ms; the records file of `api_bench.wave` 58 ms vs 91 ms. So Wave
  is level or faster on records and edits and up to 1.5x slower on strings,
  numbers and very wide objects.
- Windows on ARM64 is supported, natively and without MSVC like x64. `wjlink`
  (the MSVC-free link driver) now takes the architecture as a value: import
  libraries use the matching machine type and RVA relocation, and the startup
  object `wjrt.obj` has ARM64 machine code for `mainCRTStartup` and `__chkstk`
  (assembled with llvm-mc; `wjlink init --arch x64|arm64` overrides the default,
  the architecture wjlink runs on). `build.wave` and `setup-windows.ps1` pick the
  Rust/clang target from the machine (`WJSON_TARGET` overrides). A new CI job on
  `windows-11-arm` runs the whole suite there (Rust and C++ tests, every Wave
  test and example). The x64 object and libraries are unchanged byte for byte,
  checked by a unit test.
- `.gitattributes` marks `*.wave` as Wave for GitHub's language statistics.

### Native
- `wjn_format_f64` has a fast path for short decimals (12.5, 0.001234, ...): a
  decimal of at most 15 significant digits is unique for its double, so when
  `m / 10^k` divides back to the value exactly it is also the shortest form.
  Anything else still uses the general algorithm. About 25% faster on the
  benchmark's mixed values (about 127 ns to 97 ns; about half of them are
  short decimals); the output is unchanged: identical to the previous
  implementation on 320 million values (random bits, short decimals and their
  neighbouring doubles, integers, powers of ten).
- `native_test` checks short decimals and their neighbours against the exact
  shortest-form oracle, plus fixed cases for trailing zeros and the
  fixed/scientific choice.

### Verification
- No findings from 1,000,000 AddressSanitizer + UBSan iterations of the C++
  tests, 30,000 valgrind iterations and 200,000 Rust fuzz iterations.

## 0.4.0 — 2026-10-17

### License
- Relicensed from MIT to the Mozilla Public License 2.0. Version 0.3.0 was
  published under the MIT License and stays available under it; this and later
  versions are MPL-2.0. Every source file now carries
  `SPDX-License-Identifier: MPL-2.0`.

### Security
- Streaming reader: an incomplete string or comment is no longer rescanned
  from its first byte on every chunk (quadratic time when fed in small
  pieces), and the bytes held for one token are capped (`wj_reader_set_max_token`,
  default 256 MiB, `json_reader_set_max_token` / `json_stream_set_max_token`).
  Number lexemes over 64 KiB are refused with `TOO_LARGE`.
- `max_depth` is clamped to `JSON_MAX_DEPTH_LIMIT` (2048); deeper requests used
  to overflow the stack. `json_copy` returns the new `JSON_EDIT_TOO_DEEP` (-7)
  past that depth, the writer fails with `JSON_WRITER_ERR_DEPTH`, and
  `json_equal` / `json_depth` stop recursing.
- Lookups in large objects no longer mutate an unsynchronized cache: reading a
  document from several threads is safe (the key index sits behind a mutex).
- Wave: `json_as_str_checked`, `json_str_has_nul` and `json_key_at_bytes`
  expose strings that contain `\u0000`. Serializing and `json_equal` no longer
  cut object keys at an embedded NUL.
- `wjn_utf8_check` reports a null pointer or negative length as invalid
  instead of valid; `wjson.hpp` `escape()` checks its allocation in
  freestanding builds.
- CI: actions pinned to a commit, Wave tarball SHA-256 verified, read-only
  token. `setup-windows.ps1` pins the Wave installer's SHA-256.
- wjlink: the PE export-table reader no longer trusts the image (a header
  claiming billions of names sized an allocation; RVA arithmetic could wrap).
  `rust-lld` is resolved through PATH only, never the current directory, where
  Windows would look first; a PATH or executable path longer than the old fixed
  buffers is handled; a short read is an error instead of a silently smaller
  file; the temporary `link-<pid>.rsp` is deleted after each link.
- `JsonWriter` refuses absurd buffer sizes instead of overflowing while
  doubling; `json_write_raw*` documents that it must never receive untrusted
  text.
- `json_clear` on an object is linear (new `wj_obj_clear`) instead of quadratic.
- Examples: `csvlib` bounds-checks every cell and `csv_check_header` rejects
  files whose header rows are missing or ragged (the examples used to index
  past the field tables); missing keys print `?` instead of a null string.

### Platforms
- CI now builds and runs the full test suite on Linux ARM64, macOS Apple Silicon
  and macOS Intel as well (all passed on their first run), next to Linux x64 and
  Windows x64. This also exercises the portable (non-x86) path of the C++
  number conversion on ARM. Windows on ARM remains unsupported.

### Toolchain
- Rust edition 2021 -> 2024 for `core/` and `chain/` (`rust-version = "1.99"`,
  the version everything is tested with). C ABI entry points use
  `#[unsafe(no_mangle)]`; wjlink's Win32 and CRT declarations are
  `unsafe extern`. Windows code is now also type-checked and linked from
  Linux (`--target x86_64-pc-windows-msvc`).
- C++17 -> C++20 for the native layer, its tests and `wjson.hpp`.
- Wave stays at v0.2.1-pre-beta, the newest stable release (LLVM 21).
- CI: `ubuntu-26.04` and `windows-2025` runners, `actions/checkout` v7.0.1
  (pinned to its commit), Rust updated to stable before building.

### Added
- `WJSON_TASK=sanitize`: the C++ tests under ASan + UBSan, then the C++ and
  Rust tests under valgrind. CI runs it, and runs `clippy -D warnings` on both
  crates (wjlink also for the Windows target).

### Changed
- Version 0.4.0 (core, `vex.ws`); wjlink 0.2.0.
- Clippy-clean core and wjlink; the shared safety contract of the C ABI is documented once
  at the top of `core/src/lib.rs`.

## 0.3.0 — 2026-10-16

First public release, under the MIT License.

### Added
- `chain/` — **wjlink**, an MSVC-free Windows link driver written in Rust.
  It generates import libraries from the system DLLs' export tables,
  emits the runtime startup object (`wjrt.obj`), archives objects
  (`wjlink lib`), and links through rust-lld. cargo, wavec and the C++ test
  all use it, so a Windows build needs no Visual Studio, MSVC or Windows SDK.
- Freestanding C++: the native layer and its tests build with
  `clang -ffreestanding -nostdlibinc` (no standard library or SDK headers).
- Own number conversion in C++: correctly rounded parsing for any number of
  digits, shortest round-trip formatting, exact fixed formatting.
- `native/tests/native_test.cpp` checks numbers against an exact in-house
  oracle (decimal expansions) instead of the C runtime.
- `wjson.hpp` works with or without a C++ standard library.
- Synthetic example data (`examples/data/survey_sample.csv`).

### Changed
- `setup-windows.ps1` installs Rust, LLVM (clang) and Wave only, builds
  wjlink first and links `build.wave` itself with it.
- f64 formatting is ~175 ns (was ~73 ns with `<charconv>`); parsing ~48 ns.

## 0.2.0

- Document editing API, streaming parser (JSON Lines), key hash index,
  benchmarks and differential fuzzing with an in-house reference parser;
  zero external dependencies.

## 0.1.0

- Rust parser core, C++ conversions, Wave API, writer, path queries.
