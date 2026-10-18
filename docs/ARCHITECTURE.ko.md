# 구조와 알려진 점

소스가 어떻게 나뉘어 있는지와 아직 끝나지 않은 것을 설명합니다. [README](../README.ko.md)로 돌아가기.

| **한국어**

Rust, C++, Wave가 각자 잘하는 일을 나눠 맡는 JSON 라이브러리입니다.

| 계층 | 언어 | 담당 | 위치 |
| --- | --- | --- | --- |
| API | **Wave** | 파서, 문서 모델, 수정 연산, JSON Pointer, 증분(pull) 파서(`core_*.wave`, `ffi.wave`로 연결), 탐색, 경로 쿼리, 문서 수정·경로 기반 수정, writer, 직렬화, 깊은 비교, 파일 스트리밍 | `src/*.wave` |
| 코어 | **Rust** | 같은 RFC 8259 파서, 인덱스 기반 DOM, 수정 연산, 해시 인덱스, pull 파서, JSON Pointer, 에러 위치. C/C++ 사용자용이며 Wave 코드를 테스트할 때 비교하는 기준(`src/ffi_rust.wave`) | `core/` |
| 네이티브 | **C++20** (freestanding) | 숫자 변환(자체 구현: 정확한 반올림 파싱·최단 왕복 출력), 문자열 이스케이프, UTF-8 검사. 표준 라이브러리·SDK 헤더 없이 컴파일 | `native/` |
| 툴체인 | **Rust** | `wjlink`: Windows에서 MSVC 대신 쓰는 링크 도구. 시스템 DLL에서 import 라이브러리 생성, 시작 코드(wjrt) 생성, 아카이버, rust-lld 구동 | `chain/` |

Wave ↔ Rust/C++ 경계는 모두 C ABI(`extern(c)`)입니다. C/C++ 프로그램도 `include/wjson.h`, `native/include/wjson.hpp`로 같은 코어를 쓸 수 있습니다.

**외부 의존성 0개.** Rust crate도 C++ 라이브러리도 받지 않습니다(`core/Cargo.lock`에는 `wjson_core` 하나뿐). 퍼징에 쓰는 비교용 레퍼런스 파서도 테스트 안에 직접 구현돼 있습니다. 필요한 건 빌드 도구(wavec, cargo, C++ 컴파일러)뿐이고, Windows에서도 **MSVC/Visual Studio는 필요 없습니다**.

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
