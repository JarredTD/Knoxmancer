# Contributing

Run formatting, Clippy, tests, documentation, and coverage checks before
submitting changes. CI enforces at least 90% region, function, and line coverage
and treats public API documentation warnings as errors. Coverage exclusions must
be narrow, justified, and limited to code that cannot be exercised reliably;
ordinary branches and error paths should remain measured.

Use one single-line Conventional Commit subject per coherent change:

```text
<type>(<scope>): <imperative summary>
```

Do not add a commit body. Common types are `feat`, `fix`, `refactor`, `test`,
`docs`, `build`, and `chore`.
