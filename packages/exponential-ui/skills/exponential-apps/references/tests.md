# Package tests

> Generated from packages/exponential-ui/docs/package-format.md (Tests) (genUI 0.20.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

## Tests

VAPP-196 (T1, spec `docs/spec/apps.md` §11): a package carries its own
tests, and `validatePackage` runs them on every install (Exponential: every
upsert), so a failing test refuses the package like a type error:

```jsonc
"tests": [
  { "name": "my week", "records": { "entry": [{ "date": "2026-11-02", "hours": 2 }, { "date": "2026-11-03", "hours": 3, "author": "ada" }] },
    "query": { "name": "week", "params": { "start": "2026-11-02", "end": "2026-11-08" } },
    "expect": { "count": 2, "totals": { "hours": 5 } } },
  { "name": "my hours", "records": { "entry": [{ "date": "2026-11-02", "hours": 2 }] },
    "expr": { "expr": "entries.sum", "vars": { "entries": { "source": "records:entry?mine=1&sum=hours" } } }, "expect": 2 },
  { "name": "zero hours are refused", "action": { "name": "log", "params": { "date": "2026-11-04", "hours": 0 } },
    "expect": { "error": "Log more than 0 hours" } }
]
```

At most 50. Each test runs on its OWN in-memory store seeded with
`records` (`{<kind>: [rows]}`: the kind's data, checked against its schema,
plus an optional `id`, default `<kind>-<n>`, and `author`, default `member`, never the
test's `as`), at `now` (an ISO instant, UTC; default
`2026-01-01T00:00:00.000Z`) as the member `as` (default `member`) with the
team role `role` (`member` or `owner`, default `member`). It reads only the
rows that member may read under each kind's `policy`, and an action's writes
are judged by it (a refusal is `{error: "write <i>: <why>"}`; a row the
member may not read is not found). It runs exactly one of:

- `expr: {expr, vars?}`: the CEL value (or `{error}`); a var
  `{source: "records:<kind>?…"}` is that binding's value over the store.
- `query: {name, params?}`: the package query's records value.
- `action: {name, params?, data?, results?}`: the action's plan applied to the store
  (`results` = the answers of its `result` calls; one left out is the error
  `step <i>: <call> has no result in the test`):
  `{records: {<kind>: [{…data, id, author}]}, sets, calls}` (new ids
  `new-<n>`), or `{error}`.

`expect` matches by the keys an object names (recursively; a list item by
item, with the same length; anything else by JSON equality). The tests run
only on an otherwise valid package; a failing one is the issue `/tests/<i>`:
`<name>: expected <canonical JSON>, got <canonical JSON>` (got cut at 300
characters). `runPackageTests(pkg)` returns every result.
