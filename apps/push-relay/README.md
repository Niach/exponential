# push-relay

Hono on Bun. One process, one deploy (Coolify, `https://push.exponential.at`, built from `Dockerfile.push-relay`), **one push API**:

1. A device **registers** its install (APNs or FCM token) and gets an `installSecret` to manage it.
2. It **mints capabilities**: opaque bearer tokens, one per caller allowed to wake it.
3. A caller **pushes** with a capability and a payload **sealed** to the install's public key. The relay forwards it blind.

There are no accounts and no privileged caller. A peer device, a desktop agent and Exponential's web server are all the same thing to the relay: someone holding a capability an install minted for them.

| File | What |
| --- | --- |
| `src/push-api.ts` | the push API (`/installs…`, `/push`) |
| `src/store.ts` | its SQLite store (installs + capability hashes) |
| `src/apns.ts` | the APNs HTTP/2 client (FCM goes through firebase-admin) |
| `src/compat-send.ts` | the deprecated `/send` [compat shim](#compat-shim-send) |
| `src/app.ts` | `createRelayApp`: mounts both, plus the operator routes |
| `src/index.ts` | reads the environment, opens the store, starts |

## Routes

| Route | Auth | |
| --- | --- | --- |
| `POST /installs` | none (20 per IP per hour) | register an install |
| `PUT /installs/:id` | `Bearer <installSecret>` | rotate the device token |
| `DELETE /installs/:id` | `Bearer <installSecret>` | delete the install and its capabilities |
| `POST /installs/:id/capabilities` | `Bearer <installSecret>` | mint a capability |
| `GET /installs/:id/capabilities` | `Bearer <installSecret>` | list them (never the tokens) |
| `DELETE /installs/:id/capabilities/:capId` | `Bearer <installSecret>` | revoke one |
| `POST /push` | `Bearer cap_…` | push a sealed payload |
| `GET /healthz` | none | `{"ok":true}`, the constant Docker's HEALTHCHECK polls |
| `GET /healthz?detail=1` | none | `{"ok":true,"api":true,"compatSend":bool,"providers":{"fcm":bool,"apns":bool}}` |
| `GET /healthz?deep=1` | `x-relay-secret` | a dry-run FCM multicast that exercises the transport end to end |
| `GET /stats` | `x-relay-secret` | counters: `/send` + deep checks at the top level (the web admin performance page reads them), `providers`, and the push API's under `api` |
| `POST /send` | `x-relay-secret` | **deprecated** compat shim |

The relay stores only sha256 hashes of install secrets and capabilities (a leaked database cannot push or take over installs). The device token is stored as is, because every push needs it. Logs never contain tokens, secrets, capabilities or sealed payloads.

Errors are JSON `{"error": "…"}`. `400` invalid body, `401` bad or missing bearer (every failed auth attempt, on any route, draws from one per-IP throttle of 60/min, then `429`), `413` body or sealed payload too large, `503` provider not configured.

## The push API

### Register: `POST /installs`

```http
POST /installs
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

### Rotate the device token: `PUT /installs/:id`

```http
PUT /installs/ins_…
Authorization: Bearer <installSecret>

{ "token": "<new token>", "environment": "production" }
```

→ `200 {"installId": "ins_…"}`. The install id, secret and capabilities stay valid.

### Delete: `DELETE /installs/:id`

`Authorization: Bearer <installSecret>` → `204`. Deletes every capability of the install too.

### Mint a capability: `POST /installs/:id/capabilities`

```http
POST /installs/ins_…/capabilities
Authorization: Bearer <installSecret>

{ "label": "Anna's phone", "expiresAt": "2026-12-31T00:00:00Z", "maxPerHour": 60 }
```

All fields optional: `label` ≤ 64 chars, `expiresAt` an ISO date-time in the future, `maxPerHour` 1..600 (default 60). At most 256 capabilities per install (`409` beyond).

```http
201 Created
{ "capabilityId": "cid_…", "capability": "cap_<base64url, 32 bytes>" }
```

The capability is shown once; only its hash is kept.

### List: `GET /installs/:id/capabilities`

```json
{ "capabilities": [ { "capabilityId": "cid_…", "label": "Anna's phone", "createdAt": "…", "expiresAt": null, "lastUsedAt": "…", "maxPerHour": 60 } ] }
```

Never includes the capability token.

### Revoke: `DELETE /installs/:id/capabilities/:capId`

→ `204`, or `404` when the install has no such capability.

### Push: `POST /push`

```http
POST /push
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

`sealed` is an opaque blob the **sender** builds for the **recipient install's X25519 public key**. The recipient app hands that key out together with the capability (out of band: the pairing QR, the p2p channel, or its own registration call to a server such as Exponential's); the relay never sees keys or plaintext. It checks only that `sealed` is base64url and at most 3072 bytes decoded.

