# Example data

`survey_sample.csv` is **synthetic**. It has the shape of a typical
statistics-portal export (three header rows, quoted fields such as
`"전문,과학및기술서비스업"`, Korean labels, 69 data rows x 21 columns), but
every number was generated randomly for this repository. It is not real
survey data and must not be used as such.

The generated numbers are internally consistent so the examples can check
them: small + medium firms add up to each subtotal, and the 23
manufacturing industries add up to the manufacturing subtotal.

`survey_sample.json` is the same data converted by `examples/csv2json.wave`
(committed because `tests/stream_test.wave` reads it).
`survey_report.json` is written by `examples/survey_report.wave` and is not
tracked.

Like the rest of the repository, these files are licensed under the MIT License.
