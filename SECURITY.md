# Security policy

wjson parses untrusted input, so parser bugs can be security bugs.

Please report suspected vulnerabilities privately through GitHub's
"Report a vulnerability" (Security tab of the repository) rather than in a
public issue. Include a minimal input that reproduces the problem if you can.

Only the latest release receives fixes while the project is below 1.0.

Hardening already in place: a strict RFC 8259 parser with a depth limit
(512 by default), inputs over 2 GiB rejected, differential fuzzing against an
independent reference parser, and valgrind-clean unit and fuzz tests.