Format v1, exactly as `push_seal` / `push_open` in `apps/desktop/crates/exponential-net/src/push.rs`:

```text
sealed     = 0x01 ‖ eph_pub[32] ‖ ciphertext ‖ tag[16]          (49 bytes of overhead)
shared     = X25519(eph_secret, recipient_pub)                  (all-zero shared = refused)
okm[44]    = HKDF-SHA256(ikm  = shared,
                         salt = eph_pub ‖ recipient_pub,
                         info = "exp/enet/push/1")
key, nonce = okm[0..32], okm[32..44]
ciphertext ‖ tag = ChaCha20-Poly1305(key, nonce, aad = [0x01], plaintext)
```

A fresh ephemeral key per message keeps the derived nonce unique per key. The install's keypair is random or derived from its node identity (HKDF-SHA256, info `exp/enet/push-x25519/1`), so only the 32-byte X25519 secret has to be shared with the iOS Notification Service Extension. Use the crate on every platform (it is exposed to Swift and Kotlin through UniFFI) so senders and receivers agree byte for byte. The plaintext's shape belongs to the apps.

## A server is one more caller

Exponential's web server does not hold device tokens on this API. The flow:

1. The phone registers its install here and keeps `installSecret` on the device.
2. It mints a capability for the server (`{"label": "exponential.at"}`) and gives the server that capability plus its X25519 public key (an authenticated call to the web app).
3. To notify, the server seals the notification to that public key and `POST /push`es it with the capability. A `410` means the install is gone: drop the capability. A `401` means it was revoked or expired.
4. The user (or the app on sign-out) revokes the capability; the server loses the phone without the relay knowing who the server is.

The server needs no relay secret for any of this.

## Compat shim: `/send`

`POST /send` (`src/compat-send.ts`) is **deprecated** and exists only for the Exponential iOS and Android builds already released: they give raw FCM tokens to the web server and show plaintext FCM `notification` messages, so they cannot open sealed payloads. The web server keeps reaching them through `/send` until new builds register installs.

- Gated by `PUSH_RELAY_SECRET` (`x-relay-secret` header). Unset = not mounted (`404`).
- Body `{tokens: string[≤500], notification: {title, body?}, data: Record<string,string>}` → `200 {invalidTokens}`. FCM only: data-only on Android, an explicit APNs alert block for iOS through FCM, one transport retry. Behaviour is frozen; see the comments in the file.
- Removal is tracked under **NEW-14**, once `CLIENT_MIN_VERSION_IOS` / `CLIENT_MIN_VERSION_ANDROID` pass the first builds that register installs.

`/stats` and `/healthz?deep=1` are operator routes, not part of the shim; they keep the same secret.

## Environment

| Var | Meaning |
| --- | --- |
| `PORT` | listen port, default 4001 |
| `TRUST_PROXY` | `true` only behind a proxy you control that appends `X-Forwarded-For`; keys the per-IP limits by its rightmost entry |
| `PUSH_RELAY_DB_PATH` | the push API's SQLite file, default `./data/push-relay.sqlite` (the directory is created); the image sets `/app/data/push-relay.sqlite`. **The relay refuses to start when it cannot open it.** |
| `FIREBASE_SERVICE_ACCOUNT_JSON` | Firebase service account (single line): FCM installs and `/send` |
| `APNS_KEY_P8` | the `.p8` auth key's PEM contents (literal `\n` escapes are fine) |
| `APNS_KEY_ID` | the key's id |
| `APNS_TEAM_ID` | the Apple developer team id |
| `APNS_TOPICS` | comma list of bundle ids installs may register for |
| `PUSH_PLACEHOLDER_TITLE` | the visible APNs alert title, default `New message` |
| `PUSH_PLACEHOLDER_BODY` | optional alert body |
| `PUSH_RELAY_SECRET` | operator secret shared with the web app: `/send` shim, `/stats`, `/healthz?deep=1`. Unset = those answer 404 |

### Deploying

Mount a **persistent volume at `/app/data`** (Coolify: Persistent Storage). Without it every redeploy forgets all installs, and every capability a caller holds stops working until the devices re-register.

## Development

```bash
bun run dev          # bun --hot src/index.ts
bun test             # app.test.ts (/send shim, deep check, stats) + push-api.test.ts (in-memory SQLite, fake APNs/FCM)
bun run typecheck
```
