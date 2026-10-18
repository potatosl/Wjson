# 안내서

파싱, 쓰기, 수정, 스트리밍을 예제로 보여 주고, 메모리 규칙을 설명합니다. [README](../README.ko.md)로 돌아가기.

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

## 메모리 규칙

- `Json` 값과 `json_as_str`·`json_key_at`·`json_number_text`가 돌려준 문자열은 문서 안에 있으므로 `json_free` 이후에는 쓰면 안 됩니다. 반대로 `json_free` 전까지는 **문서를 계속 수정해도 유효해요.** 파싱된 텍스트는 파싱 후 다시 쓰지 않고, 수정으로 생기는 텍스트는 각자 따로 할당해서 옮겨지지 않아요. 그래서 `json_set_str(v, "새 키", json_as_str(json_get(v, "옛 키")))`처럼 같은 문서의 문자열을 그대로 넘겨도 안전해요.
- **문자열 안의 NUL.** JSON 문자열에는 `\u0000`이 들어갈 수 있고 `str`은 첫 NUL에서 끝나서, `"admin\u0000.txt"`는 `json_as_str`·`json_key_at`으로 읽으면 `admin`이 돼요. 믿을 수 없는 문서에는 `json_as_str_checked`(NUL이 있으면 null), `json_str_has_nul`, 또는 길이를 정확히 주는 `json_as_bytes` / `json_key_at_bytes` / `json_str_eq`를 쓰세요.
- **스레드.** 수정하지 않는 문서를 읽는 일(`json_get`, `json_at`, `json_path` 등)은 여러 스레드에서 동시에 해도 안전해요. 수정은 혼자만 써야 하고, reader/stream은 한 번에 한 스레드만 쓰세요.
- **스트리밍 한도.** 문자열·주석 하나가 끝나지 않은 동안 reader가 들고 있는 양은 `json_reader_set_max_token`(기본 256 MiB)까지이고, 64 KiB가 넘는 숫자는 거부해요(`JSON_ERR_TOO_LARGE`). 믿을 수 없는 상대가 보내는 입력이면 작게 정하세요.
- `json_writer_finish`의 결과는 writer 소유(다음 쓰기/리셋/해제 전까지 유효).
- `json_to_string*`의 결과는 호출자 소유 → `json_string_free`.
- 수정 때문에 옮겨진 자식 목록의 옛 자리는 문서를 `json_free`할 때 한꺼번에 반환돼요.
- 스트리밍의 `*_text`는 다음 `next` 호출 전까지만 유효해요.
