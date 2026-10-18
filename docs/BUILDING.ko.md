# 설치, 빌드, 테스트

wjson 설치와 빌드, 테스트, MSVC 없는 Windows 빌드를 설명합니다. [README](../README.ko.md)로 돌아가기.

## 사용 방법

wjson은 소스로 받아서 씁니다. 정적 라이브러리 두 개를 한 번 빌드한 뒤, 내 프로그램을 그 라이브러리에 링크해서 컴파일하면 돼요.

**빠른 설치 (Linux, macOS).** 명령 한 줄이면 소스를 받아 라이브러리를 빌드하고, Wave 프로그램을 그 라이브러리에 맞춰 컴파일해 주는 래퍼까지 만들어 줘요. `wavec`와 C++ 컴파일러가 `PATH`에 있어야 하고(아래 1단계), 없으면 알려 주기만 할 뿐 대신 설치하지는 않아요.

```sh
curl -fsSL https://raw.githubusercontent.com/potatosl/wjson/main/install.sh | sh
~/.wjson/bin/wjson-wavec hello.wave --run     # hello.wave는 3단계 참고
```

스크립트는 짧으니 실행 전에 [읽어 보세요](install.sh). `~/.wjson`에 설치되고(폴더를 지우면 제거돼요), `WJSON_REF=<태그 또는 커밋>`으로 `main` 대신 버전을 고르고, `WJSON_DIR`로 설치 위치를, `WJSON_SHA256`으로 받은 파일의 SHA-256이 다르면 중단하도록 지정할 수 있어요(실제 값은 항상 출력돼요). 아래 단계는 같은 일을 손으로 하는 방법이고, Windows에서는 아래 단계 또는 `setup-windows.cmd`를 쓰세요.

