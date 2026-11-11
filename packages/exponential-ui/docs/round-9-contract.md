# genUI: round 9 contract (A2UI 1.0)

Round 9 (VAPP-289) moves eUI from A2UI v0.9 to A2UI **1.0**. It is a hard
cut: every entry accepts and emits 1.0 only, after a bridge that ends on a
flag day (§14). This file is the NORMATIVE spec for both cores, the TS core
(`packages/exponential-ui/src`) and the Rust core
(`apps/desktop/crates/eui-core`). For the same input, the two cores MUST
produce the same bytes (JSON equality with key order as written here) in
every fixture listed in §15. Swift and Kotlin reach the Rust core through
`exponential-ui-ffi`. The web renderers use the TS core.

Upstream reference: `a2ui-project/a2ui` `main` at
`1e9ed04b5585721ac6df8313030f9bfb42ccc6fa` (2026-11-06), vendored
byte-pinned under `vendor/a2ui/v1_0/` (§15). Where upstream and this file
differ, this file wins. The divergences are listed in §16. Words: **agent**
= the server side, **renderer** = the client (upstream 1.0 terminology).
Where a section says "refused", the refusal code and the path rule come
from §7.

## 0. What moved

| area | change | source of truth | locked by |
|---|---|---|---|
| version | wire `version` is `"1.0"`, required and checked by every core entry | `host.json` `a2uiVersion` → generated `A2UI_VERSION` | `a2ui10-messages.json` |
| messages | + `callRendererFunction`, `agentFunctionResponse` in; + `callAgentFunction`, `rendererFunctionResponse` out; `createSurface{components, dataModel, metadata}`; `updateDataModel.value` required | §1 | `a2ui10-messages.json`, `host-router.json` |
| host-internal | `applyTemplate`, `bindDataModel` only through a TRUSTED `route()` | §2 | `host-router.json` |
| dynamic values | `{"@path"}`, `{"@call", args?, catalogId?}`, literal objects, `@@` escape; no `returnType` on the wire | §3 | `bind-time.json`, `catalog-macros.json` |
| identifiers | UAX #31 for ids, component, function, prop, arg and extension names | §4, generated `xid` tables | `a2ui10-messages.json` `ids` |
| components | envelope `ComponentCommon {id, catalogId, accessibility, metadata}`; our extensions become declared core props; `Surface` reserved; `allowedParents/allowedChildren`; mixed catalogs | §5, `core.catalog.json` | `catalog-components.json`, `a2ui10-messages.json` |
| functions | catalog function map, `allowedCallers`, `requiresUserActivation`, flat host function names, `@index`, routing, ValidationResult, CheckRule, boolean coercion, failure rules | §6 | `host-policy.json`, `bind-time.json`, `a2ui10-functions.json` |
| errors | 1.0 codes + ours; exactly one of `surfaceId`/`functionCallId` | §7, `host.json` `clientMessages.errorCodes` | `host-router.json` |
| theme | `createSurface.metadata.extensions.at_exponential_theme` | §8 | `host-router.json` |
| basic catalog | 1.0 id; Text → Markdown (inline profile); Icon `svgPath`/`@path`; Video `posterUrl`; Slider `steps`; TextField `placeholder`, `value` required; regex dialect | §9, `basic-map.json` | `catalog-basic-map.json`, `regex.json` |
| negotiation | capabilities `{"1.0": {…}}`, A2A keys, extension URI, MIME `application/a2ui+json` only | §10 | `host-transport.json` |
| catalog files | `core.schema.json` (+ core-lite, extensions) conform to `catalog_definition.json` | §11, `src/schema.ts` | `schema.test.ts` |
| packages | `formatVersion` 6, re-signed | §12 | `host-package.json`, `actions.json`, `package-*.json` |
| cards | `exponential_show` surfaces gain `"version": "1.0"` | §13 | `a2ui10-messages.json` `cards`, `apps/web` plan tests |
| transition | `upgrade09` (TS + Rust) bridges 0.9 until the flag day, then serves only the stored-data migration | §14 | `upgrade09.json` |
| host contract | `host.json` version **4** | `catalog/host.json` | `manifest.json` `hostContractVersion` |

## 1. Wire

### 1.1 Envelope

A message is ONE JSON object: `version` plus exactly one kind key. Both
directions follow this rule. `A2UI_VERSION = "1.0"` is the only constant
(generated from `host.json` `a2uiVersion` into `src/catalog.generated.ts`,
`generated/catalog.generated.rs`, `ExponentialUICatalog.generated.{swift,kt}`
and `eui-core/src/generated/catalog.rs`). Every hard-coded `"v0.9"` the
inventory lists (Swift `ExponentialHost.swift`, `define.ts`, `connector/app.ts`,
`prompt.ts`, the app hosts, the Engine runners, the guides, the site, the
samples) reads the generated constant instead. `basic-map.json` drops its
`version` key.

Agent → renderer kinds: `createSurface`, `updateComponents`,
`updateDataModel`, `deleteSurface`, `callRendererFunction`,
`agentFunctionResponse`. The host-internal kinds `applyTemplate` and
`bindDataModel` are not A2UI (§2). Renderer → agent kinds: `action`,
`callAgentFunction`, `rendererFunctionResponse`, `error`.

**Check order** in `HostRouter.route(message, options)` (TS) and
`HostRouter::route(message, options)` (Rust). The first failing check
answers and nothing else runs:

