# The package format (Apps)

VAPP-161. One format for an Exponential app and a spezl block: a declarative
genUI package (VAPP-82: data, never code) plus the file it travels
in. The format is transport-neutral: it names what a host provides (a record
store, tool runners, extensions), never how (Exponential serves records from
its synced store, spezl from a p2p store over P2P). `catalog/host.json`
`package` is the machine-readable contract; this page is the prose spec other
docs link to (spezl `docs/spec/blocks.md`). The TS reference is `src/host/`
(`package.ts`, `records.ts`, `record-store.ts`, `query.ts`, `actions.ts`, `screens.ts`, `tests.ts`, `tools.ts`, `exposes.ts`, `signature.ts`);
the Rust core (`apps/desktop/crates/eui-core/src/host/`) mirrors every
pure rule. Fixtures: `fixtures/expressions.json` (the expression language, below), `fixtures/actions.json` (effects and actions, below), `fixtures/package-tests.json` (package tests, below), `fixtures/host-router.json` (package validation, replayed
by every renderer through the core), `fixtures/records-query.json` (the
records query and its value) and `fixtures/host-package.json` (tool consent,
extension compatibility, canonical JSON, signatures).

> **Format 6 is coming.** Hosts already read it (A2UI 1.0: `@path`, `@call`,
> flat function names; `docs/round-9-contract.md` §12), but this document and
> what the server stores stay at format 5 until the flag day (§14.1): write 5.

## Versions

