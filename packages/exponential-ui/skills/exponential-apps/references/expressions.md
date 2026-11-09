# Expressions (CEL), functions and optional values

> Generated from packages/exponential-ui/docs/package-format.md (Expressions) (genUI 0.20.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

## Expressions

VAPP-191 (Q2, spec `docs/spec/apps.md` §4.2–4.3). A computed value is an
A2UI function call: either one of the expression functions, or `expr`, the
compact string form, with its inputs as `vars` (each a literal or any
dynamic value: a binding, another call):

```jsonc
{ "call": "expr", "args": {
    "expr": "formatDuration(target - entries.sum, 'h:mm')",
    "vars": { "target": { "path": "/target" }, "entries": { "path": "/entries" } },
    "default": "–" } }
{ "call": "dateAdd", "args": { "date": { "path": "/day" }, "n": 1, "unit": "month" } }
```

The string form is a **subset of CEL** (the Common Expression Language),
the same in the TS reference (`src/cel.ts`) and the Rust core (`cel`):

- literals: numbers, `'…'` / `"…"` strings (`\\ \' \" \n \t \r`), `true`,
  `false`, `null`, lists `[a, b]`, maps `{'k': e}` (string keys); names (the vars); `a.b`, `a[0]`, `m['k']`
  (a missing field, a field of null, or an index past either end is null);
- `+ - * / %` (numbers; `+` also joins two strings or two lists), unary `-`
  and `!`, `== !=` (deep JSON equality), `< <= > >=` (two numbers or two
  strings, by UTF-16 code unit), `&& ||` (short-circuit, bools only),
  `c ? a : b`;
- calls of the functions below, and the macros `list.map(x, e)`,
  `.filter(x, p)`, `.all(x, p)`, `.exists(x, p)`, one level (a macro inside a
  macro's body is refused; chaining is fine).

Unlike full CEL there is ONE number type (a double, like JSON: `7 / 2` is
`3.5`), no implicit conversion (`'a' + 1` is an error), no `in`, no user functions
and no loops. An error (`1 / 0`, a missing name,
a wrong type) gives the call no value.

| Function | A2UI args | |
| --- | --- | --- |
| `mul(a, b)` `div(a, b)` `mod(a, b)` | `a, b` | `div` / `mod` by 0 = an error; `mod` takes the sign of `a` |
| `round(x, digits?)` | `value, digits` | half away from zero, digits 0..10 |
| `floor(x)` `ceil(x)` `abs(x)` | `value` | |
| `min(x, …)` `max(x, …)` | `values: [...]` | at least one |
| `today()` | | the calendar day in the team zone |
| `now()` | | the instant, `YYYY-MM-DDTHH:MM:SS.sssZ` |
| `dateTime(d, time?)` | `date, time` | the day at a wall-clock `HH:MM` (none = midnight) in the team zone, as an instant |
| `dateAdd(d, n, unit)` | `date, n, unit` | a whole `n`; a day unit keeps the wall clock, a month end clamps (Jan 31 + 1 month = Feb 28) |
| `dateDiff(a, b, unit)` | `a, b, unit` | whole units from `a` to `b` (`b - a`, truncated) |
| `startOf(d, unit)` `endOf(d, unit)` | `date, unit` | day, week (Monday), month, quarter, year |
| `weekday(d)` | `date` | 1 = Monday … 7 = Sunday |
| `isoWeek(d)` | `date` | the ISO week number |
| `hours(minutes)` | `minutes` | `minutes / 60` |
| `formatDuration(h, format)` | `hours, format` | `h:mm` or `hh:mm` (`7.5` → `7:30`) |
| `size(x)` | (CEL only; `len` in A2UI) | a list, string (code points) or map |
| `orElse(x, default)` | `value, default` | `x`, or `default` when `x` is null (narrows `T?` to `T`) |
| `has(a.b)` | (CEL only) | the field is there and not null |
| `string(x)` | (CEL only) | a number's or bool's text, a string as it is |
| `trim(s)` `lower(s)` `upper(s)` | (CEL only) | `size(trim(x)) > 0` refuses a whitespace-only value |
| `split(s, sep)` `join(list, sep?)` | (CEL only) | `sep` non-empty for `split`; `join` takes strings |
| `sumOf(list)` `avgOf(list)` `minOf(list)` `maxOf(list)` `countOf(list)` | (CEL only) | a null entry is skipped; `sumOf([])` is 0, the average, min and max of nothing are null (`number?`) |
| `sortBy(list, field, order?)` | (CEL only) | `asc` (default) or `desc`, stable; a missing value last in either order |
| `groupOf(list, field)` | (CEL only) | `[{key, rows}]` in first-seen order |

A list of rows becomes Select options with a map literal:
`rows.map(r, {'value': r.id, 'label': r.name})`.

Units: `second minute hour day week month quarter year` (a date takes no
sub-day unit in `dateAdd`). A date is `YYYY-MM-DD` (a calendar day), a
timestamp an ISO instant (no offset = UTC). Everything date-like runs in the
**team zone**: the surface's zone (`Formatter.offsetMinutes` / the Rust
`Formatter::offset_minutes`, `ResolveOptions.zone`, the FFI's `HostZone`). A
wall-clock time in a DST gap moves forward by the gap; a repeated one takes
its later instant. There are no working days or holidays (spec §18.8): an
app keeps those as its own records.

**Missing values are explicit** (VAPP-195, E2). A type that may be missing
is `T?`: a kind's field that is not in its schema's `required` (the row keys
`id createdAt …` never are), a var the checker cannot type (`dyn?`), and a
field or index of a `T?`. VAPP-204 adds: every list index (`rows[0]`: the
list may be shorter), a field of a `map` (`dyn?`), and what a records
binding's query leaves out (`sum` without a sum, `month` without a month
view, `groups` without a group, `totals` without an aggregate, `before`
without a before; the query is the binding's source with `query=`
expanded, so `records:entry?sum=hours` makes `sum` a `number`). Inside a row
template (a component's `children: {componentId, path}`), a relative
`{"path": "hours"}` var is typed against the list's item: a records row's
field is its schema type, and a field the row has not is the install issue
`unknown field hourz in hourz` at the var's `…/path` (also below a records
binding's absolute path; a kind with `additionalProperties: true` stays
open). `has(x)` takes a field; for a bare name write `x != null`.
Arithmetic, `< <= > >=`, `&& || !`, a condition, a
macro's list and a function argument refuse a `T?` at install
(`entries.rows[0].hours may be missing: guard it with has(), a null check or
orElse()`); `==` / `!=` take one. Three guards narrow it to `T`:

- `has(r.f)` (true when the field is there and not null) or `r.f != null`
  in a condition: `has(r.note) ? size(r.note) : 0`, also through `&&`,
  `||` and `!` (`r.note == null || size(r.note) > 2`);
- `orElse(x, default)` (A2UI `{call: "orElse", args: {value, default}}`):
  `x`, or `default` when `x` is null, of `x`'s type.

**Every binding has a fallback.** A `{"call": "expr"}` in a package needs a
`default` (`{"expr": "…", "vars": {…}, "default": "0:00"}`), shown when the
evaluation fails (an error, the step limit). Every bind-time call takes the
same optional `default`, and a call that fails or throws resolves to it, so
no renderer paints undefined or throws on a binding.

**Cost caps:** at most 1000 characters (code points), nesting 32 deep, and
10 000 evaluation steps (nodes + macro iterations) per run.

**Type checking at validation:** `validatePackage` parses and type-checks
every `{"call": "expr"}` anywhere in a template. A var's type comes from a
literal, from a records binding the path lies under (`records:<kind>`:
`{rows: [row], count, sum, groups, totals, before,
month}`, a row = the kind's schema fields + `id createdAt updatedAt
createdBy authorId author`; `format: date` = a date, `date-time` = a
timestamp, `attachment` = a string), or from the template `data`; anything else is open. An issue
names the expression's JSON pointer and the reason, e.g.
`/templates/month/components/3/props/text/args/expr: unknown field hourz`.
`fixtures/expressions.json` locks the language, the checker, the A2UI form
and these issues.
