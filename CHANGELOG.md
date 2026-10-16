# Changelog

All notable changes are listed here. The project follows
[Semantic Versioning](https://semver.org); before 1.0 the API may change in
minor releases.

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
