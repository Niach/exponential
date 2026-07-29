# Install Exponential (self-hosted)

> **name**: install-exponential
> **description**: Install and operate a self-hosted Exponential instance — realtime issue tracker with local coding agents — from the published Docker images. No repo checkout, no build step.
> **when to use**: A human or agent wants Exponential running on a server or workstation they control. Follow the steps top to bottom; every command is copy-pasteable. If you are an agent: run the steps, verify each checkpoint, and ask your human only for the decisions marked **[decision]**.

## Prerequisites

- **Docker Engine with Docker Compose v2.23.1+** (the compose file uses inline `configs:` content). Check:

  ```sh
  docker compose version
  ```

- **Ports 80 and 443 free** on the host (Caddy binds both; 443 only actually serves once you configure a domain).
- **An S3-compatible bucket + access key** — the one external dependency, used for attachments and widget screenshots. Any provider works: Hetzner Object Storage, MinIO, Cloudflare R2, AWS S3, … The app uses path-style addressing and streams all attachment traffic server-side, so the endpoint never needs to be reachable by browsers — a LAN MinIO is fine. **[decision]** which provider; if none exists yet, a local [Garage](https://garagehq.deuxfleurs.fr) or MinIO container is a fine single-binary answer, run and bootstrapped by you next to (not inside) this stack.
- Outbound HTTPS to `ghcr.io` for image pulls.

## 1. Get the two files

```sh
mkdir exponential && cd exponential
curl -fsSLO https://raw.githubusercontent.com/Niach/exponential/master/selfhost/docker-compose.yaml
curl -fsSL https://raw.githubusercontent.com/Niach/exponential/master/selfhost/.env.example -o .env
```

## 2. Fill in `.env`

Generate the two secrets (once, keep forever):

```sh
sed -i "s/^POSTGRES_PASSWORD=$/POSTGRES_PASSWORD=$(openssl rand -hex 32)/" .env
sed -i "s/^BETTER_AUTH_SECRET=$/BETTER_AUTH_SECRET=$(openssl rand -hex 32)/" .env
```

(macOS: `sed -i ''` — or just edit the file.)

Then set the S3 block (`S3_ENDPOINT`, `S3_ACCESS_KEY`, `S3_SECRET_KEY`, `S3_BUCKET`, `S3_REGION`) to your provider's values. Create the bucket up front, or grant the key bucket-create permission and the app creates it at first use.

Leave `DOMAIN`/`APP_URL` commented for now — the stack serves plain HTTP on `http://localhost` out of the box.

## 3. Start

```sh
docker compose up -d
```

The web image applies database migrations **and** its custom trigger SQL at every boot — there are no manual SQL steps, on install or on any update.

**Checkpoint** (the web container waits for Postgres and can take ~30s on first boot):

```sh
docker compose ps                              # all services running
curl -fsS http://localhost/api/health          # => {"ok":true,"db":true,...}
```

If health fails, read `docker compose logs web --tail 50`.

## 4. First account

Open `http://localhost`, register, and create your first team. Verify attachments work (this is the S3 credentials smoke test): open any issue and paste or drag an image into the description — it must render back. If it errors, the `S3_*` values are wrong (`docker compose logs web` shows the S3 error).

## 5. Go live on a domain (optional)

1. Point DNS (an `A`/`AAAA` record) at the host.
2. Set **both** in `.env` — mismatched values break sign-in:

   ```sh
   DOMAIN=issues.example.com
   APP_URL=https://issues.example.com
   ```

3. `docker compose up -d` (recreates web + caddy). Caddy provisions Let's Encrypt certificates automatically; ports 80/443 must be reachable from the internet for that.
4. **[decision]** Once everyone has an account, close public sign-up: set `AUTH_SIGNUP_ENABLED=false` in `.env` and `docker compose up -d` again. New teammates then join via invite links.

Native apps (iOS, Android, desktop IDE) connect to a self-hosted instance: enter `https://issues.example.com` as the instance URL on first launch.

## 6. Optional subsystems

All configured by appending vars to `.env` (the whole file reaches the web container) and re-running `docker compose up -d`. Full reference: [`.env.example`](https://github.com/Niach/exponential/blob/master/.env.example) at the repo root; deeper walkthroughs: [self-host docs](https://exponential.at/docs/self-host/).

- **Email** (password reset, invites, notification digest, helpdesk magic links): `SMTP_HOST`/`SMTP_PORT`/`SMTP_USER`/`SMTP_PASS` + `EMAIL_FROM`, or Amazon SES via `AWS_SES_REGION` + AWS credentials. Without a transport, email features are silently off; everything else works.
- **Sign-in providers**: `GOOGLE_CLIENT_ID`/`GOOGLE_CLIENT_SECRET`/`GOOGLE_LOGIN_ENABLED=true`, or any OIDC IdP via `OIDC_PROVIDERS` (JSON array — Authentik, Keycloak, Zitadel, …).
- **GitHub App** (only needed for the coding flow — repo-backed boards, coding sessions, PRs): create a GitHub App and set `GITHUB_APP_ID`/`GITHUB_APP_SLUG`/`GITHUB_APP_PRIVATE_KEY` (+ `GITHUB_WEBHOOK_SECRET`, or `GITHUB_POLLING=true` behind NAT). Setup walkthrough in the [self-host docs](https://exponential.at/docs/self-host/#github-app).
- **Steer relay** (start coding sessions from your phone, watch/steer live): set `STEER_RELAY_SECRET` (any random string) and `STEER_RELAY_URL=ws://<host>:4002` in `.env`, then

  ```sh
  docker compose --profile steer up -d
  curl -fsS http://localhost:4002/healthz
  ```

- **Push notifications: not available for the store mobile apps.** The published iOS/Android binaries are compiled against Exponential's first-party Firebase project and only its cloud relay can reach their device tokens — a self-hosted relay cannot, by design (the relay authenticates senders). Self-hosted users get web, email, and desktop notifications; mobile push works only on the cloud, or if you build the mobile apps yourself against your own Firebase project.

## Upgrading

```sh
docker compose pull && docker compose up -d
```

The image self-migrates on boot — no other steps. `latest` tracks upstream `master`; to move deliberately instead, pin `IMAGE_TAG` in `.env` to a [release tag](https://github.com/Niach/exponential/tags) (e.g. `IMAGE_TAG=0.18.13`) and bump it when you choose.

## Troubleshooting

| Symptom | Cause / fix |
| --- | --- |
| `docker compose up` errors on `configs` | Compose < 2.23.1 — upgrade Docker Compose. |
| Port 80/443 already allocated | Another proxy owns them. Either stop it, or change the caddy `ports:` mapping and front this stack with your proxy (keep its read timeouts ≥ 5m and streaming/flush on — Electric uses long-polls). |
| Sign-in loops or "origin not allowed" | `DOMAIN` and `APP_URL` disagree (scheme included). Set both to the same origin and `docker compose up -d`. |
| Image pastes/attachments fail | Wrong `S3_*` values, missing bucket, or key without create permission — `docker compose logs web` shows the S3 error. |
| Nobody can register | `AUTH_SIGNUP_ENABLED` is false/unset in production. Set `true` while onboarding, or configure an OAuth/OIDC provider. |
| No certificates on your domain | Ports 80/443 not reachable from the internet, or DNS not propagated — `docker compose logs caddy`. |

## Licensing

Self-hosting is free while your company (affiliates included) has **fewer than 10 people**; 10 or more needs a commercial license (€590/yr up to 25 people, €1,900/yr up to 100). Full terms: [LICENSE](https://github.com/Niach/exponential/blob/master/LICENSE) (Exponential Small Team License 1.0). Nothing in the software enforces this — it's a contract, on the honor system.
