---
name: exponential-apps
description: "Build or change an Exponential app (Apps): the package format (formatVersion 5), record kinds and their policies, named queries, CEL expressions and functions, optional values, actions with require, package tests, exposes, draft vs published, with three complete sample apps as worked examples. Use when writing or editing an app package (exponential_apps_write upsert) or its records."
---

# Exponential apps

> Generated from the Apps docs (genUI 0.21.0) — do not edit; regenerate with packages/exponential-ui/scripts/generate-skills.ts.

An Exponential app is **data, never code**: a declarative package (manifest, genUI templates, bindings, record kinds, a function allowlist, `exposes`, `upstreams`) stored per team, versioned in the DB, and painted by the same genUI renderers on web, desktop, iOS and Android, as well as Spezl and eDE.

## Workflow

An app is a declarative genUI package (manifest + A2UI templates + bindings + a function allowlist; data, never code). Read the current package with exponential_apps {op: "get", slug: "<slug>"}; learn the components, bindings, host functions and records with exponential_apps {op: "catalog"}; save every change as a new draft version with exponential_apps {op: "upsert", slug: "<slug>", package, note} (members keep running the published version); publish with exponential_apps {op: "publish", slug: "<slug>"} only when the person asks (they also have a Publish button). An app keeps its own data as RECORDS (declare the kinds in the manifest, bind records:<kind>, write with records.upsert); seed or inspect them with exponential_apps {op: "records_upsert" | "records_list", slug: "<slug>", kind, data}. An app may also EXPOSE MCP tools (exposes: declarative bindings over its records, exponential.<noun>.<op> operations or a team MCP server in upstreams) that a member's MCP client runs as that member; the catalog documents the format. A person may attach a spreadsheet (CSV, TSV or xlsx) to fill the app: follow the import recipe (the exponential-apps skill's references/import.md, docs/spec/apps.md §10.2): read it, map the columns onto a kind, show a dry run and ask, then records_upsert row by row in batches.

Components and their props are NOT listed here: read exponential_apps {op: "catalog"} once (only: [names] for just a few), and the `exponential-ui` skill for the surface rules. Animations: the `exponential-motion` skill.

## Checklist

- `formatVersion` is exactly 5; `sources` lists every source a binding follows (`[]` for none) and `functions` every function a press calls.
- Declare every record kind with a schema; pick its `policy` (read/create/update/delete: member | author | owners; create never author) before writing screens.
- A kind's add/edit form or its list of records = ONE `RecordForm` / `RecordTable` node (`references/records.md`, Record macros), never a hand-built Form or Table.
- Prefer a named query (group, aggregate, running) over summing rows in expressions.
- A field not in `required` may be missing: guard it (`has(r.f)`, `orElse(r.f, d)`), else the upsert is refused at that path. Every `{call: "expr"}` has a `default`.
- Writes that must land together are one action; check inputs with `require` steps.
- Add a test for every expression, query and action: a failing test refuses the upsert as `/tests/<i>`.
- A spreadsheet to import: `references/import.md` (map, dry run, ask, then write in batches).
- Save a draft early and often; publish only when the person asks.

## References (read on demand)

- `references/package.md` — The package and its versions.
- `references/records.md` — Records: kinds, policies, queries, writes, RecordForm / RecordTable.
- `references/import.md` — Importing a spreadsheet (CSV, TSV, xlsx) into records.
- `references/expressions.md` — Expressions (CEL), functions and optional values.
- `references/actions.md` — Actions: all-or-nothing writes, require.
- `references/tests.md` — Package tests.
- `references/tools-and-exposes.md` — Tools, exposes and extensions.
- `references/automations.md` — Automations: schedule, record, event and webhook triggers; agent hand-off.

## Worked examples (complete packages)

- `examples/crm.app.json` — **CRM**: A small team CRM: companies, their contacts and the deal pipeline. Companies and contacts are shared (any member edits them); a deal is changed by its author or an owner. (kinds company (policy update member, delete member), contact (policy update member, delete member), deal; 3 named queries; 5 actions; 14 tests; template companies, contacts, pipeline)
- `examples/leave-planner.app.json` — **Leave planner**: Plan the team's time off: request leave, see the month as a calendar (who is out, every day) and the next twelve weeks per person as a timeline. On phones the calendar is a compact month with the selected day's list. Agents read who is out over MCP. (kinds leave (policy read member, create member, update author, delete author); 3 named queries; 1 actions; 4 tests; template calendar, timeline)
- `examples/standup.app.json` — **Standup**: The async daily standup: what you did yesterday, what you do today and what blocks you. Everyone reads every update; only its author (or an owner) edits or deletes it. (kinds update (policy read member, create member, update author, delete author); 3 named queries; 3 actions; 12 tests; template standup)
- `examples/time-tracker.app.json` — **Time tracker**: Log your hours per day against the team's projects; see the month per day and per ISO week, the total and what is left of the monthly target. Entries are private to their author (owners see everyone's). Agents read the weeks and log hours over MCP. (kinds entry (policy read author, create member, update author, delete author), project (policy read member, create owners, update owners, delete owners), settings (policy read member, create owners, update owners, delete owners); 3 named queries; 5 actions; 17 tests; template month)
