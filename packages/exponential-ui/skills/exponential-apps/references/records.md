# Records: kinds, policies, queries, writes, RecordForm / RecordTable

> Generated from packages/exponential-ui/docs/package-format.md (Records, Record macros, Public links) and docs/spec/apps.md §3.4 (genUI 0.19.1) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

## Records

An app's own data. `records: {<kind>: {schema, title?, description?, policy?, computed?, refs?, transitions?}}`
declares every kind (VAPP-122: the kind pattern, the JSON Schema subset and
the reserved row keys are in `catalog/host.json` `package.records`). The
host provides the store; the format fixes what a surface sees and calls.

**Policy** (Apps R3, `fixtures/record-policy.json`): who may read, create,
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

**Files** (Apps R7, VAPP-221; `fixtures/host-router.json` `acme.files` and
`attachments`): `format: "attachment"` on a string field holds one file, on a
list's string `items` a list of files:

```jsonc
"properties": {
  "receipt": { "type": "string", "format": "attachment" },
  "photos": { "type": "array", "items": { "type": "string", "format": "attachment" }, "maxLength": 5 }
}
```

The value is the host's attachment route, relative, nothing else:
`/api/attachments/<lowercase uuid>`. A `FileUpload` gets it: bind its `files`
to a path (`{"path": "/draft/files"}`), and the app host uploads the picked
bytes and writes each file's `url` into the entry of the same `name` there.
An action step then stores the urls (`data` = the surface data at the
press): `"receipt": "orElse(data.draft.files[0].url, null)"`, `"photos":
"orElse(data.draft.files, []).map(f, f.url)"`. Show a
file with `Image` (`src` = the field), a `Link`, or a `FileUpload`'s list.
Exponential checks every referenced file belongs to the record's own app
and team (`attachments must be files uploaded to this app`), serves it to
the team's members with access to the app, and reclaims a file no record
of the app references any more (a trashed record still does, until the
trash purge) after a week. `recordAttachmentIds(schema, data)` lists the
ids a record references.

**Computed fields** (Apps Q3, VAPP-214; `fixtures/records-query.json`
VAPP-214 cases, `fixtures/expressions.json` `packages`): fields evaluated on
read by the expression evaluator (below), never stored, so they never go stale:

```jsonc
"computed": { "minutes": "round(hours * 60)", "week": "isoWeek(date)" }
```

Each is a CEL expression over the record: its schema fields and the row keys
`id`, `createdAt`, `updatedAt`, `createdBy`, `authorId` by name (a field the
row lacks reads null). Every row of the kind carries them like data, so a
query filters, sorts, groups and aggregates on them (`minutes.gte=60`,
`group: ["week"]`, `aggregate: {m: ["sum", "minutes"]}`), and a template reads
them typed. A failing expression reads null. They are evaluated in UTC with
the query's clock, like the query's own buckets, and only on the hosting
app's rows (`app=` rows carry none). At install, each is type-checked against
the kind's schema (`T?` unless required: guard it, see Expressions), issues
at `/records/<kind>/computed/<name>`: a name the schema declares
(`shadows a field of the schema`) or a row key, an expression that reads
another computed field (no chains, so no cycles), or one that does not
check. At most 16 per kind, plus the expression caps. Every host applies
them: the clients' records query (`recordsValue(rows, query, {…, computed})`,
the FFI context's `computed`; `kindComputed(records, kind)` picks a kind's)
and Exponential's server (the records list, MCP and the broker's reads).

**Refs** (Apps R5, VAPP-219; `fixtures/records-query.json` VAPP-219
cases, `fixtures/host-router.json` `acme.crm` / `acme.badrefs`): string
fields that hold an id, so a kind points at another instead of nesting:

```jsonc
"refs": { "customer": { "kind": "company" }, "owner": "member", "task": "issue" }
```

`{kind}` = a record of that kind of the same app (cross-app refs are out),
`"issue"` = an issue of the team by identifier (`EXP-12`), `"member"` = a
member of the team by user id. At install, issues at `/records/<kind>/refs/<field>`:
the field is a declared `string` field, a `{kind}` names a declared kind.
The host validates every write: a ref that does not resolve (no live record
of that kind in the app, no such issue or member in the team) refuses the
write (Exponential: BAD_REQUEST naming the field). A read follows a ref with
`expand` (below); deleting the target never cascades: the ref dangles and
an expanded one reads null.

**Transitions** (Apps R6, VAPP-222; `fixtures/record-transitions.json`): a
state machine on ONE enum field:

