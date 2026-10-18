<p align="center">
  <a href="https://wave-lang.dev"><img src="docs/wave.svg" alt="Wave" width="220"></a>
</p>

# wjson — Wave용 JSON 라이브러리

[English](README.md) | **한국어**

Rust, C++, Wave가 각자 잘하는 일을 나눠 맡는 JSON 라이브러리입니다.

| 계층 | 언어 | 담당 | 위치 |
| --- | --- | --- | --- |
| API | **Wave** | 파서, 문서 모델, 수정 연산, JSON Pointer, 증분(pull) 파서(`core_*.wave`, `ffi.wave`로 연결), 탐색, 경로 쿼리, 문서 수정·경로 기반 수정, writer, 직렬화, 깊은 비교, 파일 스트리밍 | `src/*.wave` |
| 코어 | **Rust** | 같은 RFC 8259 파서, 인덱스 기반 DOM, 수정 연산, 해시 인덱스, pull 파서, JSON Pointer, 에러 위치. C/C++ 사용자용이며 Wave 코드를 테스트할 때 비교하는 기준(`src/ffi_rust.wave`) | `core/` |
| 네이티브 | **C++20** (freestanding) | 숫자 변환(자체 구현: 정확한 반올림 파싱·최단 왕복 출력), 문자열 이스케이프, UTF-8 검사. 표준 라이브러리·SDK 헤더 없이 컴파일 | `native/` |
| 툴체인 | **Rust** | `wjlink`: Windows에서 MSVC 대신 쓰는 링크 도구. 시스템 DLL에서 import 라이브러리 생성, 시작 코드(wjrt) 생성, 아카이버, rust-lld 구동 | `chain/` |

Wave ↔ Rust/C++ 경계는 모두 C ABI(`extern(c)`)입니다. C/C++ 프로그램도 `include/wjson.h`, `native/include/wjson.hpp`로 같은 코어를 쓸 수 있습니다.

**외부 의존성 0개.** Rust crate도 C++ 라이브러리도 받지 않습니다(`core/Cargo.lock`에는 `wjson_core` 하나뿐). 퍼징에 쓰는 비교용 레퍼런스 파서도 테스트 안에 직접 구현돼 있습니다. 필요한 건 빌드 도구(wavec, cargo, C++ 컴파일러)뿐이고, Windows에서도 **MSVC/Visual Studio는 필요 없습니다**.

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

