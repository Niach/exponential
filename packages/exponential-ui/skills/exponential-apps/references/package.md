# The package and its versions

> Generated from packages/exponential-ui/docs/package-format.md (Versions, The package, The package file) and docs/spec/apps.md §2.3 (genUI 0.20.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

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

### 2.3 Versions and channels

Today:

- ✅ Every `upsert` writes `app_versions.number` and becomes the head (the draft).
- ✅ Restore makes a new head.
- ✅ The editor session id is recorded on the version.
- ✅ **Draft vs published** (V1, #VAPP-210/#VAPP-211, decided 2026-11-01; fixture `apps.json` `publishCases` ×4):
  - `apps.package` is the PUBLISHED version's package (`published_version`, 0 = never published); `apps.draft_package` is the head's while it is ahead.
  - Every version write (`apps upsert`, Restore) makes a draft. Members, the exposed MCP tools (the broker), record policies and automations run the published package.
  - The editor's preview shows the head while the viewer's own editor session is live (or nothing is published), else the published version.
  - "Publish" (the editor header; phones: the … menu; any Versions row; the viewing banner) or `exponential_apps {op: "publish", version?}` moves the pointer. An older version = a rollback; past a version whose sweep ran, the Publish first warns that rollback doesn't reverse migrated data (#VAPP-256, apps.json `rollbackCases`, no data moves). An imported app, or one created with a package, publishes v1 at once.
  - Grants follow the published package: an owner's publish keeps the still-declared grants, anyone else's clears them.
  - Records validate against the published kinds plus a draft's new kinds (so an editor can seed them before publishing).

Proposed:

- **`version` (SemVer)** is the author's label. **`number`** is the truth. A major bump with no `migrations` for a changed kind is refused (§3.6). ✅ R8 (#VAPP-233): refused at upsert, naming the kind and the change (`migrationRefusal`, `record-migrations.json`).