**1. 도구 설치** — Wave 컴파일러 `wavec`(0.2.1-pre-beta, 현재 최신)와 C++20 컴파일러. Rust 1.99 이상은 전체 테스트를 돌릴 때, 그리고 `install.sh`로 설치하면서 프로그램을 `-O0`으로 빌드하는 환경(macOS arm64)에서 필요해요. 그 환경에서는 설치기가 더 빠른 Rust 백엔드를 골라요(`WJSON_BACKEND=wave|rust|auto`). Wave 공식 설치 방법은 [설치 페이지](https://wave-lang.dev/docs/en/getting-started/install)에 있어요:

```sh
curl -fsSL https://wave-lang.dev/install.sh | bash     # Linux, macOS, FreeBSD
```

Windows는 [Windows (MSVC 없이)](#windows-msvc-없이)를 보세요. `setup-windows.cmd`가 필요한 걸 전부 설치해 줘요.

**2. wjson 받고 라이브러리 빌드**

```sh
git clone https://github.com/potatosl/wjson.git
cd wjson
WJSON_TASK=libs wavec run build.wave
```

마지막 줄들에 라이브러리 경로 두 개와, 내 컴퓨터에 맞는 링커 플래그가 든 `link :` 줄이 출력돼요. 그 줄을 4단계에서 써요.

**3. 프로그램 작성** (`hello.wave`, 위치는 아무 데나 — 예를 들면 `wjson` 폴더 옆):

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

**4. 컴파일하고 실행**

```sh
wavec build hello.wave \
      wjson/build/native/libwjson_native.a wjson/build/cargo/release/libwjson_core.a \
      --dep=wjson=wjson \
      --link=gcc_s --link=util --link=rt --link=pthread --link=m --link=dl --link=c \
      -L /usr/lib/gcc/x86_64-linux-gnu/15 --run
```

wavec는 따로 지정하지 않으면 `-O0`으로 컴파일해요. wjson의 네이티브 코드는 이미 최적화돼 있지만, 내 Wave 코드는 `-O2`(`wavec -O2 build ...`)를 주면 몇 배 빨라져요. 단 macOS arm64에서는 최적화 빌드가 Wave의 `std::mem::alloc` 안에서 깨져요(Wave 이슈 [#933](https://github.com/wavefnd/Wave/issues/933)). 고쳐질 때까지는 거기서 `-O0`을 쓰세요.

`0.0.0.0:8080`이 출력돼요. `--dep=wjson=wjson`은 받은 폴더를 가리켜요. `--link=...`와 `-L ...` 부분은 2단계의 `link :` 줄로 바꿔 쓰세요(gcc 폴더는 컴퓨터마다 달라요). `wavec`이 표준 라이브러리를 못 찾는다고 하면(`wavec print std-path`가 찾는 위치를 보여 주고, 설치 스크립트는 `~/.wave/bin/std`에 둬요) `--std-root=<Wave의 std가 있는 폴더>`를 붙이거나, Wave 문제 해결 페이지의 안내처럼 `wavec install std`를 실행하세요.

**C / C++에서 쓰기** (Wave 없이) — 같은 라이브러리 두 개를 링크하고 헤더를 include 하면 돼요. `wjson.hpp`는 헤더만으로 된 C++ 래퍼이고, `wjson.h`는 순수 C API예요(`libwjson_core.a`만 있으면 됩니다):

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

다음은 [안내서](GUIDE.ko.md), [API 요약](API.ko.md), 더 긴 프로그램이 있는 `examples/`를 보세요.

## 빌드와 테스트

빌드 드라이버도 Wave로 짜여 있어요(`build.wave`). 필요한 것: [wavec](https://wave-lang.dev/docs/en/getting-started/install)(LLVM 21 기반), Rust 1.99 이상(cargo), C++20 컴파일러.

```sh
wavec run build.wave                                   # Rust + C++ + Wave 테스트, 예제 전부 실행
WJSON_RUN=examples/show_data.wave wavec run build.wave # 프로그램 하나 빌드 후 실행
WJSON_TASK=libs wavec run build.wave                   # 정적 라이브러리만 빌드 (+ 링크 플래그 출력)
WJSON_TASK=bench wavec run build.wave                  # 성능 측정 (Rust 코어 + C++ 변환 계층)
WJSON_DRY_RUN=1 wavec run build.wave                   # 실행할 명령만 출력
```

테스트 구성:

| 테스트 | 내용 |
| --- | --- |
| `core/src` 단위 테스트 | 파서·수정·키 인덱스·스트리밍 (조각 크기 1바이트까지) |
| `core/tests/fuzz.rs` | 직접 만든 레퍼런스 파서와 대조, 스트리밍 vs DOM(오류 코드·위치까지), 무작위 수정 vs 모델, 큰 객체 인덱스 |
| `native/tests/native_test.cpp` | C++ 변환 + 자체 정확 오라클(10진 전개)과 무작위 대조: 파싱 반올림, 중간값(tie), 최단 출력, 고정소수점, 이스케이프→파싱 왕복 |
| `chain/src/tests.rs` | wjlink: import 라이브러리·아카이브 구조, 런타임 오브젝트, 명령줄 인용 |
| `tests/*.wave` | Wave API 종단 간: 기본 기능, 수정, 스트리밍(10만 줄 JSON Lines 포함) |
| `tests/number_test.wave`, `tests/escape_test.wave` | 순수 Wave 포팅(아래)을 C++ 레이어와 대조: 모든 입력에서 상태 코드와 결과 비트·바이트가 같은지(버퍼가 모자란 경우 포함, 기본 5,000 / 20,000회) |

퍼징 횟수는 `WJSON_FUZZ_ITERS`로 늘릴 수 있어요(기본 5,000 / C++ 50,000). 개발 중에는 테스트별 1,000,000회로 돌려서 통과를 확인했어요. `WJSON_TASK=sanitize`는 C++ 테스트를 AddressSanitizer + UBSan과 valgrind로, Rust 단위 테스트를 valgrind로 돌려요. 새니타이저 100만 회, valgrind 3만 회(그리고 Rust 퍼징 20만 회)로 돌렸을 때 발견된 문제는 없었어요.

**Wave로 쓴 숫자·문자열 코드.** `src/number.wave`(숫자 파싱, 최단·고정소수점 출력)와 `src/escape.wave`(문자열 이스케이프, UTF-8 검사·개수)는 C++ 레이어를 Wave로 다시 구현한 거고, Wave API가 `src/ffi.wave`를 통해 이걸 써요. C++ 코드는 C/C++ 사용자를 위해 그대로 두고, 위 테스트가 Wave 코드를 C++ 코드와 비트 단위로 대조해요. `-O2`로 빌드하면 C++과 비슷한 속도예요(값 30만 개 기준 Wave 대 C++: 가격 같은 짧은 10진수 출력 6ms 대 6ms, 파싱 3ms 대 4ms, 무작위 double 출력 461ms 대 439ms). `-O0`으로 빌드하면 짧은 10진수는 약 3배(출력 21ms 대 7ms), 무작위 double은 약 4배 느려요.

성능 (Linux, Xeon 2.8GHz 1코어, `WJSON_TASK=bench`):

| 항목 | 결과 |
| --- | --- |
| DOM 파싱 | 레코드형 150~176 MB/s, 숫자 위주 145~197 MB/s, 문자열 위주 300~350 MB/s |
| 스트리밍 파싱 | 180~310 MB/s, 메모리는 청크 + 토큰 1개 크기로 고정 |
| 키 조회 (키 10만 개 객체) | 해시 인덱스 약 330 ns, 선형 탐색 약 150 µs |
| 배열 추가 (두 배열 번갈아 100만 회) | 약 42 ns/회 |
| f64 → 문자열 | 약 175 ns (snprintf `%.17g` 약 340 ns) |
| 문자열 → f64 | 약 48 ns (strtod 약 88 ns) |

### Windows (MSVC 없이)

Windows 빌드는 MSVC·Visual Studio·Windows SDK를 쓰지 않아요. 대신:

- **링크**: `chain/`의 `wjlink`(Rust)가 맡아요. `build.wave`가 맨 먼저 rust-lld로 wjlink를 빌드하고(wjlink 자체는 `no_std` + raw-dylib라 아무 라이브러리 없이 링크돼요), `wjlink init`이 `C:\Windows\System32`의 DLL에서 import 라이브러리(kernel32.lib, msvcrt.lib 등)를 직접 만들고 시작 코드 `wjrt.obj`를 생성해요. 이후 cargo, wavec, C++ 테스트 모두 wjlink를 링커로 써요. C 런타임은 Windows에 기본으로 들어 있는 `msvcrt.dll`이에요.
- **C++**: clang으로 freestanding 컴파일해요(`-ffreestanding -nostdlibinc`). 표준 라이브러리·SDK 헤더를 하나도 안 써요.
- **정적 라이브러리**: `wjlink lib`가 lib.exe 대신 묶어요.

가장 쉬운 방법은 **`setup-windows.cmd`를 더블클릭**하는 거예요. 빠진 도구(Rust, LLVM/clang, Wave)를 설치하고 `wavec run build.wave`까지 실행해요. 출력은 `setup-log.txt`, `run-log.txt`에 남아요. LLVM 설치 때 UAC 창이 뜨면 "예"를 눌러 주세요.

직접 설치하려면(한 번만):

```powershell
# 1) Wave 컴파일러 (공식 설치 스크립트) — 설치 후 터미널을 새로 열기
irm https://wave-lang.dev/install.ps1 -OutFile install.ps1; powershell -ExecutionPolicy Bypass -File .\install.ps1
# 2) Rust (rust-lld가 같이 들어 있어요)
winget install Rustlang.Rustup
# 3) clang
winget install LLVM.LLVM
```

Wave의 Windows 설치 스크립트는 마지막에 스모크 컴파일을 하는데, Wave 문서에 따르면 이때 Windows SDK와 MSVC/UCRT 라이브러리가 보여야 해요(GitHub의 `windows-2025` 러너에서는 통과해요). 이 검사는 wjson 빌드가 아니라 Wave 설치 스크립트의 것이에요. SDK가 없다면 [Wave 릴리스](https://github.com/wavefnd/Wave/releases)에서 `wave-<버전>-x86_64-pc-windows-msvc.zip`을 받아 폴더 구조(`wavec.exe`, `std`, `llvm`)를 그대로 풀고, 그 폴더를 `PATH`에 추가한 뒤 아래처럼 `--std-root`를 넘기세요.

일반 PowerShell에서 실행하면 돼요(Developer PowerShell 불필요). `build.wave`도 wavec가 링크해야 하는 Wave 프로그램이라, wjlink를 먼저 한 번 빌드한 뒤 wavec에 넘겨 줘야 해요. std 위치도 함께 넘겨 주세요. Windows 설치 프로그램은 std를 `wavec.exe` 옆에 두는데, wavec가 그 위치를 자동으로 찾지 않아요. (`setup-windows.cmd`는 이걸 전부 알아서 해요.)

```powershell
# 한 번만: rust-lld로 wjlink 빌드 + import 라이브러리 생성
cargo build --release --manifest-path chain/Cargo.toml --target-dir build/chain `
  --config "target.x86_64-pc-windows-msvc.linker='rust-lld'" `
  --config "target.x86_64-pc-windows-msvc.rustflags=['-Clink-arg=/NODEFAULTLIB']"
build\chain\release\wjlink.exe init --lld "$(rustc --print sysroot)\lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe"

# 그다음부터
$wj = "-Clinker=build\chain\release\wjlink.exe", "-L", "build\chain\release\lib"
wavec --std-root="$env:LOCALAPPDATA\Wave\bin\std" @wj run build.wave
$env:WJSON_RUN="examples/show_data.wave"; wavec --std-root="$env:LOCALAPPDATA\Wave\bin\std" @wj run build.wave
```

Windows 11 + wavec 0.2.1-pre-beta + Rust 1.99.1 + clang 23.1.3 조합에서, MSVC 경로를 전부 비운 상태로 전체 테스트가 통과하는 것을 확인했어요.

`build.wave`는 모든 Wave 프로그램(테스트와 예제)을 `-O2`로 컴파일해요. wavec의 기본값 `-O0`은 같은 코드에서 약 4배 느렸어요. macOS arm64에서는 대신 `-O0`을 써요. Wave std가 커널이 덮어쓰는 레지스터를 선언하지 않고 시스템 호출을 해서 최적화 빌드가 깨지기 때문이에요([#933](https://github.com/wavefnd/Wave/issues/933)). `WJSON_OPT=-O2`처럼 환경 변수로 직접 지정할 수도 있어요.

Wave의 `main`은 인자를 받을 수 없어서 작업 선택은 환경 변수로 해요. 그 밖에 `WAVEC`, `WAVE_STD`(std 루트 직접 지정), `CARGO`, `RUSTC`, `CXX`를 쓸 수 있어요. `std::process`가 아직 Windows에서 프로세스를 띄우지 못해서, 드라이버는 C 라이브러리의 `system()`을 `extern(c)`로 불러 명령을 실행해요.

라이브러리를 한 번 빌드한 뒤에는 wavec만으로 바로 빌드·실행할 수도 있어요. `WJSON_TASK=libs`가 이 머신에 맞는 링크 플래그를 출력해 줘요. 예제는 데이터 파일을 `examples/` 기준 상대 경로로 읽으니 `examples/` 안에서 실행하세요.

```sh
cd examples
wavec build show_data.wave ../build/native/libwjson_native.a ../build/cargo/release/libwjson_core.a \
      --dep=wjson=.. --link=gcc_s --link=pthread --link=m --link=dl --link=c \
      -L /usr/lib/gcc/x86_64-linux-gnu/15 --run   # gcc 폴더는 컴퓨터마다 달라요:
                                                  # `WJSON_TASK=libs`가 출력한 플래그를 쓰세요
```

`vex.ws`는 Vex 매니페스트 규격(format 2, `lib = true`)에 맞춰 두었어요. 다만 Vex(2026-10-12 기준)의 매니페스트에는 네이티브 라이브러리를 링크하는 필드가 없고 모르는 필드는 오류로 처리돼서, Rust·C++ 라이브러리를 링크해야 하는 wjson은 아직 `vex build`로 빌드할 수 없어요. 빌드는 `build.wave`로 해요.
