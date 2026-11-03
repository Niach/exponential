# The package format (eApps)

VAPP-161. One format for an Exponential app and a spezl block: a declarative
Exponential UI package (VAPP-82: data, never code) plus the file it travels
in. The format is transport-neutral: it names what a host provides (a record
store, tool runners, extensions), never how (Exponential serves records from
its synced store, spezl from a p2p store over eNet). `catalog/host.json`
`package` is the machine-readable contract; this page is the prose spec other
docs link to (spezl `docs/spec/blocks.md`). The TS reference is `src/host/`
(`package.ts`, `records.ts`, `record-store.ts`, `query.ts`, `actions.ts`, `tests.ts`, `tools.ts`, `exposes.ts`, `signature.ts`);
the Rust core (`apps/desktop/crates/eui-core/src/host/`) mirrors every
pure rule. Fixtures: `fixtures/expressions.json` (the expression language, below), `fixtures/actions.json` (effects and actions, below), `fixtures/package-tests.json` (package tests, below), `fixtures/host-router.json` (package validation, replayed
by every renderer through the core), `fixtures/records-query.json` (the
records query and its value) and `fixtures/host-package.json` (tool consent,
extension compatibility, canonical JSON, signatures).

## Versions

A package MUST carry `formatVersion`. This document is format version
**3** (`catalog/host.json` `package.formatVersion`; 3 added the per-kind
`policy`), and a host reads exactly its own version: eApps keeps no
backwards compatibility. A missing one is
`/formatVersion: formatVersion is required (3)`, an older one `format version
2 is no longer read: write 3`, a newer one `format version 4 is newer than
this host (3)`; a host never guesses at keys it does not know. The catalog a package
renders against keeps its own version in its id (`catalogId`,
`…/catalogs/core/v1`) and is negotiated as before (`supportedCatalogIds`).

## The package

```jsonc
{
  "formatVersion": 3,                  // required: the current format
  "id": "acme.hours", "name": "Working hours", "version": "1.2.0",
  "catalogId": "https://ui.exponential.at/catalogs/core/v1",
  "templates": { "<id>": { "components": [/* flat, one root */], "data": {}, "bindings": [{ "path": "/entries", "source": "records:entry?sort=date" }] } },
  "functions": ["records.*", "tools.weather.*"],   // allowlist: listed = allow, else deny
  "sources": ["records:*"],                         // allowlist for bindings, unset = any
  "records": { "entry": { "title": "Entry", "schema": { "type": "object", "properties": { "date": { "type": "string", "format": "date" } } } } },
  "tools": { "weather": { "kind": "mcp", "title": "Weather", "url": "https://mcp.example.com/weather" } },
  "upstreams": ["crm"], "exposes": [/* MCP tools a host's broker runs: "Exposes" below */],
  "queries": { "month": { "group": ["author"], "aggregate": { "hours": ["sum", "hours"] } } },
  "actions": { "log": { "params": { /* a records schema */ }, "steps": [/* "Actions" below */] } },
  "tests": [{ "name": "a duration", "expr": { "expr": "formatDuration(7.5, 'h:mm')" }, "expect": "7:30" }],
  "extensions": [{ "id": "https://acme.example/catalogs/charts/v1", "version": "1.4.0", "components": ["Gantt"] }],
  "theme": "exponential", "icon": { "glyph": "clock" }, "description": "…"
}
```

Required: `formatVersion`, `id`, `name`, `version`, `catalogId`,
`templates`. Everything else is optional (`functions`, `sources`, `theme`,
`icon`, `description`, `records` kinds: `catalog/host.json` `package`).

## Records

An app's own data. `records: {<kind>: {schema, title?, description?, policy?}}`
declares every kind (VAPP-122: the kind pattern, the JSON Schema subset and
the reserved row keys are in `catalog/host.json` `package.records`). The
host provides the store; the format fixes what a surface sees and calls.

**Policy** (eApps R3, `fixtures/record-policy.json`): who may read, create,
update and delete a kind's records, one level per op:

```jsonc
"policy": { "read": "author", "create": "member", "update": "author", "delete": "owners" }
```

