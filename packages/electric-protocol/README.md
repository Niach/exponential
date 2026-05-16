# @exp/electric-protocol

The single source-of-truth for how every Exponential client (web, iOS,
Android) talks to the Electric SQL shape proxy at `/api/shapes/*`.

This package ships **no code** — it is documentation plus JSON fixtures
that all three clients use in their tests so the protocol contract can't
drift across platforms.

If you're touching `apps/web/src/lib/shape-route.ts`,
`apps/web/src/lib/electric-proxy.ts`, the iOS `ShapeClient.swift`, the
Android `ShapeClient.kt`, or the Bun / Caddy timeout knobs — read this
first.

---

## Why long-polling

The web client uses `@electric-sql/client` (via
`@tanstack/electric-db-collection`). Mobile clients implement the
protocol by hand. All three converge on the same wire format and the
same long-polling loop.

"No polling" is achieved by Electric's `live=true` parameter: after the
initial snapshot, the client requests `?live=true`, and the **server
holds the HTTP response open for up to ~60 seconds**, either streaming
new rows the moment they arrive or returning an `up-to-date` control
message just before timing out. The client immediately reconnects with
the new `electric-handle` / `electric-offset` from the previous response
headers. There is no setInterval anywhere.

Misconfigured middleboxes silently turn long-polls into short-polls — if
Bun's `idleTimeout` is below ~60s, or Caddy / Traefik close the upstream
connection early, the client appears to work but is now hitting the
backend every <1s instead of every 60s. The canary test in
`packages/electric-protocol/fixtures/long-poll-canary.md` exists to
catch this.

---

## Wire format

### Initial snapshot

```
GET /api/shapes/{table}?offset=-1
Authorization: Bearer <session-token>   (mobile)
Cookie: better-auth.session_token=...   (web)
```

Response:

```
200 OK
electric-handle: <opaque-string>
electric-offset: <opaque-string>
Content-Type: application/json

[
  { "headers": { "operation": "insert" }, "key": "row-id-1", "value": { … } },
  { "headers": { "operation": "insert" }, "key": "row-id-2", "value": { … } },
  { "headers": { "control":   "up-to-date" } }
]
```

### Live loop (after snapshot)

```
GET /api/shapes/{table}?offset=<previous-electric-offset>&handle=<previous-electric-handle>&live=true
```

Server holds the connection open up to ~60s. Either:

- New rows arrive → server returns 200 with the delta + `up-to-date`
  control message, plus new `electric-handle` / `electric-offset` headers.
- No new rows → server returns 200 with `[{ "headers": { "control": "up-to-date" } }]`
  just before the long-poll timeout.

In both cases, the client persists the new handle/offset and immediately
opens the next `live=true` request.

### Control messages

| `headers.control` value | Client response |
| --- | --- |
| `up-to-date` | Persist new handle/offset, reopen live loop. |
| `must-refetch` | Discard local data for this shape, reset to `offset=-1`. |

### Message operations

| `headers.operation` value | Meaning |
| --- | --- |
| `insert` | Upsert (treat as insert-or-update). |
| `update` | Upsert. |
| `delete` | Delete by `key`; `value` may be absent or partial. |

### Casing

Electric delivers Postgres column names verbatim (snake_case:
`workspace_id`, `created_at`). Some server-side rewriting can yield
camelCase. **Clients must accept both** — the Android `ShapeClient`
uses `@JsonNames` on entity fields; the iOS client maps in the entity
initializer. The Drizzle/tRPC mutation path stays camelCase. See
`fixtures/` for both variants.

---

## Required client behavior

1. **Persistent cursor.** Persist `electric-handle` + `electric-offset`
   per shape, keyed by `(instanceUrl, table)`. After app restart, resume
   from the persisted cursor — never re-fetch from `offset=-1` unless
   the server tells you to via `must-refetch`.

2. **One transaction per poll.** Apply all messages from one response
   inside a single DB transaction. Don't write per row — that's the
   write-contention bug that disabled iOS's sync.

3. **Backoff on transport error.** 500ms base, exponential, cap at 30s.
   Reset on the first successful response. Do **not** back off on a
   normal long-poll close — that's the happy path.

4. **Background lifecycle.** On app background, cancel the running poll
   `Task` / coroutine. On foreground, resume from the persisted cursor.
   Mobile OSes will tear down HTTP connections in background regardless.

5. **Single auth header.** `Authorization: Bearer <token>` (mobile) or
   the session cookie (web). The shape proxy accepts either via
   `better-auth`'s `bearer()` plugin (registered in
   `apps/web/src/lib/auth.ts`).

6. **Timeout headroom.** Client request timeout should be **longer**
   than the server-side long-poll window (~60s). Android uses
   `LIVE_TIMEOUT_MS + 30_000L` = 90s. iOS uses 90s. Web's
   `@electric-sql/client` manages this internally.

---

## Infra knobs that must stay long-poll-friendly

| Layer | File | Value |
| --- | --- | --- |
| Bun | `apps/web/src/server-bun.ts` | `idleTimeout: 255` (max) |
| Caddy | `Caddyfile` | `transport http { read_timeout 5m; write_timeout 5m; keepalive 5m }`, `flush_interval -1` |
| Electric upstream | `docker-compose.yaml` `electric` service | defaults are fine |
| TanStack Start proxy | `apps/web/src/lib/electric-proxy.ts` | forwards `request.signal` (client cancel propagates); buffers body for HTTP/1.1 framing |

If you tune any of these down, the canary test will fail.

---

## Reference implementations

- Web: `apps/web/src/lib/collections.ts` (via `@electric-sql/client` / `@tanstack/electric-db-collection`)
- iOS: `apps/ios/Exponential/Data/Electric/ShapeClient.swift`
- Android: `apps/android/app/src/main/java/com/exponential/app/data/electric/ShapeClient.kt`

## Fixtures

See `fixtures/`:

- `initial-snapshot.json` — typical first response with `offset=-1`.
- `live-update.json` — a delta arriving over a live=true connection.
- `up-to-date.json` — empty long-poll response just before timeout.
- `must-refetch.json` — server signals client should reset to `offset=-1`.
- `snake-case.json`, `camel-case.json` — same row in both casings.
- `long-poll-canary.md` — the manual test for verifying ~60s connection hold.
