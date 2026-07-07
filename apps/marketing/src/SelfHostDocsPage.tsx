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
  { id: `github-app`, num: `02`, label: `GitHub App` },
  { id: `push`, num: `03`, label: `Push notifications` },
  { id: `environment`, num: `04`, label: `Environment variables` },
  { id: `updating`, num: `05`, label: `Updating` },
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
            <code>Caddyfile</code> is gitignored but the compose file
            bind-mounts it, so copy the example first — it ships the long-poll
            timeouts Electric needs. <code>storage:init</code> prints the S3
            keys you&apos;ll need in the next step.
          </p>
          <DocsCode language="shell">{`
bun install
cp Caddyfile.example Caddyfile
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
            projects requires a configured GitHub App — the next section
            walks through creating one.
          </DocsCallout>

          <h3>Connect the apps</h3>
          <p>
            All native clients — iOS, Android, macOS, Windows, Linux — work
            with self-hosted instances: on first launch, enter your instance
            URL instead of <code>app.exponential.at</code> and sign in.
          </p>

          <h3>Going live</h3>
          <p>
            Build the web image with the root <code>Dockerfile</code> and run
            it behind the Caddy from step 3 — the <code>Caddyfile</code> you
            copied proxies <code>host.docker.internal:5173</code>, so serve
            the app on port 5173:
          </p>
          <DocsCode language="shell">{`
docker build -f Dockerfile -t exponential-web:latest .
docker run -d --name exponential-web \\
  --network host \\
  --env-file .env \\
  -e PORT=5173 \\
  -e AUTH_SIGNUP_ENABLED=true \\
  exponential-web:latest
`}</DocsCode>
          <p>
            The image applies pending migrations on boot and listens on{` `}
            <code>PORT</code>; <code>--network host</code> keeps the{` `}
            <code>localhost</code> URLs in your <code>.env</code> working from
            inside the container.
          </p>
          <DocsCallout kind="warn" title="Sign-up is off in production by default">
            The production image runs with <code>NODE_ENV=production</code>,
            which disables password registration unless{` `}
            <code>AUTH_SIGNUP_ENABLED=true</code> is set — without it (or an
            OAuth/OIDC provider) nobody can register on your instance. Drop
            the flag later to close public sign-up again.
          </DocsCallout>
        </DocsSection>

        {/* ── 02 GitHub App ── */}
        <DocsSection id="github-app" num="02" label="GitHub App">
          <h2>GitHub App</h2>
          <p>
            Every project is backed by exactly one GitHub repository, so a
            GitHub App is a <strong>hard prerequisite</strong> for creating
            projects. The server uses it to mint short-lived per-repo
            installation tokens — no personal access tokens, no stored user
            OAuth tokens.
          </p>

          <h3>1. Create the App</h3>
          <p>
            Go to{` `}
            <a href="https://github.com/settings/apps/new">
              github.com/settings/apps/new
            </a>{` `}
            (or your org&apos;s equivalent). Set the homepage URL to your
            instance and the <strong>Setup URL</strong> to{` `}
            <code>{`\${BETTER_AUTH_URL}/api/integrations/github/setup`}</code>{` `}
            — GitHub redirects there after each install.
          </p>

          <h3>2. Permissions &amp; events</h3>
          <p>
            Repository permissions: <strong>Contents — Read &amp; write</strong>{` `}
            and <strong>Pull requests — Read &amp; write</strong> (Metadata —
            Read is added automatically). Subscribe to the{` `}
            <strong>Installation</strong> and <strong>Pull request</strong>{` `}
            webhook events; the webhook URL is{` `}
            <code>{`\${BETTER_AUTH_URL}/api/webhooks/github`}</code> with a
            secret of your choosing (goes into{` `}
            <code>GITHUB_WEBHOOK_SECRET</code>).
          </p>
          <DocsCallout kind="note" title="Server behind NAT?">
            If GitHub can&apos;t reach your webhook URL, set{` `}
            <code>GITHUB_POLLING=true</code> instead — the server polls for PR
            merges rather than waiting for webhooks.
          </DocsCallout>

          <h3>3. Wire the env vars</h3>
          <p>
            Generate a private key on the App page, then base64-encode it into
            a single line:
          </p>
          <DocsCode language="shell">{`
base64 -w0 your-app.private-key.pem   # macOS: base64 -i your-app.private-key.pem
`}</DocsCode>
          <DocsCode language="env">{`
GITHUB_APP_ID=123456                  # the App's numeric ID
GITHUB_APP_SLUG=your-app-slug         # from the App's URL — builds the install link
GITHUB_APP_PRIVATE_KEY=<base64 PEM>
GITHUB_WEBHOOK_SECRET=<webhook secret>
`}</DocsCode>

          <h3>4. Install it</h3>
          <p>
            Restart the app, then connect a repository from{` `}
            <strong>workspace settings → Repositories</strong> — the connect
            dialog sends you to the App&apos;s install page on GitHub and
            picks up the installation when you land back.
          </p>
        </DocsSection>

        {/* ── 03 Push notifications ── */}
        <DocsSection id="push" num="03" label="Push notifications">
          <h2>Push notifications</h2>
          <p>
            Native push goes through a small companion service, the{` `}
            <strong>push-relay</strong>, that wraps Firebase Cloud Messaging.
          </p>

          <p>
            A relay authenticates senders: when its{` `}
            <code>PUSH_RELAY_SECRET</code> is set, it rejects any{` `}
            <code>/send</code> request whose <code>x-relay-secret</code> header
            doesn&apos;t match. The public relay at{` `}
            <code>https://push.exponential.at</code> serves the official cloud
            and mobile builds and its secret is not published — a self-hosted
            instance pointing at it gets <code>401</code>s, so run your own.
          </p>

          <h3>Run your own relay</h3>
          <p>
            Host the relay with your own Firebase project — you&apos;ll also
            need to build the mobile apps with your own FCM credentials, since
            the published binaries are wired to the public relay.
          </p>
          <DocsCode language="shell">{`
docker build -f Dockerfile.push-relay -t push-relay:latest .
docker run -d \\
  -p 4001:4001 \\
  -e FIREBASE_SERVICE_ACCOUNT_JSON='<single-line JSON>' \\
  -e PUSH_RELAY_SECRET='<shared secret>' \\
  push-relay:latest

# verify
curl https://push.yourapp.com/healthz   # => {"ok":true}
`}</DocsCode>
          <p>Then point the web app at it with the same secret:</p>
          <DocsCode language="env">{`
PUSH_RELAY_URL=https://push.yourapp.com
PUSH_RELAY_SECRET=<shared secret>
`}</DocsCode>
          <DocsCallout kind="warn" title="Set the secret on both sides">
            Without <code>PUSH_RELAY_SECRET</code> the relay accepts
            unauthenticated <code>/send</code> requests — fine on a private
            network, not on the open internet. Set the same value on the relay
            process and the web app; the web app sends it as the{` `}
            <code>x-relay-secret</code> header.
          </DocsCallout>
          <DocsCallout kind="note" title="What a relay sees">
            The FCM device token, the notification title/body, and the data
            payload (typically an issue ID). Never your database, auth state,
            or credentials.
          </DocsCallout>
        </DocsSection>

        {/* ── 04 Environment variables ── */}
        <DocsSection id="environment" num="04" label="Environment variables">
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
            <EnvVar name="AUTH_SIGNUP_ENABLED">
              Allow public password sign-up. Defaults to on in dev but{` `}
              <strong>off</strong> when <code>NODE_ENV=production</code> — set{` `}
              <code>true</code> or nobody can register on your instance.
            </EnvVar>
            <EnvVar name="INITIAL_ADMIN_EMAILS">
              Comma-separated emails auto-promoted to instance admin at
              startup.
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
            <EnvVar name="PUSH_RELAY_SECRET">
              Shared secret between the web app and the relay (sent as the{` `}
              <code>x-relay-secret</code> header) — must match the relay
              process&apos;s env.
            </EnvVar>
            <EnvVar name="FEEDBACK_WIDGET_SCRIPT_URL">
              Loader URL of the cloud feedback widget — powers the in-app
              &quot;Send feedback&quot; button on self-hosted instances.
            </EnvVar>
            <EnvVar name="FEEDBACK_WIDGET_KEY">
              <code>expw_</code> key of the cloud feedback widget config
              (pairs with the script URL above).
            </EnvVar>
          </dl>
        </DocsSection>

        {/* ── 05 Updating ── */}
        <DocsSection id="updating" num="05" label="Updating">
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
          <p>
            The compose command restarts the backend services (Postgres,
            Electric, Garage, Caddy); the web app is a separate container, so
            recreate it from the freshly built image.
          </p>
          <DocsCode language="shell">{`
docker compose down && docker compose up -d
docker rm -f exponential-web
# then re-run the \`docker run\` from "Going live"
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
