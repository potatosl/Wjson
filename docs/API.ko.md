# API 요약

영역별로 정리한 Wave API입니다. `src/`의 함수마다 주석도 달려 있습니다. [README](../README.ko.md)로 돌아가기.

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