```jsonc
"transitions": {
  "field": "status",                       // a string field with an enum: its values are the states
  "initial": ["requested"],                // optional; default = every from state no edge enters
  "from": { "requested": ["approved", "rejected"], "approved": ["paid"] },
  "who": { "requested->approved": "role:approver", "paid": "owners" }
}
```

A write that moves the field along an edge `from` does not list is refused
(Exponential: BAD_REQUEST `status: requested → paid is not a declared
transition`; clearing the state is an edge too), and a create must start in
an initial state. `who` names an edge (`a->b`) or a target state (`b`: every
edge into it; the edge wins); its level is `member` (the default), `author`
(the record's author or a team owner), `owners`, or `role:<field>`: the
member a `member` ref of the record names BEFORE the write (or a team owner);
a mover the level excludes is refused (FORBIDDEN `Only the record's approver
or a team owner can move status to approved`). A write that leaves the state
as it is always passes the machine (the policy still applies). At install,
issues at `/records/<kind>/transitions…`: the field is a declared enum field,
every state is one of its values, a `who` key is a declared edge or a state,
a level is one of the above (a `role:` field a `member` ref of the kind).
Every row of such a kind carries `transitions`: the next states the viewer
may take (no state yet = the initial states), so a template offers only
those (a `Select` over `row.transitions`); a kind with transitions declares
no field of that name. The rule is `transitionRefusal` / `transitionsFrom` /
`validateRecordTransitions` (Rust `transition_refusal` / `transitions_from`,
the FFI's `*_json`, the SDKs' `RecordTransitions`); the records value takes
the kind's machine and the viewer's role (`recordsValue(rows, query, {…,
transitions: kindTransitions(kind), role})`). Exponential's server enforces
it on every create, patch, batch write and restore (an owner may restore any
state).

**Migrations** (Apps R8, VAPP-233; `fixtures/record-migrations.json`):
a new version that reshapes a kind says how its records move:

```jsonc
"migrations": [{ "from": "^1", "kind": "entry", "ops": [
  { "rename": { "from": "issues", "to": "refs" } },           // a present field moves (the target overwritten)
  { "default": { "field": "project", "value": "Exponential" } }, // a missing or null field is set
  { "drop": { "field": "legacy" } },
  { "map": { "field": "hours", "expr": "minutes / 60.0" } },   // CEL over the record (a missing name reads null)
  { "renameKind": "worklog" }                                  // later migrations follow the new kind
]}]
```

A record written under version `v` as kind `k` runs, in order, every
migration whose `from` range holds `v` and whose `kind` is the record's kind
at that step (`migrateRecord(migrations, v, k, data)`; ranges: `1.2.3`,
`1.x`, `1`, `*`, `^v`, `~v`, `>=v <w`, `a || b`). A failing `map` fails the
record (`migration <i> op <j>: …`) and leaves it as it was. At install,
issues at `/migrations/<i>…`: `from` a range, `kind` a kind name, each op
exactly one of the five, a `map` that parses, the kind a migration ends in
declared; at most 64 migrations of 32 ops. **The major-bump rule**: a new
version whose `version` has a higher MAJOR than the published one and that
changes a published kind incompatibly (the kind removed, a field dropped,
retyped, its enum narrowed, or newly required) must cover every change with
an op of a migration from the published version (`removed` = renameKind;
dropped = drop or rename from; retyped / narrowed = map; required = default,
map or rename to), else the host refuses the upsert (`migrationRefusal`:
`entry drops note in 2.0.0: add a migration from 1.4.0 that covers it`). An
added optional field or kind needs none, and a minor bump is never refused.
Exponential sweeps the app's records into the new shape when the version is
published (progress on the Versions row), and the editor previews a draft
with migrations on migrated copies of the records (nothing written; the
FFI's `migrate_record_json` / `migration_refusal_json`, the SDKs'
`RecordMigrations`).

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
| `until=<endField>` | Apps U3: a row SPANS its `by` day to its `until` day (inclusive); the calendar views below place it on every day it covers |
| `view=week&day=YYYY-MM-DD` | Apps U3: the week view (Monday first, the week holding `day`, default today): `week` = {key, label, prev, next, today, days, segments} |
| `view=timeline&start=…&scale=week\|month\|quarter&columns=n&row=<field>` | Apps U3: the timeline: `timeline` = {scale, key, label, prev, next, today, columns, bands, rows: [{key, index, lanes, count, items}], now} |
| `day=`, `lanes=1..8`, `tone=<field>` | Apps U3: the month view's selected day (its `agenda`), the lanes per week before "+N more" (default 3), the field a row's colour (`chart1`..`chart8`) comes from |
| `app=<slug>` | another app's rows (the HOST decides which rows that is) |
| `<field>=v` | a data field equals `v` (as text) |
| `<field>.gt\|gte\|lt\|lte=v` | numbers when both parse, else text (code units) |
| `<field>.in=a,b` | the field (as text) is one of the comma-separated values |
| `<field>.between=lo,hi` | inclusive, numbers when both parse, else text |
| `before=<date>` | the rows whose date (`by`'s field, else `createdAt`) is before it leave the rows and are totalled into `before` |
| `expand=<field>[,<field>]` | Apps R5: each ref field reads its target inlined (below) |
| `q=<json>` | the JSON form below (the other params are ignored) |
| `query=<name>` | one of the package's `queries` (below) |

Ties sort by `id`. The reserved keys (`catalog/host.json`
`package.records.query.reserved`) never filter a field. VAPP-189: ONE
evaluator for every host. The params parse INTO the structured query
`{from, app, mine, user, where: [{field, op, value}], sort: [{field, order}],
limit, sum, month: {field, key} | null}` (`parseRecordsQuery(params, kind?)`,
`parseRecordsSource(source)`), and `recordsValue(rows, query, {me, now,
names, computed?})` evaluates it over the rows of any store
(`@exponential_app/eui/host/query`, the Rust core's `host::query`, the FFI's
`parse_records_query_json` / `parse_records_source_json` /
the `RecordStore` handle for the Swift and Compose SDKs);
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
  "expand": ["customer"],                           // ref fields inlined (Apps R5)
}
```

- `expand: [field…]` (Apps R5): every output row carries each named REF's
  target in place of its id, one level: a `{kind}` ref = that record as a
  row (`{…data, id, createdAt, updatedAt, createdBy, authorId, author}`, its
  own refs not followed), an `issue` ref = the issue's summary `{id,
  identifier, title, status}`, a `member` ref = `{id, name}`. A dangling ref
  (deleted, unknown, not readable by the viewer) and an empty one read null;
  a name that is not a ref of the kind stays as stored. Filters, sorts and
  groups read the stored id; groups and totals are never expanded. The host
  hands the evaluator the kind's refs (`kindRefs(records, kind)`, the FFI
  context's `refs`) and the issue summaries (context `issues`, by
  identifier); a named query's `expand` must name refs of its `from`.
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

**Calendar views** (Apps U3, `src/host/calendar.ts`, the Rust core's
`host::calendar`, fixtures/records-query.json): the month view's `month`
gains `weeks` (Monday-first, padding days included: `{key, days: 7 ×
{date, day, weekday, initial, inMonth, weekend, today, selected, total,
dots: [{tone}]}, segments}`), `day` (the selected day) and `agenda` (`{date, label,
count, items: [{id, tone, start, end, days, row}]}`). A segment is `{id,
col (1 = Monday), span, lane, contL, contR, tone, more, row}`: the spans
crossing the week take the lowest free lane (first day, then the longer
span, then the query's order); past `lanes` a day gets one `more: N`
marker in lane `lanes`. The JSON form spells the views `month: {field,
key?, until?, day?, lanes?, tone?}`, `week: {field, key?, until?, lanes?,
tone?}` and `timeline: {field, until?, row?, scale?, start?, columns?,
tone?}`, one per query. The `Calendar` and `Timeline` components draw them
(catalog macros): bind the view to a path and name it in `source`:

```json
{"bindings": [{"path": "/leave", "source": "records:leave?query=month&month=${month}&day=${day}"}],
 "components": [{"id": "cal", "component": "Calendar", "source": "/leave", "labelField": "label",
   "on": {"select": {"functionCall": {"call": "bind", "args": {"path": "/leave",
     "source": "records:leave?query=month", "params": {"month": {"path": "/leave/month/key"}, "day": {"path": "date"}}}}}}}]}
```

A day's `select` and a bar's `open` run in the pressed item's scope: a
relative `{"path": "date"}` reads the day, `{"path": "id"}` the record (a
bar's row is under `row`). `Timeline` takes `source` (a `view=timeline`
binding), `labelField`, `rowTitle` and `showCount`.

**The viewer** (VAPP-206): in Exponential, the source `exp:me` binds
`{id, name, email, avatar, role, access}` (team role `owner` | `member`, app
access `read` | `write`). An owners-only card reads it in `visible`; the kind's
`policy` stays the authority.

**Named queries:** `queries: {<name>: <JSON query>}` (names
`^[a-z][A-Za-z0-9_]{0,31}$`, each must parse clean, `from` a declared kind).
**Host params** (VAPP-206): a binding source and a query read `${today}`,
`${now}` and `${month}`, filled by the host in the team zone when the screen
opens and at each `bind`, so `records:entry?date=${today}` has rows at once. In
a named query a binding's own param of the same name wins. `mine` may be a
template (`"mine": "${mine}"`; `true` or `1` = mine). A sort key's third item
places a missing value whatever the order: `["due", "asc", "last"]` (unset = it
compares as empty text, first ascending, last descending).

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

## Record macros

VAPP-217 (Apps U2, `src/host/record-macros.ts`, Rust `host::record_macros`,
`fixtures/record-macros.json`; the expansion: `catalog/macros.json`,
`fixtures/catalog-macros.json` `RecordForm/*` + `RecordTable/*`). Two catalog
macros put a kind's whole form or table on a screen as ONE node:

```jsonc
{ "id": "deals", "component": "RecordTable", "kind": "deal", "query": "sort=close&order=asc", "columns": ["title", "amount", "stage"] },
{ "id": "newDeal", "component": "RecordForm", "kind": "deal", "submitLabel": "Add deal" }
```

- **RecordForm** `{kind, recordId?, submitLabel?, readOnly?, cancel?, path?}`:
  a Form with one field per schema field (required first, then by name;
  files are left out): `date` → DatePicker, `time` → TimePicker,
  number / integer → NumberField (integer: step 1), boolean → Switch, a
  string enum → Select, an enum list → a multiple Select, another list →
  ChipInput, a `member` / `issue` ref → a searchable Select over
  `exp:members` / `exp:issues`, a string without `maxLength` or longer
  than 200 → Textarea, `date-time` → DatePicker + TimePicker side by side
  (in the team zone; `<path>Parts/<field>/date|time`, seed them to edit;
  no day picked = the draft's value as it is), else Input. Labels are the field names humanized, placeholders
  the schema `description`. Checks come from the schema: required,
  `maxLength`, `minimum` / `maximum`. Short fields pair in a two-column
  FieldGroup on wide frames. The draft lives at `path` (default
  `/recordForm/<id>`; seed it to edit); submit = `records.upsert {kind,
  id: recordId, data}` unless the node has its own `on.submit`.
  `readOnly` disables the form and drops the submit. For a team owner the
  first field is **Author** (a searchable Select of the members, default the
  viewer; seed `<path>Author` and `<path>AuthorBase` to edit), sent as
  `author` only when it moved. A refused save shows inline: the host writes
  invalid input (a schema issue, a bad author) at `<path>Error`, under the
  fields, anything else toasts.
- **RecordTable** `{kind, query?, columns?, search?, filter?, totals?, add?, editable?}`: a toolbar
  (the count, Add) over the stock Table (compact, sticky header, sortable)
  with the Data tab's typed widths, a relative Updated column and an
  Edit / Delete cell. A `columns` entry is a field name or a Table column
  object (`{"key": "gross", "type": "currency", "currency": "EUR", "decimals": 2}`,
  `{"key": "date", "format": "dd.MM.yy"}`, a `label`, `align`, `width`) over
  the field's defaults. `search: true` adds a search field over the text
  columns, `filter: "<enum field>"` a Select of its values (All first), and
  `totals: true` the Table's sum row (number and currency columns over the
  rows the search and filter leave); phones show the same rows as a List of Row, each
  with its own Delete. Add, Edit or a row press opens a RecordForm in a Drawer; Delete asks first,
  then `records.delete`. The Drawer stays open on a refused save (Cancel
  closes it, so no Escape or backdrop). Rows follow `records:<kind>?<query>` (default
  newest first) at `/recordTable/<id>/rows`. States: loading (Skeleton),
  empty (`No records yet`), an undeclared kind (the unknown state),
  `editable: false` or a read-only viewer (no Add, no Edit / Delete).
- **The host fills them.** A macro sees only its own props, so before a
  template is sent (`templateMessages`) or checked at install, the host
  fills every RecordForm / RecordTable from the package's `records`: the
  fields, columns, bindings and actions (catalog props marked `host`,
  never written by hand and left out of the model's prompt). Install
  needs `records.upsert` (a table also `records.delete`) in `functions`
  and `records:<kind>` in `sources` when `sources` is set; the macros' own
  member and issue lists (`/recordMembers`, `/recordIssues`) need no entry.
- **A host's `records.upsert`** gets the form's `author` (write on behalf,
  when given) and `form {error, close}` (paths): a landed write clears
  `error` and sets `close` false; the TS runtime does it, and a rejection
  with `RecordRefusal` writes its message at `error`. Other SDKs' hosts write
  them through `setData`.

## Public links

VAPP-241 (Apps P1, `src/host/share.ts`, Rust `host::share`,
`fixtures/package-share.json`; the links themselves:
`packages/domain-contract/fixtures/app-share.json`). `share` declares the
links an owner can turn on; nothing is public until one does.

```jsonc
"share": [
  { "name": "signup", "mode": "submit", "kind": "lead", "fields": ["email", "note"], "expires": 30 },
  { "name": "wall", "mode": "view", "kind": "lead", "fields": ["note"], "query": "recent" },
  { "name": "board", "mode": "view", "kind": "lead", "fields": ["note"], "query": "recent", "template": "publicBoard" }
]
```

- At most **5**. `name` = a lowercase letter, then lowercase letters, digits
  and underscores (24 at most), unique. `mode` = `submit` or `view`. `kind` =
  a declared kind.
- `fields` = the fields the link exposes, in order; default every field but
  files and refs. A file or ref field is refused (a guest has no picker for
  a member, an issue or another record); a `submit` link must name every
  required field, and cannot fill a required file or ref field.
- `query` (view only) = a package query of the kind. `expires` = whole days
  from enabling, 1 to 3650; default never. An owner can change a live link's
  expiry from the host's settings until the next enable.
- `template` (view only, VAPP-246) = a package template the guest's page
  renders; default a read-only RecordTable. Install refuses one a guest's host
  could not run. It may read only `records:<kind>`, or
  `records:<kind>?query=<the link's query>`, and every read gets the one
  snapshot as `{rows, count}`. It holds no RecordForm, and a RecordTable sets
  `editable: false`. A press may call only `set` and `openUrl`. It uses no
  listed extension component and no `exp:` string. Its expressions must check
  over the named fields with no query, so a totals or sum read and a field
  outside `fields` are refused; the `template` issue names the expression's reason.
- **submit:** a guest fills one RecordForm and creates exactly one record,
  validated like any write. **view:** the `template`, else a read-only
  RecordTable, over a snapshot the host computes per request: only `id`,
  `updatedAt` and the named fields, no live data.
- A host offers no other kind, no member data and no `exp:` source to a guest.

### 3.4 Permissions

Layers, all intersected:

1. **Team membership** ✅. Only members, unless a public link applies (§9.4).
2. **Per-app member policy** `app_access` none/read/write. ✅ (#VAPP-199) applied everywhere: MCP, the records service (none = NOT_FOUND, read = writes FORBIDDEN), the apps router, the apps / app-versions / app-records shapes (none = the app and its rows never sync), the RLS scope (`eapps.write`), and the clients (`appAccess.mine`; the Data tab disables writes). An app's own writing controls paint disabled at read (#VAPP-200, the genUI `readOnly` host option ×7 renderers).
3. **Per-kind policy** ✅ (R3, #VAPP-199, `packages/exponential-ui/fixtures/record-policy.json`, TS `host/record-policy.ts` + Rust `host::record_policy`, iOS/Android hand ports replaying the fixture):

```jsonc
"policy": {
  "read":   "member",          // member | author | owners (owners = not even the author)
  "create": "member",
  "update": "author",          // the author or a team owner
  "delete": "author"
}
```

The defaults are `member/member/author/author`. This is the common case; it ships with `formatVersion 3`, and as Apps keeps no backwards compatibility (decided 2026-11-02) v2 packages are not read after that.

The server enforces the policy (the records service on every read and write, the broker's reads, the import's creates; a row the member may not read is NOT_FOUND). The app-records shape enforces `read: author` / `read: owners` with a row filter for non-owners (`lib/apps/records-shape.ts`): a private kind never syncs to other members' devices. `create` is never `author`. Package tests take `role` and run under the policies. ✅ R6 (#VAPP-222): `role:<field>` levels live on a kind's `transitions.who` (the member a `member` ref names), not on `policy`.

   ✅ #VAPP-206: a package reads the viewer through the `exp:me` source, `{id, name, email, avatar, role, access}` (`app-sources.json` ×4; in `sources` like any other), so an owners-only card hides itself: `"visible": {"call": "eq", "args": {"a": {"path": "/me/role"}, "b": "owner"}}`. An action reads it as `data.me.role`. This is presentation only: the policy above stays the authority.

4. **Field-level privacy is out.** Use a separate kind with `read: owners` (for example, salary next to a hiring candidate).
5. **The Postgres scope** ✅ (#VAPP-193), a backstop under layers 1 to 3, not a replacement. Every server read and write of a record runs through ONE service (`apps/web/src/lib/apps/records-service.ts`), which checks layers 1 to 3, ownership and quota, then runs the record statements as the `NOLOGIN` role `eapps` scoped to one team and its apps (§12.2). Ownership and policy stay in the service, never in RLS.
