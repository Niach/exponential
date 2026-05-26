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
  { id: `installation`, num: `01`, label: `Installation` },
  { id: `push`, num: `02`, label: `Push notifications` },
  { id: `environment`, num: `03`, label: `Environment variables` },
  { id: `updating`, num: `04`, label: `Updating` },
]

export function SelfHostDocsPage() {
  return (
    <>
      <SiteHeader />

      <section className="docs-hero">
        <div className="hero-grid" />
        <div className="shell docs-hero-content fade-in">
          <span className="tag-row">
            <span className="tag-pill">Self-host docs</span>
          </span>
          <h1>
            Your infrastructure,
            <br />
            <em>your data.</em>
          </h1>
          <p>
            Install Exponential on your own server with Docker Compose.
            One command, four containers, no SaaS dependencies.
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
        {/* ── 01 Installation ── */}
        <DocsSection id="installation" num="01" label="Installation">
          <h2>Installation</h2>

          <DocsCallout kind="tip" title="Just want to use Exponential?">
            Sign up free at{` `}
            <a href="https://app.exponential.at">app.exponential.at</a> — no
            install needed. Check the{` `}
            <a href="/docs/">usage guide</a> for how things work.
          </DocsCallout>

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

          <h3>2. Pick your sign-in method</h3>
          <p>
            Three options, and they can be combined. Email and password is
            on by default, so you can skip this step and come back to it
            later if you just want to kick the tyres.
          </p>
          <p>
            <strong>Email &amp; password.</strong> Enabled out of the box. To
            turn it off later, set <code>AUTH_PASSWORD_ENABLED=false</code>.
          </p>
          <p>
            <strong>OIDC (Authentik, Keycloak, Zitadel, …).</strong> Configure
            one or more providers as a JSON array. Each entry needs an{` `}
            <code>id</code>, <code>name</code>, your client credentials, and
            the provider's discovery URL.
          </p>
          <DocsCode language="env">{`
OIDC_PROVIDERS='[{"id":"authentik","name":"Authentik","clientId":"...","clientSecret":"...","discoveryUrl":"https://auth.example.com/application/o/app/.well-known/openid-configuration"}]'
`}</DocsCode>
          <p>
            The authorized redirect URI in your IdP is{` `}
            <code>{`\${BETTER_AUTH_URL}/api/auth/oauth2/callback/<id>`}</code>.
            Optional <code>adminGroups</code> + <code>groupsClaim</code>{` `}
            promote users to admin based on a claim from the ID token.
          </p>
          <p>
            <strong>Google sign-in.</strong> Same Google OAuth client you'd
            use for the Calendar integration. Setting both client vars and
            flipping the flag adds a "Sign in with Google" button to the
            login screen.
          </p>
          <DocsCode language="env">{`
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
GOOGLE_LOGIN_ENABLED=true
`}</DocsCode>

          <h3>3. Bring the stack up</h3>
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

          <h3>4. Configure</h3>
          <p>
            Copy the example env file and fill in the three values you
            actually need: the database URL (default works for local), a
            session secret, and the S3 keys from the previous step. Add the
            sign-in env vars from step 2 here too.
          </p>
          <DocsCode language="shell">{`
cp apps/web/.env.example .env
# generate a 32-character session secret
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
        </DocsSection>

        {/* ── 02 Push notifications ── */}
        <DocsSection id="push" num="02" label="Push notifications">
          <h2>Push notifications</h2>
          <p>
            Native push to phones is handled by a small companion service,
            the <strong>push-relay</strong>, that wraps Firebase Cloud
            Messaging. The web app posts payloads to it; the relay
            multicasts to your devices.
          </p>

          <h3>Use the public relay</h3>
          <p>
            A public instance runs at{` `}
            <code>https://push.exponential.at</code> and is what
            the official Android and iOS builds talk to by default. The
            quickest path is to point your web deployment at it and you're
            done — no Firebase project, no extra container.
          </p>
          <DocsCode language="env">{`
PUSH_RELAY_URL=https://push.exponential.at
`}</DocsCode>
          <DocsCallout kind="note" title="What the public relay sees">
            The relay receives the FCM device token, the notification title
            and body, and the data payload — typically just an issue ID. It
            never sees your database, your auth state, or your users'
            credentials, and it forwards straight to Google's FCM.
          </DocsCallout>

          <h3>Or run your own</h3>
          <p>
            If you'd rather not depend on the public relay — different trust
            model, different Firebase project, your own region — host one
            yourself. The image is in the repo. You'll also need to{` `}
            <strong>build the mobile apps yourself with your own FCM
            credentials</strong>; the published Android/iOS binaries are
            wired to the public Firebase project and won't deliver
            notifications through your relay.
          </p>
          <p>
            Create a Firebase project, generate a service-account key from
            its settings, then run the relay:
          </p>
          <DocsCode language="shell">{`
docker build -f Dockerfile.push-relay -t push-relay:latest .
docker run -d \\
  -p 4001:4001 \\
  -e FIREBASE_SERVICE_ACCOUNT_JSON='<single-line JSON>' \\
  push-relay:latest

# verify
curl https://push.yourapp.com/healthz   # => {"ok":true}
`}</DocsCode>
          <p>
            Point the web app at your relay:
          </p>
          <DocsCode language="env">{`
PUSH_RELAY_URL=https://push.yourapp.com
`}</DocsCode>
          <DocsCallout kind="warn" title="Open by default">
            The reference relay leaves <code>/send</code> unauthenticated so
            anything inside your trust boundary can call it. If you expose
            it to the public internet, restrict it by network policy or
            re-add the bearer-token middleware in{` `}
            <code>apps/push-relay/src/index.ts</code>.
          </DocsCallout>
        </DocsSection>

        {/* ── 03 Environment variables ── */}
        <DocsSection id="environment" num="03" label="Environment variables">
          <h2>Environment variables</h2>
          <p>
            All configuration is via environment variables. Only three are
            strictly required for a minimal install — the rest have sensible
            defaults.
          </p>

          <dl className="docs-env-list">
            <EnvVar name="DATABASE_URL" required>
              Postgres connection string.
            </EnvVar>
            <EnvVar name="BETTER_AUTH_SECRET" required>
              32+ character secret for session signing.
            </EnvVar>
            <EnvVar name="BETTER_AUTH_URL" required>
              Base URL of your instance (e.g.{` `}
              <code>https://issues.yourcompany.com</code>).
            </EnvVar>
            <EnvVar name="BETTER_AUTH_TRUSTED_ORIGINS">
              Comma-separated allowed origins.
            </EnvVar>
            <EnvVar name="ELECTRIC_URL">
              Electric service URL (default: <code>http://localhost:30000</code>).
            </EnvVar>
            <EnvVar name="S3_ENDPOINT">
              S3-compatible storage URL.
            </EnvVar>
            <EnvVar name="S3_ACCESS_KEY">
              S3 access key.
            </EnvVar>
            <EnvVar name="S3_SECRET_KEY">
              S3 secret key.
            </EnvVar>
            <EnvVar name="S3_BUCKET">
              Attachment bucket name (default: <code>exponential-attachments</code>).
            </EnvVar>
            <EnvVar name="S3_REGION">
              S3 region (default: <code>garage</code>).
            </EnvVar>
            <EnvVar name="AUTH_PASSWORD_ENABLED">
              Enable email/password login (default: <code>true</code>).
            </EnvVar>
            <EnvVar name="OIDC_PROVIDERS">
              JSON array of OIDC provider configs.
            </EnvVar>
            <EnvVar name="GOOGLE_CLIENT_ID">
              Google OAuth client ID.
            </EnvVar>
            <EnvVar name="GOOGLE_CLIENT_SECRET">
              Google OAuth client secret.
            </EnvVar>
            <EnvVar name="GOOGLE_LOGIN_ENABLED">
              Show Google sign-in button (default: <code>false</code>).
            </EnvVar>
            <EnvVar name="GOOGLE_CALENDAR_ENABLED">
              Enable Calendar integration (default: <code>false</code>).
            </EnvVar>
            <EnvVar name="PUSH_RELAY_URL">
              Push notification relay URL.
            </EnvVar>
          </dl>
        </DocsSection>

        {/* ── 04 Updating ── */}
        <DocsSection id="updating" num="04" label="Updating">
          <h2>Updating</h2>
          <p>
            Exponential follows a rolling-release model on the{` `}
            <code>master</code> branch. To update a self-hosted install:
          </p>

          <h3>1. Pull the latest code</h3>
          <DocsCode language="shell">{`
git pull origin master
`}</DocsCode>

          <h3>2. Rebuild the image</h3>
          <DocsCode language="shell">{`
docker build -f Dockerfile -t exponential-web:latest .
`}</DocsCode>

          <h3>3. Run migrations</h3>
          <p>
            Always apply migrations <strong>before</strong> restarting the
            containers — schema changes are not backwards compatible.
          </p>
          <DocsCode language="shell">{`
bun run migrate
docker exec -i exponential-postgres-1 \\
  psql -U postgres -d exponential \\
  < apps/web/src/db/out/custom/0001_triggers.sql
`}</DocsCode>

          <h3>4. Restart</h3>
          <DocsCode language="shell">{`
docker compose down && docker compose up -d
`}</DocsCode>

          <DocsCallout kind="warn" title="Migrations first">
            If you restart containers before running migrations, the app may
            fail to start or behave unexpectedly. Always migrate first.
          </DocsCallout>
        </DocsSection>
      </DocsLayout>

      <SiteFooter />
    </>
  )
}
