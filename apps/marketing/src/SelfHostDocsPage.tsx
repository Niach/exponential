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
import { LINKS } from "./lib/links"

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
        <div className="shell docs-hero-content">
          <h1>Self-host</h1>
          <p>Docker Compose on your own server. No SaaS dependencies, no limits.</p>
          <div className="docs-hero-cta">
            <a className="btn btn-primary" href="#installation">
              <IcDocker size={14} /> Install
            </a>
            <a className="btn btn-ghost" href={LINKS.github.repo}>
              <IcGithub size={14} /> View source
            </a>
          </div>
        </div>
      </section>

      <DocsLayout sections={SECTIONS}>
        {/* ── 01 Installation ── */}
        <DocsSection id="installation" num="01" label="Installation">
          <h2>Installation</h2>

          <p>
            One <code>docker compose</code> file, four services: Postgres,
            Electric (real-time sync), Garage (S3-compatible attachment
            storage), and Caddy (reverse proxy). Set{` `}
            <code>SELF_HOSTED=true</code> and every plan limit disappears —
            billing is disabled entirely.
          </p>

          <DocsCallout kind="tip" title="Just want to use Exponential?">
            Sign up free at{` `}
            <a href="https://app.exponential.at">app.exponential.at</a> — no
            install needed.
          </DocsCallout>

          <h3>1. Clone the repo</h3>
          <DocsCode language="shell">{`
git clone https://github.com/Niach/exponential
cd exponential
`}</DocsCode>

          <h3>2. Pick your sign-in method</h3>
          <p>
            Email &amp; password is on by default (<code>AUTH_PASSWORD_ENABLED=false</code>{` `}
            turns it off). For OIDC (Authentik, Keycloak, Zitadel, …),
            configure providers as a JSON array:
          </p>
          <DocsCode language="env">{`
OIDC_PROVIDERS='[{"id":"authentik","name":"Authentik","clientId":"...","clientSecret":"...","discoveryUrl":"https://auth.example.com/application/o/app/.well-known/openid-configuration"}]'
`}</DocsCode>
          <p>
            The redirect URI in your IdP is{` `}
            <code>{`\${BETTER_AUTH_URL}/api/auth/oauth2/callback/<id>`}</code>.
            Google sign-in works too:
          </p>
          <DocsCode language="env">{`
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
GOOGLE_LOGIN_ENABLED=true
`}</DocsCode>

          <h3>3. Bring the stack up</h3>
          <p>
            <code>storage:init</code> prints the S3 keys you&apos;ll need in
            the next step.
          </p>
          <DocsCode language="shell">{`
bun install
bun run backend:up
bun run storage:init
`}</DocsCode>

          <h3>4. Configure</h3>
          <p>
            Copy the example env file and fill in the database URL, a session
            secret, and the S3 keys from the previous step.
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
            Open <code>http://localhost:5173</code> and register the first
            user — that&apos;s your instance.
          </p>

          <DocsCallout kind="note" title="Projects need a GitHub App">
            Every project is backed by a GitHub repository, so creating
            projects requires a configured GitHub App — see the{` `}
            <code>GITHUB_APP_*</code> variables below.
          </DocsCallout>

          <h3>Connect the apps</h3>
          <p>
            All native clients — iOS, Android, macOS, Linux — work with
            self-hosted instances: on first launch, enter your instance URL
            instead of <code>app.exponential.at</code> and sign in.
          </p>

          <h3>Going live</h3>
          <p>
            Build the web image with the root <code>Dockerfile</code> and put
            it behind Caddy — <code>Caddyfile.example</code> ships with the
            long-poll timeouts Electric needs.
          </p>
          <DocsCode language="shell">{`
docker build -f Dockerfile -t exponential-web:latest .
`}</DocsCode>
        </DocsSection>

        {/* ── 02 Push notifications ── */}
        <DocsSection id="push" num="02" label="Push notifications">
          <h2>Push notifications</h2>
          <p>
            Native push goes through a small companion service, the{` `}
            <strong>push-relay</strong>, that wraps Firebase Cloud Messaging.
          </p>

          <h3>Use the public relay</h3>
          <p>
            The official mobile builds talk to the public relay at{` `}
            <code>https://push.exponential.at</code>. Point your deployment at
            it and you&apos;re done — no Firebase project, no extra container.
          </p>
          <DocsCode language="env">{`
PUSH_RELAY_URL=https://push.exponential.at
`}</DocsCode>
          <DocsCallout kind="note" title="What the public relay sees">
            The FCM device token, the notification title/body, and the data
            payload (typically an issue ID). Never your database, auth state,
            or credentials.
          </DocsCallout>

          <h3>Or run your own</h3>
          <p>
            Host the relay yourself with your own Firebase project — you&apos;ll
            also need to build the mobile apps with your own FCM credentials,
            since the published binaries are wired to the public relay.
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
          <DocsCode language="env">{`
PUSH_RELAY_URL=https://push.yourapp.com
`}</DocsCode>
          <DocsCallout kind="warn" title="Open by default">
            The reference relay leaves <code>/send</code> unauthenticated. If
            you expose it publicly, restrict it by network policy or re-add
            the bearer-token middleware in{` `}
            <code>apps/push-relay/src/index.ts</code>.
          </DocsCallout>
        </DocsSection>

        {/* ── 03 Environment variables ── */}
        <DocsSection id="environment" num="03" label="Environment variables">
          <h2>Environment variables</h2>
          <p>
            Only three are strictly required — the rest have sensible
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
            <EnvVar name="S3_ENDPOINT">S3-compatible storage URL.</EnvVar>
            <EnvVar name="S3_ACCESS_KEY">S3 access key.</EnvVar>
            <EnvVar name="S3_SECRET_KEY">S3 secret key.</EnvVar>
            <EnvVar name="S3_BUCKET">
              Attachment bucket name (default: <code>exponential-attachments</code>).
            </EnvVar>
            <EnvVar name="S3_REGION">
              S3 region (default: <code>garage</code>).
            </EnvVar>
            <EnvVar name="SELF_HOSTED">
              Set to <code>true</code> to disable billing and unlock all plan
              limits.
            </EnvVar>
            <EnvVar name="AUTH_PASSWORD_ENABLED">
              Enable email/password login (default: <code>true</code>).
            </EnvVar>
            <EnvVar name="OIDC_PROVIDERS">
              JSON array of OIDC provider configs.
            </EnvVar>
            <EnvVar name="GOOGLE_CLIENT_ID">Google OAuth client ID.</EnvVar>
            <EnvVar name="GOOGLE_CLIENT_SECRET">
              Google OAuth client secret.
            </EnvVar>
            <EnvVar name="GOOGLE_LOGIN_ENABLED">
              Show Google sign-in button (default: <code>false</code>).
            </EnvVar>
            <EnvVar name="GITHUB_APP_ID">
              GitHub App numeric ID — required to connect repositories and
              create projects.
            </EnvVar>
            <EnvVar name="GITHUB_APP_SLUG">
              GitHub App URL slug (builds the install link).
            </EnvVar>
            <EnvVar name="GITHUB_APP_PRIVATE_KEY">
              GitHub App PEM private key, base64-encoded.
            </EnvVar>
            <EnvVar name="GITHUB_WEBHOOK_SECRET">
              GitHub App webhook secret (PR-merge detection via webhook).
            </EnvVar>
            <EnvVar name="GITHUB_POLLING">
              Set to <code>true</code> to poll for PR merges instead — for
              servers behind NAT that webhooks can&apos;t reach.
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
            Exponential rolls forward on <code>master</code>. To update:
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
            Always apply migrations before restarting the containers — the
            app may fail to start otherwise.
          </DocsCallout>
        </DocsSection>
      </DocsLayout>

      <SiteFooter />
    </>
  )
}