A package MUST carry `formatVersion`. This document is format version
**5** (`catalog/host.json` `package.formatVersion`; 3 added the per-kind
`policy`, 4 added a screen's `nav` and `exposed`, 5 added `capabilities`), and a host reads exactly its
own version: Apps keeps no backwards compatibility. A missing one is
`/formatVersion: formatVersion is required (5)`, an older one `format version
3 is no longer read: write 5`, a newer one `format version 6 is newer than
this host (5)`; a host never guesses at keys it does not know. The catalog a package
renders against keeps its own version in its id (`catalogId`,
`…/catalogs/core/v1`) and is negotiated as before (`supportedCatalogIds`).

## The package

```jsonc
{
  "formatVersion": 5,                  // required: the current format
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
  "screens": [{ "template": "month", "title": "Month", "icon": "calendar" }, { "template": "team", "title": "Team", "icon": "users", "params": { "range": "week" } }],
  "tests": [{ "name": "a duration", "expr": { "expr": "formatDuration(7.5, 'h:mm')" }, "expect": "7:30" }],
  "extensions": [{ "id": "https://acme.example/catalogs/charts/v1", "version": "1.4.0", "components": ["Gantt"] }],
  "capabilities": [{ "id": "photos.read", "why": "Pick the photos for an album." }],
  "theme": "exponential", "icon": { "glyph": "clock" }, "description": "…"
}
```

Required: `formatVersion`, `id`, `name`, `version`, `catalogId`,
`templates`. Everything else is optional (`functions`, `sources`, `theme`,
`icon`, `description`, `records` kinds: `catalog/host.json` `package`).

**Components are checked at install (VAPP-203):** `validatePackage` reduces
every template exactly as the renderer will (`reduceSurface`), so a surface
that would render broken never installs. An unknown component, a missing,
unknown or mistyped prop, a slot the component has not, a cycle or a style
mistake is an issue at the component's entry
(`/templates/<id>/components/<i>`, the reducer's message, e.g. `props.text:
required`); an id no component has is an issue at every place that names it
(`…/children/<j>`, `…/children/componentId`, `…/slots/<name>`, `<id>: no
component with this id`); a `children`, `slots`, `on` or `style` of the wrong
shape is an issue at that key. Extension components are checked against the
extension's definition when the host has it (TS: `AvailableExtension.def`,
filled by `availableExtensions(defs)`; Rust: `validate_package_in` with a
`CatalogView`); without one their names stay open. A component no tree
reaches (neither the root nor a list's row template) is not checked.

## Screens

VAPP-218 (Apps U1, `src/host/screens.ts`, `fixtures/host-router.json`
`screens` + `navigate`; `catalog/host.json` `package.screens`). `screens`
is the navigation manifest: the templates a person can open, in order.

```jsonc
"screens": [
  { "template": "pipeline", "title": "Pipeline", "icon": "square-kanban" },
  { "template": "deals", "title": "Deals", "icon": "briefcase", "params": { "stage": "open" } }
]
```

- At least 1 and at most **5** screens in the navigation, each keyed by its
  `template` (a declared template, no duplicates); `title` 1..32 characters;
  `icon` a genUI icon name; `params` an object of strings, the params
  it opens with. Install refuses an unknown template, a duplicate, a sixth
  navigation screen, an unknown key, a bad icon or a non-string param.
- **`nav: false`** (the only allowed value) is a detail screen: outside the
  sidebar and the bottom bar, and not counted in the 5, since that limit is the
  phone bar's room. It opens by `app.navigate` or a link. A package with only
  `nav: false` screens is refused.
- **`exposed: {description, params?}`** is the screen's public contract, the
  one thing another app may link to: a stable name and checked params
  (`params: {<key>: {required?: boolean, description?}}`, keys letters, digits
  and `_`; `description` 1..200 characters). It is not access control: the
  target's own policy still decides what the viewer sees.
- **The host draws the navigation**, never the app: a sidebar on wide
  frames (desktop, web ≥ md), the phone bottom bar on phones. A package
  without `screens` shows its first template as before. `app.navigate`
  opens screens only, so a detail view is a `nav: false` screen or a
  Dialog/Drawer. `app.navigate` is for the app's OWN screens and replaces the
  one shown.
- **`app.navigate {screen, params?}`** opens a screen: as a press
  (`on.press.functionCall {call: "app.navigate", args: {screen, params}}`)
  or as an action step (`{call: "app.navigate", args: {screen: "'deals'",
  params: {stage: "params.stage"}}}`, CEL like every step arg), after the
  action's writes. It is always allowed (the package need not list it in
  `functions`). A literal target names a screen at install (`navigate:
  <x> is not a screen`); at run time a target that is no screen, or params
  that are not strings, is refused with `FUNCTION_DENIED`.
- **Params:** the opened template gets `{params}` over its own data, so a
  component reads `{"path": "/params/<key>"}`, and a binding source's
  `${params.<key>}` is filled with the percent-encoded value (missing = empty)
  when the template opens (`records:deal?query=byStage&stage=${params.stage}`).
  A screen's params = its own `params`, then the request's, key by key
  (`resolveScreen`; strings only).
  Params are data: opening a screen runs no action or function.
- **Deep links** carry the same pair: on Exponential
  `/t/<team>/apps/<slug>/<screen>?<params>` and the natives' app links.
- **Links.** `Link.href`, `Row.href`, `Button.href` and `openUrl` go through the
  host's URL policy. `~/x` expands against the host's optional `linkBase`; a
  host without one denies it, so it paints as text, and `~//…` or a `.` or `..`
  segment (percent-encoded too) is always denied. On Exponential `linkBase` is
  `/t/<team>`: `~/issues/EXP-12` opens the issue, `~/devices` the Devices page,
  `~/apps/<slug>/<screen>?k=v` an app screen. `href` is a literal; a link per
  list row is a press: `{"on": {"press": {"functionCall": {"call": "openUrl",
  "args": {"url": {"call": "formatString", "args": {"value": "~/issues/${identifier}"}}}}}}}`.
- **A link at another app** (`~/apps/<slug>/<screen>?k=v`, from a surface of a
  different app): the screen must be `exposed` and its required params present
  and non-empty; only declared params arrive. Anything else, a missing app
  included, is one toast ("Not found or no access"). The link pushes, so back
  returns to the first app. `crossAppTarget(pkg, screen, params)` is the rule. A
  person's URL or an agent's chat link opens any declared screen.
- **App files** (VAPP-271, `catalog/host.json` `media.appFile`): an image
  `src` of the form `app-file:<fileId>` (the `fileId` a capability answered:
  `photos.pick`, `print.pdf`, `files.open`) shows that file of the app. The
  host resolves it from its own file store under the usual image limits; the
  Engine's runners do (web, desktop, iOS, Android). A host without app files
  (Exponential itself) loads nothing and paints the image's fallback. Bind it:
  `{"src": {"call": "expr", "args": {"expr": "'app-file:' + f", "vars": {"f": {"path": ""}}, "default": ""}}}`.
- **`@media (form-factor: phone|wide)`** is the style condition for a
  layout that truly differs on phones (the host's form factor; unset = the
  surface width against `$breakpoint.md`).

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

## Capabilities

VAPP-260 (format version 5, `src/host/capabilities.ts`, Rust
`host::capabilities`, `fixtures/host-router.json` `acme.capabilities` /
`acme.badcapabilities`; the runtime: `packages/eengine`
`fixtures/capabilities.json` + `consent.json`). `capabilities` is the
package's **capability manifest**: the most its UI and its agents may reach
through the Engine's broker, and what a bundler maps onto platform permissions.

```jsonc
"capabilities": [
  { "id": "photos.read", "why": "Pick the photos for an album." },
  { "id": "http.fetch", "why": "Load exchange rates.", "origins": ["https://api.example.com"] }
]
```

- At most **16**. `id` = one of `camera.capture`, `clipboard.read`,
  `clipboard.write`, `files.read`, `files.write`, `http.fetch`,
  `location.read`, `notifications.post`, `photos.read`, `print.pdf`,
  `share.send` (`catalog/host.json` `package.capabilities.scopes`), each at
  most once. A host's own scopes (eDE's `shell.edit`) are never declarable.
- `why` = the author's line on the consent sheet, 1 to 200 characters.
- `origins` = required for `http.fetch` and refused anywhere else: a
  non-empty list of distinct `https://host[:port]` origins (lowercase, no
  path). A call to any other origin is refused.
- Declaring is not granting: the person answers the Engine's consent sheet
  (an `ask` scope, once) or the platform picker (`files.*`, `print.pdf`,
  `share.send`, every call). Absent = no capabilities.

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
package lists in `functions`, + `args`, + `result`); at most 32 actions of 64 steps. A
JSON string in a step is CEL over `params`, `data` (the surface data at the
press), `me` (the member's id) and `results` (the answers named so far);
anything else is a literal. Install
checks every expression's type (a `require` is a bool, an upsert's fields
are in the kind's schema with its types, a create names every required
field). VAPP-204: a press's `actions.<name>` args are checked against
`params` at install too (a key that is not a param, a required param left
out, a literal of another type; a dynamic arg is checked when it runs), and
a press of `bind` with a literal `source` is checked like a template binding
(a URI, in `sources`, a declared kind, a package query).

