# Contributing to wjson

Thanks for your interest! Issues and pull requests are welcome.

## Building and testing

You need [wavec](https://wave-lang.dev) (0.2.1-pre-beta, LLVM 21), Rust
(cargo) and a C++17 compiler (clang on Windows). From the repository root:

```sh
wavec run build.wave          # all tests and examples (Rust, C++, Wave)
```

On Windows, `setup-windows.cmd` installs what is missing and runs the same
build. No Visual Studio is needed (see README).

Please make sure the full suite passes before opening a pull request. For
changes to parsing or number conversion, also run the randomized tests
longer, e.g. `WJSON_FUZZ_ITERS=1000000 wavec run build.wave`.

## Guidelines

- Keep the project dependency-free: no external crates, no C++ libraries.
- The C++ sources must stay freestanding (no `#include` of standard library
  or platform headers).
- Every source file starts with `SPDX-License-Identifier: MIT`.
- Add tests with new behaviour: Rust unit tests in `core/`, C++ checks in
  `native/tests/`, end-to-end checks in `tests/*.wave`.
- Formatting: `cargo fmt` for Rust; 4-space indentation elsewhere.

By contributing you agree that your contributions are licensed under the
MIT License.