| # | check | answer |
|---|---|---|
| 1 | UTF-8 JSON size > `maxMessageBytes` (4 MiB) | INVALID_MESSAGE, surfaceId = `surfaceIdOf(message)` |
| 2 | not a JSON object | INVALID_MESSAGE `a message is a JSON object`, surfaceId `""` |
| 3 | `version` absent or `"v0.9"` | `acceptV09` true: route each message of `upgrade09(message)` in order (with the same options) and concatenate the ops. `acceptV09` false: INVALID_MESSAGE `version is required` (absent) or `unsupported version v0.9` |
| 4 | `version` ≠ `"1.0"` | INVALID_MESSAGE `unsupported version <v>` (`String(v)`; a non-string prints as JSON) |
| 5 | not exactly one key besides `version`, or that key is not a known kind | INVALID_MESSAGE `expected exactly one of createSurface, updateComponents, updateDataModel, deleteSurface, callRendererFunction, agentFunctionResponse` |
| 6 | host-internal kind and `!options.trusted` | INVALID_MESSAGE `<kind> is host-internal` |
| 7 | body not an object | INVALID_MESSAGE `<kind> is an object` |
| 8 | body has a key the kind does not list (§1.2) | INVALID_MESSAGE `<kind>.<key> is not allowed` (the first such key in the body's key order) |
| 9 | the kind's own checks (§1.2) | as listed |

`surfaceIdOf(message)` = the first string `surfaceId` among the message's
object values, else `""`. A 0.9 bridged message is upgraded BEFORE checks 4
to 9, so its errors are 1.0 errors about the upgraded form.

`options`: `{trusted?: boolean (default false), acceptV09?: boolean
(default = generated ACCEPT_V09, §14)}`. Rust: `RouteOptions { trusted,
accept_v09 }`. FFI: `route_json(message_json, trusted)`.

### 1.2 Agent → renderer messages

Each row lists the allowed body keys. The "ops" column says what the router
emits. The op shapes are in `src/host/contract.ts` `HostOp` and are mirrored by Rust.

| kind | keys (req **bold**) | checks, in order | ops |
|---|---|---|---|
| `createSurface` | **surfaceId**, catalogId, components, dataModel, sendDataModel, metadata | surfaceId non-empty string; catalogId string when present; catalogId in the available set (§5.5) else `send UNSUPPORTED_CATALOG` and nothing is created; surfaceId never seen before (§1.4) else INTEGRITY_ERROR; components (when present) an array with ≥ 1 item; dataModel (when present) an object (not an array, not null); sendDataModel boolean when present; metadata = `{extensions?}` (§5.1) | `create {surfaceId, catalogId?, sendDataModel?, metadata?, theme?}`, then `data {path: "", value: dataModel}` when dataModel is present, then the `components` op of the `updateComponents` rule when components are present |
| `updateComponents` | **surfaceId**, **components** | surfaceId string; components an array with ≥ 1 item else INVALID_MESSAGE `updateComponents.components needs at least one component`; surface exists else SURFACE_NOT_FOUND; then every entry passes the ENTRY CHECK (§5.2) | one `send INVALID_MESSAGE` per refused entry (in entry order), then `components {surfaceId, components: [accepted entries in order]}` when at least one entry passed |
| `updateDataModel` | **surfaceId**, path, **value** | surfaceId string; surface exists else SURFACE_NOT_FOUND; `value` key present else INVALID_MESSAGE `updateDataModel.value is required (null deletes)`; path a string when present | `data {surfaceId, path: normalizePath(path), value}`. `value: null` → `data {surfaceId, path}` without `value` = remove. On the whole model (`""`) that resets it to `{}` |
| `deleteSurface` | **surfaceId** | surface exists else SURFACE_NOT_FOUND | `delete {surfaceId}` |
| `callRendererFunction` | surfaceId, **functionCallId**, **callFunction** | §6.7 | `call {functionCallId, surfaceId?, catalogId, name, args}` or `send rendererFunctionResponse{error}` |
| `agentFunctionResponse` | **functionCallId**, value, error | functionCallId non-empty string else INVALID_MESSAGE (surfaceId `""`); exactly one of `value` / `error` else INVALID_MESSAGE with `functionCallId`; `error` = `{code: string, message: string}` and no other key | `respond {functionCallId, value}` or `respond {functionCallId, error}` |

`normalizePath(p)`: absent or `"/"` → `""`, else `p` unchanged. Pointers are
RFC 6901 with the existing limits (`maxPointerBytes` 1024,
`maxPointerSegments` 64). A refused write answers VALIDATION_FAILED at the
pointer (`"/"` for the whole model), as today.

`create` replaces nothing any more: a seen id is INTEGRITY_ERROR. The `theme`
field of the `create` op is filled from `metadata.extensions.at_exponential_theme`
(§8). The host performs ops in order. `dataModel` is applied before
`components`, so the first reduce binds against it.

Example:

```json
{"version": "1.0", "createSurface": {
  "surfaceId": "profile_1",
  "catalogId": "https://ui.exponential.at/catalogs/core/v1",
  "components": [
    {"id": "root", "component": "Stack", "children": ["name"]},
    {"id": "name", "component": "Text", "text": {"@path": "/name"}}],
  "dataModel": {"name": "Ada"},
  "metadata": {"extensions": {"at_exponential_theme": "neutral"}}}}
```

### 1.3 Renderer → agent messages

Every outbound message is `{"version": "1.0", <kind>: …}`, built only by
`src/host/contract.ts` and `eui-core/src/host/contract.rs`.

**`action`**: keys in this order: `name`, `surfaceId`,
`sourceComponentId`, `timestamp`, `context`, then `userMessage` when the
Action's `event.userMessage` resolved to a string, then `metadata` when
it has extensions. `payload` is GONE. The component's own event data (a
Select's value, a Canvas press `{x, y}` …) travels as
`metadata.extensions.at_exponential_payload` (an object, omitted when
empty). It is still merged over `context` with the payload winning
(round 1 §3, unchanged). `sourceComponentId` = the authored id: the
painted id up to its first `.`, if it has one (§4.3). When the painted id
differs from the authored id, `metadata.extensions.at_exponential_instance`
carries the full painted id.

```json
{"version": "1.0", "action": {"name": "pick", "surfaceId": "s1",
  "sourceComponentId": "row_btn", "timestamp": "2026-11-07T10:00:00.000Z",
  "context": {"id": "a1", "value": "b"}, "userMessage": "Picked b",
  "metadata": {"extensions": {"at_exponential_payload": {"value": "b"},
                              "at_exponential_instance": "row_btn.a1"}}}}
```

**`callAgentFunction`**: `{surfaceId, functionCallId, callFunction:
{"@call", catalogId, args}}`. `catalogId` = the RESOLVED catalog (always
set). `args` = the resolved values (always present, `{}` when the call
had none), with the declared schema's `default`s applied when the
function is declared. `functionCallId` = `<surfaceId>:<n>`, where n is a
per-surface counter from 1 (deterministic for fixtures; unique because
surface ids are lifetime-unique).

**`rendererFunctionResponse`**: `{functionCallId, value}` or
`{functionCallId, error: {code, message}}`. A function returning nothing
answers `value: null`.

**`error`**: §7. VALIDATION_FAILED, UNALLOWED_PARENT and UNALLOWED_CHILD
carry `{code, surfaceId, path, message}` (exactly these keys). Every other
code carries `{code, message}` plus EXACTLY ONE of `surfaceId` /
`functionCallId`, plus an optional `path`. `errorMessage(code, target, message, path?)`
takes `target` = `{surfaceId}` | `{functionCallId}`. Key order: `code`,
`surfaceId` | `functionCallId`, `message`, `path`.

**Data model sync**: when a surface has `sendDataModel: true`, every
outbound message about it (`action`, `callAgentFunction`, an `error` with
its surfaceId) is handed to the transport with the metadata
`{"a2uiRendererDataModel": {"version": "1.0", "surfaces": {<id>: <data>}}}`.
The MCP carrier sends it as `metadata` beside `message` in the
`a2ui_event` arguments. The WebSocket and JSONL transports wrap the
message as `{"message": …, "metadata": …}` only when metadata is present.

### 1.4 Surface ids: lifetime uniqueness and minting

Each router keeps `seen`: every surfaceId it ever created, for the router's
lifetime. A new `HostRuntime` or a new Rust `HostRouter` = a new lifetime. A
`createSurface`, or an `applyTemplate` that would create one, naming a seen
id answers `error {code: INTEGRITY_ERROR, surfaceId, message: "surface <id>
was already created; surface ids are unique for the renderer's lifetime"}`
and changes nothing, even after `deleteSurface`.

Hosts that re-open a surface (app navigation, a screen switch, the editor
preview re-applying a template) MUST mint a fresh id:
`router.mintSurfaceId(base)` (Rust `mint_surface_id`, FFI
`mintSurfaceId`). The router keeps one counter `n` that starts at 1. The
candidate is `` `${base}#${n}` ``. While the candidate is in `seen` or in the
live surfaces, `n += 1`. The router returns the candidate and then sets
`n += 1`. Minting does NOT add to `seen`. Creating the surface does.
Fixture: `host-router.json` `mint`. The navigation sequence is: mint, then
`applyTemplate` onto the new id, then `deleteSurface` of the old one, so
the old surface paints until the new one exists.

### 1.5 Entries that check the version

Every core entry checks the version the same way (§1.1 steps 3 and 4)
through one function, `checkEnvelope(message, acceptV09)` (Rust
`check_envelope`):

- `HostRouter.route`
- the bare surface path (TS `SurfaceStore.apply` used by React
  `useSurface` and Solid `createSurface`; Rust `Surface::apply`, which
  currently ignores `version`)
- `reduceNested` for cards (§13)
- `loadPackage` / `validatePackage` for `formatVersion` (§12)

React `use-surface.ts` and Solid `create-surface.ts` stop parsing messages
themselves. They call the TS core's `SurfaceStore.apply`.

## 2. Host-internal messages

`applyTemplate {surfaceId, templateId, packageId?, data?}` and
`bindDataModel {surfaceId, path, source}` (both may also carry the internal
`hostParams`) are accepted only by `route(message, {trusted: true})`.
Only host code passes `trusted: true`: `HostRuntime.applyTemplate()`,
`HostRuntime.bind()`, the app hosts and the Engine runners. Every
transport (in-memory included) calls `route(message)`, which is
untrusted. On an untrusted route both kinds answer INVALID_MESSAGE
`<kind> is host-internal`. A package's surface still reaches
`bindDataModel` through the `bind` function (§6.3), which the runtime
routes trusted after the package's `sources` allowlist.

**applyTemplate** expands BEFORE the reducer into exactly one standard
message plus binds, each routed with `trusted: true`:

```json
{"version": "1.0", "createSurface": {"surfaceId": "<surfaceId>",
  "catalogId": "<pkg.catalogId>", "components": ["<template components, record macros filled>"],
  "dataModel": "<mergeData(template.data, data), only when defined>"}}
{"version": "1.0", "bindDataModel": {"surfaceId": "<surfaceId>", "path": "<b.path>", "source": "<fillScreenParams(b.source, params)>"}}
```

- One `bindDataModel` per template binding, in binding order.
- `template.data` and `data` must be objects when present. A non-object
  `data` answers INVALID_MESSAGE `applyTemplate.data is an object`.
  `mergeData` merges one level deep, as today.
- The surface belongs to the package from its `create` op on (its function
  policy and source allowlist apply to the template's own binds).
- The checks run in this order: `templateId` is a string, else
  INVALID_MESSAGE; the package is found, else TEMPLATE_NOT_FOUND; then the
  createSurface rules (INTEGRITY_ERROR included).

`bindDataModel` keeps its checks: SURFACE_NOT_FOUND; path + `scheme:name`
source else INVALID_MESSAGE; the package `sources` allowlist else
SOURCE_DENIED at the path; `?query=` expansion and host params as today.

## 3. Dynamic values

### 3.1 Forms

| form | shape | where |
|---|---|---|
| binding | `{"@path": "<JSON Pointer>"}`: exactly this one key, a string | any dynamic position |
| call | `{"@call": "<name>", "args"?: {…}, "catalogId"?: "<id>"}`: only these keys; `args` an object | any dynamic position |
| literal | string, number, boolean, null, array, or an object with neither `@path` nor `@call` | any dynamic position |

- A path without a leading `/` is relative to the innermost template item
  (or to the literal row scope), unchanged.
- `returnType` is gone from the wire. A call carrying it, or carrying any
  other extra key, is a malformed directive (§3.3).
- An object with plain `"path"` or `"call"` keys is a LITERAL.
- Unprefixed paths that stay unprefixed:
  - ChildList `{"componentId", "path", "key"?}`;
  - `updateDataModel.path`;
  - `bindDataModel.path`;
  - the `set` / `bind` function's `path` argument (a string argument, not a binding);
  - package binding paths.

### 3.2 Dynamic positions

A value is interpreted (bindings and calls resolved, `@@` unescaped) only in
a DYNAMIC POSITION:

- every component prop value, except DATA positions and `style`;
- `visible`;
- `accessibility.label`, `.description` and `.hidden`;
- `checks[].condition`;
- `on.<event>.event.context.<key>` and `.userMessage`;
- `on.<event>.functionCall` (a call itself);
- basic `action` (the same as `on`);
- every value under a call's `args`.

Inside a dynamic position, values are resolved at ANY depth (arrays and
object literals included). This is unchanged and is an extension of upstream.

DATA positions are never interpreted: the literal value of a prop whose
schema is a data schema (`isDataSchema`, TS `src/dynamic.ts` / Rust
`data::is_data_schema`: shape-less `object` props and Table `rows`), and
anything a binding returns. Inside such a literal `{"@path"}` is plain data
and `@@` stays `@@`. A `{"@path"}` or `{"@call"}` that IS the prop's value
still resolves (a bound Table `rows`).
`createSurface.dataModel`, `updateDataModel.value` and every data model
value are data.

`style` is the Style leaf (`style.json`), not a DynamicValue. Its condition
keys (`@media …`, `@container …`, `:hover`, `:focus-visible`, `:pressed`) are
style grammar, exempt from §3.3. Authors cannot write a binding inside
`style`. After macro expansion a style VALUE may be a `{"@path"}` /
`{"@call"}` (style.json, unchanged).

### 3.3 Literal objects, `@@`, refusals

In a dynamic position, an object is:

1. a binding if it has the key `@path`;
2. else a call if it has the key `@call`;
3. else a literal object. Each key that starts with `@@` loses ONE leading
   `@` at resolution (`"@@type"` → `"@type"`, `"@@@x"` → `"@@x"`). A key
   matching `^@([^@]|$)` (one `@`, or `@` alone) is refused.

The following are MALFORMED DIRECTIVES:

- a binding with any other key or a non-string `@path`;
- a call with a non-string `@call`, or a key other than `@call`, `args`, `catalogId`;
- `args` that is not an object;
- `catalogId` that is not a string;
- an unknown single-`@` key.

Each one refuses its whole component entry (§5.2: INVALID_MESSAGE, path to
the offending key). In a card, it refuses the card. In a package template,
it is an install issue.

### 3.4 ChildList

`children` is either an array of component ids or
`{"componentId": <id>, "path": <pointer>, "key"?: <pointer relative to the item>}`.
`key` is our declared extension (§5.3). Key rules, lifted templates and
instance suffixes are unchanged from round 2 §4.

## 4. Identifiers

### 4.1 The rule

```
^[\p{XID_Start}_][\p{XID_Continue}]*$
```

The rule is evaluated over Unicode code points with GENERATED tables. No
platform regex engine is used, because engines ship different Unicode
versions. `bun run --filter @exponential_app/eui generate` reads
`DerivedCoreProperties.txt` of **Unicode 17.0.0** (vendored,
sha256-pinned in `vendor/unicode/`) into sorted code point ranges:
`generated/xid.ts`, `generated/xid.rs` (eui-core `src/generated/xid.rs`),
`generated/Xid.generated.swift` and `generated/Xid.generated.kt`. The
function is `isIdentifier(s)` (Rust `is_identifier`). The empty string is
not an identifier. There is no normalization (NFC/NFKC): two ids are equal
iff their code points are equal.

### 4.2 Where it applies

| name | scope | violation |
|---|---|---|
| component id `id` | every component entry, every nested card node | entry refused, INVALID_MESSAGE `<path>/id`, message `id "<id>" is not an identifier (UAX #31)` |
| id references | `children[]`, `children.componentId`, `slots.<name>`, props typed `componentId` (§5.3), nested `template.component` | entry refused, INVALID_MESSAGE at the reference |
| component name `component` | entry | entry refused (a non-identifier is never in a catalog); `Surface` refused too (§5.4) |
| prop names | the entry's keys outside the envelope | entry refused |
| function names `@call` | every call in the entry (`@index` is the only `@` name) | entry refused |
| arg names | the keys of every `args` | entry refused |
| extension keys | `metadata.extensions` in all five scopes (§5.1) | the entry, or the whole message for `createSurface.metadata`, refused (INVALID_MESSAGE); in a catalog, a load error |
| catalog names | component, prop, function and arg names of the core, core-lite, extension and inline catalogs (`ExtensionDef` included) | catalog load error: `registerExtension` returns the issues and does not register; a core catalog violation fails `generate` |

Not identifiers (left free): `surfaceId`, `catalogId`, `functionCallId`,
event names, template ids, package ids, record kinds, data model keys,
`context` keys, slot names (they are keys of our `slots` prop; slot names
in the catalog are already identifiers) and style keys.

### 4.3 Derived ids stay internal

Template instance ids (`<id>.<key>`, accumulating, `~0`/`~1` escaped, round
2 §4), macro part ids (`<id>.<part>[.<i>]`) and Table part ids are INTERNAL.
They name painted nodes, a11y nodes, layout dumps and test ids, and they
never travel as a ComponentId on the wire. We keep their format unchanged
and do not make them identifiers. The reasons:

1. `.` is not in XID_Continue. So a derived id can never equal an authored
   id, and "the authored id = the prefix before the first `.`" is an exact,
   reversible split. Any identifier-shaped encoding (`_`, `__`) could
   collide with an author's id.
2. Every fixture that dumps painted ids (layout, a11y, conformance) keeps
   its ids.

On the wire they appear only as the authored id (`sourceComponentId`,
§1.3), as the full painted id inside
`metadata.extensions.at_exponential_instance`, and in error paths
(`/components/<authored id>`).

## 5. Components

### 5.1 Envelope and metadata

A component entry is `ComponentCommon` + the catalog component,
closed (`unevaluatedProperties: false`):

| key | type | rule |
|---|---|---|
| `id` | identifier | required |
| `component` | identifier, not `Surface` | required |
| `catalogId` | string | optional; overrides the surface default (§5.5) |
| `accessibility` | `{label?: DynamicString, description?: DynamicString, live?: "off"\|"polite"\|"assertive" (default off), hidden?: DynamicBoolean, selected?: DynamicBoolean, expanded?: DynamicBoolean}` (#EXP-1629: `selected` / `expanded`, fixtures/a11y-states.json) | closed object; another key = a reducer issue VALIDATION_FAILED (the key is ignored) |
| `metadata` | `{extensions?: {<identifier>: any}}` | closed; stored and exposed (`UiNode.metadata`, Rust `UiNode::metadata`), never painted |
| everything else | declared props of the resolved catalog component | §5.3 |

`metadata.extensions` exists in five scopes:

- the catalog root;
- `createSurface.metadata`;
- `ComponentCommon.metadata`;
- a catalog component definition;
- `action.metadata`.

Unknown keys are ignored without error. The prefix `a2ui_` is reserved
upstream; we know no such key and ignore it. Our keys are
`at_exponential_*`:

| key | scope | value |
|---|---|---|
| `at_exponential_theme` | createSurface | a built-in theme id or a full theme JSON (§8) |
| `at_exponential_payload` | action | the component's own event data |
| `at_exponential_instance` | action | the painted template-instance id (§4.3) |
| `at_exponential_lite_catalog_id` | catalog root | core: `https://ui.exponential.at/catalogs/core-lite/v1` |
| `at_exponential_component` | catalog component definition | `{kind, group, lite, children, slots?, slotScope?, events?, hidden?, status?}` (the core.catalog.json fields JSON Schema has no word for) |

An `updateComponents` entry replaces the whole stored component, its
`metadata` included. An entry without `metadata` clears it. This is the
existing merge-by-id rule.

### 5.2 Entry check (closes the TS/Rust divergence)

Every entry of `createSurface.components` and `updateComponents.components`
goes through `checkEntry(entry, index, kind)` (Rust `check_entry`) in the
ROUTER, before any store sees it. An entry fails if any of these holds:

- it is not an object;
- `id` is missing, not a string, or not an identifier;
- `component` is missing, not a string, not an identifier, or `Surface`;
- `catalogId` is present and not a string;
- `metadata` is malformed (not an object, a key other than `extensions`,
  `extensions` not an object, or a non-identifier extension key);
- a prop name is not an identifier;
- any id reference (§4.2) is not an identifier;
- it holds a malformed directive (§3.3) or a non-identifier function or
  arg name.

A failing entry is dropped with ONE
`error {code: INVALID_MESSAGE, surfaceId, message, path}`. The path is
`/<kind>/components/<index>` + the pointer to the first failing key in
check order (`/id`, `/component`, `/catalogId`, `/metadata…`, then props
in key order, depth-first). The rest of the message applies. The Rust
core no longer refuses a whole `updateComponents` at serde parse time
(`surface/mod.rs` `apply`). It deserializes entry by entry, with the same
rule. The reducer's `components[i] is not a component` issue disappears,
because the router now drops such entries.

Everything the catalog decides stays a REDUCER issue on a kept component:

- an unknown component;
- a prop type or enum mismatch;
- a missing required prop;
- an unknown prop;
- a missing child id;
- `X takes no children`;
- an unknown slot;
- the limits.

Reducer issues gain `code` and `path`. Issues with a code are SENT by the
runtime, once per `(surfaceId, code, path, message)` for the surface's
lifetime. The path is `/components/<authored id>` + `/<prop pointer>`:

| issue | code |
|---|---|
| `X takes no children` (and every `allowedChildren` violation) | UNALLOWED_CHILD |
| an `allowedParents` violation | UNALLOWED_PARENT |
| the component's resolved catalog is missing or unavailable (§5.5) | UNSUPPORTED_CATALOG |
| every other reducer issue | VALIDATION_FAILED |

### 5.3 Core catalog declared props

Our extensions stop being "reserved keys". Every core component
DECLARES them as ordinary props in its flat definition (§11), so
`RESERVED_KEYS` shrinks to the envelope `id, component, catalogId,
accessibility, metadata`:

| prop | declared on | schema (leaf `$defs` in `core.schema.json`) |
|---|---|---|
| `children` | `children: one\|many` components | `ChildList` = array of `ComponentId` \| `{componentId, path, key?}` |
| `slots` | components with `slots` | `Slots_<C>` = `{<slot name>: ComponentId}`; the names come from the component's `slots` (`*` = any name: Table's column keys) |
| `on` | every component with `events` | `{<event>: EuiAction}`, events = the component's `events` |
| `style` | every component | `Style` |
| `visible` | every component | `DynamicBoolean` |
| `checks` | every component whose source declares `checks` (today Input, Textarea, NumberField, Checkbox, Radio, ChoiceTiles, Select, ChipInput, Button) | array of `CheckRule` (§6.6), via the `Checkable` trait |

- `EuiAction` = `{event?: {name, context?, userMessage?}, functionCall?: {"@call", args?, catalogId?}}`,
  at least one of the two. Its `functionCall` names any function (a host
  function such as `app_toast` included), so it is our own closed object,
  not `common_types.json` `FunctionCall` (which admits only `anyFunction`) (anyOf, round 1: both = evaluate, run the call,
  then the event; §6.8 for failure). The basic catalog's `action` stays
  upstream's `oneOf` Action.
- A prop that holds a component id is typed `componentId` in
  `core.catalog.json` (new prop type, schema
  `common_types.json#/$defs/ComponentId`): `Label.for`, `Popover.anchor`,
  `Menu.anchor`. `Outline.value`, `Tree.value` and `Tree.expanded` are row
  ids, not component ids.
- `template` is no longer a wire key of flat components (it was the reduced
  form of a ChildList object). The NESTED card form keeps
  `template: {component, path, key?}` (§13).

### 5.4 The `Surface` root

`createSurface` implies the reserved container `{"component": "Surface", "child": "root"}`.
`Surface` is never a `UiNode`. The reduced tree's root stays the
component with id `root` (`ReduceOptions.rootId`). No catalog may define
`Surface` (catalog load error). An entry with `component: "Surface"` is
refused (§5.2). `Surface` counts as the parent of `root` for
`allowedParents`. `allowedParents: ["Surface"]` = the component may only
be `root`.

### 5.5 Mixed catalogs

The AVAILABLE set of a router =

1. `supportedCatalogIds`: core, core-lite, the 1.0 basic catalog, then the
   registered extension ids in order, de-duplicated;
2. the ids of inline catalogs (§10).

A component's catalog = its own `catalogId`, else the surface default
(`createSurface.catalogId`), else none. The same rule applies to a call:
its own `catalogId`, else the surface default. There is no fallback to
the capabilities.

- **Component.** No catalog, or a catalog outside the available set, →
  the `Unknown` placeholder + an UNSUPPORTED_CATALOG issue (`no catalog for
  this component` / `catalog <id> is not available`). A name is looked up
  ONLY in its resolved catalog.
  - The core's set is its components.
  - Core-lite = the `lite: true` subset.
  - An extension = its components ∪ the core's (`extends`, unchanged).
  - The basic catalog = the 18 basic components, mapped per component by
    `basic-map.json`. The mapping now runs per COMPONENT, so a basic
    component may sit in a core surface and vice versa. `buttonChild`
    consumes its child only when that child is a basic `Text`/`Icon`.
- **Call.** The call's catalog functions:
  - core = its function map;
  - core-lite = the core's;
  - an extension = the core's plus its own declared ones;
  - basic = the 14, each identical to the core function of the same name.
  
  Host functions (§6.3) are available in EVERY catalog. An unresolvable
  catalog fails the call (§6.8) and is never routed.

  The surface default reaches the evaluator in both cores. One refinement
  for a basic surface: its reduced tree is core vocabulary (`basic-map.json`
  and the macros emit `cond`, `map`, `div`, …), so a call WITHOUT its own
  `catalogId` looks its local function up in the core's table, while the
  resolved catalog on the wire (`callAgentFunction`, the routing key) stays
  basic. A call that names the basic catalog itself gets the 14.

All mixed catalogs are 1.0 catalogs (`protocolVersion: "1.0"`). There is no
mixing with 0.9.

### 5.6 `allowedParents` / `allowedChildren`

Both are generated into each component definition (§11) from
`core.catalog.json`:

- `allowedChildren: []` when `children: none` and the component has no
  `slots`. Otherwise it is omitted (any child).
- `allowedParents` comes from a new optional source field
  `parents: [names]` in `core.catalog.json`. No component sets it today,
  so it is omitted everywhere.

The reducer checks both after building a node: every child and every slot
child against the parent's `allowedChildren` (UNALLOWED_CHILD, issue on
the PARENT, path `/components/<parent>/children`), and every node against
its own `allowedParents` (UNALLOWED_PARENT, issue on the child, path
`/components/<child>`). Rendering is unchanged: the offending child still
renders, as today's `takes no children` issue does.

## 6. Functions

### 6.1 Catalog function definitions

Every catalog function is a flat definition (§11):
`{type: "object", description?, properties: {"@call": {const: <name>}, args: {…, unevaluatedProperties: false}}, required: ["@call", (+ "args" when an arg is required)], returnType, allowedCallers, requiresUserActivation}`.
There is no root `unevaluatedProperties`: upstream's `FunctionDefinition`
is itself closed over exactly these keys, and the wire `FunctionCall` adds
`catalogId` through `FunctionCommon`. The 14 basic functions keep upstream
1.0's argument schemas (required args included). A core function's args
come from `core.catalog.json` (`string`/`number`/`boolean` → the Dynamic*,
`array`/`object` → the literal or a binding or a call, `any` → `{}`; the
`path` of `set`/`bind` is a plain string) and none is required.

`returnType` takes one of `string number boolean array object
validationResult any void`. The mapping from core.catalog.json `returns`:
`string|number|boolean|array|object|any|void` keep their names; the
validators `required regex length numeric email` become `validationResult`.

| functions | allowedCallers | requiresUserActivation |
|---|---|---|
| every pure function (all except the next rows) | `rendererOrAgent` | false |
| `set`, `bind` | `rendererOnly` | false (but effectful: §6.4) |
| `openUrl` | `rendererOnly` | **true** |
| host functions (§6.3, not catalog-declared) | `rendererOnly` | false (but effectful: §6.4) |

`openUrl.url` = `oneOf [uri string, DataBinding, FunctionCall]`, as upstream.

### 6.2 `@index`

`{"@call": "@index", "args"?: {"offset"?: DynamicNumber = 0}}` → the
0-based index of the INNERMOST template item + offset:

- the List/Stack/Grid ChildList item index in data order, sections
  included;
- the Table row index.

A macro's `$each` index is expansion-time and is not `@index`. Outside any
item scope, a binding/check evaluation fails (§6.8 EXPRESSION_ERROR). In a
`callRendererFunction` it answers EXECUTION_ERROR. `formatString` accepts
`${@index()}` and `${@index(offset: <number literal>)}` as its only call
syntax. `@index` is never routed.

### 6.3 Host functions: flat names

Every namespaced host function is renamed: each `.` becomes `_`. The two
exceptions are `tools` and `upstream`, which use `__` before the tool name
(the exposes separator), so that a tool name holding `_` stays
unambiguous. `mangle(s)` = §14.3's id mangle without collision suffixes.

| 0.9 name | 1.0 name | effect | registered by |
|---|---|---|---|
| `openUrl`, `set`, `bind` | unchanged | host / write / host | the core |
| `app.navigate` | `app_navigate` | host | every app host (always allowed, checked by `navigateTarget`) |
| `app.toast` | `app_toast` | host | hosts that register it |
| `records.upsert`, `records.delete`, `records.batch` | `records_upsert`, `records_delete`, `records_batch` | write | records hosts |
| `actions.<name>` | `actions_<name>` (e.g. `actions_log`) | host | package actions (`callPrefix`) |
| `harness.toast`, `.mcp`, `.ask`, `.track`, `.records.upsert` (+ test-only `.boom`, `.nope`, `.wipe`) | `harness_toast`, `harness_mcp`, `harness_ask`, `harness_track`, `harness_records_upsert`, `harness_boom`, `harness_nope`, `harness_wipe` | host | the Exponential hosts, the conformance harness |
| `exponential.<noun>.<op>` (86 names + 20 aliases, `domain-contract` `app-actions.json`) | `exponential_<noun>_<op>`: `exponential_issues_create`, `exponential_issues_update_status`, `exponential_comments_create`, `exponential_apps_records_upsert`, `exponential_mcp_servers_create`, `exponential_sessions_send` (= the verb tool's own name), aliases `exponential_team_config_<op>` … | host | the Exponential app hosts (`harness_mcp` transport) |
| `camera.capture` | `camera_capture` | host | Engine capability |
| `clipboard.read`, `clipboard.write` | `clipboard_read`, `clipboard_write` | host | Engine |
| `files.open`, `files.save` | `files_open`, `files_save` | host | Engine |
| `http.fetch` | `http_fetch` | host | Engine |
| `location.current` | `location_current` | host | Engine |
| `notifications.post`, `notifications.cancel` | `notifications_post`, `notifications_cancel` | host | Engine |
| `photos.get`, `photos.list`, `photos.pick` | `photos_get`, `photos_list`, `photos_pick` | host | Engine |
| `print.pdf`, `print.print` | `print_pdf`, `print_print` | host | Engine |
| `share.send` | `share_send` | host | Engine |
| `tools.<id>` (script) | `tools_<mangle(id)>` | host | package `tools` |
| `tools.<id>.<tool>` (MCP) | `tools_<mangle(id)>__<mangle(tool)>` | host | package `tools` |
| `upstream.<server>.<tool>` | `upstream_<mangle(server)>__<mangle(tool)>` | host | Exponential (team MCP servers) |

- An `exponential_*` name parses ONLY through the `app-actions.json`
  `tools` table, keyed by the flat names. It never splits on `_`. The
  fixture's `prefix` becomes `exponential_`, and its function list,
  aliases, destructive and owner lists, grant cases and declared cases
  are all renamed.
- A tool or upstream call resolves at call time:
  - `<server>` (`<id>`) matches the ONE declared upstream (tool) whose
    `mangle` equals it;
  - `<tool>` matches the ONE live tool of that server whose `mangle`
    equals it;
  - none → UNKNOWN_FUNCTION;
  - two or more → INVALID_FUNCTION_CALL `ambiguous tool name`.
- Engine capability SCOPE ids (`photos.read`, `print.pdf` …) are not
  function names and keep their dots. Capability names become
  `^[a-z][a-z0-9]*$` (no `-` or `_`) in `eengine` `capabilities.json`
  `scopePattern`, so `<capability>_<tool>` splits at its first `_`.

**Reserved host prefixes**: `HOST_FUNCTION_PREFIXES` = `actions_`,
`harness_`, `exponential_`, `tools_`, `upstream_`, `camera_`,
`clipboard_`, `files_`, `http_`, `location_`, `notifications_`,
`photos_`, `print_`, `share_`, `app_`. The exact host names are
`openUrl`, `bind`, `app_navigate`, `app_toast`; `WRITE_FUNCTIONS` = `set`,
`records_upsert`, `records_delete`, `records_batch`. A catalog function
name (core or extension) that starts with a reserved prefix, or contains
`__`, is a catalog load error.

**Policy patterns** keep their grammar (an exact name, or a prefix ending
in `*`): `harness.*` → `harness_*`, `exponential.issues.*` →
`exponential_issues_*`, `tools.crm.*` → `tools_crm__*`, `*` unchanged.
Order: deny wins, then allow, then ask, then `default`. This is unchanged.

### 6.4 Effects and reactive positions

`functionEffect(name)` keeps its three classes on the flat names. REACTIVE
positions are bindings, `visible`, `accessibility.*`, checks and computed
values. In a reactive position, the following never run and are never
routed; the evaluation fails with EXPRESSION_ERROR (§6.8):

- a function with `requiresUserActivation`;
- any `write` or `host` effect function;
- an `agentOnly` function.

Packages and cards keep the static check as an install issue / card
refusal. ACTIONS (an `on.<event>` / `action` `functionCall`, a package
action's `call` step) are user-activated.

### 6.5 Routing

`evaluateCall(call, position, scope)` (TS `src/dynamic.ts`, Rust
`data.rs`). The first rule that applies decides:

1. `@call` = `@index` → local (§6.2).
2. Resolve the catalog (§5.5). Unresolvable → fail (§6.8).
3. Resolve the args (any depth, §3.2). A failing arg fails the call.
4. **Catalog function** of the resolved catalog with a local
   implementation:
   - a reactive position and the function is effectful,
     `requiresUserActivation` or `agentOnly` → fail EXPRESSION_ERROR;
   - otherwise run it (args are not schema-checked at run time: each function keeps the coercion rounds 1–2 lock; only `callRendererFunction` type-checks them, §6.9).
5. **Any other name** (a host name, a host-prefixed name, or an unknown
   name). The policy is IN FRONT: `decide(policy ∧ package policy, name)`.
   - `deny` → `error {INVALID_FUNCTION_CALL, surfaceId}`, the call fails.
   - `ask` → the consent hook (`onFunctionCall`); false/throw = deny.
   - `allow`:
     - registered locally → run (consent for tools, as today);
     - else, when the SURFACE HAS AN AGENT → `callAgentFunction` (§1.3);
     - else → `error {UNKNOWN_FUNCTION, surfaceId}`, the call fails.
   
   In a reactive position a host-prefixed name is effectful (§6.4) and
   fails before the policy. An unknown un-prefixed name in a reactive
   position routes to the agent (upstream's agent functions).

A surface HAS AN AGENT when the runtime was given an agent channel
(`HostRuntimeOptions.agent: true`: a transport whose far end reads
renderer → agent messages: the MCP carrier, WebSocket, SSE + POST) AND the
surface was created by an UNTRUSTED `createSurface`. Template, package
and card surfaces have no agent.

**Caching**. Reactive routed calls are de-duplicated per surface by the key
`catalogId + "\u0000" + name + "\u0000" + RFC 8785 canonical JSON of the
resolved args`. While a call is in flight, an identical call shares it.
A settled answer is reused for the surface's lifetime. ACTION calls are
NEVER shared or cached: each one sends a new `callAgentFunction`. Local
pure functions are pure, so caching them is unobservable. Local
effectful functions only run in actions and are never cached.

**Pending**. While a routed reactive call is pending, its value is the
previous settled value of the same position, if any, else `null`. A
check is `{"valid": true}` while pending (it does not block). A routed
action call blocks the rest of its action until it settles. The timeout
is `limits.json` `agentFunctionTimeoutMs` = 30000. A timeout is a failure
(§6.8, `functionCallId`).

**Agent errors**. An `agentFunctionResponse.error` fails the call. Its
`code` (e.g. UNKNOWN_FUNCTION) is the agent's, and the renderer reports
§6.8's code.

### 6.6 ValidationResult, CheckRule, severities

- `ValidationResult` = `{valid: boolean, code?: string, message?: string,
  severity?: "error"|"warning"|"info" (default "error"), …}`. Extra keys
  are allowed and kept.
- The validators `required regex length numeric email` return EXACTLY
  `{"valid": true}` or `{"valid": false}` (no code, message or severity).
  Their logic is unchanged:
  - `required`: valid iff `present(value)` and the value is not an empty
    array;
  - the others as round 1.
- `CheckRule` = `{condition: DataBinding | FunctionCall, message?: string}`.
  A literal condition is invalid: in the source catalog `defs.check` it
  becomes `condition: dynamic (binding or call only)` and
  `message: optional`.
- Evaluating a check gives a ValidationResult:

| condition result | ValidationResult |
|---|---|
| `true` | `{"valid": true}` |
| `false` | `{"valid": false, "message": rule.message, "severity": "error"}` (no `message` key without one) |
| an object with a boolean `valid` | that object; `message` filled from `rule.message` when the object has none; `severity` defaults to `"error"` |
| anything else | the boolean coercion (§6.7) of it, then as above |
| failed (§6.8) | `{"valid": false, "message": rule.message ?? <the failure message>, "severity": "error"}` |

Severity semantics on every renderer (React, Solid, SwiftUI, Compose, gpui):

- A field shows ONE result: the first failing result in `checks` order
  with severity `error`, else the first `warning`, else the first `info`.
- `error`: the field takes the `invalid` recipe state. A Button with a
  failing `error` check is disabled. A Form's `submit` is blocked and
  fires `invalid {errors}`.
- `warning` / `info`: advisory. The field shows the message on the line
  under the control, in its `error` part (id `<field id>.error`; on the web
  `role="status"` instead of `alert`) with the new recipe states `warning` /
  `info` (`recipes.json`; neutral theme: `$color.warning` /
  `$color.mutedForeground` text, the rest of the part unchanged). An authored
  description stays. They never disable or block anything; the control keeps
  its normal border. A submit reveals them, like every check.
- A failing result with no message at all shows `$string.checkFailed`
  (new built-in string: "Invalid value").
- `validateOn` (change | blur | submit) is unchanged.
- `invalid {errors}`, a Form's `summary` and `failingChecks` list EVERY
  failing `error`, in order. `fieldChecks(results, fallback)` (Rust
  `data::field_checks`) gives both: `errors` and the one `shown`. Locked by
  `interactions-round1.json` `checkSeverities`, `buttonChecks`.

### 6.7 Boolean coercion and presence

`coerceBoolean(v)` (TS `src/expr.ts`, Rust `expr.rs`) is the 1.0 table. It
applies at every DynamicBoolean position: boolean-typed props, `visible`,
`accessibility.hidden`, the args of `and` / `or` / `not`, and a check
condition that does not return a ValidationResult.

| resolved value | boolean |
|---|---|
| boolean | itself |
| object with a boolean `valid` (a ValidationResult) | `.valid` |
| string `"true"` / `"false"`, any letter case (ASCII case folding) | true / false |
| any other string (`""`, `"hello"`) | false |
| number `0` (and `-0`) | false |
| any other number | true |
| null / absent | false |
| any other object or array | false |

`present(v)` is the OLD `truthy`, renamed. undefined, null, false and `""`
are false; everything else, `0` included, is true. It stays for the
places that test PRESENCE:

- macro conditions over literal inputs at expansion time;
- `cond.if`, `coalesce`, `required`;
- CEL is unchanged (CEL `!` / `&&` on a non-bool stays an error).

When a macro condition (`$if`, `$any`, `$not`, `{!…}`) has a DYNAMIC
operand from a prop whose type is not `boolean`, the expander wraps the
operand as `{"@call": "cond", "args": {"if": <operand>, "then": true, "else": false}}`,
so presence semantics survive the new coercion. A boolean-typed operand
passes through. `catalog-macros.json` locks the output.

### 6.8 Failure rules

| where | effect | error sent |
|---|---|---|
| binding (a prop, `visible`, `accessibility.*`) | resolves to `null`; the component and its siblings stay mounted | EXPRESSION_ERROR: `functionCallId` when a routed call failed, else `surfaceId` (+ `path: /components/<id>/<prop>`) |
| check | `{valid: false, message, severity: "error"}` (§6.6) | the same as a binding |
| action | stops the action: no later step, no event dispatch; earlier effects are kept (a `set` that already ran stays) | routed call: EXPRESSION_ERROR + `functionCallId`; local function that threw or answered an error: EXECUTION_ERROR + `surfaceId`; policy deny: INVALID_FUNCTION_CALL + `surfaceId`; unknown without an agent: UNKNOWN_FUNCTION + `surfaceId` |
| `callRendererFunction` | `rendererFunctionResponse.error` | §6.9 |

Errors from reactive positions are de-duplicated like reducer issues
(§5.2). Action errors are sent every time. The action order of round 1
stands: resolve args and context, run `functionCall`, then dispatch the
event. A failing `functionCall` means no event.

### 6.9 `callRendererFunction`

The checks run in order. Each failure answers
`rendererFunctionResponse {functionCallId, error {code, message}}`,
except rows 1–2:

| # | check | code |
|---|---|---|
| 1 | `functionCallId` non-empty string | INVALID_MESSAGE `error` with surfaceId `""` |
| 2 | `callFunction` is a call with a string `@call` and a string `catalogId`; `args` an object when present; `surfaceId` a string when present | INVALID_MESSAGE `error` with `functionCallId` |
| 3 | `catalogId` in the available set | INVALID_FUNCTION_CALL `catalog <id> is not available` |
| 4 | `surfaceId` (when present) names a live surface | INVALID_FUNCTION_CALL `no surface <id>` |
| 5 | `@call` = `@index` | EXECUTION_ERROR `@index outside a template` |
| 6 | the function is declared in that catalog | INVALID_FUNCTION_CALL `unknown function <name>` |
| 7 | `allowedCallers` ≠ `rendererOnly` | INVALID_FUNCTION_CALL `<name> cannot be called by the agent` |
| 8 | not `requiresUserActivation` | INVALID_FUNCTION_CALL `<name> needs a user gesture` |
| 9 | args validate | INVALID_FUNCTION_CALL `args: <first issue>` |

On success the router emits op `call`. The host resolves `args` against
the surface's data at `/` (or `{}` without `surfaceId`), runs the function
and answers `{functionCallId, value}`. A throw answers EXECUTION_ERROR.
Envelopes are dispatched in arrival order. A synchronous mutation before
the call applies first, and an async run never blocks later envelopes.

## 7. Errors

| code | branch | when | old code |
|---|---|---|---|
| VALIDATION_FAILED | validation (`surfaceId`, `path` required) | reducer issues; a refused data write; an unusable theme; a bind without resolver | VALIDATION_FAILED (path now always set) |
| UNALLOWED_PARENT | validation | §5.6 | new |
| UNALLOWED_CHILD | validation | §5.6, `takes no children` | new (was an unsent issue) |
| INTEGRITY_ERROR | generic | §1.4 | new |
| INVALID_FUNCTION_CALL | generic | policy deny; consent refused; tool consent denied; §6.9; ambiguous tool name | FUNCTION_DENIED |
| EXPRESSION_ERROR | generic | §6.8 bindings/checks, routed action calls; regex refusals (§9.3) | new |
| EXECUTION_ERROR | generic | a local action function failed; `@index` in `callRendererFunction`; a renderer function threw | ACTION_FAILED |
| UNKNOWN_FUNCTION | generic | allowed, unregistered, no agent; a host that cannot run a tool kind | FUNCTION_NOT_FOUND |
| INVALID_MESSAGE | generic | §1.1, §1.2, §5.2, §6.9 | INVALID_MESSAGE |
| UNSUPPORTED_CATALOG | generic (createSurface) / validation-free issue (components) | §1.2, §5.5 | UNSUPPORTED_CATALOG |
| SURFACE_NOT_FOUND | generic | updates to a missing surface | unchanged |
| TEMPLATE_NOT_FOUND | generic | §2 | unchanged |
| SOURCE_DENIED | generic (+ `path`) | §2 | unchanged |
| RENDER_FAILED | generic (+ `path: /components/<id>`) | `onPaintError` | unchanged |

- `host.json` `clientMessages.errorCodes` = the 14 codes above, in this
  order.
- `HostErrorCode` (TS) and the Rust enum follow the same order.
- `PACKAGE_INVALID` stays a host ISSUE code. It is never sent.
- A generic error with no attributable surface (checks 1–2 of §1.1) uses
  `surfaceId: ""`.
- `ExponentialHost.paintError` keeps `RENDER_FAILED`.

## 8. Theme

`createSurface.theme` is GONE. A body key `theme` is refused (§1.1 check 8,
`createSurface.theme is not allowed`). The surface theme is
`createSurface.metadata.extensions.at_exponential_theme`: a built-in theme
id (string) or a full eUI theme JSON (`theme.schema.json`), resolved by
`resolveSurfaceTheme` exactly as `theme` was. When it is unusable, the
surface is still created with the default theme, and the renderer sends
`VALIDATION_FAILED` at path
`/createSurface/metadata/extensions/at_exponential_theme`. The React
`host-surface.tsx` and every native host read it from the `create` op's
`theme` field, which the router fills (§1.2). A package's own `theme` is
unchanged.

## 9. Basic catalog 1.0

### 9.1 Id and map

- The catalog id is `https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json`.
- `basic-map.json` `from` holds that id. It is mapped from
  `vendor/a2ui/v1_0/catalogs/basic/v1/catalog.json`.
- Both 0.9 ids (`…/v0_9/basic_catalog.json` and
  `…/v0_9/catalogs/basic/catalog.json`) are refused as
  UNSUPPORTED_CATALOG, unless upgrade09 rewrote them.
- `functions` stays the identity map for the 14.
- The 18 components are unchanged. Changes:

| basic | 1.0 change | core mapping |
|---|---|---|
| `Text` | `variant` ∈ `caption`, `body` (h1–h5 refused: a reducer issue VALIDATION_FAILED `variant: h1 is not caption\|body`, painted as body); `text` is Markdown | core `Markdown {text, profile: "inline", variant: <variant or body>}` (transform `textMarkdown` replaces `textVariant`) |
| `Icon` | `name` ∈ the 60 names \| `{"svgPath": DynamicString}` \| `{"@path"}` | name → `icons[name]` (null → `placeholderIcon`); `{svgPath}` → `placeholderIcon`; `{"@path": p}` → `{"@call": "map", "args": {"value": {"@path": p}, "cases": <the icons table, nulls as placeholderIcon>, "default": "ui-icon-placeholder"}}` |
| `Video` | + `posterUrl` (DynamicString) | `poster` ← `posterUrl` |
| `Slider` | + `steps` (integer ≥ 1) | `step` = `(max − min) / steps` when all three are literal numbers (rounded to 1e-9); otherwise `{"@call": "div", "args": {"a": {"@call": "sub", "args": {"a": <max>, "b": <min or 0>}}, "b": <steps>}}`; absent → core default |
| `TextField` | + `placeholder`; `value` REQUIRED (missing = a reducer issue VALIDATION_FAILED, painted with `""`); − `validationRegexp` (a reducer issue: unknown prop) | `placeholder` → Input/Textarea `placeholder`; transform `textField` loses its regex step |
| `weight` | per component | still dropped |

**Markdown inline profile.** Core `Markdown` gains two props:
`profile: "document" | "inline"` (default `document`) and
`variant: "body" | "caption"` (default `body`; only read when inline). In
the inline profile:

- the box has no leading or trailing block gap;
- a paragraph paints in the core `Text` `variant` type (body or caption):
  same recipe, same measure. So a plain basic Text looks the same as it
  did in 0.9;
- ATX/setext headings paint with the `Heading` levels (h5, h6 = h4);
- the rest of GFM is unchanged;
- a host's own Markdown painter is skipped.

`markdownInlineRecipes(part, {variant, level})` names the recipes;
`fixtures/markdown-inline.json` locks them.

The parser is the same everywhere. `recipes.json` gains
the Markdown when-props `profile` and `variant`.

### 9.2 Basic function changes

- The validators return ValidationResult (§6.6).
- `openUrl` has `requiresUserActivation: true` (§6.1).
- `and`, `or` and `not` take DynamicBoolean args under the coercion table
  (§6.7).

### 9.3 The regex dialect

`regex(value, pattern)` (TS `src/regex.ts`, Rust `eui-core/src/regex.rs`):
ONE hand-written parser + checker + backtracking matcher in each core. No
platform engine is used. The current Rust `simple_regex` and the TS
`RegExp` call are both removed.

Order:

1. A pattern that is not a string → EXPRESSION_ERROR
   `regex: pattern is not a string at 0`. Longer than **256** code
   points → EXPRESSION_ERROR `regex: pattern longer than 256 code points`.
2. Parse in the dialect, left to right. The first syntax error or
   disallowed construct → EXPRESSION_ERROR `regex: <reason> at <code
   point offset>` (the reasons and offsets: `fixtures/regex.json`
   `$comment`, `limits.json` `issues.regexReasons`).
3. Safety check (rules R2–R4 below) → EXPRESSION_ERROR
   `regex: rule <n>`.
4. `value`: null/absent → `""`; number/boolean → standard text; an object
   or array → `{"valid": false}`. More than **4096** code points →
   `{"valid": false}`, no error.
5. Match, with search semantics: try each start offset in order. At most
   **1,000,000** steps (one step = one attempt of one AST node at one
   position; the exact count, which the fixture locks, is in
   `fixtures/regex.json` `$comment`). Past that → EXPRESSION_ERROR
   `regex: too expensive`.
6. → `{"valid": true}` or `{"valid": false}`.

The limits live in `limits.json` `regex`: `maxPatternCodePoints 256`,
`maxValueCodePoints 4096`, `maxSteps 1000000`. The check failures are
locked by `limits.json` `issues`.

**Dialect** (the ECMAScript Pattern subset in `u` mode, no other flags):

- Atoms:
  - a literal code point;
  - `.` (any code point except U+000A, U+000D, U+2028, U+2029);
  - `[…]` / `[^…]` with single code points, ranges `a-z` (start ≤ end, no
    class escape as an endpoint) and the class escapes. `\b` inside a
    class is U+0008. `-` is literal first or last, as `\-`, or right
   after a range (`[a-c-e]`, as ECMAScript);
  - `\d \D \w \W \s \S`. `\d` = `[0-9]`, `\w` = `[A-Za-z0-9_]`, `\s` =
    U+0009–U+000D, U+0020, U+00A0, U+1680, U+2000–U+200A, U+2028, U+2029,
    U+202F, U+205F, U+3000, U+FEFF. The upper-case forms are the
    complements over U+0000–U+10FFFF;
  - `\t \n \v \f \r`, `\0` (not followed by a digit), `\xHH`, `\uHHHH`
    (a lead + trail surrogate pair of `\uHHHH` is one code point, as
    ECMAScript `u`), `\u{H…}` (≤ 10FFFF);
  - identity escapes of `^ $ \ . * + ? ( ) [ ] { } | /` (and `-` in a
    class).
- Assertions: `^`, `$` (input start and end only), `\b`, `\B` (ASCII word
  boundary), `(?=…)`, `(?!…)`. A lookahead is atomic: no backtracking
  into it once it has matched.
- Groups `(…)` and `(?:…)`, and alternation `|`.
- Quantifiers `* + ? {n} {n,} {n,m}` (n ≤ m, decimal), each with an
  optional lazy `?`. A quantifier on an assertion is a syntax error.
- Matching is case-sensitive. Greedy and lazy order follow ECMAScript
  backtracking.
- Refused:
  - backreferences `\1`–`\9`, `\k<…>`;
  - lookbehind `(?<=` and `(?<!`;
  - named groups `(?<name>`;
  - `\p{…}` and `\P{…}`;
  - `\cX`;
  - legacy octal;
  - inline flags `(?i)`;
  - any other escape;
  - an unmatched `(`, `)`, `[` or `]`, a lone `{` that does not start a
    valid quantifier, or a lone `}` (ECMAScript `u` mode refuses it too).

**Safety check** over the AST. Groups (capturing or not) are transparent
nodes, and assertions and lookaheads are zero-width. Definitions:

- `chars(n)`: the code points `n` can consume anywhere. A leaf gives its
  set. A group, quantifier, concatenation or alternation gives the union
  of its children. An assertion or lookahead gives ∅.
- `nullable(n)`: a leaf is false. An assertion or lookahead is true. A
  quantifier is `min = 0 ∨ nullable(atom)`. A group is `nullable(child)`.
  A concatenation is true when every item is nullable (so the empty one is
  true). An alternation is true when any branch is.
- `first(n)`: a leaf gives its set. An assertion or lookahead gives ∅. A
  quantifier gives `first(atom)`. A group gives `first(child)`. An
  alternation gives the union over its branches. A concatenation gives the
  union of `first(item_k)` for k from 0 up to and including the first
  non-nullable item.
- `unwrap(n)`: while `n` is a group, or a concatenation or alternation
  with exactly one child, replace `n` by that child.
- UNBOUNDED quantifier = max ∞ (`*`, `+`, `{n,}`). `?` and `{0,1}` have
  max 1.

The rules:

- **R2**: a quantifier with max > 1 whose atom contains (at any depth,
  lookaheads included) an unbounded quantifier. Rejects `(a+)+`,
  `(?:a*b){2,5}`.
- **R3**: an unbounded quantifier whose `unwrap(atom)` is an alternation
  with two branches i < j where `first(b_i) ∩ first(b_j) ≠ ∅`. Rejects
  `(a|ab)*c`.
- **R4**: a concatenation with items x_i, x_j (i < j) such that:
  - `unwrap(x_i)` and `unwrap(x_j)` are both unbounded quantifiers;
  - every x_m with i < m < j is nullable;
  - `chars(x_i) ∩ chars(x_j) ≠ ∅`.
  
  Rejects `\d+\s*\d+` and `.*.*`. Accepts `\w+\s+\w+` and
  `^(?=.*\d).{8,}$`.

The rules apply to every node (nested concatenations included). A
pattern breaking several rules reports the LOWEST rule number (R2
anywhere in the tree before R3, R3 before R4).
`regex.json` locks the upstream accepted/rejected examples plus one case
per rule, every syntax refusal and the step cap.

## 10. Capabilities, negotiation, transports

- **Renderer capabilities** `rendererCapabilities(extensionIds, {inline?})`
  (renamed from `clientCapabilities`; Rust `renderer_capabilities`, Swift
  and Kotlin the same) returns
  `{"1.0": {"supportedCatalogIds": [...], "inlineCatalogs"?: [...]}}`.
  - `supportedCatalogIds` = §5.5 item 1.
  - `inlineCatalogs` (only with `inline: true`) = the generated 1.0
    catalog definitions (§11) of the registered extension catalogs, in
    registration order. Their ids also join the available set.
  - A host that receives an agent's `acceptsInlineCatalogs: false`
    leaves `inline` off.
- **A2A**: the metadata keys are `a2uiRendererCapabilities` and
  `a2uiRendererDataModel`; the extension URI is
  `https://a2ui.org/a2a-extension/a2ui/v1.0`; the AgentCard `params` is
  the inner `1.0` object; a DataPart carries `metadata.mimeType`
  `application/a2ui+json` and `data` = an ARRAY of messages, processed
  sequentially and non-transactionally (a bad message is reported and the
  rest continue). These live in `host.json` `negotiation` as constants
  (`a2aCapabilitiesKey`, `a2aDataModelKey`, `a2aExtensionUri`,
  `mimeType`).
- **MIME**: `application/a2ui+json` only. `host.json` `transport.mcpMimeTypes`
  = `["application/a2ui+json"]`. A resource with `application/json+a2ui`
  is ignored like any other MIME type. `structuredContent.a2ui` (an array)
  stays.
- **MCP carrier**: renderer → agent messages (`action`,
  `callAgentFunction`, `rendererFunctionResponse`, `error`) go out as
  `tools/call a2ui_event {message, metadata?}`. The tool result is decoded
  like any carrier result. An `agentFunctionResponse` in it, or later on
  the stream, settles the pending call (op `respond`, matched by
  `functionCallId`). A response for an unknown or already settled id is a
  host issue INVALID_MESSAGE and is never sent.
- JSONL, SSE (`message`/`a2ui` events) and WebSocket framing are
  unchanged. Their bad lines/frames stay issues.

## 11. Catalog files

`src/schema.ts` generates, through ONE generator, files that conform to
upstream `catalog_definition.json`:

- `catalog/core.schema.json`;
- `catalog/core-lite.schema.json` (new);
- one file per extension (`extensionSchema(def)`, used for
  `inlineCatalogs` and the extension's published file).

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://ui.exponential.at/catalogs/core/v1",
  "catalogId": "https://ui.exponential.at/catalogs/core/v1",
  "protocolVersion": "1.0",
  "title": "Exponential UI core",
  "description": "…",
  "instructions": "<Markdown: the catalog rules, from core.catalog.json $comment-free `instructions`>",
  "metadata": {"extensions": {"at_exponential_lite_catalog_id": "https://ui.exponential.at/catalogs/core-lite/v1"}},
  "components": {"Button": {"type": "object", "properties": {"component": {"const": "Button"}, "label": {"$ref": "common_types.json#/$defs/DynamicString"}, "on": {"$ref": "#/$defs/On_Button"}, "style": {"$ref": "#/$defs/Style"}, "visible": {"$ref": "common_types.json#/$defs/DynamicBoolean"}}, "required": ["component"], "metadata": {"extensions": {"at_exponential_component": {"kind": "native", "group": "action", "lite": true, "children": "one"}}}}},
  "functions": {"formatDate": {"type": "object", "properties": {"@call": {"const": "formatDate"}, "args": {"type": "object", "properties": {…}, "required": ["format", "value"], "unevaluatedProperties": false}}, "required": ["@call", "args"], "returnType": "string", "allowedCallers": "rendererOrAgent", "requiresUserActivation": false}},
  "$defs": {"anyComponent": {"oneOf": [{"$ref": "#/components/Button"}], "discriminator": {"propertyName": "component"}}, "anyFunction": {"oneOf": [{"$ref": "#/functions/formatDate"}]}}
}
```

Rules:

- **Root keys**: only `$schema, $id, protocolVersion, title, description,
  catalogId, metadata, instructions, components, functions, $defs`.
  `liteCatalogId` moves to `metadata.extensions`. `$id` = `catalogId`, and
  `protocolVersion` = `"1.0"`.
- **`instructions`** comes from a new `instructions` field in
  `core.catalog.json` (Markdown). Extensions take an optional
  `instructions`.
- **Components** are FLAT. There is no `ComponentCommon` wrap and no root
  `oneOf`/`anyOf`; the only `allOf` allowed is `[{"$ref":
  "common_types.json#/$defs/Checkable"}]` for components with `checks`.
  `component.const` = the key, and the declared props follow §5.3.
  `allowedParents`/`allowedChildren` follow §5.6. `deprecated: true` plus
  `x-deprecated-reason` mark `status: deprecated` props. A component
  definition carries no `unevaluatedProperties` (as upstream's basic
  catalog): the message schema closes the entry over `ComponentCommon` +
  `anyComponent`. A source prop named like an envelope key (the hidden
  `Unknown` placeholder's `component`/`catalogId`) is left out.
- **`$ref` targets**:
  - relative `common_types.json#/$defs/{ComponentId, Child, ChildList,
    DynamicString, DynamicNumber, DynamicBoolean, DynamicStringList,
    DynamicValue, AccessibilityAttributes, CheckRule, Checkable, Action,
    DataBinding, FunctionCall}`;
  - `#/components/<X>`, `#/functions/<X>`;
  - `#/$defs/<leaf>`.
  
  Our leaves are `Style`, `Tokens`, `ChildList` (with `key`),
  `EuiAction`, `On_<Component>`, `Slots_<Component>` and the shared shapes
  (`option`, `tab`, …). Any other `$ref` fails generation. The old
  `ComponentId`/`DataBinding`/`FunctionCall`/`Visible`/`Action`/`ComponentCommon`
  copies in `$defs` are deleted.
- **Prop schemas**:
  - a `bindable` prop of type string (also `markdown`, `url`, `date`,
    `color`), number or boolean is `DynamicString`, `DynamicNumber` or
    `DynamicBoolean`; the literal's `format`/`pattern`/bounds ride along
    as siblings of the `$ref` (they only constrain a literal);
  - a bindable array of strings is `DynamicStringList`;
  - a bindable shape-less `object` or array of shape-less objects is
    `DynamicValue`;
  - any other bindable prop (an enum, an icon, a shaped object, a typed
    list) is `oneOf [<the literal schema>, DataBinding, FunctionCall]`, so
    the literal keeps its constraint;
  - `componentId` is `ComponentId`; `dynamic` (a check's `condition`) is
    `oneOf [DataBinding, FunctionCall]`;
  - `responsive` props are `oneOf [<the above>, {base, sm?, …}]`, the
    breakpoint object inline (`additionalProperties: false`).
- **Functions**: as in §6.1, in `functions.names` order.
- **core-lite** = the `lite: true` components and the core's functions and
  leaves; no `metadata`. **An extension's file** is self-contained (an
  inline catalog must be): the core's components then its own, the core's
  functions, the core's shapes plus its own; it has no `extends` key (not
  in the root whitelist).
- **Lint**: `src/schema.test.ts` checks every generated file against these
  rules with a hand-written structural checker (no ajv):
  - the root key whitelist;
  - `$ref` targets;
  - flatness;
  - `component.const`;
  - function shape;
  - UAX #31 names;
  - `Surface` absent;
  - no reserved host prefix (§6.3).
  
  Rust `eui-core` `catalog::check_catalog_definition` mirrors the checker
  for inline catalogs it receives. `checkCatalogDefinition(doc, {flat:
  false})` admits upstream's style, the props inside an `allOf` member: the
  vendored 1.0 basic catalog passes it (and fails only flatness without
  it).

`core.catalog.json` (the SOURCE) keeps its own format, plus:

- `instructions`;
- prop type `componentId`;
- optional `parents`;
- `functions.core[*].returns` may be `validationResult`;
- `defs.check` becomes `condition: {type: "dynamic", required: true}`,
  `message: optional`;
- Markdown `profile` / `variant`;
- the `cond`-wrap rule for macros (§6.7).

`core.runtime.json` slices the same data.

## 12. App packages: formatVersion 6

`host.json` `package.formatVersion` = **6**. `PACKAGE_FORMAT_VERSION = 6`.
A package names exactly 6. Version 5 is upgraded through §14 while
`ACCEPT_V09` holds, and is refused after the flag day
(`/formatVersion: format version 5 is no longer read; 6 is required`).
What changes from 5:

| part | v6 rule |
|---|---|
| `templates.<id>.components` | 1.0 component entries: identifier ids, `@path`/`@call`, flat function names, the 1.0 basic id if basic; the §5.2 entry check is an install issue at `/templates/<id>/components/<i>/…` |
| `templates.<id>.data` | an object or absent (else an install issue) |
| `templates.<id>.bindings` | unchanged (`path` is an unprefixed pointer) |
| `catalogId` | as before; a basic package names the 1.0 basic id |
| `functions` | flat names and `prefix_*` patterns (§6.3) |
| `actions` | `stepKinds` = `require`, `records_upsert`, `records_delete`, `set`, `call`; `call` values are flat names; `callPrefix` `actions_`; `batchFunction` `records_batch`; the CEL slots are unchanged |
| `screens` | `navigateFunction` `app_navigate` |
| `tools` | `idPattern` `^[a-z][a-z0-9]*(_[a-z0-9]+)*$` (no `-`, no `__`, no leading/trailing `_`); functions `tools_<id>` / `tools_<id>__<tool>`; `functionPrefix` `tools_`. The consent key format is unchanged, but it includes the id, so a re-keyed tool asks for consent again |
| `exposes` | `run.action` = a flat host function name: `actionPattern` `^[a-z][a-z0-9]*(_[a-z0-9]+)+$` whose first `_`-segment + `_` is a reserved host prefix (§6.3); `readOpWords` and `destructiveWords` read the `_`-separated words |
| `share[].template` | "calls nothing but `set` and `openUrl`" (unchanged names) |
| `tests` | expectations name flat functions |
| `records`, `queries`, `capabilities` (scope ids stay dotted), `extensions`, `theme`, `icon`, `upstreams` | unchanged |
| record macros | RecordForm/RecordTable emit `records_upsert`/`records_delete` and the 1.0 forms |
| `signature` | same algorithm, `context` and key format; a v6 file is signed over its v6 content. Every signed fixture (`host-package.json`, the RFC 8032 TEST 1 key) is RE-SIGNED |

`docs/package-format.md` is rewritten to format 6 in the same change.

## 13. `exponential_show` cards

A card surface stays the NESTED form `{root, data}` and gains a version:

```json
{"version": "1.0",
 "root": {"id": "s", "component": "Stack", "children": [
   {"id": "r", "component": "Row", "props": {"title": {"@path": "/title"}}},
   {"id": "b", "component": "Button", "props": {"label": "Ship"},
    "on": {"press": {"event": {"name": "reply", "context": {"text": "Ship"}, "userMessage": "Ship it"}}}}]},
 "data": {"title": "Tests green"}}
```

- `version` is required and must be `"1.0"`. A missing `version` is the
  0.9 form: upgraded by §14 while `ACCEPT_V09` holds, refused after
  (BAD_REQUEST naming the fix).
- `data` is an object, null or absent.
- A node is `{id, component, catalogId?, props?, children?, slots?, on?,
  style?, visible?, accessibility?, metadata?, template?}`.
- Ids are identifiers.
- Dynamic values are the 1.0 forms (§3), and function names are flat.
- `catalogId` may only be the core id. There are no extensions.
- `validateCardSurface` (apps/web `lib/sessions/plan.ts`) runs the entry
  check on every node plus `reduceNested` and requires zero issues (codes
  included).
- The reply is `on.<event>` = a 1.0 Action (`event {name: "reply",
  context, userMessage?}`). The client sends the 1.0 `action` message
  (§1.3), and the server reads `name`, `context` and `userMessage`.
- Cards have no agent channel (§6.5): unknown functions are a card
  refusal.
- The `exponential_show` tool description (`apps/web/src/lib/emcp/tools.ts`)
  teaches `"version":"1.0"` and `{"@path"}`, and its example uses `_`
  ids. It stays inside the tool's own cap; the MCP budget gate is
  `lib/emcp/context-budget.test.ts`.

## 14. Transition and flag day

### 14.1 Plan (approved 2026-11-07)

1. **Phases A–C on master, shipped by the normal client trains.** Every core entry accepts 1.0. The generated constant `ACCEPT_V09` (from `host.json` `transition.acceptV09: true`) makes each entry below run `upgrade09` on 0.9 input and continue with the result:
   - `route()`: version `"v0.9"` or absent;
   - `SurfaceStore.apply` / `Surface::apply`: likewise;
   - `reduceNested` card entries: no `version`;
   - package load: `formatVersion` 5.

   Renderers emit 1.0. The server still stores and serves 0.9 (agents and stored data are unchanged), and on intake it accepts both versions of `action`/`error` messages. No min-version bump yet.
2. **Release.** The 1.0-reading clients go live everywhere: desktop/CLI published, Android promoted, iOS approved and released manually, web deployed.
3. **Flag day D** (one held draft PR, merged only with Danny's OK):
   - The server and the `exponential_show` tool text emit 1.0.
   - `transition.acceptV09: false`: each entry refuses 0.9 with INVALID_MESSAGE (§1.1 check 3). Cards answer BAD_REQUEST, and packages give the install issue of §12.
   - The migration (`apps/web/scripts/a2ui10-migrate.ts`, logic in `lib/apps/a2ui10-migrate.ts`: one-shot, idempotent, skips 1.0 data, dry run by default) rewrites stored data with `upgrade09`. Order: backup, then staging, then prod. The list, re-checked against `packages/db-schema` (§18.8):
     - `apps.package`, `apps.draft_package`, `app_versions.package` (→ formatVersion 6);
     - `apps.granted_tools` (flat names, through `flatAction`);
     - `messages.card`: every clear card with a `surface` object, of ANY kind (`surface`, `plan`, `question`) → `version: "1.0"`. `{sealed}` and an `ask`'s `sealedCard` are skipped and counted.
   - The shepherd bumps `CLIENT_MIN_VERSION_*` (Coolify env; `selfhost/.env.example` in the PR) to the first release that reads 1.0.
   - `upgrade09` stays in the migration script. Clients keep only `upgradeCard09`, and only to DISPLAY a stored card that has no `version: "1.0"` (sealed cards the server cannot rewrite). Nothing is ever emitted in 0.9.
   - `vendor/a2ui/v0_9/` stays vendored as upgrade09's reference.

**Audited, no rewrite.** Each of these was read for a surface, a package, a function name or a dotted grant:

| column | why it stays |
|---|---|
| `conversations.digest` | copies a card's `kind`, `question`, `plan`, `title`, never `surface`; a sealed card is copied whole and stays sealed. The digest of a rewritten card is unchanged, so the trigger writes nothing |
| `apps.mcp_disabled_tools`, `apps.automation_state`, `app_hooks.name` | `exposes[].name` and automation names: `^[a-z][a-z0-9_]*$`, not function names, and not renamed by `upgrade09` |
| `app_share_links`, `app_share_submissions` | a share NAME, a token and an audit. No frozen template or query: the guest package is built from the published `apps.package` on each read |
| `app_upstream_bindings.upstream`, `mcp_grants`, `app_access` | `upstreams` is unchanged by format 6; ids and levels only |
| `app_records.data`, `app_record_events` | the app's own data; data keys are never re-keyed, and no component renders a surface out of a record |
| `sessions.guide`, `sessions.blocked`, `agents.triggers`, `issue_events.payload`, `issue_drafts`, `pins`, `notifications`, `device_commands.payload`, `widget_configs.form_config` | none holds a surface, a package or a function name (`agents.triggers` = a cron or an event name) |
| `events.payload`, `mcp_device_calls.args` | free JSON an emitter chose to publish, or a short-lived call's arguments; nothing renders them as eUI |
| `import_rows.row` (`imports`) | a Local desktop's staged `apps`, `app_versions` and `messages` rows, live for at most 24 h. Not walked: `imports.commit` must upgrade them instead (below), which also covers a Local desktop that shares after the flag day |
| `mcp_calls.tool` | the audit keeps the name as called |
| `mcp_servers.scopes` | scope ids stay dotted (§12) |
| `prompts.body`, `skills.body`, `agents.instructions`, `teams.agent_prompt`, `messages.body` | free text, never parsed. A person's text teaching `{"path": …}` stays; an agent following it gets the BAD_REQUEST that names the fix. Built-in skills reconcile on boot from the shipped set |

**How a run behaves.** `runA2ui10Migration(db, {apply})` walks each table by primary key, 200 rows a batch. A batch is read outside any transaction, then written in one with a compare-and-swap per row (`md5` of the row's migrated columns as read). A row changed since is read again, up to three rounds, then reported as `conflicted`. It is safe beside a live server and after a crash. A row with a failing column stays whole (its good columns count as `heldBack`). An upgraded value must pass what a client's load passes with the bridge off, and the caps: a package `migratedPackageIssues` (the SDK validator, `packageMaxBytes`, components per template), a card `cardV10(…, false)` + `validateCardSurface`. The report holds per-column counts, every failed row id with its reason, and the Electric total. `--apply` exits non-zero on a failed or conflicted row.

**What Electric re-sends.** `exponential.preserve_timestamps` is set in each write transaction, so `updated_at` does not move and a row's delta is the rewritten column alone, no snapshot and no shape rotation:

| column | shape | size per row |
|---|---|---|
| `apps.package`, `apps.draft_package`, `apps.granted_tools` | `apps`, every member | at most `packageMaxBytes` (128 KB) ×2, plus the names; an open app repaints from the new package |
| `messages.card` | `messages`: natives receive every visible rewritten card; web on demand, only for a chat that is open | at most 64 KB, typically under 1 KB |
| `app_versions.package` | behind the allowlist | nothing |
| `conversations.digest` | | nothing |

The dry run's `electric` block is the exact figure for an instance.

**Boot (the flag-day PR wires it, this phase does not).** An operator runs `bun scripts/a2ui10-migrate.ts --report dry.json` on the target first and reads the failures. The same module then runs in `runBootstrapPass`, on self-host and cloud alike, before the server listens, after `applyCustomSql`, under a Postgres advisory lock so that replicas take turns. A marker row (a one-row `data_migrations` entry the flag-day PR adds) skips it from then on. The marker is written when a run finishes, failures included: a failed row is reported in the log and the marker's report, does not stop the boot, and is no worse off than before (a sealed-style display for a card, the install issue for a package); the operator re-runs the script for it. Rows a still-running old replica writes after the marker are 0.9: the deploy runs the script once more when the rollout ends. `INSTALL.md` and `selfhost/` wording land in that PR.

**The flag-day PR must also** (found while auditing):

- store flat grants: drop `storedAction` from `apps.update` and from `stripUngrantableGrants`;
- keep a card's version: `validateCardSurface` returns `{root, data}` and `cardSurfaceInput` / `cardSurfaceSchema` have no `version`, so a 1.0 card stored without it would be read as 0.9 and upgraded twice;
- move the bundled format 5 packages to 6: `packages/ui/exponential-ui/templates/*.json` (`devices.json` is `include_str!`'d by the desktop), `apps/runner/*`, `apps/desktop/crates/ui/src/app_screen.test-package.json`, the skills' examples;
- re-sign `samples/apps/*.app.json`: `EXP_APP_TEMPLATES_SEED=… bun scripts/a2ui10-migrate.ts --apply --files ../../samples/apps/*.app.json` (they are `{package, author, signature}` files, which `upgradePackageBody09` leaves alone; the script reads `.package`, and without the seed it writes nothing);
- upgrade at `imports.commit`: a team import writes staged rows straight into `apps`, `app_versions` and `messages`, so it runs `upgradeStoredPackage`, `upgradeGrants` and `upgradeStoredCard` on them (they are pure and exported);
- decide the Local desktops: `crates/elocal`'s store holds `apps` and messages this migration never sees, and format 5 stops loading there too, so the desktop needs its own pass over `upgrade09` (the Rust mirror) when it opens a store;
- rewrite `docs/package-format.md` to format 6 (§12).

In the repo, drafts, samples (`samples/exponential-ui`, the site playground, `eval/fixtures`, `eval/baseline-1181`, guides, skills) and fixtures are re-keyed `user-card` → `user_card` in phase A.

### 14.2 The converter

`upgrade09` is pure and exists in two mirrors: TS
`packages/exponential-ui/src/upgrade09.ts` and Rust
`eui-core/src/upgrade09.rs` (FFI `upgrade09Json`).

```ts
upgradeMessage09(message, state): { messages: unknown[]; state: Upgrade09State }  // agent→renderer and host-internal
upgradeClientMessage09(message): unknown                                       // renderer→agent (action, error)
upgradeCard09(surface): unknown                                                 // nested {root, data}
upgradePackage09(file, verify = verifyPackageFile): Promise<{ package: unknown; status: "unsigned" | "valid" | "invalid"; author?: unknown }>  // package null when invalid
upgradePackageBody09(pkg): unknown                                              // a package OBJECT, steps 2–5 of Packages, no signature handling (the router's install path)
upgradeComponents09(components, catalogId, state, surfaceId): { components; state } // one flat list as one surface
rename09(name) / renamePattern09(pattern) / emptyUpgrade09State()
```

`Upgrade09State = {surfaces: {[surfaceId]: {[oldId]: newId}}, catalogs:
{[surfaceId]: catalogId}}`. It threads id maps across the messages of a
stream: every id seen is recorded, a kept id mapping to itself. `catalogs`
records a `createSurface`'s catalogId (after the basic rewrite), so a later
`updateComponents` of a basic surface gets the basic rules. A single
message, card or package starts from `{surfaces: {}, catalogs: {}}`. The
state passed in is never mutated. Each 0.9 message yields exactly one 1.0
message. Output key order follows the input's, with renamed keys in the
renamed key's place and added keys at the end of their object; `version`
is always the FIRST key of a message or card. A package that does not name
`formatVersion` 5 is returned unchanged by `upgradePackageBody09`.

### 14.3 Rules

**Envelope**: `version` → `"1.0"` (added when absent).

| 0.9 | 1.0 |
|---|---|
| `createSurface.theme` | removed; its value goes to `metadata.extensions.at_exponential_theme` (merged into an existing `metadata.extensions`, `theme` wins) |
| `createSurface.catalogId` = either 0.9 basic id | the 1.0 basic id |
| `updateDataModel` without `value` | `value: null` |
| client `action.payload` | removed; → `metadata.extensions.at_exponential_payload` (omitted when absent, null, `false`, `""` or `{}`); the extensions are added in the order payload, instance |
| client `action.sourceComponentId` | `mangle(prefix before the first ".")`; when that differs from the original, the original goes to `metadata.extensions.at_exponential_instance` |
| client `error.code` | FUNCTION_NOT_FOUND → UNKNOWN_FUNCTION, FUNCTION_DENIED → INVALID_FUNCTION_CALL, ACTION_FAILED → EXECUTION_ERROR; others kept |
| `applyTemplate`, `bindDataModel` | only `version` changes |

**Dynamic conversion** `dyn(v)` applies only in the 0.9 dynamic positions,
which are the §3.2 positions read with the 0.9 key names: props except
`style` and the INSIDE of a literal at an `isDataSchema` prop (a binding or
call AT such a prop, Table `rows: {"path": …}`, still converts, as §3.2's
resolver reads it), `visible`, `accessibility.label/description`,
`checks[].condition`, `on.*`/`action` event context and functionCall, and
call args. Recursively:

- An object whose ONLY key is `path`, holding a string, →
  `{"@path": path}`.
- Else an object with a string `call` → `{"@call": rename(call)}`
  followed by `"args"` when `args` is an object: the same keys (arg names,
  never escaped) with each value `dyn`'d. `returnType` and every other key
  are dropped.
- Else an object → a literal: each key starting with `@` gets one more
  `@` prepended, and each value gets `dyn`.
- An array → element-wise.
- Anything else is unchanged.

`children`, `slots`, `id`, `component` and `template` are never `dyn`'d.

**Presence wrap** `asPresence(x)`, with
`C(x) = {"@call": "cond", "args": {"if": x, "then": true, "else": false}}`:

| x after `dyn` | `visible` | `checks[].condition` | `and.values[i]`, `or.values[i]`, `not.value` |
|---|---|---|---|
| boolean literal | x | `C(x)` | x |
| other literal | `present(x)` (a boolean literal) | `C(x)` | `present(x)` |
| `{"@path"}` | `C(x)` | `C(x)` | `C(x)` |
| `{"@call": f}` with core `returns` `boolean` or `validationResult` | x | x | x |
| other `{"@call"}` | `C(x)` | `C(x)` | `C(x)` |

**Basic components** (when the surface or component catalog is the basic
catalog, before `dyn`):

- `Icon.name` `{"path": s}` → `{"svgPath": s}`.
- `Text.variant` `h1`–`h5` (N = 1–5): remove `variant`. A literal `text`
  t becomes `"#"×N + " " + t`. A dynamic `text` becomes
  `{"@call": "concat", "args": {"values": ["#"×N + " ", dyn(text)]}}`.
- `TextField.validationRegexp` P: remove it, and append to `checks`
  `{"condition": {"@call": "regex", "args": {"value": V, "pattern": P}}, "message": "Invalid value"}`.
  V = `dyn(value)` when `value` is a 0.9 binding, else
  `{"@path": <the ORIGINAL id>}` (the 0.9 transform's relative path; data
  keys are never re-keyed). The pattern is not validated (a bad one fails
  at run time, §9.3).
- `TextField` without `value` → `"value": ""` (added after a new `checks`).
- Event `context` keys are not escaped (they are data keys); their values
  are `dyn`'d. Props of a component with no catalog definition are all
  dynamic.

**Host function rename** `rename(name)`: `app.`/`records.`/`actions.`/
`harness.`/`exponential.`/`camera.`/`clipboard.`/`files.`/`http.`/
`location.`/`notifications.`/`photos.`/`print.`/`share.` names have every
`.` → `_`. `tools.<id>` → `tools_<mangle(id)>`. `tools.<id>.<rest>` →
`tools_<mangle(id)>__<mangle(rest)>` (id = up to the second `.`).
`upstream.<server>.<rest>` → `upstream_<mangle(server)>__<mangle(rest)>`
(server = up to the second `.`). Any other name is unchanged. Patterns
(`functions` lists, policies) apply the same function to the part before
`*`, keeping the `*`. A part ending in `.` renames without that `.` and
then takes the separator back: `harness.*` → `harness_*`, `tools.*` →
`tools_*`, `exponential.issues.*` → `exponential_issues_*`, and
`tools.x.*` / `upstream.x.*` → `tools_x__*` / `upstream_x__*`.

**Id re-key**. `mangle(s)`:

1. Every code point not in XID_Continue becomes `_`.
2. The empty result becomes `_`.
3. If the first code point is neither XID_Start nor `_`, prepend `_`.

Examples: `user-card` → `user_card`, `a.b` → `a_b`, `1st` → `_1st`,
`ab c` → `ab_c`.

Per surface (a message stream's surfaceId, a card, one package template):

1. Collect the DEFINED ids in definition order. That is entry order for
   flat lists, and pre-order for cards (node, then slots in key order,
   then children, as `preorder`).
2. KEEP an id when it is an identifier and is not already the target of
   a different original id in `state`.
3. Rewrite every other id, in order, to the first free candidate of
   `mangle(id)`, `mangle(id)_2`, `mangle(id)_3`, …. A candidate is free
   when no kept id, no earlier target and no `state` target uses it.
4. Map REFERENCES through the same map. A reference to an id with no
   entry yet is assigned then, by the same rule, in order of appearance.

The fields rewritten:

- `id`;
- `children[]`, `children.componentId`;
- `slots.*`;
- basic `child`, `Tabs.tabs[].child`, `Modal.trigger`, `Modal.content`;
- core `componentId` props (`Label.for`, `Popover.anchor`, `Menu.anchor`);
- nested `template.component`.

Data keys, paths, event names and `context` are never re-keyed.

**Cards**: every node is upgraded as a core component (props in `props`),
then the card gets `"version": "1.0"` as its first key.

**Packages** (`formatVersion` 5 → 6):

1. `verify` (the existing `verifyPackageFile`) runs on the ORIGINAL
   file. `invalid` → no output, `status: "invalid"`.
2. Each template is upgraded as one surface with a fresh state.
3. `functions[]`, the `actions.*.steps[]` keys `records.upsert`/
   `records.delete` → `records_upsert`/`records_delete`, the `call`
   values, `exposes[].run.action`, `automations[].run.action`, and every string in `tests[*].expect`
   shaped like a dotted function name (`^[A-Za-z_][A-Za-z0-9_-]*(\.[A-Za-z0-9_-]+)+$`)
   all go through `rename`.
4. `tools` keys → `mangle` with the `_2` collision rule (ids are tested
   against the v6 `idPattern` after mangling; a leading or trailing `_` is
   stripped, and a `__` collapses to `_` before the collision suffix).
   An id already matching the v6 `idPattern` is kept; the others, in key
   order, take the first free candidate. Their references move with them:
   `tools.<id>…` names everywhere in the package use the re-keyed id.
5. `formatVersion` → 6. `signature` is DROPPED. `author` is kept as an
   unverified claim. `status` = the original's (`valid` keeps its
   `author`).

A host may treat a `valid` upgraded package as signed by that author,
because `upgradePackage09` is deterministic over verified bytes. The
migration script re-signs only packages whose key it holds (§17).

**Fixture**: `fixtures/upgrade09.json` = `{cases: [{name, kind:
"message"|"stream"|"client"|"card"|"package", input | inputs, expected,
expectedState?}]}`. It covers:

- every row above;
- every rename prefix;
- `tools`/`upstream` splits;
- each re-key collision path (a valid id kept, the `_2` suffix, the
  cross-message state collision);
- the presence wraps;
- Text h1–h5 literal and dynamic;
- `validationRegexp` with a bound and an unbound value;
- Icon `{path}`;
- theme → metadata;
- `value: null`;
- payload → extensions;
- a signed v5 package (valid, invalid, unsigned).

The TS and Rust cores replay it. Swift and Kotlin replay two smoke cases
through `upgrade09Json` (FFI `FacadeTest`).

## 15. Fixture plan

R = Rust `eui-core` tests, T = TS `bun test`, S = Swift (FFI + painters),
K = Kotlin (FFI + painters), C = every conformance runner (React, Solid,
element, SwiftUI, Compose, gpui) through `conformance/manifest.json`
(`hostContractVersion` 4).

| file | change | T | R | S | K | C |
|---|---|---|---|---|---|---|
| **new** `a2ui10-messages.json` | the 1.0 wire: every message kind, the check order (§1.1), the entry check (§5.2), INTEGRITY_ERROR + `mint`, trusted vs untrusted, `callRendererFunction` table (§6.9), `agentFunctionResponse`, error shapes, `ids` (UAX #31 incl. non-ASCII, the generated tables' edges), `cards`, bridged 0.9 with `acceptV09` true AND false | ✓ | ✓ | ✓ | ✓ | suite `a2ui10` |
| **new** `a2ui10-functions.json` | routing (§6.5) with a fake agent channel and policy, caching/no-caching, pending, failure rules (§6.8), coercion table (§6.7), CheckRule → ValidationResult, severities, `@index`, `formatString ${@index(...)}` | ✓ | ✓ | | | |
| **new** `regex.json` | §9.3 dialect, R2–R4, caps | ✓ | ✓ | | | |
| **new** `upgrade09.json` | §14 | ✓ | ✓ | smoke | smoke | |
| **new** `a2ui10-conformance.json` | GENERATED from `vendor/a2ui/v1_0/test/cases/*.json`: one entry per upstream case `{file, index, description, schema, data, upstreamValid, expect, why?}`; `expect` = our router outcome for agent/renderer messages (`ok` or the first error code), `catalog-ok`/`catalog-error` for catalog cases, `skip` + `why` where upstream tests JSON-Schema-only details our router does not check (the 4 MiB, closed-object keys inside catalog props) or where §16 diverges | ✓ | ✓ | | | |
| `host-router.json` | `"1.0"`, ops `call`/`respond`, the `create` op without replace, `theme` → metadata, `trusted` flag per case, codes renamed, `mint`, applyTemplate → createSurface{components, dataModel}, flat names in `screens`/`navigate`/`crossApp`/`acme.*` packages, re-keyed ids, formatVersion 6 | ✓ | ✓ | ✓ | ✓ | `host-router` |
| `host-transport.json` | `"1.0"`; one MIME type; `a2ui_event` carries `callAgentFunction` and metadata | ✓ | ✓ | ✓ | ✓ | `host-transport` |
| `host-policy.json` | flat names and patterns; decisions → INVALID_FUNCTION_CALL / UNKNOWN_FUNCTION | ✓ | ✓ | ✓ | ✓ | `host-policy` |
| `host-package.json`, `package-share.json`, `package-tests.json`, `actions.json`, `record-macros.json`, `read-only.json`, `round4-contract.json` | format 6, flat names, step kinds, re-signed, re-keyed | ✓ | ✓ | | | `read-only` |
| `catalog-basic-map.json` | 1.0 basic id; Text → Markdown inline; Icon `svgPath`/`@path`; Video `posterUrl`; Slider `steps`; TextField `placeholder`/`value`; h1 refusal | ✓ | ✓ | ✓ | ✓ | `basic-map` |
| `catalog-macros.json`, `bind-time.json`, `kitchen-sink.json`, `kitchen-sink.expanded.json`, `template-items.json`, `tree.json`, `catalog-components.json`, `catalog-extension.json`, `specimens.json`, `primitive-specimens.json`, `demo-surface.json`, `dashboard.json`, `interactions-round1.json`, `expressions.json` (call form), `records-query.json`, `layout-geometry*.json`, `text-direction.json`, `conformance-*.json`, `canvas*.json`, `motion-*.json` (authored ids only) | `@path`/`@call`, the `cond` wraps, re-keyed AUTHORED ids (derived `.` ids unchanged), Markdown `profile`/`variant` | ✓ | ✓ | ✓ | ✓ | their suites |
| `prompt-budget.json` | the prompt teaches 1.0; still under 10,000 tokens | ✓ | | | | |
| `packages/domain-contract/fixtures/app-actions.json`, `via-app.json` | `exponential_` prefix, flat names | ✓ (web) | ✓ (domain) | ✓ (iOS) | ✓ (Android) | |
| `packages/emcp/fixtures/exposes.json`, `automations.json` | flat `run.action`/function names | ✓ | ✓ | | | |
| `packages/eengine/fixtures/capabilities.json` | capability name pattern, flat tool function names | ✓ | ✓ | ✓ | ✓ | |
| `eval/fixtures/*`, `eval/baseline-1181/*`, `samples/exponential-ui/shared/surface.jsonl` | `upgrade09` output | ✓ | | | | |

The vendor tree is `vendor/a2ui/v1_0/`:

- `json/*.json` (the 12 schemas);
- `catalogs/basic/v1/catalog.json`;
- `test/cases/*.json` + `test/testing_catalog.json`;
- `docs/a2ui_protocol.md`.

Each file is sha256-pinned in `vendor/a2ui/README.md` and
`src/vendor.test.ts`. `vendor/unicode/DerivedCoreProperties.txt`
(17.0.0) is pinned the same way.

## 16. Divergences from upstream 1.0 (ours win)

| upstream | ours |
|---|---|
| A malformed component makes the whole message invalid | the entry is dropped (INVALID_MESSAGE + path) and the rest applies (§5.2) |
| `Action` is `oneOf` event / functionCall | core `on` actions may carry both (§5.3); basic `action` keeps `oneOf` |
| `DynamicValue` literal objects are not resolved inside | resolved at any depth except data positions (§3.2) |
| `ChildList` object is closed | + `key` |
| error `path` like `/components/0/text` | `/components/<authored id>/…` for reducer issues; `/<kind>/components/<index>/…` for entry refusals |
| `surfaceId` reuse after `deleteSurface` (the prose is ambiguous) | always INTEGRITY_ERROR (§1.4) |
| an agent function fallback for every unimplemented call | the policy is in front; package and card surfaces have no agent |
| `required` "not empty" | `false` and `""` invalid, `0` valid, `[]` invalid (unchanged) |
| severity unspecified for blocking | only `error` blocks (§6.6) |
| regex: no step cap | + 1,000,000-step cap (§9.3) |
| no `theme` | `metadata.extensions.at_exponential_theme` |
| the renderer emits UNKNOWN_FUNCTION only via the agent | the renderer also emits it (§6.5) |

## 17. Checklists

- [ ] TS core: §1–§11, `upgrade09.ts`, `regex.ts`, `xid` tables, `schema.ts`, `prompt.ts`, `web/data.ts` uses the core function table (no second `required/and/or/not`).
- [ ] Rust core: the same, `check_entry` per entry, `Surface::apply` version check, `upgrade09.rs`, `regex.rs`, FFI (`route_json(trusted)`, `mintSurfaceId`, `upgrade09Json`, renderer capabilities, `call`/`respond` ops, FFI comments "1.0").
- [ ] Hosts: React/Solid `useSurface`/`createSurface` through the core; `HostRuntime` agent channel, pending calls, `callAgentFunction`/`rendererFunctionResponse`, de-dup of sent issues; Swift/Compose/gpui hosts likewise; app hosts (web preview, share page, iOS, Android, desktop, Engine runners) mint ids and route trusted.
- [ ] Painters ×5: `accessibility.live` (aria-live / SwiftUI announcement on change / Compose `liveRegion` / AccessKit `Live`), `accessibility.hidden` (hidden from AT, still painted, subtree too), check severities (§6.6), Markdown inline profile.
- [ ] `a11y.json`: `live`, `hidden` rows; `Text.live` stays, and `accessibility.live` wins when both are set.
- [ ] `macros.json`: emits `@call`/`@path` and the `cond` wraps.
- [ ] `limits.json`: `regex`, `agentFunctionTimeoutMs`.
- [ ] `host.json` v4: `a2uiVersion`, `messages {a2ui (6), host (2)}`, `ops.kinds` + `call`, `respond`, `clientMessages {kinds (4), errorCodes (14)}`, `functions` text, `transport.mcpMimeTypes`, `negotiation` constants, `package` (formatVersion 6, stepKinds, callPrefix, batchFunction, navigateFunction, tools, exposes, records.functions), `transition.acceptV09`.
- [ ] Prompt, `skills/exponential-genui/SKILL.md` (regenerated), README, site docs, `exponential_show` description, `docs/package-format.md`.
- [ ] Migration script (§14.1) and the server's 0.9 intake conversion.
- [ ] `CLIENT_MIN_VERSION_*` bump and changelog entries (web + desktop mirror).

## 18. Decisions on the former open points (2026-11-07)

1. **Re-signing.** The migration re-signs only files signed with a key we hold (fixtures, Exponential-authored samples). A stored package carries no file signature and is rewritten as is. When the original signature of a signed v5 file verifies, its upgrade keeps `status: valid` and `author`.
2. **Sealed cards.** Danny confirmed: clients keep `upgradeCard09`, and only to DISPLAY a stored card that has no `version: "1.0"` (a sealed card the server cannot rewrite). Nothing is ever emitted in 0.9; every other entry refuses 0.9 after the flag day.
3. **Issues sent to the agent** are capped at 32 per message. Past the cap, one final `VALIDATION_FAILED` says how many were dropped. The de-dup per surface lifetime stays.
4. **`tools` / `upstream` flat names** stay mangled (§6.3). Two tools whose names mangle alike are a package load error, so the ambiguity never reaches call time.
5. **Pure functions** are `rendererOrAgent` (our choice).
6. **The Markdown inline profile look** is drafted on web with pictures before the painters port it (phase B).
7. **The agent function timeout** is 30 s.
8. **The migration's table list** was re-checked against `packages/db-schema` in phase C: §14.1.
9. **XID tables** are pinned to Unicode 17.0.0. Fixture ids stay inside identifiers that every engine agrees on.

## 19. Embed: a nested surface in place (#EDE-65)

A native (group layout, not lite, no children): a genUI card inside a genUI
surface, drawn in place. The chat's three uses are its specimen
(`exponential-ui-embed`): an agent's result card with its own data, a
question card whose buttons reply, and a bad surface's error box.

**Props.** `surface` (object, required, bindable) = `{root, data?,
version?}`: `root` a nested component, `data` the card's OWN model.
**Events.** `action`. **Parts.** `root` (no recipe props). **a11y.** No role
of its own: the nested surface's nodes join the host's order in place, Tab
moves through its controls; the error box is an Alert.

**Parse** (`fixtures/embed.json` `parse`, the TS reference `src/embed.ts`
`embedCard`). The resolved `surface` value is a card when it is an object
whose `root` is sound (a non-empty string `id` and `component`; `props`,
`on`, `style` objects when present; `children` an array of sound nodes;
`slots` an object of sound nodes, recursively), `data` is absent, null or an
object (absent or null = `{}`) and `version` is absent or `"1.0"`. An
unknown component inside a sound root is still a card (the nested surface
paints its Unknown placeholder). Anything else paints `errorRoot` instead:
an error Alert titled `$string.embedInvalid` (`This card could not be
shown`); the host around it keeps painting.

**Sandbox** (`render`, `events`). `surface` is a DATA prop: a binding
`{"@path"}` AT ITS TOP resolves against the host (the Embed's scope),
nothing inside it ever does. Inner bindings read the card's data, never the
host's; an inner `set` writes the card's model only; the nested surface id
is `<host surface id>/<embed component id>` (`embedSurfaceId`), so inner ids
never meet the host's (a host `t` and a card `t` both paint). An inner host
function is DENIED: never forwarded, and its action stops (the `event` after
it does not fire). An Embed may nest another Embed.

**Action.** Every inner `event` action reaches the host as the Embed's
`action`, with the payload `{name: <inner event name>, context: <inner
event context>}` (`embedPayload`; inner bindings in the context resolved
against the card's data) merged over the author's `on.action` context like
any component payload, so the payload's `name` and `context` win over an
author's keys of the same name. No `on.action` = the host hears nothing.

- [x] Catalog, schema, strings (`embedInvalid`), a11y, recipes, regenerated tables, prompt budget (9,748 → 9,812 tokens).
- [x] `fixtures/embed.json` (parse, render, events) + the TS reference `src/embed.ts` and `src/embed.test.ts`.
- [x] Specimen `exponential-ui-embed` (example, result card · own data, question card · reply, bad surface · error box).
- [ ] Rust core `eui_core::embed` + `tests/embed_fixture.rs`.
- [ ] Painters ×5: React and Solid (`embed.test.tsx`), gpui (`tests/embed.rs`), SwiftUI `EmbedTests`, Compose `EmbedTest`.
- [x] Styleguide entry `embed` (web + the desktop twin), `views.json` `exponential-ui-embed`.
- [ ] Shots ×4 (`shots/exponential-ui-embed/`).
