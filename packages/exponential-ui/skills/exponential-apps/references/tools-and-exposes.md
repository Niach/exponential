# Tools, exposes and extensions

> Generated from packages/exponential-ui/docs/package-format.md (Tools, Exposes, Extensions) (eUI 0.8.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

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
