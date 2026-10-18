<div align="center">

<img src="docs/wjson.svg" alt="wjson" height="90">

<hr>

**Wave용 JSON 라이브러리.**

엄격한 파싱, 안전한 경로, 제자리 수정, 어떤 크기의 파일이든 스트리밍.

[안내서](docs/GUIDE.ko.md) · [API](docs/API.ko.md) · [예제](examples) · [빌드](docs/BUILDING.ko.md) · [변경 내역](CHANGELOG.md)

[![CI](https://github.com/potatosl/wjson/actions/workflows/ci.yml/badge.svg)](https://github.com/potatosl/wjson/actions/workflows/ci.yml) [![License](https://img.shields.io/github/license/potatosl/wjson?color=6654F1)](LICENSE) [![Made for Wave](https://img.shields.io/badge/made%20for-Wave-6654F1)](https://wave-lang.dev)

[English](README.md) | **한국어**

</div>

JSON을 읽고 쓰고, 절대 죽지 않는 경로로 값을 찾고, 문서를 그 자리에서 고치고, 메모리에 다 올라가지 않는 큰 파일도
스트리밍으로 읽습니다. 파서는 RFC 8259를 엄격하게 따르고 깊이 제한이 있으며, 설정 파일용으로 주석과 끝의 쉼표를
허용하도록 켤 수도 있습니다. 외부 의존성이 없고, Windows에서도 MSVC나 Visual Studio가 필요 없습니다.

## 맛보기

```wave
import("wjson")::{JsonDoc, Json, json_parse, json_ok, json_root, json_path_str,
                  json_path_i64, json_free};

fun main() {
    var doc: JsonDoc = json_parse("{\"server\": {\"host\": \"0.0.0.0\", \"ports\": [8080, 8443]}}");
    if (!json_ok(doc)) { return; }
    var cfg: Json = json_root(doc);
    println("{}:{}", json_path_str(cfg, "server.host", "localhost"),
                     json_path_i64(cfg, "server.ports[0]", 80));     // 0.0.0.0:8080
    json_free(&doc);
}
```

`"server.ports[0]"` 같은 경로에는 기본값을 줄 수 있어서, 키가 없거나 타입이 달라도 죽지 않고 기본값을 돌려줍니다.
쓰기와 수정은 이렇게 합니다.

```wave
var doc: JsonDoc = json_parse("{\"name\": \"wave\", \"tags\": []}");
var root: Json = json_root(doc);
json_set_str(root, "name", "wjson");               // 값 바꾸기
json_set_path_i64(root, "limits.depth", 512);      // 중간 객체는 알아서 만들어짐
json_push_str(json_get(root, "tags"), "json");     // 배열 끝에 추가
```

## 들어 있는 것

- **파싱**: 엄격한 RFC 8259, 깊이 제한(기본 512), 선택적 주석과 끝 쉼표, 정확한 오류 위치.
- **읽기**: 기본값이 있는 타입별 접근자, JSON Pointer, 점으로 쓰는 경로, 안전한 연쇄 호출.
- **쓰기**: 쉼표와 중첩을 알아서 맞춰 주는 writer, 들여쓰기 출력, ASCII 또는 HTML 안전 이스케이프.
- **수정**: 키나 경로로 값을 설정, 삽입, 교체, 삭제, 복사.
- **스트리밍**: 파일 크기와 상관없이 일정한 메모리, JSON Lines, 전체 파서와 같은 문법과 오류 위치.
- **숫자**: 정확하게 반올림하는 파싱과, 왕복해도 같은 값이 되는 가장 짧은 출력.
- **C와 C++**: 같은 코어를 `wjson.h`와 헤더만으로 된 `wjson.hpp`로 쓸 수 있습니다.

## 어떻게 만들었나

Wave, Rust, C++가 각자 잘하는 일을 나눠 맡습니다. API와, 완전한 파서, 문서 모델, 수정기, 스트리밍 리더는 Wave입니다.
같은 파서가 Rust로도 있는데, C와 C++ 사용자를 위한 것이고 Wave 코드를 테스트할 때 비교하는 기준입니다(퍼징으로 둘을
비교합니다). 숫자 변환과 문자열 이스케이프도 Wave와 freestanding C++로 두 벌 있습니다. Windows에서는 작은 Rust 링커
(`wjlink`)가 MSVC를 대신합니다. 외부 crate나 라이브러리는 없습니다. 자세한 내용은 [구조](docs/ARCHITECTURE.ko.md)에 있습니다.

## 지원 플랫폼

CI가 push마다 Linux(x64, ARM64), Windows(x64, ARM64), macOS(Apple silicon, Intel)에서 전체 테스트를 빌드하고 실행합니다.

## 시작하기

[Wave 컴파일러](https://wave-lang.dev/docs/en/getting-started/install)(`wavec`)와 C++20 컴파일러가 필요합니다. Linux와
macOS에서는 설치 스크립트가 wjson을 빌드하고, 내 프로그램을 그에 맞춰 컴파일해 주는 래퍼를 만들어 줍니다(스크립트가
짧으니 먼저 읽어 보세요).

```sh
curl -fsSL https://raw.githubusercontent.com/potatosl/wjson/main/install.sh | sh
~/.wjson/bin/wjson-wavec hello.wave --run
```

Windows에서는 클론한 폴더의 `setup-windows.cmd`를 더블클릭하면 빠진 것을 설치하고 테스트까지 돌립니다. 손으로 하는 방법,
라이브러리를 직접 링크하는 방법, C나 C++에서 쓰는 방법, `vex`는 [설치, 빌드, 테스트](docs/BUILDING.ko.md)에 있습니다.

## 더 알아보기

- [안내서](docs/GUIDE.ko.md): 파싱, 쓰기, 수정, 스트리밍을 예제로 보고, 메모리 규칙을 익힙니다.
- [API 요약](docs/API.ko.md): 영역별로 정리한 Wave API.
- [구조와 알려진 점](docs/ARCHITECTURE.ko.md).
- [설치, 빌드, 테스트](docs/BUILDING.ko.md): 벤치마크와 MSVC 없는 Windows 빌드 포함.
- [변경 내역](CHANGELOG.md), [기여](CONTRIBUTING.md), [보안](SECURITY.md).
- `examples/`: 주석 있는 설정 읽기, 수정, 행 스트리밍, CSV 변환.

## 현재 상태

wjson은 0.x이고 Wave 자체가 pre-beta라서 마이너 버전에서 API가 바뀔 수 있습니다. 파서와 숫자 코드는 독립적인 기준
구현과 비교하는 퍼징을 거쳤습니다.

## 라이선스

[MPL-2.0](LICENSE). Copyright (c) 2026 potatosl and the wjson contributors. 이 저장소 파일을 수정한 사본은 MPL-2.0을
유지해야 하고, wjson을 그냥 사용하는 프로그램은 어떤 라이선스든 괜찮습니다.

<p align="center">
  <a href="https://wave-lang.dev"><img src="docs/wave.svg" alt="Wave" height="28"></a>
</p>
