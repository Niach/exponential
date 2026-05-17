import {
  DocsCallout,
  DocsCode,
  DocsLayout,
  DocsSection,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcDocker, IcGithub } from "./components/icons"

const SECTIONS: DocsSectionType[] = [
  { id: `installation`, num: `01`, label: `Installation` },
  { id: `push`, num: `02`, label: `Push notifications` },
  { id: `integrations`, num: `03`, label: `Integrations` },
  { id: `mobile`, num: `04`, label: `Mobile apps` },
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
            <span>Run Exponential on your own machines</span>
          </span>
          <h1>
            Yours, end-to-end.
            <br />
            <em>Set up in minutes.</em>
          </h1>
          <p>
            A short guide to installing Exponential on your own server,
            wiring up push notifications, connecting integrations, and
            getting the mobile apps running.
          </p>
          <div className="docs-hero-cta">
            <a className="btn btn-primary" href="#installation">
              <IcDocker size={14} /> Install
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
        <DocsSection id="installation" num="01" label="Installation">
          <h2>Installation</h2>
          <p>
            Exponential ships as a single <code>docker compose</code> file.
            Four services come up — Postgres, Electric (real-time sync),
            Garage (S3-compatible attachment storage), and Caddy (HTTP/2
            reverse proxy with automatic TLS). No SaaS dependencies. No
            telemetry.
          </p>

          <h3>1. Clone the repo</h3>
          <DocsCode language="shell">{`
git clone https://github.com/Niach/exponential
cd exponential
`}</DocsCode>

          <h3>2. Bring the stack up</h3>
          <p>
            <code>bun install</code> sets up dependencies,{` `}
            <code>backend:up</code> starts the four containers, and{` `}
            <code>storage:init</code> prints the S3 keys you'll need in a
            moment.
          </p>
          <DocsCode language="shell">{`
bun install
bun run backend:up
bun run storage:init
`}</DocsCode>

          <h3>3. Configure</h3>
          <p>
            Copy the example env file and fill in the three values you
            actually need: the database URL (default works for local), a
            session secret, and the S3 keys from the previous step.
          </p>
          <DocsCode language="shell">{`
cp apps/web/.env.example .env
# generate a 32-character session secret
openssl rand -hex 32
`}</DocsCode>
          <DocsCallout kind="tip" title="Sign-in methods">
            Email and password works out of the box. To add Google sign-in or
            an OIDC provider (Authentik, Keycloak, etc.) later, set a handful
            of env vars — the full list is in{` `}
            <code>apps/web/.env.example</code>.
          </DocsCallout>

          <h3>4. Migrate &amp; run</h3>
          <DocsCode language="shell">{`
bun run migrate
docker exec -i exponential-postgres-1 \\
  psql -U postgres -d exponential \\
  < apps/web/src/db/out/custom/0001_triggers.sql
bun dev
`}</DocsCode>
          <p>
            Open <code>http://localhost:5173</code>, register the first user
            — that's your instance.
          </p>

          <h3>Going live</h3>
          <p>
            For production, build the web image with the root{` `}
            <code>Dockerfile</code>, point a domain at your server, and put
            it behind Caddy. The included{` `}
            <code>Caddyfile.example</code> is preconfigured with the long-poll
            timeouts Electric needs for real-time sync — copy it across.
          </p>
          <DocsCode language="shell">{`
docker build -f Dockerfile -t exponential-web:latest .
`}</DocsCode>
          <DocsCallout kind="note" title="Known-working hosts">
            The reference deployment runs the web container on Portainer
            (triggered by tagged Gitea releases) and the marketing site +
            push-relay on Coolify. Any Docker host with Caddy in front works.
          </DocsCallout>
        </DocsSection>

        <DocsSection id="push" num="02" label="Push notifications">
          <h2>Push notifications</h2>
          <p>
            Native push to phones is handled by a small companion service,
            the <strong>push-relay</strong>, that wraps Firebase Cloud
            Messaging. The web app posts payloads to it; the relay
            multicasts to your devices. Running it separately means push
            can scale and fail independently of the main app.
          </p>

          <h3>1. Get Firebase credentials</h3>
          <p>
            Create a Firebase project, generate a service-account key from
            the project settings, and save the JSON file. You'll paste it
            into the relay as a single env var.
          </p>

          <h3>2. Run the relay</h3>
          <p>
            Build the image from the repo root using{` `}
            <code>Dockerfile.push-relay</code> and expose port 4001 behind
            your reverse proxy.
          </p>
          <DocsCode language="shell">{`
docker build -f Dockerfile.push-relay -t push-relay:latest .
docker run -d \\
  -p 4001:4001 \\
  -e FIREBASE_SERVICE_ACCOUNT_JSON='<single-line JSON>' \\
  push-relay:latest
`}</DocsCode>
          <p>
            Verify it's healthy:
          </p>
          <DocsCode language="shell">{`
curl https://push.yourapp.com/healthz
# => {"ok":true}
`}</DocsCode>

          <h3>3. Point the web app at it</h3>
          <p>
            Set <code>PUSH_RELAY_URL</code> on the web deployment and
            redeploy. From then on, issue events that target a user with a
            registered device send a push.
          </p>
          <DocsCode language="env">{`
PUSH_RELAY_URL=https://push.yourapp.com
`}</DocsCode>

          <DocsCallout kind="warn" title="Open by default">
            The reference relay leaves <code>/send</code> unauthenticated so
            anything inside your trust boundary can call it. If you expose
            the relay to the public internet, restrict it by network policy
            or re-add the bearer-token middleware in{` `}
            <code>apps/push-relay/src/index.ts</code>.
          </DocsCallout>
        </DocsSection>

        <DocsSection id="integrations" num="03" label="Integrations">
          <h2>Integrations</h2>
          <p>
            Integrations are opt-in and per-user. Each one is configured
            with a couple of env vars, then linked from the user's account
            page.
          </p>

          <h3>Google Calendar</h3>
          <p>
            One-way sync from issues to a user's primary calendar. Issues
            with a <strong>due date</strong> and a non-closed status appear
            as all-day events; they're updated when the issue changes and
            removed when it's done, cancelled, or archived. Failures are
            logged on the issue but never block your work.
          </p>
          <p>
            To enable it, set up a Google OAuth client and add four env
            vars. Each user then links their own Google account from{` `}
            <code>/account/integrations</code>.
          </p>
          <DocsCode language="env">{`
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
GOOGLE_CALENDAR_ENABLED=true
# optional — adds a "Sign in with Google" button
GOOGLE_LOGIN_ENABLED=true
`}</DocsCode>

          <h3>MCP (Model Context Protocol)</h3>
          <p>
            Exponential exposes an MCP endpoint at <code>/api/mcp</code> so
            an AI agent like Claude can list, create, and update issues,
            projects, and labels — scoped to a specific user's workspaces.
            Useful for inbox triage, weekly digests, or letting Claude
            actually do the work it talks about.
          </p>
          <DocsCode language="env">{`
MCP_API_TOKEN=<a long random string>
MCP_USER_EMAIL=you@example.com
`}</DocsCode>
          <p>
            Point any MCP-aware client at <code>https://yourapp.com/api/mcp</code>{` `}
            with that bearer token; tool calls execute as the email you
            chose.
          </p>
        </DocsSection>

        <DocsSection id="mobile" num="04" label="Mobile apps">
          <h2>Mobile apps</h2>
          <p>
            Native mobile clients connect to the same instance you just set
            up. Both apps support the core flow — read, create, edit,
            comment, react to live updates — and respect the same auth and
            workspace scoping as the web app.
          </p>

          <h3>Android</h3>
          <p>
            Built with Kotlin and Jetpack Compose. The source lives in{` `}
            <code>apps/android/</code>.
          </p>
          <DocsCode language="shell">{`
bun run android:build     # debug APK
bun run android:install   # install on a connected device
`}</DocsCode>
          <p>
            Tagging the repo with <code>android-vX.Y.Z</code> kicks off a CI
            build that produces a debug + unsigned release APK as workflow
            artifacts. For Play Store distribution, add a signing config and
            keystore to <code>app/build.gradle.kts</code>.
          </p>

          <h3>iOS</h3>
          <p>
            A SwiftUI client with feature parity for the create / edit /
            list flow. It's in active development and isn't on the release
            track yet.
          </p>

          <DocsCallout kind="tip" title="Push needs the relay">
            Both mobile apps deliver push notifications via the push-relay
            service above. If you skipped that step, the apps still work —
            they just won't ping you when something changes while they're
            backgrounded.
          </DocsCallout>
        </DocsSection>
      </DocsLayout>

      <SiteFooter />
    </>
  )
}
