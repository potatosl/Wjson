# Security policy

wjson parses untrusted input, so parser bugs can be security bugs.

Please report suspected vulnerabilities privately through GitHub's
"Report a vulnerability" (Security tab of the repository) rather than in a
public issue. Include a minimal input that reproduces the problem if you can.

Only the latest release receives fixes while the project is below 1.0.

Hardening already in place: a strict RFC 8259 parser with a depth limit
(512 by default, never above 2048 whatever the caller asks for), inputs over
2 GiB rejected, differential fuzzing against an independent reference parser,
and valgrind-clean unit and fuzz tests; the C++ layer is also tested under
AddressSanitizer and UBSan. `WJSON_TASK=sanitize wavec run build.wave` runs
the C++ tests under ASan/UBSan and valgrind and the Rust unit tests under
valgrind (the long randomized Rust fuzz tests are too slow for valgrind and
run normally); CI runs it on every push.

Limits for untrusted input:

- The streaming reader resumes scanning where it stopped, so a token fed in
  tiny chunks costs linear time, and it stops buffering after
  `wj_reader_set_max_token` bytes (default 256 MiB; lower it for network
  input). Number lexemes are capped at 64 KiB there.
- Reading a document is thread-safe while nobody edits it; editing needs
  exclusive access.
- JSON strings can contain `\u0000`. Use the length-aware accessors (or
  `json_as_str_checked`) before handing text to code that treats it as a C
  string.

Supply chain: CI pins its actions to a commit, verifies the Wave download's
SHA-256 and runs with a read-only token; `setup-windows.ps1` refuses a Wave
installer whose SHA-256 differs from the reviewed one.
