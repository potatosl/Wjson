# Installing, building and testing

Installing wjson, building it, the tests, Windows without MSVC. Back to the [README](../README.md).

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

The script is short, [read it](../install.sh) before running it. It installs into
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

Next: the [guide](GUIDE.md), the [API overview](API.md), and `examples/` for longer programs.

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
