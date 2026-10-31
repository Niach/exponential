# The package format (eApps)

VAPP-161. One format for an Exponential app and a spezl block: a declarative
Exponential UI package (VAPP-82: data, never code) plus the file it travels
in. The format is transport-neutral: it names what a host provides (a record
store, tool runners, extensions), never how (Exponential serves records from
its synced store, spezl from a p2p store over eNet). `catalog/host.json`
`package` is the machine-readable contract; this page is the prose spec other
docs link to (spezl `docs/spec/blocks.md`). The TS reference is `src/host/`
(`package.ts`, `records.ts`, `record-store.ts`, `tools.ts`, `exposes.ts`, `signature.ts`);
the Rust core (`apps/desktop/crates/eui-core/src/host/`) mirrors every
pure rule. Fixtures: `fixtures/host-router.json` (package validation, replayed
by every renderer through the core) and `fixtures/host-package.json` (records
queries and values, tool consent, extension compatibility, canonical JSON,
signatures).

## Versions

A package MAY carry `formatVersion` (a positive integer; absent = 1). This
document is format version **2** (`catalog/host.json` `package.formatVersion`).
Version 2 only ADDS optional keys: every version-1 package is a valid
version-2 package, unchanged. A host refuses a package whose `formatVersion`
is newer than its own (`/formatVersion: format version 3 is newer than this
host (2)`); it never guesses at keys it does not know. The catalog a package
renders against keeps its own version in its id (`catalogId`,
`…/catalogs/core/v1`) and is negotiated as before (`supportedCatalogIds`).

## The package

```jsonc
{
  "formatVersion": 2,                  // optional, default 1
  "id": "acme.hours", "name": "Working hours", "version": "1.2.0",
  "catalogId": "https://ui.exponential.at/catalogs/core/v1",
  "templates": { "<id>": { "components": [/* flat, one root */], "data": {}, "bindings": [{ "path": "/entries", "source": "records:entry?sort=date" }] } },
  "functions": ["records.*", "tools.weather.*"],   // allowlist: listed = allow, else deny
  "sources": ["records:*"],                         // allowlist for bindings, unset = any
  "records": { "entry": { "title": "Entry", "schema": { "type": "object", "properties": { "date": { "type": "string", "format": "date" } } } } },
  "tools": { "weather": { "kind": "mcp", "title": "Weather", "url": "https://mcp.example.com/weather" } },
  "upstreams": ["m5"], "exposes": [/* MCP tools a host's broker runs: "Exposes" below */],
  "extensions": [{ "id": "https://acme.example/catalogs/charts/v1", "version": "1.4.0", "components": ["Gantt"] }],
  "theme": "exponential", "icon": { "glyph": "clock" }, "description": "…"
}
```

Required: `id`, `name`, `version`, `catalogId`, `templates`. Everything else
is optional and keeps its version-1 meaning (`functions`, `sources`, `theme`,
`icon`, `description`, `records` kinds: `catalog/host.json` `package`).

## Records

An app's own data. `records: {<kind>: {schema, title?, description?}}`
declares every kind (VAPP-122: the kind pattern, the JSON Schema subset and
the reserved row keys are in `catalog/host.json` `package.records`). The
host provides the store; the format fixes what a surface sees and calls.

**Read** = the binding source `records:<kind>[?query]`. The value is
`{rows: [{…data, id, createdAt, updatedAt, createdBy, author}], count}`:
`count` = the matching rows before `limit`. The neutral query:

| key | meaning |
| --- | --- |
| `mine=1` | only the viewer's rows (`createdBy` = the viewer) |
| `sort=<field\|createdAt\|updatedAt>` | default `createdAt` |
| `order=asc\|desc` | default `desc` (newest first) |
| `limit=<n>` | default 100, capped at 500 |
| `<field>=v` | a data field equals `v` (as text) |
| `<field>.gt\|gte\|lt\|lte=v` | numbers when both parse, else text (code points) |

Ties sort by `id`. The keys `kind`, `app`, `user`, `sum`, `by`, `month` are
RESERVED for host extensions (Exponential implements them: `sum`, the month
view, another app's rows; `packages/domain-contract/fixtures/app-records.json`);
a host that does not implement one ignores it, never filters a field by it.
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
are the reference (resolver + both functions), `recordsValue` the derivation
`fixtures/host-package.json` `recordsValue` locks.

Exponential: `records:<kind>?q` = its `exp:records?kind=<kind>&q` (the name
wins over a `kind` param) and `records.*` = `harness.records.*`; both spellings
stay valid on all four clients.

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
"upstreams": ["m5"],
"exposes": [
  { "name": "list_entries", "title": "List entries", "description": "The time entries of a day.",
    "input": { "type": "object", "properties": { "day": { "type": "string" } } },
    "access": "read",
    "run": { "records": { "op": "list", "kind": "entry", "where": { "date": "${input.day}" } } } },
  { "name": "log_comment", "title": "Log a comment", "description": "Comment the logged hours on an issue.",
    "input": { "type": "object", "properties": { "issue": { "type": "string" }, "hours": { "type": "number" } }, "required": ["issue", "hours"] },
    "access": "write",
    "run": { "action": "exponential.comments.create", "args": { "issueId": "${input.issue}", "body": "Logged ${input.hours} h" } } },
  { "name": "find_customer", "title": "Find a customer", "description": "Search m5 customers.",
    "input": { "type": "object", "properties": { "q": { "type": "string" } } },
    "access": "read",
    "run": { "upstream": "m5", "tool": "search_customers", "args": { "query": "${input.q}" } } }
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
