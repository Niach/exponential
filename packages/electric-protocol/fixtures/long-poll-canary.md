# Long-poll canary test

Manual test to verify the entire request path (client → Caddy → Bun → TanStack
Start → Electric upstream) actually holds a `live=true` request open for the
full ~60s long-poll window, instead of silently degrading to short-polls.

If this test fails, "real-time sync" is technically working but blasting the
backend with a fresh HTTP request every <1s instead of every ~60s.

## Setup

1. `bun run backend:up`
2. `bun dev`
3. Log into the web app once so you have a valid session cookie *or* mint a
   bearer token (`POST /api/auth/sign-in/email`).

## Run

```bash
# Replace the token / cookie with one from your session.
time curl -s -o /tmp/canary.json -D /tmp/canary.headers \
  -H "Authorization: Bearer <token>" \
  "http://localhost:3000/api/shapes/issues?offset=0_0&handle=<handle-from-initial-snapshot>&live=true"
```

You need a real `<handle>` from a prior `?offset=-1` request, since Electric
will short-circuit if it can't recognize the cursor.

## Expected

- `time` reports roughly 40–60 seconds elapsed.
- `/tmp/canary.json` contains `[{"headers":{"control":"up-to-date"}}]` (no
  changes happened during the hold).
- `/tmp/canary.headers` contains updated `electric-handle` and
  `electric-offset` values.

## Failing signal

- `time` reports <2 seconds → some middlebox returned early. Check, in order:
  1. `apps/web/src/server-bun.ts` — `idleTimeout` still 255?
  2. `Caddyfile` — `transport http { read_timeout 5m; … }` still present?
  3. `apps/web/src/lib/electric-proxy.ts` — `fetch(originUrl, { signal })`
     not wrapped in an `AbortSignal.timeout(...)`?
  4. Electric docker container healthy and `wal_level=logical` on Postgres?
- 502 Bad Gateway with `EOF` → Bun closed the keep-alive connection mid-poll;
  raise `idleTimeout`.
- 499 Client Closed Request → your curl gave up (raise `--max-time` if you
  set one).