`member` = any member of the app's team; `author` = the record's author
(`authorId ?? createdBy`) or a team owner; `owners` = team owners only (not
even the author). `create` is `member` or `owners` (a new record has no
author yet). An op left out keeps the default `member/member/author/author`.
Over it sits the member's access level for the app, set by an owner:
`none` (the app is hidden and syncs nothing), `read` (reads only, inputs
disabled) or `write` (the default); it binds owners too. The host enforces
it (Exponential: the server, and a `read` other than `member` filters the
synced rows, so a private kind never reaches another member's device);
`recordRefusal(policy, op, viewer, row?)` / `readableRows` are the rule,
the refusal texts are the fixture's `copy`. Field-level privacy is out: use
a separate kind with `read: owners`.

**Read** = the binding source `records:<kind>[?query]`. The value is
`{rows: [{…data, id, createdAt, updatedAt, createdBy, authorId, author}], count, sum?, month?, groups?, totals?, before?}`
(`authorId` = the row's author, `authorId ?? createdBy`):
`count` = the matching rows before `limit`. The query (URL sugar of the
structured query, below):

| key | meaning |
| --- | --- |
| `mine=1` | only the viewer's rows (by author: `authorId`, else `createdBy`) |
| `user=<id>` | only that user's rows (by author) |
| `sort=<field\|createdAt\|updatedAt>` | default `createdAt` |
| `order=asc\|desc` | default `desc` (newest first) |
| `limit=<n>` | default 100, capped at 500 |
| `sum=<field>` | `sum` = Σ of the field over the matches (before `limit`) |
| `by=<dateField>&month=YYYY-MM` | the month view: that month's rows only, `month` = {key, label, prev, next, today, days (every day with its count and sum), labels/counts/sums}; no `month` = the month of now |
| `app=<slug>` | another app's rows (the HOST decides which rows that is) |
| `<field>=v` | a data field equals `v` (as text) |
| `<field>.gt\|gte\|lt\|lte=v` | numbers when both parse, else text (code units) |
| `<field>.in=a,b` | the field (as text) is one of the comma-separated values |
| `<field>.between=lo,hi` | inclusive, numbers when both parse, else text |
| `before=<date>` | the rows whose date (`by`'s field, else `createdAt`) is before it leave the rows and are totalled into `before` |
| `q=<json>` | the JSON form below (the other params are ignored) |
| `query=<name>` | one of the package's `queries` (below) |

Ties sort by `id`. The reserved keys (`catalog/host.json`
`package.records.query.reserved`) never filter a field. VAPP-189: ONE
evaluator for every host. The params parse INTO the structured query
`{from, app, mine, user, where: [{field, op, value}], sort: [{field, order}],
limit, sum, month: {field, key} | null}` (`parseRecordsQuery(params, kind?)`,
`parseRecordsSource(source)`), and `recordsValue(rows, query, {me, now,
names})` evaluates it over the rows of any store
(`@exponential_app/eui/host/query`, the Rust core's `host::query`, the FFI's
`parse_records_query_json` / `parse_records_source_json` /
`records_value_json` for the Swift and Compose SDKs);
`fixtures/records-query.json` locks all of it.

**The JSON form** (VAPP-190, `parseRecordsJson(json, kind?)` → `{query,
issues}`; the FFI's `parse_records_json`): the same query written as JSON,
plus groups, aggregates and running values. A problem (an issue with a JSON
pointer) drops that part; the rest still runs.

```jsonc
{
  "from": "entry",                                  // a source's own kind wins
  "where": [
    ["date", "between", "2026-10-01", "2026-10-31"], // eq gt gte lt lte in between, or = == > >= < <=
    ["status", "in", ["open", "doing"]],
    { "or": [[["owner", "eq", "u1"]], [["owner", "eq", "u2"], ["hours", ">", 1]]] }  // one per query
  ],
  "group": ["author", { "bucket": "date", "by": "day" }],   // any field; author; day week month quarter year
  "aggregate": { "hours": ["sum", "hours"], "n": ["count"] }, // sum avg min max count countDistinct first last
  "running": { "hours": "sum" },                    // sum count avg min max, cumulative
  "before": "2026-10-01",                           // or {field, date}
  "sort": [["date", "desc"]], "limit": 500,         // also mine, user, app, sum, month: {field, key?}
}
```

- `groups`: one object per group: its columns (`author` = the author's name
  plus `authorId`; a bucket = `2026-10-05`, `2026-W41` (ISO), `2026-10`,
  `2026-Q4`, `2026`, null without a date that precise; any other field its
  value), `count`, `sum` (with `sum`) and every aggregate. Ordered by the
  `sort` keys that name a group column or an aggregate, then by the group
  columns ascending.
- `totals`: count, sum? and the aggregates over every match (with
  `aggregate`).
- `before`: the matches whose date field (a string `before` = the first
  bucket's field, else the month view's, else `createdAt`) is before the
  date leave the rows and the groups; `before` = their count, sum? and
  aggregates, and they seed every running value (a balance carried over).
- `running`: `{field: value}` on every group (in order), else on every row
  (in the sort order), and on every day of the month view.
- Aggregates: `count` without a field counts rows (with one, rows where it is
  non-empty); `sum` / `avg` over the numbers (rounded to 1e-6; `avg` of none
  = null); `min` / `max` compare like the ranges; `first` / `last` in the
  sort order. The names `count`, `sum`, `running`, `rows` and a group
  column's are taken.

**Named queries:** `queries: {<name>: <JSON query>}` (names
`^[a-z][A-Za-z0-9_]{0,31}$`, each must parse clean, `from` a declared kind).
A template binding `records:<kind>?query=<name>&<params>` follows it: the router replaces it
with `?q=<canonical JSON>` (`expandQuerySource`), every `${param}` in the
query's strings filled from the binding's other params (unset = empty), so a
`bind` action's params drive it (`{"where": [["date", "between",
"${start}", "${end}"]]}`). A binding naming no package query is an install
issue. The date maths (`src/dates.ts`, the Rust core's `dates`) is shared
with the bind-time expressions.
A template binding to `records:<kind>` names a declared kind (else an install
issue). A package with a `sources` allowlist lists `records:<kind>` or
`records:*`.

**Write** = two host functions, listed in `functions` like any other (exact
or `records.*`), never behind a consent card (the app's own rows):

- `records.upsert {kind, data, id?}` → `{id}`: creates a record, or with `id`
  shallow-patches that record's `data`. The host validates the result against
  the kind's schema (`validateRecord`) BEFORE it stores anything; a refusal
  fails the call (the host's toast).
- `records.delete {id}`: deletes one record.

Every write lands in every live `records:` binding of the package (on every
peer, for a p2p store). `RecordStore` (`src/host/record-store.ts`) is the
interface a host implements; `MemoryRecordStore` + `recordsHost(pkg, store)`
are the reference (resolver + both functions); the evaluator reads the rows
of whatever store the host has.

Records have ONE spelling on every host: read `records:<kind>?q`, write
`records.upsert` / `records.delete` / `records.batch` (Exponential's former
`exp:records?kind=` and `harness.records.*` are gone).

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
  `false`, `null`, lists `[a, b]`; names (the vars); `a.b`, `a[0]`, `m['k']`
  (a missing field, or a field of null, is null);
- `+ - * / %` (numbers; `+` also joins two strings or two lists), unary `-`
  and `!`, `== !=` (deep JSON equality), `< <= > >=` (two numbers or two
  strings, by UTF-16 code unit), `&& ||` (short-circuit, bools only),
  `c ? a : b`;
- calls of the functions below, and the macros `list.map(x, e)`,
  `.filter(x, p)`, `.all(x, p)`, `.exists(x, p)`, one level (a macro inside a
  macro's body is refused; chaining is fine).

Unlike full CEL there is ONE number type (a double, like JSON: `7 / 2` is
`3.5`), no implicit conversion (`'a' + 1` is an error), no `in`, no map
literals, no user functions and no loops. An error (`1 / 0`, a missing name,
a wrong type) gives the call no value.

| Function | A2UI args | |
| --- | --- | --- |
| `mul(a, b)` `div(a, b)` `mod(a, b)` | `a, b` | `div` / `mod` by 0 = an error; `mod` takes the sign of `a` |
| `round(x, digits?)` | `value, digits` | half away from zero, digits 0..10 |
| `floor(x)` `ceil(x)` `abs(x)` | `value` | |
| `min(x, …)` `max(x, …)` | `values: [...]` | at least one |
| `today()` | | the calendar day in the team zone |
| `now()` | | the instant, `YYYY-MM-DDTHH:MM:SS.sssZ` |
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
field or index of a `T?`. Arithmetic, `< <= > >=`, `&& || !`, a condition, a
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
timestamp), or from the template `data`; anything else is open. An issue
names the expression's JSON pointer and the reason, e.g.
`/templates/month/components/3/props/text/args/expr: unknown field hourz`.
`fixtures/expressions.json` locks the language, the checker, the A2UI form
and these issues.

## Actions

VAPP-194 (E1, spec `docs/spec/apps.md` §4.5): **typed effects**. Every
function has an effect (`functionEffect`): `pure` (every expression and
bind-time function), `write` (`set`, `records.upsert`, `records.delete`,
`records.batch`) or `host` (`openUrl`, `bind`, and every
name under `actions.`, `harness.`, `exponential.`, `tools.`, `upstream.`).

- **Expressions are pure.** A binding, `visible`, a computed value, a call's
  args and an event's context never write: `validatePackage` refuses a write
  or host function anywhere in a template except an `on.<event>.functionCall`
  (`/templates/t/components/3/props/text/call: records.upsert writes: a
  binding is pure, name an action in on.press`), and CEL refuses one in a
  string (`at 0: set writes: an expression is pure, use an action step`).
- **Actions** are the package's writes, named and checked at install:

```jsonc
"actions": {
  "log": {
    "params": { "type": "object", "properties": { "date": { "type": "string", "format": "date" }, "hours": { "type": "number" } }, "required": ["date", "hours"] },
    "steps": [
      { "require": "params.hours > 0 && params.hours <= 24", "message": "Log 0 to 24 hours" },
      { "records.upsert": { "kind": "entry", "data": { "date": "params.date", "hours": "params.hours" } } },
      { "set": { "path": "/form/hours", "value": 0 } },
      { "call": "harness.toast", "args": { "message": "'Logged ' + formatDuration(params.hours, 'h:mm')" } }
    ]
  }
}
// a Button: "on": { "press": { "functionCall": { "call": "actions.log", "args": { "date": { "path": "/form/date" }, "hours": { "path": "/form/hours" } } } } }
```

`params` is a records schema (the call's args are checked against it). A
step is ONE of `require` (+ `message`), `records.upsert {kind, data, id?}`,
`records.delete {id}`, `set {path, value}` and `call` (a host function the
package lists in `functions`, + `args`); at most 32 actions of 64 steps. A
JSON string in a step is CEL over `params`, `data` (the surface data at the
press) and `me` (the member's id); anything else is a literal. Install
checks every expression's type (a `require` is a bool, an upsert's fields
are in the kind's schema with its types, a create names every required
field).

**All or nothing.** `runAction` evaluates the steps in order against ONE
snapshot (a `set` never changes what a later step reads) and returns the
plan `{writes, sets, calls}`, or the one error that discards the action: a
false `require` (its `message`, the toast), an evaluation error, a create
the kind's schema refuses, or the run's budget of 100 000 evaluation steps
(`step <i>: …`). The host then asks every call's policy and consent, commits
the writes as ONE unit through `records.batch` (`RecordStore.batch`;
Exponential: one Postgres transaction, `apps.records.batch`, where the
server checks every write again and stays the authority), applies the sets
and runs the calls. A failure before the commit lands nothing and is sent as
`ACTION_FAILED` (the host's `onActionFailed` shows the toast). A press runs
an action on the client host; the server broker will run them for `exposes`
and automations (A1). P2P stores (spezl) promise no cross-peer invariants.

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
plus an optional `id`, default `<kind>-<n>`, and `author`, default the
test's member), at `now` (an ISO instant, UTC; default
`2026-01-01T00:00:00.000Z`) as the member `as` (default `member`) with the
team role `role` (`member` or `owner`, default `member`). It reads only the
rows that member may read under each kind's `policy`, and an action's writes
are judged by it (a refusal is `{error: "write <i>: <why>"}`; a row the
member may not read is not found). It runs exactly one of:

- `expr: {expr, vars?}`: the CEL value (or `{error}`); a var
  `{source: "records:<kind>?…"}` is that binding's value over the store.
- `query: {name, params?}`: the package query's records value.
- `action: {name, params?, data?}`: the action's plan applied to the store:
  `{records: {<kind>: [{…data, id, author}]}, sets, calls}` (new ids
  `new-<n>`), or `{error}`.

`expect` matches by the keys an object names (recursively; a list item by
item, with the same length; anything else by JSON equality). The tests run
only on an otherwise valid package; a failing one is the issue `/tests/<i>`:
`<name>: expected <canonical JSON>, got <canonical JSON>` (got cut at 300
characters). `runPackageTests(pkg)` returns every result.

## Tools

`tools: {<id>: Tool}` registers MCP servers or scripts a package's functions
call. A tool id follows `^[a-z][a-z0-9_-]{0,31}$`.

| kind | keys | runs |
| --- | --- | --- |
| `mcp` (remote) | `title`, `description?`, `url` (absolute `https:`; `http:` only for a private host the host's URL policy opts in) | an MCP server over Streamable HTTP |
| `mcp` (local) | `title`, `description?`, `command`, `args?: [string]`, `env?: [NAME]` | an MCP server over stdio, a local process |
| `script` | `title`, `description?`, `command`, `args?: [string]`, `env?: [NAME]` | one process per call: the call's `args` as JSON on stdin, its stdout (JSON) is the result |

An `mcp` tool has exactly one of `url` / `command`. `env` lists variable
NAMES the host passes through from its own environment; a package never
carries values or secrets. Functions: an MCP tool's tool `<name>` is the host
function `tools.<id>.<name>` (args = its arguments); a script is
`tools.<id>`. They are listed in `functions` like any other.

**Consent (the host MUST show it).** No tool runs before the person allowed
it. Before a tool's FIRST call the host shows a consent card with the tool's
row from `toolConsent(pkg)`: `{id, kind, transport: http|stdio|script,
title, description, target, env}`, `target` = the url, or the command line
exactly as it will run (`command` then every arg, an arg with a space, quote
or backslash JSON-quoted). Choices: allow (remembered) or deny (the call
fails `FUNCTION_DENIED`). The remembered grant is keyed by
`toolConsentKey(packageId, toolId, tool)` = `<packageId>/<toolId>:` + the
canonical JSON of the tool: ANY change to the definition in a later version
asks again. A tool call is never `allow` by policy alone; a host that cannot
run a kind (a browser cannot run a command) refuses its calls with
`FUNCTION_NOT_FOUND` and says so on the card instead of an allow button.
Exponential's app hosts run no package tools: its server refuses a package
with `tools` (an app reaches tools through `exponential.<noun>.<op>` and
`harness.mcp`).

## Exposes

`exposes: [Expose]` lists MCP TOOLS the package offers to a host's MCP broker
(Exponential: `/api/mcp/apps`). They are declarative bindings, never code:
the broker lists them as `<app>__<name>` and runs them as the CALLING person,
so no device has to be online and nothing is hosted. `upstreams: [name]`
names the MCP servers the package's upstream bindings call; the host provides
them (Exponential: the team's MCP servers, Settings › MCP).

```jsonc
"upstreams": ["crm"],
"exposes": [
  { "name": "list_entries", "title": "List entries", "description": "The time entries of a day.",
    "input": { "type": "object", "properties": { "day": { "type": "string" } } },
    "access": "read",
    "run": { "records": { "op": "list", "kind": "entry", "where": { "date": "${input.day}" } } } },
  { "name": "log_comment", "title": "Log a comment", "description": "Comment the logged hours on an issue.",
    "input": { "type": "object", "properties": { "issue": { "type": "string" }, "hours": { "type": "number" } }, "required": ["issue", "hours"] },
    "access": "write",
    "run": { "action": "exponential.comments.create", "args": { "issueId": "${input.issue}", "body": "Logged ${input.hours} h" } } },
  { "name": "find_customer", "title": "Find a customer", "description": "Search CRM customers.",
    "input": { "type": "object", "properties": { "q": { "type": "string" } } },
    "access": "read",
    "run": { "upstream": "crm", "tool": "search_customers", "args": { "query": "${input.q}" } } }
]
```

| key | rule |
| --- | --- |
| `name` | `^[a-z][a-z0-9_]*$`, at most 40, never `__`, unique |
| `title` / `description` | non-blank, at most 80 / 1024 characters |
| `input` | a JSON Schema `{type: "object", properties?}`: the tool's `inputSchema` |
| `access` | `read` or `write`: who may see it (a read-only connection never lists a write tool) |
| `run` | EXACTLY ONE of `records` / `action` / `upstream` |

- `records: {op: list | get | upsert, kind, id?, where?, data?, limit?}` reads
  or writes the package's OWN records of a declared kind. A delete is
  destructive and never exposed.
- `action` (+ `args`) calls a host function (`exponential.<noun>.<op>`). It
  reads when the last word of its name is list, get, show, catalog,
  activity or files; anything else writes.
- `upstream` (+ `tool`, `args`) calls a tool of one of the package's
  `upstreams` with the person's own credential. Its level is the tool's live
  annotations: it reads only with `readOnlyHint: true`.

Every string in `run` may hold `${input.<path>}` (dotted keys into the
call's input; the first key must be a declared input property). A string
that is exactly one template keeps the value's type; inside text the value
is printed. Destructive is never exposed: a name holding delete, merge, close,
end, remove or trash is refused, and a `read` binding whose run writes is
refused. At most 32 tools. `exposeIssues` returns every issue with its eMCP
error code; the host adds its own rules on top (Exponential: only grantable,
non-destructive actions; upstreams the team has).

## Extensions

`extensions: [{id, version, components?}]` declares the custom component
catalogs (eUI extensions, VAPP-85: `defineExtension`) a package needs beyond
its `catalogId`. `id` = the extension catalog's id, `version` = the minimum
`MAJOR.MINOR.PATCH` it needs (caret: the host's version has the same major and
is not lower), `components` = the names the package uses from it. A host
registers its extensions with a version (`ExtensionDef.version`); install:

- an id the host has not registered → `/extensions/<i>/id: extension <id> is
  not available on this host`;
- a registered version that does not satisfy it → `/extensions/<i>/version:
  extension <id> <have> does not satisfy ^<want>` (an unversioned
  registration satisfies any version);
- a listed component the registered extension does not define →
  `/extensions/<i>/components/<j>: extension <id> has no component <name>`.

The package's own `catalogId` may itself be an extension id (the Exponential
app extension); it needs no entry.

## The package file

A package travels as ONE JSON object holding it under `package`:
Exponential's export (`<slug>.app.json`, `{format: "exponential-app/1", app,
package, records?}`, `packages/domain-contract/fixtures/app-export.json`) or
any other host's envelope. Two optional keys make it attributable:

```jsonc
{
  "format": "exponential-app/1", "app": { … }, "package": { … }, "records": [ … ],
  "author": { "name": "Ada", "key": "ed25519:<base64url, 32 bytes>" },
  "signature": { "alg": "ed25519", "value": "<base64url, 64 bytes>" }
}
```

- `author.name` is a display name (1–80 characters); `author.key` the
  author's Ed25519 public key, `ed25519:` + unpadded base64url.
- The SIGNED BYTES are UTF-8 of `exponential-ui/package-file/v1\n` followed by
  the canonical JSON of the whole file WITHOUT `signature` (so `author` is
  signed too). Canonical JSON = RFC 8785 (JCS): no whitespace, object keys
  sorted by UTF-16 code units, strings with the minimal JSON escapes, numbers
  in ECMAScript's shortest form (`1`, not `1.0`; `1e+21`).
- `signature.value` = the Ed25519 signature (RFC 8032) over the signed
  bytes, unpadded base64url.

`verifyPackageFile(file)` → `{status: "unsigned"}` (no `signature`; an
`author` alone is an unverified claim), `{status: "valid", author}` or
`{status: "invalid", issues}` (a malformed `author`/`signature`, a key or
signature of the wrong length, or bytes that do not verify). A host refuses
to import an invalid file and may show a valid one's author;
`signPackageFile(file, {name, seed})` is the reference signer. The
fixtures sign with the RFC 8032 §7.1 TEST 1 keypair (never a real key).
Exponential's own export is unsigned (its members hold no keys); its import
verifies every signed file on the server (`apps.validateImport`).
