# push-relay

Hono on Bun. One process, one deploy (Coolify, `https://push.exponential.at`, built from `Dockerfile.push-relay`), two modes:

| Mode | Routes | Who calls it | Auth | Device tokens live in |
| --- | --- | --- | --- | --- |
| **v1** (Exponential) | `POST /send`, `GET /stats`, `GET /healthz?deep=1` | the web app only | `x-relay-secret: $PUSH_RELAY_SECRET` | the web app's Postgres |
| **accountless** (spezl, later Exponential) | `/v2/*` | devices and their peers | install secret / capability bearer | the relay's SQLite file |

`src/app.ts` builds the HTTP surface (`createRelayApp`), `src/accountless.ts` mounts `/v2`, `src/store.ts` is the SQLite store, `src/apns.ts` the APNs HTTP/2 client, `src/index.ts` reads the environment.

## Which modes run

- `PUSH_RELAY_SECRET` set: v1 on, unchanged.
- `PUSH_RELAY_SECRET` unset: the v1 routes answer **404** (an open v1 would push operator-signed notifications to harvested tokens).
- The accountless mode is on whenever the SQLite store opens (`PUSH_RELAY_DB_PATH`, default `./data/push-relay.sqlite`; the directory is created). `PUSH_RELAY_ACCOUNTLESS=false` turns it off.
- Neither secret nor store: the relay refuses to start.

`GET /healthz` stays the constant `{"ok":true}` the Docker HEALTHCHECK polls. `GET /healthz?modes=1` (public, booleans only):

```json
{ "ok": true, "modes": { "v1": true, "accountless": true }, "providers": { "fcm": true, "apns": true } }
```

`GET /stats` (v1 secret) carries an `accountless` block of counters when that mode is on.

## v1

`POST /send` with `x-relay-secret`, body `{tokens: string[≤500], notification: {title, body?}, data: Record<string,string>}` → `200 {invalidTokens}`. FCM only, data-only on Android, an explicit APNs alert block for iOS through FCM. See the comments in `src/app.ts`.

## Accountless mode (`/v2`)

No accounts. A device **registers** itself and gets an `installSecret` to manage its install. It then **mints capabilities**: opaque bearer tokens it hands to a peer (a friend, its own desktop agent) so the peer can wake it without ever learning the device token. The payload is **sealed** end to end; the relay forwards it blind.

The relay stores only sha256 hashes of install secrets and capabilities (a leaked database cannot push or take over installs). The device token is stored as is, because every push needs it. Logs never contain tokens, secrets, capabilities or sealed payloads.

