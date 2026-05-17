import {
  DocsCallout,
  DocsCode,
  DocsLayout,
  DocsSection,
  EnvVar,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcDocker, IcGithub } from "./components/icons"

const SECTIONS: DocsSectionType[] = [
  { id: `introduction`, num: `01`, label: `Introduction` },
  { id: `quick-start`, num: `02`, label: `Quick start` },
  { id: `production`, num: `03`, label: `Production deploy` },
  { id: `configuration`, num: `04`, label: `Configuration` },
  { id: `authentication`, num: `05`, label: `Authentication` },
  { id: `storage`, num: `06`, label: `Storage` },
  { id: `realtime`, num: `07`, label: `Real-time sync` },
  { id: `push`, num: `08`, label: `Push notifications` },
  { id: `integrations`, num: `09`, label: `Integrations` },
  { id: `mobile`, num: `10`, label: `Mobile apps` },
  { id: `upgrading`, num: `11`, label: `Upgrading` },
  { id: `troubleshooting`, num: `12`, label: `Troubleshooting` },
]

export function DocsPage() {
  return (
    <>
      <SiteHeader />

      <section className="docs-hero">
        <div className="hero-grid" />
        <div className="shell docs-hero-content fade-in">
          <span className="tag-row">
            <span className="tag-pill">Docs</span>
            <span>Self-host Exponential in fifteen minutes</span>
          </span>
          <h1>
            Run Exponential
            <br />
            <em>on your own machines.</em>
          </h1>
          <p>
            A practical guide to installing, configuring, and operating your own
            Exponential instance. Real-time issue tracking, no SaaS, no
            telemetry, no vendor lock-in.
          </p>
          <div className="docs-hero-cta">
            <a
              className="btn btn-primary"
              href="#quick-start"
            >
              <IcDocker size={14} /> Quick start
            </a>
            <a
              className="btn btn-ghost"
              href="https://github.com/Niach/exponential"
            >
              <IcGithub size={14} /> View source
            </a>
          </div>
        </div>
      </section>

      <DocsLayout sections={SECTIONS}>
        <DocsSection id="introduction" num="01" label="Introduction">
          <h2>Introduction</h2>
          <p>
            <strong>Exponential</strong> is a real-time issue tracker designed
            to live on your own infrastructure. It pairs a TanStack Start web
            app with a Postgres database streamed live to clients through
            ElectricSQL, so every workspace member sees the same state without
            spinners, refetches, or stale lists.
          </p>
          <p>
            One <code>docker compose up</code> brings the full stack online:
            Postgres for durable storage, Electric for live sync, Garage for
            S3-compatible attachment storage, and Caddy as the HTTP/2 reverse
            proxy. There are no SaaS dependencies, and the only opt-in outbound
            calls are to providers you configure yourself — OIDC, Google
            Calendar, Firebase Cloud Messaging.
          </p>
          <h3>What you get</h3>
          <ul>
            <li>Optimistic, real-time issue tracking with labels, priorities, statuses, due dates and attachments.</li>
            <li>Workspace + project scoping with sharable, slugged URLs.</li>
            <li>Better Auth with email/password, multi-provider OIDC, and optional Google sign-in.</li>
            <li>Google Calendar one-way sync per user.</li>
            <li>Push notifications via a standalone <code>push-relay</code> service.</li>
            <li>An MCP server endpoint for AI agents (Claude and friends) to read and write issues.</li>
            <li>Native Android app (Kotlin + Jetpack Compose); iOS in progress.</li>
          </ul>
          <h3>What you need</h3>
          <ul>
            <li>A machine that can run Docker and reach the public internet on port 80/443 (for Let's Encrypt).</li>
            <li>A domain you control, with DNS pointed at that machine.</li>
            <li>
              For local development: <code>bun</code> ≥ 1.1 and a recent
              Docker.
            </li>
          </ul>
        </DocsSection>

        <DocsSection id="quick-start" num="02" label="Quick start">
          <h2>Quick start</h2>
          <p>
            This is the five-minute path for local development. It boots the
            full stack on your machine, seeds storage credentials, runs the
            migrations, and starts the web app on{` `}
            <code>http://localhost:5173</code>.
          </p>
          <h3>1. Clone &amp; install</h3>
          <DocsCode language="shell">{`
git clone https://github.com/Niach/exponential
cd exponential
bun install
`}</DocsCode>
          <h3>2. Bring the backend up</h3>
          <p>
            This starts Postgres on <code>54321</code>, Electric on{` `}
            <code>30000</code>, Garage on <code>3900</code>, and Caddy on{` `}
            <code>3000</code>.
          </p>
          <DocsCode language="shell">{`
bun run backend:up
`}</DocsCode>
          <h3>3. Bootstrap storage</h3>
          <p>
            Run the Garage initializer once; it prints an{` `}
            <code>S3_ACCESS_KEY</code> and <code>S3_SECRET_KEY</code> you'll
            paste into <code>.env</code>.
          </p>
          <DocsCode language="shell">{`
bun run storage:init
`}</DocsCode>
          <h3>4. Configure environment</h3>
          <p>
            Copy the example file and fill in the database URL, a 32+ char auth
            secret, and the S3 keys from the previous step.
          </p>
          <DocsCode language="shell">{`
cp apps/web/.env.example .env
# generate a secret
openssl rand -hex 32
`}</DocsCode>
          <h3>5. Migrate &amp; run</h3>
          <DocsCode language="shell">{`
bun run migrate
docker exec -i exponential-postgres-1 \\
  psql -U postgres -d exponential \\
  < apps/web/src/db/out/custom/0001_triggers.sql
bun dev
`}</DocsCode>
          <DocsCallout kind="tip" title="Triggers">
            The custom SQL in <code>apps/web/src/db/out/custom/0001_triggers.sql</code>{` `}
            auto-increments per-project issue numbers and keeps{` `}
            <code>updated_at</code> fresh on every table. It must be applied
            once after the initial migration — Drizzle does not own it.
          </DocsCallout>
          <p>
            Open <code>http://localhost:5173</code>, register the first user,
            and you'll land in a default workspace ready to create projects.
          </p>
        </DocsSection>

        <DocsSection id="production" num="03" label="Production deploy">
          <h2>Production deploy</h2>
          <p>
            Production runs the same stack with three additions: a built Docker
            image for the web app, a public reverse proxy (Caddy) with TLS, and
            persistent volumes for Postgres and Garage.
          </p>
          <h3>Build the web image</h3>
          <p>
            The root <code>Dockerfile</code> builds the TanStack Start server.
            The build context must be the repo root because it copies the bun
            workspace manifests.
          </p>
          <DocsCode language="shell">{`
docker build -f Dockerfile -t exponential-web:latest .
`}</DocsCode>
          <h3>Reverse proxy &amp; TLS</h3>
          <p>
            Copy <code>Caddyfile.example</code> to <code>Caddyfile</code> and
            adjust the host blocks. The example is preconfigured with the
            long-poll timeouts Electric needs — leaving them out will cause
            shape requests to disconnect every 30 seconds.
          </p>
          <DocsCode language="caddyfile">{`
yourapp.com {
  reverse_proxy web:3000
}

# Electric shape proxy — long polls
yourapp.com/v1/shape/* {
  reverse_proxy electric:3000 {
    transport http {
      response_header_timeout 5m
      read_timeout 5m
    }
  }
}
`}</DocsCode>
          <h3>Persistent volumes</h3>
          <p>
            The default <code>docker-compose.yaml</code> creates named volumes
            for Postgres (<code>pgdata</code>) and Garage (
            <code>garage-data</code>). Back them up however you back up your
            other databases — Postgres logical dump is enough for the schema
            and rows; Garage holds attachment blobs.
          </p>
          <DocsCallout kind="note" title="Known-working hosts">
            The reference deployment runs the web container on Portainer
            (triggered by tagged Gitea releases) and the marketing + push-relay
            services on Coolify. Both flows are simple — any Docker host with
            Caddy in front works.
          </DocsCallout>
        </DocsSection>

        <DocsSection id="configuration" num="04" label="Configuration">
          <h2>Configuration</h2>
          <p>
            Configuration lives entirely in <code>.env</code>. The full
            authoritative list is{` `}
            <a href="https://github.com/Niach/exponential/blob/master/apps/web/.env.example">
              apps/web/.env.example
            </a>
            ; below is the working set, grouped by concern.
          </p>

          <h3>Core</h3>
          <dl className="docs-env-list">
            <EnvVar name="DATABASE_URL" required>
              Postgres connection string used by Drizzle and tRPC.
            </EnvVar>
            <EnvVar name="BETTER_AUTH_SECRET" required>
              32+ char secret used by Better Auth to sign sessions. Generate
              with <code>openssl rand -hex 32</code>.
            </EnvVar>
            <EnvVar name="BETTER_AUTH_URL" required>
              Public base URL of the app. OAuth callbacks are derived from
              this.
            </EnvVar>
            <EnvVar name="BETTER_AUTH_TRUSTED_ORIGINS">
              Comma-separated list of origins Better Auth will accept.
            </EnvVar>
          </dl>

          <h3>ElectricSQL</h3>
          <dl className="docs-env-list">
            <EnvVar name="ELECTRIC_URL">
              Defaults to <code>http://localhost:30000</code> (the docker
              compose service).
            </EnvVar>
            <EnvVar name="ELECTRIC_SOURCE_ID">
              Only needed when using hosted Electric Cloud.
            </EnvVar>
            <EnvVar name="ELECTRIC_SECRET">Companion to <code>ELECTRIC_SOURCE_ID</code>.</EnvVar>
          </dl>

          <h3>Storage (S3)</h3>
          <dl className="docs-env-list">
            <EnvVar name="S3_ENDPOINT">Garage default: <code>http://localhost:3900</code>.</EnvVar>
            <EnvVar name="S3_ACCESS_KEY" required>Printed by <code>bun run storage:init</code>.</EnvVar>
            <EnvVar name="S3_SECRET_KEY" required>Printed by <code>bun run storage:init</code>.</EnvVar>
            <EnvVar name="S3_BUCKET">Default: <code>exponential-attachments</code>.</EnvVar>
            <EnvVar name="S3_REGION">Default: <code>garage</code>.</EnvVar>
          </dl>

          <h3>Auth toggles</h3>
          <dl className="docs-env-list">
            <EnvVar name="AUTH_PASSWORD_ENABLED">
              Set to <code>false</code> to disable email/password login.
              Default: enabled.
            </EnvVar>
            <EnvVar name="OIDC_PROVIDERS">
              JSON array of OIDC providers (see Authentication).
            </EnvVar>
            <EnvVar name="GOOGLE_CLIENT_ID">
              Setting <code>GOOGLE_CLIENT_ID</code> + <code>SECRET</code>{` `}
              unlocks Google sign-in and Calendar sync.
            </EnvVar>
            <EnvVar name="GOOGLE_CLIENT_SECRET">Required alongside <code>GOOGLE_CLIENT_ID</code>.</EnvVar>
            <EnvVar name="GOOGLE_LOGIN_ENABLED">Default: <code>false</code>.</EnvVar>
            <EnvVar name="GOOGLE_CALENDAR_ENABLED">Default: <code>false</code>.</EnvVar>
          </dl>

          <h3>Push &amp; MCP</h3>
          <dl className="docs-env-list">
            <EnvVar name="PUSH_RELAY_URL">
              Base URL of your push-relay deployment, e.g.{` `}
              <code>https://push.yourapp.com</code>. Leave unset to disable push.
            </EnvVar>
            <EnvVar name="MCP_API_TOKEN">Static Bearer token for the MCP endpoint.</EnvVar>
            <EnvVar name="MCP_USER_EMAIL">User identity all MCP tool calls run as.</EnvVar>
          </dl>
        </DocsSection>

        <DocsSection id="authentication" num="05" label="Authentication">
          <h2>Authentication</h2>
          <p>
            Auth is handled by Better Auth, with sessions stored in Postgres
            and signed with <code>BETTER_AUTH_SECRET</code>. Three sign-in
            methods are available and can be combined.
          </p>
          <h3>Email &amp; password</h3>
          <p>
            Enabled by default. Set <code>AUTH_PASSWORD_ENABLED=false</code> if
            you want to force everyone through OIDC or Google.
          </p>
          <h3>OIDC (multi-provider)</h3>
          <p>
            Configure one or more OIDC providers as a JSON array. Each entry
            needs an <code>id</code>, <code>name</code>, client credentials,
            and the provider's discovery URL.
          </p>
          <DocsCode language="env">{`
OIDC_PROVIDERS='[{"id":"authentik","name":"Authentik","clientId":"...","clientSecret":"...","discoveryUrl":"https://auth.example.com/application/o/app/.well-known/openid-configuration"}]'
`}</DocsCode>
          <p>
            The authorized redirect URI in your IdP is{` `}
            <code>{`\${BETTER_AUTH_URL}/api/auth/oauth2/callback/<id>`}</code>.
            Optional <code>adminGroups</code> + <code>groupsClaim</code>{` `}
            promote users to admin based on a claim.
          </p>
          <h3>Google sign-in</h3>
          <p>
            Set <code>GOOGLE_CLIENT_ID</code> and <code>GOOGLE_CLIENT_SECRET</code>
            {` `}then flip <code>GOOGLE_LOGIN_ENABLED=true</code>. The same
            credentials are reused by the Calendar integration if{` `}
            <code>GOOGLE_CALENDAR_ENABLED=true</code>.
          </p>
        </DocsSection>

        <DocsSection id="storage" num="06" label="Storage">
          <h2>Storage</h2>
          <p>
            Attachments are stored in an S3-compatible bucket. The default
            stack runs Garage locally — a fully self-hosted, S3-compatible
            object store — but any provider works (AWS S3, Cloudflare R2,
            MinIO).
          </p>
          <h3>Bootstrap Garage</h3>
          <DocsCode language="shell">{`
bun run backend:up
bun run storage:init
`}</DocsCode>
          <p>
            The init script creates the <code>exponential-attachments</code>{` `}
            bucket and prints an access/secret pair. Paste those into{` `}
            <code>.env</code> as <code>S3_ACCESS_KEY</code> and{` `}
            <code>S3_SECRET_KEY</code>. Run it once per environment.
          </p>
          <h3>Use a different provider</h3>
          <p>
            Point <code>S3_ENDPOINT</code>, <code>S3_REGION</code>, and{` `}
            <code>S3_BUCKET</code> at your alternate store. Anything that
            speaks SigV4 will work. R2 users: set <code>S3_REGION=auto</code>.
          </p>
        </DocsSection>

        <DocsSection id="realtime" num="07" label="Real-time sync">
          <h2>Real-time sync</h2>
          <p>
            ElectricSQL streams Postgres rows directly to the browser using
            shapes — server-defined slices of tables filtered to what the user
            is allowed to see. Each synced table has a shape proxy under{` `}
            <code>src/routes/api/shapes/</code> that authenticates the request
            and forwards to Electric.
          </p>
          <h3>Caddy long polls</h3>
          <p>
            Electric uses long-poll HTTP requests that can stay open for
            minutes. The default Caddy timeouts will sever them — use the
            timeouts from <code>Caddyfile.example</code> or you'll see
            disconnect storms in the browser console.
          </p>
          <DocsCallout kind="warn" title="snake_case columns">
            Electric delivers Postgres column names verbatim, so collections
            must use <code>columnMapper: snakeCamelMapper()</code> from{` `}
            <code>@electric-sql/client</code>. Without it, <code>useLiveQuery</code>{` `}
            filters on camelCase fields silently fail — the field is{` `}
            <code>undefined</code> and the row never matches.
          </DocsCallout>
          <h3>Mutations</h3>
          <p>
            Writes go through tRPC, not Electric. Each mutation returns a{` `}
            Postgres transaction ID via <code>generateTxId</code>, so the
            client can wait for that exact write to round-trip through
            Electric before clearing the optimistic state.
          </p>
        </DocsSection>

        <DocsSection id="push" num="08" label="Push notifications">
          <h2>Push notifications</h2>
          <p>
            Push delivery is handled by a small standalone service — the
            <code>push-relay</code> — that wraps Firebase Cloud Messaging. The
            web app sends a payload + token list; the relay multicasts via
            FCM. The relay is deployed independently so it can scale and fail
            separately from the main app.
          </p>
          <h3>Build &amp; deploy the relay</h3>
          <DocsCode language="shell">{`
docker build -f Dockerfile.push-relay -t push-relay:latest .
# expose port 4001 and set FIREBASE_SERVICE_ACCOUNT_JSON as a one-line env var
`}</DocsCode>
          <p>
            The relay needs one env var:{` `}
            <code>FIREBASE_SERVICE_ACCOUNT_JSON</code> — the contents of a
            Firebase service-account JSON file as a single-line string. It
            exposes <code>POST /send</code> and <code>GET /healthz</code>.
          </p>
          <h3>Wire the web app</h3>
          <p>
            Set <code>PUSH_RELAY_URL</code> on the web deployment to the
            relay's public URL. The web app calls{` `}
            <code>POST $PUSH_RELAY_URL/send</code> when an issue change
            generates a notification.
          </p>
          <DocsCallout kind="warn" title="Trust boundary">
            The <code>/send</code> endpoint is currently unauthenticated. If
            you expose the relay to the public internet, anyone who can reach
            it can push notifications to FCM tokens they obtain. Restrict
            access by network policy or re-add the bearer-token middleware in{` `}
            <code>apps/push-relay/src/index.ts</code> if that's a concern.
          </DocsCallout>
        </DocsSection>

        <DocsSection id="integrations" num="09" label="Integrations">
          <h2>Integrations</h2>
          <h3>Google Calendar</h3>
          <p>
            Per-user opt-in. Users link Google from{` `}
            <code>/account/integrations</code> with the{` `}
            <code>calendar.events</code> scope. Tokens land in the existing{` `}
            Better Auth <code>accounts</code> table, and access tokens are
            auto-refreshed via <code>auth.api.getAccessToken</code>.
          </p>
          <p>
            Sync is one-way (issue → calendar) and writes all-day events to
            the user's primary calendar based on these rules:
          </p>
          <ul>
            <li>
              Issue has a <code>dueDate</code>, status is not{` `}
              <code>done</code>/<code>cancelled</code>, and not archived →
              event exists.
            </li>
            <li>Otherwise → no event.</li>
          </ul>
          <p>
            Failures are logged to <code>issues.googleCalendarLastSyncError</code>{` `}
            but never block the underlying mutation.
          </p>
          <h3>MCP server</h3>
          <p>
            The app exposes an MCP endpoint at <code>/api/mcp</code> authed
            with a static Bearer token (<code>MCP_API_TOKEN</code>). Tool
            calls run as the user identified by{` `}
            <code>MCP_USER_EMAIL</code>. Point Claude (or any MCP client) at
            it and it can list, create, and update issues, projects, and
            labels — scoped to that user's workspaces.
          </p>
        </DocsSection>

        <DocsSection id="mobile" num="10" label="Mobile apps">
          <h2>Mobile apps</h2>
          <h3>Android</h3>
          <p>
            Native Kotlin + Jetpack Compose app in <code>apps/android/</code>.
            Build from the repo root:
          </p>
          <DocsCode language="shell">{`
bun run android:build     # ./gradlew :app:assembleDebug
bun run android:install   # ./gradlew :app:installDebug
`}</DocsCode>
          <p>
            Tagging the repo with <code>android-vX.Y.Z</code> triggers{` `}
            <code>.gitea/workflows/build-android.yml</code>, which builds debug
            + unsigned release APKs and uploads them as artifacts. Distribution
            still needs a signing config + keystore in{` `}
            <code>app/build.gradle.kts</code>.
          </p>
          <h3>iOS</h3>
          <p>
            In progress — early SwiftUI implementation with feature parity for
            create/edit/list. Not yet in the repo's release flow.
          </p>
        </DocsSection>

        <DocsSection id="upgrading" num="11" label="Upgrading">
          <h2>Upgrading</h2>
          <p>
            Pull, install, migrate. Two of those steps have small gotchas
            worth knowing.
          </p>
          <DocsCode language="shell">{`
git pull
bun install
bun run migrate:generate    # if you changed the schema
bun run migrate
`}</DocsCode>
          <DocsCallout kind="warn" title="Custom triggers don't auto-apply">
            Drizzle migrations do not own the SQL in{` `}
            <code>apps/web/src/db/out/custom/</code>. After any migration that
            touches tables managed by those triggers, re-apply the file by
            hand:
          </DocsCallout>
          <DocsCode language="shell">{`
docker exec -i exponential-postgres-1 \\
  psql -U postgres -d exponential \\
  < apps/web/src/db/out/custom/0001_triggers.sql
`}</DocsCode>
          <p>
            Tagged releases (<code>vX.Y.Z</code>) build a new web image via{` `}
            <code>.gitea/workflows/build-release.yml</code>, push it to the
            Gitea registry, and trigger a Portainer redeploy. The marketing
            site and push-relay are watched by Coolify and redeploy on push to
            <code>master</code>.
          </p>
        </DocsSection>

        <DocsSection id="troubleshooting" num="12" label="Troubleshooting">
          <h2>Troubleshooting</h2>
          <h3>Rows don't appear in the UI but exist in Postgres</h3>
          <p>
            Almost always the snake_case column mapping. Confirm the affected
            collection in <code>src/lib/collections.ts</code> uses{` `}
            <code>columnMapper: snakeCamelMapper()</code>. Without it,{` `}
            <code>useLiveQuery</code> filters on camelCase fields silently
            return zero rows.
          </p>
          <h3>Electric requests disconnect every 30 seconds</h3>
          <p>
            Caddy is cutting the long poll. Use the timeouts in{` `}
            <code>Caddyfile.example</code> (<code>response_header_timeout</code>{` `}
            and <code>read_timeout</code> set to 5+ minutes on the shape
            routes).
          </p>
          <h3>Issue numbers don't auto-increment</h3>
          <p>
            The custom trigger SQL wasn't applied. Run it manually:
          </p>
          <DocsCode language="shell">{`
docker exec -i exponential-postgres-1 \\
  psql -U postgres -d exponential \\
  < apps/web/src/db/out/custom/0001_triggers.sql
`}</DocsCode>
          <h3>bun install fails inside a workspace</h3>
          <p>
            Always run <code>bun install</code> from the repo root. The
            workspace links resolve there; running it inside{` `}
            <code>apps/web</code> can leave you with a broken{` `}
            <code>node_modules</code> tree.
          </p>
          <h3>Push notifications silently no-op</h3>
          <p>
            Check the web app logs for{` `}
            <code>[fcm] PUSH_RELAY_URL not set</code>. If you see it,{` `}
            <code>PUSH_RELAY_URL</code> isn't reaching the deployed
            container. If you don't see it but pushes still don't arrive,
            check the relay's logs for FCM errors — invalid tokens come back
            in the <code>invalidTokens</code> response and are auto-deleted on
            the next attempt.
          </p>
        </DocsSection>
      </DocsLayout>

      <SiteFooter />
    </>
  )
}
