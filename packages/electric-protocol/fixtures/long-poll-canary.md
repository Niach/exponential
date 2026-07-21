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

---

# Outbound-concurrency canary (REV2-6)

Verifies the proxy holds MORE than 256 concurrent live long-polls. Bun caps
simultaneous outbound `fetch()` at 256 per process
(`BUN_CONFIG_MAX_HTTP_REQUESTS`); each synced client holds 14 live polls, so
the default saturated at ~18 clients and queued every further outbound fetch —
shape polls AND GitHub / push / Creem / steer calls. The web Docker image
bakes `ENV BUN_CONFIG_MAX_HTTP_REQUESTS=65336`; this canary proves the RUNNING
server actually got it.

## Setup

1. Run the production server path, NOT `vite dev` — either the Docker image,
   or `bun --filter @exp/web build` then
   `BUN_CONFIG_MAX_HTTP_REQUESTS=65336 bun apps/web/.output/server/index.mjs`
   (the var must be in the process env BEFORE bun starts; check the boot log
   for the `[server-bun]` warning — silence means the cap is raised).
2. Mint a bearer token (as in the canary above).
3. Grab a live cursor once:

   ```bash
   curl -s -D /tmp/snap.headers -o /dev/null -H "Authorization: Bearer $TOKEN" \
     "http://localhost:3000/api/shapes/issues?offset=-1"
   ```

   → read `electric-handle` / `electric-offset` from `/tmp/snap.headers`. All
   300 polls may share this one cursor — concurrent listeners per shape is
   Electric's designed fan-out.
4. `ulimit -n` ≥ ~1000 on the curl side.

## Run

```bash
N=300
start=$(date +%s)
seq $N | xargs -P $N -I{} curl -s -o /dev/null \
  -w "%{http_code} %{time_total}\n" \
  -H "Authorization: Bearer $TOKEN" \
  "http://localhost:3000/api/shapes/issues?offset=$OFFSET&handle=$HANDLE&live=true" \
  > /tmp/concurrency-canary.txt
echo "wall: $(( $(date +%s) - start ))s"
```

Do NOT rewrite this as a single Bun script firing 300 `fetch()` calls: the
CLIENT process would hit the same 256 default and serialize on its own side.
curl is one process per request.

## Expected

- Wall clock ≈ ONE long-poll window (~40-60s): all 300 held concurrently.
- Every line `200` with `time_total` ≈ 40-60s, unimodal. (A data change during
  the window legitimately returns everything early — rerun on a quiet DB.)

## Failing signal

- Bimodal times (~256 lines around ~45s, the rest ≥ ~90s) or wall clock ≈ 2×
  the poll window → the outbound cap is back at 256. Check the Dockerfile
  `ENV`, the running container's env, and the `[server-bun]` boot warning.
- All requests return in <2s → the long-poll hold itself regressed; run the
  first canary in this file.