Errors are JSON `{"error": "…"}`. `400` invalid body, `401` bad or missing bearer (failed attempts share v1's per-IP throttle of 60/min, then `429`), `413` body or sealed payload too large, `503` provider not configured.

### Register: `POST /v2/installs`

```http
POST /v2/installs
Content-Type: application/json

{ "platform": "apns", "token": "<64+ hex chars>", "topic": "app.spezl.ios", "environment": "sandbox" }
```

- `platform`: `"apns"` or `"fcm"`.
- `token`: the APNs device token (hex) or the FCM registration token.
- `topic` (APNs only): the bundle id; must be in `APNS_TOPICS`. Optional when `APNS_TOPICS` lists exactly one.
- `environment` (APNs only): `"production"` (default) or `"sandbox"` (debug builds).

```http
201 Created
{ "installId": "ins_…", "installSecret": "<base64url, 32 bytes>" }
```

The secret is shown once. Registrations are limited per client IP (20/hour; keyed by the forwarded address with `TRUST_PROXY=true`, else by the socket address).

### Rotate the device token: `PUT /v2/installs/:id`

```http
PUT /v2/installs/ins_…
Authorization: Bearer <installSecret>

{ "token": "<new token>", "environment": "production" }
```

→ `200 {"installId": "ins_…"}`. The install id, secret and capabilities stay valid.

### Delete: `DELETE /v2/installs/:id`

`Authorization: Bearer <installSecret>` → `204`. Deletes every capability of the install too.

### Mint a capability: `POST /v2/installs/:id/capabilities`

```http
POST /v2/installs/ins_…/capabilities
Authorization: Bearer <installSecret>

{ "label": "Anna's phone", "expiresAt": "2026-12-31T00:00:00Z", "maxPerHour": 60 }
```

All fields optional: `label` ≤ 64 chars, `expiresAt` an ISO date-time in the future, `maxPerHour` 1..600 (default 60). At most 256 capabilities per install (`409` beyond).

```http
201 Created
{ "capabilityId": "cid_…", "capability": "cap_<base64url, 32 bytes>" }
```

The capability is shown once; only its hash is kept.

### List: `GET /v2/installs/:id/capabilities`

```json
{ "capabilities": [ { "capabilityId": "cid_…", "label": "Anna's phone", "createdAt": "…", "expiresAt": null, "lastUsedAt": "…", "maxPerHour": 60 } ] }
```

Never includes the capability token.

### Revoke: `DELETE /v2/installs/:id/capabilities/:capId`

→ `204`, or `404` when the install has no such capability.

### Push: `POST /v2/push`

```http
POST /v2/push
Authorization: Bearer cap_…

{ "sealed": "<base64url>", "collapseId": "chat-42", "urgency": "high", "ttlSeconds": 3600 }
```

- `sealed`: the end-to-end encrypted payload, base64url, at most 3072 bytes decoded.
- `collapseId` (≤ 64 chars): `apns-collapse-id` / FCM `collapseKey`.
- `urgency`: `"high"` (default) or `"normal"`: APNs priority 10/5, FCM android priority.
- `ttlSeconds`: 0..2419200. APNs `apns-expiration` (0 = now or never), FCM `ttl`. Unset = the provider's default.

Answers:

| Status | Meaning |
| --- | --- |
| `202 {"delivered": true}` | the provider accepted it |
| `401` | unknown, revoked or expired capability |
| `410` | the install is gone: its device token is dead (APNs 410 / `BadDeviceToken` / `Unregistered`, FCM unregistered / sender mismatch). The install and all its capabilities are deleted; stop using the capability. |
| `429` | this capability's hourly budget (`maxPerHour`, a fixed one-hour window) is spent. Malformed pushes do not spend budget. |
| `502` / `504` | provider error / timeout; the install is kept |

What goes on the wire:

- **APNs** (HTTP/2 to `api.push.apple.com` or `api.sandbox.push.apple.com`, ES256 provider token cached 50 min): `apns-push-type: alert`, the install's topic, and

  ```json
  { "aps": { "alert": { "title": "New message" }, "mutable-content": 1 }, "s": "<sealed>" }
  ```

  The alert is a fixed placeholder (`PUSH_PLACEHOLDER_TITLE`, optional `PUSH_PLACEHOLDER_BODY`). The app's Notification Service Extension opens `s` and replaces title and body, so neither the relay nor Apple sees the text.
- **FCM**: data-only, `{"data": {"s": "<sealed>"}}` with the android priority/ttl/collapseKey above. The app's messaging service opens `s` and posts the notification itself.

### The sealing contract

`sealed` is an opaque, end-to-end encrypted blob the **sender** builds for the **recipient install's public key**. The recipient app publishes that key to its peers together with the capability (out of band, e.g. in the pairing QR or the p2p channel); the relay never sees keys. The relay only checks that `sealed` is base64url and at most 3072 bytes decoded; everything else (algorithm, framing, replay protection, what the plaintext contains) belongs to the clients.

The reference implementation is `push_seal` / `push_open` in the eNet Rust crate `exponential-net`. Use it on every platform (it is exposed to Swift/Kotlin) so senders and the iOS NSE / Android service agree byte for byte.

## Environment

| Var | Mode | Meaning |
| --- | --- | --- |
| `PORT` | both | listen port, default 4001 |
| `TRUST_PROXY` | both | `true` only behind a proxy you control that appends `X-Forwarded-For`; keys the per-IP limits by its rightmost entry |
| `FIREBASE_SERVICE_ACCOUNT_JSON` | both | Firebase service account (single line). v1 and accountless FCM installs |
| `PUSH_RELAY_SECRET` | v1 | shared with the web app; unset = v1 off (404) |
| `PUSH_RELAY_DB_PATH` | accountless | SQLite file, default `./data/push-relay.sqlite`; the image sets `/app/data/push-relay.sqlite` |
| `PUSH_RELAY_ACCOUNTLESS` | accountless | `false` turns `/v2` off |
| `APNS_KEY_P8` | accountless | the `.p8` auth key's PEM contents (literal `\n` escapes are fine) |
| `APNS_KEY_ID` | accountless | the key's id |
| `APNS_TEAM_ID` | accountless | the Apple developer team id |
| `APNS_TOPICS` | accountless | comma list of bundle ids installs may register for |
| `PUSH_PLACEHOLDER_TITLE` | accountless | the visible APNs alert title, default `New message` |
| `PUSH_PLACEHOLDER_BODY` | accountless | optional alert body |

### Deploying

Mount a **persistent volume at `/app/data`** (Coolify: Persistent Storage). Without it every redeploy forgets all installs, and every capability a peer holds stops working until the devices re-register.

## Development

```bash
bun run dev          # bun --hot src/index.ts
bun test             # app.test.ts (v1) + accountless.test.ts (/v2, in-memory SQLite, fake APNs/FCM)
bun run typecheck
```