**A call's answer** (VAPP-271). A `call` step may name what it answers,
`result: <name>` (a letter, then letters, digits and `_`; once per action),
and the steps after it read that answer as `results.<name>` (open: `dyn`):

```jsonc
"addPage": {
  "params": { "type": "object", "properties": { "album": { "type": "string" } }, "required": ["album"] },
  "steps": [
    { "call": "photos.pick", "args": { "limit": 10 }, "result": "picked" },
    { "require": "size(orElse(results.picked.photos, [])) > 0", "message": "No photos picked" },
    { "records.upsert": { "kind": "page", "data": { "album": "params.album", "photos": "orElse(results.picked.photos, []).map(p, p.fileId)" } } }
  ]
}
```

Such a call runs WHEN THE PLAN REACHES IT, before the commit (the policy and
consent first); a refusal or a failure discards the action, and nothing
lands. Every other call still runs after the commit. Install refuses a
`result` on a step that is no call, a bad or repeated name, and a step that
reads `results.<name>` before the step naming it (`unknown field <name>`). A
list of open items (an answer mapped) fits any list field; the kind's schema
checks it when it runs. Every Engine capability (`photos.`, `files.`,
`print.`, `share.`, `camera.`, `clipboard.`, `http.`, `location.`,
`notifications.`) is a host function: a step may call it when `functions`
lists it, a binding never. A package test gives the answers:
`{"action": {"name": "addPage", "params": {…}, "results": {"picked": {"photos": [{"fileId": "f1"}]}}}}`.

**Create or patch, and absent stays absent** (VAPP-206). One action can do
both: `params` may name `id` (a param is no record), and a `records.upsert`
`id` may be `string?`; then its `data` names every required field, and at run
time a null or `""` id creates while a non-empty string patches. Writing a
field: an expression that evaluates to null writes nothing, so a param left
out neither stores null nor clears the row's value. `""` in a field the kind
does not require (an unpicked Select, an emptied Input) writes nothing on a
create and clears on a patch. A literal JSON `null` writes nothing on a create
and clears on a patch. A create never stores a null field, and a RecordForm
follows the same rules.