다음은 아래 예제들, [API 요약](#api-요약-wave), 더 긴 프로그램이 있는 `examples/`를 보세요.

## 빠른 예제

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

JSON 만들기:

```wave
var w: JsonWriter = json_writer_new(2);      // 들여쓰기 2칸, 0이면 한 줄
json_begin_object(&w);
json_field_str(&w, "name", "wave");
json_key(&w, "tags");
json_begin_array(&w);
json_write_i64(&w, 1);
json_write_f64(&w, 2.5);
json_end_array(&w);
json_end_object(&w);
println("{}", json_writer_finish(&w));       // 구조가 잘못되면 null 문자열
json_writer_free(&w);
```

문서 수정:

```wave
var doc: JsonDoc = json_parse("{\"name\": \"wave\", \"tags\": []}");
var root: Json = json_root(doc);
json_set_str(root, "name", "wjson");               // 값 바꾸기
json_set_path_i64(root, "limits.depth", 512);      // 중간 객체 자동 생성
json_push_str(json_get(root, "tags"), "json");     // 배열에 추가
json_remove(root, "obsolete");                     // 키 삭제
```

큰 파일 스트리밍(파일 크기와 관계없이 메모리 일정):

```wave
var s: JsonStream = json_stream_open("big.jsonl", JSON_STREAM_MULTI);
var ev: i32 = json_stream_next(&s);
while (ev != JSON_EV_END && ev != JSON_EV_ERROR) {
    if (ev == JSON_EV_KEY && json_stream_text_eq(&s, "id")) { ... }
    ev = json_stream_next(&s);
}
json_stream_close(&s);
```

더 긴 예제는 `examples/config.wave`(파일 읽기 + 주석 허용 설정 파일), `examples/transform.wave`(필터링 후 새 JSON 출력)에 있습니다.
`examples/edit_config.wave`(설정 파일 수정), `examples/stream_rows.wave`(4KB 청크로 스트리밍하며 조건 검색)도 있습니다.
`examples/survey_report.wave`는 같은 CSV를 수정 API만으로 문서로 만든 뒤(writer로 만든 결과와 완전히 같은지 확인), 제조업 23개 업종을 새 문서로 복사해서 정렬·순위·필드 이름 변경·요약까지 하고 `data/survey_report.json`으로 저장하는 예제예요. CSV 읽기 코드는 `examples/csvlib.wave`에 있어요.
`examples/csv2json.wave`는 통계 포털 형식의 CSV(헤더 3줄, 따옴표 필드 포함)를 중첩 JSON으로 변환한 뒤, 다시 파싱해서 1,449개 셀을 원본과 대조하는 테스트입니다. 예제 데이터는 이 저장소용으로 만든 **가짜(합성) 데이터**예요(`examples/data/README.md`).

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

## API 요약 (Wave)

모두 `import("wjson")::{...}`로 가져옵니다.

**파싱** — `json_parse(text)`, `json_parse_with(text, max_depth, flags)`, `json_parse_bytes(ptr, len, max_depth, flags)`, `json_ok(doc)`, `json_error(doc) -> JsonError{code, message, offset, line, column}`, `json_root(doc)`, `json_free(&doc)`

- flags: `JSON_STRICT`(기본), `JSON_ALLOW_TRAILING_COMMAS`, `JSON_ALLOW_COMMENTS`, `JSON_RELAXED`(둘 다)
- 기본 최대 중첩 깊이 512, 파싱이 재귀라서 `max_depth`를 아무리 크게 줘도 `JSON_MAX_DEPTH_LIMIT`(2048)로 제한, UTF-8 BOM 허용, 잘못된 UTF-8·홀로 있는 서로게이트는 거부

**탐색** — `json_get(v, key)`, `json_at(v, i)`, `json_key_at(v, i)`, `json_len(v)`, `json_has(v, key)`, `json_pointer(v, "/a/0/b")`, `json_path(v, "a.b[0][\"x.y\"]")`

- 없는 키·범위 밖 인덱스·타입 불일치는 실패 대신 "invalid" 값을 돌려줘서 체인이 항상 안전합니다. 끝에서 `json_is_valid()`로 확인하세요.
- 중복 키는 마지막 값이 이깁니다(JavaScript `JSON.parse`와 동일).

**값 읽기** — `json_type`, `json_is_*`, `json_as_bool`, `json_as_str`, `json_as_bytes`, `json_as_str_checked`, `json_str_has_nul`, `json_as_i64`, `json_as_f64`(+ `_checked` 버전), `json_number_text`(원문 숫자), 기본값 버전 `json_i64_or` / `json_f64_or` / `json_bool_or` / `json_str_or`, 경로+기본값 `json_path_i64` 등, `json_str_eq`

**쓰기** — `json_writer_new(indent)`, `json_begin_object/array`, `json_end_object/array`, `json_key`, `json_write_null/bool/i64/f64/f64_fixed/str/raw`, `json_field_*`, `json_writer_finish`, `json_writer_status`, `json_writer_reset`, `json_writer_free`, `json_writer_set_escape(JSON_ESCAPE_ASCII | JSON_ESCAPE_HTML)`

- 쉼표·콜론·중첩을 writer가 관리하고, 순서가 틀린 호출(키 없이 값, 닫는 괄호 불일치, 최상위 값 두 개 등)은 에러 상태가 됩니다.
- NaN/무한대는 JSON에 표현이 없으므로 `JSON_WRITER_ERR_NUMBER`.

**트리** — `json_write_value(&w, v)`, `json_to_string(v, indent)`, `json_to_string_with(v, indent, escape_flags)`, `json_string_free(s)`, `json_equal(a, b)`(멤버 순서·숫자 표기 무관), `json_depth(v)`

**수정** — 새 값: `json_new_document()`, `json_new_null/bool/i64/f64/str/array/object(doc, …)`, `json_new_number_text(doc, "12.50")`, `json_new_number_bytes(doc, p, n)`, `json_new_f64_fixed(doc, 82.349, 1)`(→ `82.3`), `json_copy(doc, v)`(다른 문서에서도), `json_set_root(doc, v)`
객체: `json_set(obj, key, v)`, `json_set_i64/f64/str/bool/null`, `json_set_object/array`(새 컨테이너를 넣고 돌려줌), `json_remove(obj, key)`
배열: `json_push`, `json_push_i64/…/object/array`, `json_insert(arr, i, v)`, `json_replace(arr, i, v)`, `json_remove_at(arr, i)`, `json_clear(v)`
경로: `json_set_path(root, "a.b[0].c", v)`(중간 객체 자동 생성), `json_set_path_i64/f64/str/bool`, `json_remove_path`
구조: `json_parent(v)`, `json_is_attached(v)`, `json_edit_error(v)`

- 결과는 `JSON_EDIT_*` 코드예요. 값은 한 곳에만 붙을 수 있고(`JSON_EDIT_ATTACHED` → `json_copy`), 자기 자신 안에 넣으면 `JSON_EDIT_CYCLE`, NaN·무한대·잘못된 숫자 문자열은 `JSON_EDIT_BAD_VALUE`.
- 교체·삭제된 값은 "떨어진" 상태가 되고 다시 붙일 수 있어요. 파싱에 실패한 문서는 수정할 수 없어요.
- 키 16개 이상인 객체는 첫 조회 때 해시 인덱스를 만들고, 키 추가·삭제 때 함께 갱신해요.

**스트리밍** — 저수준: `json_reader_new(flags)`, `json_reader_feed/feed_str/finish/next`, `json_reader_text/text_eq/i64/f64`, `json_reader_depth/documents/buffered/error`, `json_reader_set_max_token`, `json_reader_free`
파일: `json_stream_open(path, flags)`, `json_stream_open_with(path, flags, chunk)`, `json_stream_next(&s)`, `json_stream_skip(&s)`(현재 컨테이너 통째로 건너뛰기), `json_stream_text/text_eq/i64/f64`, `json_stream_depth/documents/bytes_read/buffered/error/io_error`, `json_stream_set_max_token`, `json_stream_close(&s)`

- 이벤트: `JSON_EV_OBJECT_START/END`, `ARRAY_START/END`, `KEY`, `STRING`, `NUMBER`, `TRUE`, `FALSE`, `NULL`, `END`, `ERROR`(리더는 `NEED_MORE`도).
- flags: `JSON_ALLOW_TRAILING_COMMAS`, `JSON_ALLOW_COMMENTS`, `JSON_STREAM_MULTI`(여러 최상위 값, JSON Lines).
- DOM 파서와 같은 문법·같은 오류 코드·같은 오류 위치를 보고해요(퍼징으로 확인).

**문자열 null** — Wave의 `str`은 null을 가질 수 없어서, "문자열 없음"은 포인터가 null인 `str`로 돌려줍니다. `json_str_is_null(s)`로 확인하세요.

## 메모리 규칙

- `Json` 값과 `json_as_str`·`json_key_at`·`json_number_text`가 돌려준 문자열은 문서 안에 있으므로 `json_free` 이후에는 쓰면 안 됩니다. 반대로 `json_free` 전까지는 **문서를 계속 수정해도 유효해요.** 파싱된 텍스트는 파싱 후 다시 쓰지 않고, 수정으로 생기는 텍스트는 각자 따로 할당해서 옮겨지지 않아요. 그래서 `json_set_str(v, "새 키", json_as_str(json_get(v, "옛 키")))`처럼 같은 문서의 문자열을 그대로 넘겨도 안전해요.
- **문자열 안의 NUL.** JSON 문자열에는 `\u0000`이 들어갈 수 있고 `str`은 첫 NUL에서 끝나서, `"admin\u0000.txt"`는 `json_as_str`·`json_key_at`으로 읽으면 `admin`이 돼요. 믿을 수 없는 문서에는 `json_as_str_checked`(NUL이 있으면 null), `json_str_has_nul`, 또는 길이를 정확히 주는 `json_as_bytes` / `json_key_at_bytes` / `json_str_eq`를 쓰세요.
- **스레드.** 수정하지 않는 문서를 읽는 일(`json_get`, `json_at`, `json_path` 등)은 여러 스레드에서 동시에 해도 안전해요. 수정은 혼자만 써야 하고, reader/stream은 한 번에 한 스레드만 쓰세요.
- **스트리밍 한도.** 문자열·주석 하나가 끝나지 않은 동안 reader가 들고 있는 양은 `json_reader_set_max_token`(기본 256 MiB)까지이고, 64 KiB가 넘는 숫자는 거부해요(`JSON_ERR_TOO_LARGE`). 믿을 수 없는 상대가 보내는 입력이면 작게 정하세요.
- `json_writer_finish`의 결과는 writer 소유(다음 쓰기/리셋/해제 전까지 유효).
- `json_to_string*`의 결과는 호출자 소유 → `json_string_free`.
- 수정 때문에 옮겨진 자식 목록의 옛 자리는 문서를 `json_free`할 때 한꺼번에 반환돼요.
- 스트리밍의 `*_text`는 다음 `next` 호출 전까지만 유효해요.

## 구조

```
core/          Rust 코어: parser/dom/edit/stream (staticlib + 단위 테스트, tests/fuzz.rs, examples/bench.rs)
native/        C++ 변환 계층, wjson.hpp(C++용 래퍼), C++ 테스트·벤치마크
chain/         wjlink: MSVC 없는 Windows 링크 도구 (Rust)
include/       Rust 코어의 C 헤더
src/           Wave API (lib.wave가 진입점), number.wave·escape.wave는 순수 Wave 숫자·문자열 코드
tests/         Wave 종단 간 테스트와 포팅 대조 테스트
examples/      Wave 예제와 데이터
build.wave     빌드 드라이버 (Wave)
```

## 알려진 점

- Wave 언어가 pre-beta라 문법이 바뀌면 `src/`를 맞춰야 할 수 있습니다.
- 다른 모듈에서 `extern(c) fun name(...)` 형태(심볼 이름 생략)로 선언하면 현재 wavec가 이름을 맹글링해 링크가 깨집니다. 그래서 `src/ffi.wave`는 항상 `extern(c, "symbol")` 형태를 씁니다.
- Linux x86-64와 Windows 11 x64(MSVC 없이)에서 검증했고, CI가 푸시할 때마다 전체 테스트를 Linux x64·ARM64(`ubuntu-26.04`, `ubuntu-26.04-arm`), Windows x64(`windows-2025`), Windows ARM64(`windows-11-arm`, 네이티브, 역시 MSVC 없이), macOS Apple Silicon·Intel(`macos-26`, `macos-26-intel`)에서 돌려요. Windows에서 wjlink는 자신이 실행되는 아키텍처용 import 라이브러리와 시작 오브젝트를 만들어요(`wjlink init --arch x64|arm64`로 바꿀 수 있어요). 32비트 x86 Windows는 지원하지 않아요.
- 1.0 전이라 API가 바뀔 수 있어요. 바뀐 점은 `CHANGELOG.md`에 적어요.
- Windows 빌드는 C 런타임으로 시스템의 `msvcrt.dll`을 써요(MinGW와 같은 방식). Microsoft가 앱용으로 권장하는 런타임은 아니에요. C99 수학 함수 일부(`fma`)만 `ucrtbase.dll`에서 가져와요.
- `wjlink`는 wjson 빌드에 필요한 범위(x64·ARM64 콘솔 프로그램)에서 검증했어요. 범용 링커로 쓰기에는 아직 이릅니다.

## 기여와 보안

기여 방법은 `CONTRIBUTING.md`, 보안 취약점 신고는 `SECURITY.md`를 봐 주세요.

## 라이선스

Mozilla Public License 2.0 (MPL-2.0) — `LICENSE` 참고. Copyright (c) 2026 potatosl and the wjson contributors. 이 저장소의 파일을 수정해 배포하면 그 파일은 MPL-2.0을 유지해야 하고, wjson을 가져다 쓰는 프로그램은 어떤 라이선스든 괜찮아요.
