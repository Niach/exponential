# Actions: all-or-nothing writes, require

> Generated from packages/exponential-ui/docs/package-format.md (Actions) (genUI 0.20.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

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
