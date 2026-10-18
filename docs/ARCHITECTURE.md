# Architecture and limitations

How the source is organised, and what is not finished. Back to the [README](../README.md).

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