**All or nothing.** `runAction` evaluates the steps in order against ONE
snapshot (a `set` never changes what a later step reads) and returns the
plan `{writes, sets, calls}`, `{pending}` (the first `result` call without
an answer: the host runs it and runs the action again with
`ctx.results`), or the one error that discards the action: a
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
them (Exponential: the team's server of exactly that name, or the one a team
owner picks for this app at install or on its Settings). The package always
keeps the declared name, so a shared package works with any server name.

A template can also READ an upstream: a binding source
`upstream:<server>/<tool>?<args>` (the name splits at its last `/`; each
param is one argument, read as JSON when it is a number, `true`, `false`,
`null`, an object or an array) follows one read-only tool of a declared
upstream. The host runs it as the viewer (Exponential: the server broker, the
viewer's own connection, cached 60 s) and writes its answer at the path:
`structuredContent`, else the text as JSON, else the text. List it in
`sources` (`upstream:crm/*`); a tool that sets no `readOnlyHint` reads only
when `sources` names it exactly (`upstream:crm/mrr`). A viewer without a
connection keeps the path's template data and sees "Connect <server>".

An upstream only a member's machine reaches (a stdio command, a LAN Home
Assistant) is a team server that names that member's device (Exponential:
Settings › MCP). The package does not change: calls and reads by that member
(or automations they sponsor) run on their device; anyone else's are refused
(`deviceNotOwned`), and while the device is offline the path keeps its
template data and the host shows "<server> runs on <device>, offline".

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
| `run` | EXACTLY ONE of `records` / `action` / `upstream` / `agent` |

- `records: {op: list | get | upsert, kind, id?, where?, data?, limit?}` reads
  or writes the package's OWN records of a declared kind. A delete is
  destructive and never exposed. A `list` gets the SAME query as a binding
  (VAPP-216): any key of the JSON query form ("Records") rides on the run
  (`where` as a list of conditions, `sort`, `limit`, `sum`, `month`, `group`,
  `aggregate`, `running`, `before`, `mine`, `user`), or `query: "<name>"`
  starts from one of the package's `queries` (its `${param}` templates
  filled from the call's input; the run's own keys win). `where` as an
  object = data containment (`{"date": "${input.day}"}`). The host reads the
  caller's rows of the kind (its read policy first) and answers the records
  value (`rows`, `count`, `sum?`, `groups?`, `totals?`, `before?`, `month?`);
  `app` (another app) and another `from` are refused. Exponential caps the
  rows read at 25,000 (`truncated: true` beyond).
- `action` (+ `args`) calls a host function (`exponential.<noun>.<op>`). It
  reads when the last word of its name is list, get, show, catalog,
  activity or files; anything else writes.
- `upstream` (+ `tool`, `args`) calls a tool of one of the package's
  `upstreams` with the person's own credential. Its level is the tool's live
  annotations: it reads only with `readOnlyHint: true`.
- `agent` (+ `prompt`, `device?`) hands judgement to a TEAM AGENT, named by
  its slug (its name lower-cased, everything outside `a-z0-9` one `-`:
  "Triage Bot" = `triage-bot`). The host starts or resumes the person's own
  conversation with that agent (Exponential: `sessions.send`, the session
  reading "via <app>") with the package's `prompt` (at most 4000
  characters, frozen per version; `${input.x}` lands as inline data in
  backticks) followed by the whole input as a fenced JSON DATA block,
  never instructions. It always writes (`access: "write"`); `device` is
  absent or `"default"` (the agent's own device). At most 10 hand-offs per
  app a minute. The same form runs from an automation (as its sponsor,
  "Automations") and from the `exponential.apps.handoff` action
  (`{slug, tool, input, record?}`: a press runs one of the package's
  `agent` exposes as the viewer, behind the consent card unless an owner
  granted it).

Every string in `run` may hold `${input.<path>}` (dotted keys into the
call's input; the first key must be a declared input property). A string
that is exactly one template keeps the value's type; inside text the value
is printed. Destructive is never exposed: a name holding delete, merge, close,
end, remove or trash is refused, and a `read` binding whose run writes is
refused. At most 32 tools. `exposeIssues` returns every issue with its MCP Proxy
error code; the host adds its own rules on top (Exponential: only grantable,
non-destructive actions; upstreams the team has).

## Extensions

`extensions: [{id, version, components?}]` declares the custom component
catalogs (genUI extensions, VAPP-85: `defineExtension`) a package needs beyond
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
