import { useRef, useState } from "react"
import {
  DocsCallout,
  DocsCode,
  DocsLayout,
  DocsSection,
  EnvVar,
  type DocsSection as DocsSectionType,
} from "./components/DocsLayout"
import { SiteFooter, SiteHeader } from "./components/SiteShell"
import { IcCheck, IcCopy, IcDocker, IcGithub } from "./components/icons"
import { LINKS } from "./lib/links"

const SECTIONS: DocsSectionType[] = [
  { id: `installation`, num: `01`, label: `Installation` },
  { id: `storage`, num: `02`, label: `S3 storage` },
  { id: `github-app`, num: `03`, label: `GitHub App` },
  { id: `push`, num: `04`, label: `Push notifications` },
  { id: `steer`, num: `05`, label: `Steer relay` },
  { id: `email`, num: `06`, label: `Email` },
  { id: `environment`, num: `07`, label: `Environment variables` },
  { id: `updating`, num: `08`, label: `Updating` },
  { id: `licensing`, num: `09`, label: `Licensing` },
]

/* The prompt the "Copy prompt for your agent" button puts on the clipboard —
   INSTALL.md (repo root) is written as an agent-followable runbook, so the
   prompt just points an agent at it and names the decisions it should ask
   about instead of guessing. */
const AGENT_PROMPT = `Install Exponential (a self-hosted issue tracker) on this machine.

Fetch https://raw.githubusercontent.com/Niach/exponential/master/INSTALL.md and follow it end-to-end: download the two selfhost files, fill in .env, bring the stack up with Docker Compose, and verify /api/health responds. Ask me for the decisions the runbook marks [decision] — which S3-compatible storage to use (and its credentials), and whether/which domain to go live on — instead of guessing.`

function CopyPromptButton() {
  const [copied, setCopied] = useState(false)
  const timeout = useRef<ReturnType<typeof setTimeout> | null>(null)
  const onCopy = () => {
    if (typeof navigator === `undefined` || !navigator.clipboard) return
    navigator.clipboard.writeText(AGENT_PROMPT)
    setCopied(true)
    if (timeout.current) clearTimeout(timeout.current)
    timeout.current = setTimeout(() => setCopied(false), 1400)
  }
  return (
    <button type="button" className="btn btn-primary" onClick={onCopy}>
      {copied ? <IcCheck size={14} /> : <IcCopy size={14} />}
      {copied ? `Copied — paste it to your agent` : `Copy prompt for your agent`}
    </button>
  )
}

export function SelfHostDocsPage() {
  return (
    <>
      <SiteHeader />

      <main>
        <section className="docs-hero">
          <div className="shell docs-hero-content">
            <h1>Self-host</h1>
            <p>
              Two files and a <code>docker compose up</code> — pulling the
              published image, no checkout, no build. No plan limits. Free for
              everyone, open source under Apache-2.0. Or skip reading
              entirely: copy the prompt and let your coding agent do the
              install.
            </p>
            <div className="docs-hero-cta">
              <CopyPromptButton />
              <a className="btn btn-ghost" href="#installation">
                <IcDocker size={14} /> Install by hand
              </a>
              <a className="btn btn-ghost" href={LINKS.github.repo}>
                <IcGithub size={14} /> View source
              </a>
            </div>
          </div>
        </section>

        <DocsLayout sections={SECTIONS} currentPath="/docs/self-host/">
          {/* ── 01 Installation ── */}
          <DocsSection id="installation" num="01" label="Installation">
            <h2>Installation</h2>

            <p>
              One <code>docker compose</code> file pulling published images:
              the web app (
              <code>ghcr.io/niach/exponential-web</code>, amd64 + arm64),
              Postgres, Electric (real-time sync), and Caddy (reverse proxy +
              automatic HTTPS) — plus the optional <a href="#steer">steer
              relay</a> behind a compose profile. You bring an{` `}
              <a href="#storage">S3-compatible bucket</a> for attachments. The
              stack runs in self-hosted mode by default (nothing to set), so
              every plan limit disappears — seats, storage, widgets — and
              billing is disabled entirely. It&apos;s free for any team size — open
              source under Apache-2.0, see <a href="#licensing">Licensing</a>.
            </p>

            <DocsCallout kind="tip" title="Just want to use Exponential?">
              Sign up free at{` `}
              <a href="https://app.exponential.at">app.exponential.at</a> — no
              install needed.
            </DocsCallout>

            <p>
              The step-by-step below also lives as{` `}
              <a href={`${LINKS.github.repo}/blob/master/INSTALL.md`}>
                INSTALL.md
              </a>
              {` `}
              in the repo — written so a coding agent can follow it
              end-to-end. The &quot;Copy prompt for your agent&quot; button
              above hands it to whatever agent you use.
            </p>

            <h3>1. Get the two files</h3>
            <p>
              Prerequisites: Docker Engine with Compose ≥ 2.23.1 (
              <code>docker compose version</code>), free ports 80/443, and an
              S3 bucket + key (<a href="#storage">next section</a>).
            </p>
            <DocsCode language="shell">{`
mkdir exponential && cd exponential
curl -fsSLO https://raw.githubusercontent.com/Niach/exponential/master/selfhost/docker-compose.yaml
curl -fsSL https://raw.githubusercontent.com/Niach/exponential/master/selfhost/.env.example -o .env
`}</DocsCode>

            <h3>2. Fill in .env</h3>
            <p>
              Two generated secrets plus your S3 credentials — the file is
              short and every entry is commented:
            </p>
            <DocsCode language="shell">{`
sed -i "s/^POSTGRES_PASSWORD=$/POSTGRES_PASSWORD=$(openssl rand -hex 32)/" .env
sed -i "s/^BETTER_AUTH_SECRET=$/BETTER_AUTH_SECRET=$(openssl rand -hex 32)/" .env
# then set S3_ENDPOINT / S3_ACCESS_KEY / S3_SECRET_KEY / S3_BUCKET / S3_REGION
`}</DocsCode>

            <h3>3. Up</h3>
            <DocsCode language="shell">{`
docker compose up -d
curl -fsS http://localhost/api/health   # => {"ok":true,"db":true,...}
`}</DocsCode>
            <p>
              Open <code>http://localhost</code> and register the first user —
              that&apos;s your instance. Migrations <em>and</em> the custom
              trigger SQL apply themselves at every boot; there are no manual
              SQL steps, on install or on any update.
            </p>

            <h3>4. Go live on a domain</h3>
            <p>
              Point DNS at the host, then set <strong>both</strong> values in
              {` `}
              <code>.env</code> and <code>docker compose up -d</code> again —
              Caddy provisions Let&apos;s Encrypt certificates automatically:
            </p>
            <DocsCode language="env">{`
DOMAIN=issues.example.com
APP_URL=https://issues.example.com
`}</DocsCode>
            <DocsCallout kind="warn" title="Set both, identically">
              <code>DOMAIN</code> is what Caddy serves; <code>APP_URL</code>
              {` `}
              is the origin the app runs auth against. If they disagree
              (scheme included), sign-in breaks with origin errors.
            </DocsCallout>
            <DocsCallout
              kind="warn"
              title="Sign-up is off in production by default"
            >
              The image runs with <code>NODE_ENV=production</code>, which
              disables password registration unless{` `}
              <code>AUTH_SIGNUP_ENABLED=true</code> is set — the shipped
              compose defaults it to <code>true</code> so your first account
              works; set it to <code>false</code> in <code>.env</code> once
              your accounts exist, and new teammates join via invite links.
            </DocsCallout>

            <h3>Connect the apps</h3>
            <p>
              All native clients — iOS, Android, macOS, Windows, Linux — work
              with self-hosted instances: on first launch, enter your instance
              URL instead of <code>app.exponential.at</code> and sign in.
            </p>

            <DocsCallout kind="note" title="GitHub App — only for coding">
              Boards work out of the box. Only coding — backing a board with a
              GitHub repository for coding sessions and PRs — needs a
              configured GitHub App; <a href="#github-app">section 03</a>
              {` `}
              walks through creating one, and you can skip it if you just want
              issue tracking.
            </DocsCallout>

            <p>
              Prefer running from a checkout (dev server, hacking on the
              code)? That&apos;s the{` `}
              <a href={`${LINKS.github.repo}#development`}>
                Development section of the README
              </a>
              {` `}
              — the compose file above is only for running released images.
            </p>
          </DocsSection>

          {/* ── 02 S3 storage ── */}
          <DocsSection id="storage" num="02" label="S3 storage">
            <h2>S3 storage</h2>
            <p>
              Attachments and widget screenshots live in an S3-compatible
              bucket — the one external dependency you bring. <strong>Any
              provider works</strong>: Hetzner Object Storage, MinIO,
              Cloudflare R2, AWS S3, Garage, … The app talks to it with
              path-style addressing (<code>forcePathStyle</code>) and streams
              all attachment traffic server-side, so the endpoint never needs
              to be reachable by browsers — a LAN-only MinIO is fine. The
              bucket is created automatically at first use when the key has
              create permission; otherwise create it up front.
            </p>
            <DocsCode language="env">{`
# examples — one provider's block, into .env
S3_ENDPOINT=https://nbg1.your-objectstorage.com   # Hetzner
S3_REGION=nbg1
# S3_ENDPOINT=https://<account-id>.r2.cloudflarestorage.com   # Cloudflare R2
# S3_REGION=auto
# S3_ENDPOINT=http://minio.lan:9000                # MinIO on your network
# S3_REGION=us-east-1
S3_ACCESS_KEY=...
S3_SECRET_KEY=...
S3_BUCKET=exponential-attachments
`}</DocsCode>
            <p>
              Verify with the app itself: paste an image into any issue
              description — if it renders back, the credentials are right (
              <code>docker compose logs web</code> shows the S3 error
              otherwise).
            </p>
            <DocsCallout kind="tip" title="Want local S3?">
              <a href="https://garagehq.deuxfleurs.fr">Garage</a> is a great
              single-binary S3 server (it&apos;s what Exponential&apos;s dev
              stack uses), MinIO the classic. Run either next to the stack,
              point <code>S3_ENDPOINT</code> at it, and your attachments never
              leave the machine.
            </DocsCallout>
          </DocsSection>

          {/* ── 03 GitHub App ── */}
          <DocsSection id="github-app" num="03" label="GitHub App">
            <h2>GitHub App</h2>
            <p>
              Coding runs against a GitHub repository, so a GitHub App is a{" "}
              <strong>prerequisite for coding sessions and PRs</strong>
              {` `}
              (repo-less boards need none). The server uses it to mint
              short-lived per-repo installation tokens — no personal access
              tokens, no stored user OAuth tokens.
            </p>

            <h3>1. Create the App</h3>
            <p>
              Go to{` `}
              <a href="https://github.com/settings/apps/new">
                github.com/settings/apps/new
              </a>
              {` `}
              (or your org&apos;s equivalent). Set the homepage URL to your
              instance and the <strong>Setup URL</strong> to{` `}
              <code>{`\${BETTER_AUTH_URL}/api/integrations/github/setup`}</code>
              {` `}
              with <strong>&quot;Redirect on update&quot;</strong> ticked —
              GitHub redirects there after each install or repo-access change.
              Set the OAuth <strong>Callback URL</strong> to{` `}
              <code>{`\${BETTER_AUTH_URL}/api/integrations/github/callback`}</code>
              {` `}— that&apos;s where the lightweight connect flow lands back.
            </p>

            <h3>2. Permissions &amp; events</h3>
            <p>
              Repository permissions:{" "}
              <strong>Contents — Read &amp; write</strong>
              {` `}
              and <strong>Pull requests — Read &amp; write</strong> (Metadata —
              Read is added automatically). Subscribe to the{` `}
              <strong>Pull request</strong> webhook event — installation and
              repo-selection events are delivered to GitHub Apps automatically
              once the webhook is active, so they never appear in the subscribe
              list. The webhook URL is{` `}
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
GITHUB_APP_CLIENT_ID=<oauth client id>          # optional — enables the lightweight connect flow
GITHUB_APP_CLIENT_SECRET=<oauth client secret>
`}</DocsCode>
            <p>
              <code>GITHUB_APP_CLIENT_ID</code> and{` `}
              <code>GITHUB_APP_CLIENT_SECRET</code> are the App&apos;s own OAuth
              credentials — the client ID is on the App page, and you generate
              the secret there too. They power the lightweight connect flow: the
              user token they mint is transient, used once to enumerate
              installations and then discarded, never stored. Leave them unset
              and connecting a repository falls back to the install-page
              round-trip.
            </p>

            <h3>4. Connect an account</h3>
            <p>
              Apply the env changes (<code>docker compose up -d</code>), then
              connect a GitHub account from{` `}
              <strong>Team settings → Repositories</strong>. With the OAuth
              credentials above configured, this opens a lightweight GitHub
              authorization — one consent screen, and if you manage several
              installations you pick which to connect from an in-app account
              picker. Without them it falls back to the install-page round-trip,
              which is also how you install the App on a new account or grant it
              access to more repositories.
            </p>
            <DocsCallout kind="note" title="If the App loses repo access">
              Drop a repo from the installation on GitHub and{` `}
              <strong>Team settings → Repositories</strong> flags it with a{` `}
              <strong>&quot;no access — re-grant on GitHub&quot;</strong> badge
              and a re-grant link; coding-session token minting fails with a
              clear message instead of handing out a broken token.
            </DocsCallout>
          </DocsSection>

          {/* ── 04 Push notifications ── */}
          <DocsSection id="push" num="04" label="Push notifications">
            <h2>Push notifications</h2>
            <p>
              The honest version first:{" "}
              <strong>
                mobile push does not work for self-hosted instances with the
                store apps
              </strong>
              . The iOS and Android apps from the App Store / Play Store are
              compiled against Exponential&apos;s first-party Firebase
              project, and only the cloud&apos;s push relay can reach their
              device tokens — a self-hosted relay cannot, by design.
            </p>
            <p>
              Self-hosted users still get web notifications, the email digest,
              and the desktop app&apos;s notifications — only mobile push is
              cloud-only.
            </p>

            <h3>Why a self-hosted relay can&apos;t serve the store apps</h3>
            <p>
              Native push goes through a small companion service, the{` `}
              <strong>push-relay</strong>, that wraps Firebase Cloud Messaging.
              FCM only accepts sends from the Firebase project an app was
              built against, and a relay always authenticates its senders: it
              refuses to start without a <code>PUSH_RELAY_SECRET</code> and
              rejects any <code>/send</code> whose{` `}
              <code>x-relay-secret</code> doesn&apos;t match. The public relay
              at <code>https://push.exponential.at</code> serves the official
              cloud and mobile builds; its secret is not published, so a
              self-hosted instance pointing at it just collects{` `}
              <code>401</code>s.
            </p>

            <h3>The escape hatch: build the apps yourself</h3>
            <p>
              If you build the mobile apps from source against{` `}
              <em>your own</em> Firebase project, your own relay serves them
              (build from a checkout — the relay image isn&apos;t published,
              precisely because this path is the exception):
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
            <DocsCallout kind="note" title="What a relay sees">
              The FCM device token, the notification title/body, and the data
              payload (typically an issue ID). Never your database, auth state,
              or credentials.
            </DocsCallout>
          </DocsSection>

          {/* ── 05 Steer relay ── */}
          <DocsSection id="steer" num="05" label="Steer relay">
            <h2>Steer relay</h2>
            <p>
              Starting a coding session on your desktop from the phone or the
              web app — and then watching and steering it live — goes through
              the <strong>steer-relay</strong>, a second companion service. It
              is a dumb pipe with auth: every connection dials{` `}
              <em>out</em> to it, so the desktop never needs an inbound port,
              and the relay itself holds device presence and session rooms in
              memory only. Leave it unconfigured and remote start plus live
              watch/steer are simply off — local coding sessions on the desktop
              are unaffected.
            </p>

            <h3>Run the relay</h3>
            <p>
              The self-host compose ships it behind an opt-in profile, pulling
              the published image (
              <code>ghcr.io/niach/exponential-steer-relay</code>). Set the two
              vars in <code>.env</code> — they configure the relay container
              and the web app at once — then bring the profile up:
            </p>
            <DocsCode language="env">{`
STEER_RELAY_URL=ws://your-host:4002   # what desktops + phones dial
STEER_RELAY_SECRET=<shared secret>    # openssl rand -hex 32
`}</DocsCode>
            <DocsCode language="shell">{`
docker compose --profile steer up -d

# verify
curl http://localhost:4002/healthz   # => {"ok":true,...}
`}</DocsCode>
            <p>
              To host it on its own box instead, run the same image there:
            </p>
            <DocsCode language="shell">{`
docker run -d \\
  -p 4002:4002 \\
  -e STEER_RELAY_SECRET='<shared secret>' \\
  ghcr.io/niach/exponential-steer-relay:latest
`}</DocsCode>
            <p>
              The relay is reached over WebSocket, so give{` `}
              <code>STEER_RELAY_URL</code> a <code>ws://</code> or{` `}
              <code>wss://</code> URL (an <code>http(s)://</code> one works too
              — the server converts it); the web app derives the HTTP origin
              from it for its own server-to-server calls. A LAN address is
              fine: every client dials out, so the relay never has to be
              reachable from the internet.
            </p>
            <DocsCallout kind="warn" title="Set the secret on both sides">
              <code>STEER_RELAY_SECRET</code> is the shared HS256 key: the web
              app signs the short-lived tickets clients present, the relay
              verifies them. Without it the relay answers <code>503</code> on
              everything but <code>/healthz</code>, and the web app treats the
              subsystem as off unless <strong>both</strong>{` `}
              <code>STEER_RELAY_URL</code> and <code>STEER_RELAY_SECRET</code>
              {` `}
              are set.
            </DocsCallout>
            <DocsCallout kind="note" title="Behind a reverse proxy?">
              Set <code>TRUST_PROXY=true</code> on the relay process whenever a
              proxy fronts it. The relay rate-limits WebSocket upgrades per
              client IP, and without this flag every connection keys to one
              shared bucket instead of the real address from{` `}
              <code>X-Forwarded-For</code>. Leave it off when the relay is
              exposed directly — forwarded headers from unknown peers are
              forgeable.
            </DocsCallout>
          </DocsSection>

          {/* ── 06 Email ── */}
          <DocsSection id="email" num="06" label="Email">
            <h2>Email</h2>
            <p>
              One sender handles all outgoing mail: password reset and address
              verification, invite emails, the notification digest, the
              helpdesk reporter magic links, and the contact form. With no
              transport configured every send is a logged no-op — nothing
              throws, the UI hides the affordances that depend on it (
              &quot;Forgot password?&quot;, the email-notification prefs), and
              in-app notifications keep working.
            </p>

            <h3>SMTP</h3>
            <p>
              The straightforward option for a self-hosted instance — any
              relay you already run. <code>SMTP_PORT</code> defaults to{` `}
              <code>587</code> (STARTTLS); set <code>SMTP_SECURE=true</code>
              {` `}
              for implicit TLS on port 465. <code>SMTP_USER</code> and{` `}
              <code>SMTP_PASS</code> are optional for unauthenticated relays.
            </p>
            <DocsCode language="env">{`
SMTP_HOST=smtp.yourcompany.com
SMTP_PORT=587
SMTP_USER=
SMTP_PASS=
SMTP_SECURE=false
EMAIL_FROM="Exponential <noreply@yourcompany.com>"
`}</DocsCode>

            <h3>Amazon SES</h3>
            <p>
              Setting <code>AWS_SES_REGION</code> switches the sender to the
              SESv2 API; credentials come from the standard AWS chain.{` `}
              <code>EMAIL_FROM</code> must be a verified identity in that
              region, and the account has to be out of the SES sandbox to mail
              arbitrary recipients.
            </p>
            <DocsCode language="env">{`
AWS_SES_REGION=eu-central-1
AWS_ACCESS_KEY_ID=<IAM user allowed ses:SendEmail>
AWS_SECRET_ACCESS_KEY=
EMAIL_FROM="Exponential <noreply@yourcompany.com>"
`}</DocsCode>
            <p>
              SES wins when both transports are configured.{` `}
              <code>EMAIL_REPLY_TO</code> sets a monitored default Reply-To on
              every message (individual sends may override it).
            </p>

            <DocsCallout kind="warn" title="The helpdesk needs a transport">
              A support reporter&apos;s only credential is the magic link
              emailed to them, so support mode on the feedback widget can&apos;t
              work without SMTP or SES. Password reset, address verification,
              emailed invites, and the notification digest are equally inert —
              they fail silently rather than erroring.
            </DocsCallout>
          </DocsSection>

          {/* ── 07 Environment variables ── */}
          <DocsSection id="environment" num="07" label="Environment variables">
            <h2>Environment variables</h2>
            <p>
              With the shipped compose file, the in-network plumbing (
              <code>DATABASE_URL</code>, <code>ELECTRIC_URL</code>) is wired
              for you — what&apos;s left in
              {` `}
              <code>.env</code> is two secrets, the S3 block, your domain, and
              whichever optional subsystems above you turn on. Everything in
              {` `}
              <code>.env</code> reaches the web container, so optional vars
              are simply appended. The list below documents what the app
              reads, for the shipped compose and custom setups alike; the
              exhaustive commented reference is{` `}
              <a href={`${LINKS.github.repo}/blob/master/.env.example`}>
                .env.example
              </a>
              {` `}
              at the repo root.
            </p>

            <dl className="docs-env-list">
              <EnvVar name="POSTGRES_PASSWORD" required>
                Postgres password (compose wires it into{` `}
                <code>DATABASE_URL</code> for you).
              </EnvVar>
              <EnvVar name="BETTER_AUTH_SECRET" required>
                32+ character secret for session signing.
              </EnvVar>
              <EnvVar name="S3_ENDPOINT" required>
                S3-compatible storage URL — any provider, see{` `}
                <a href="#storage">S3 storage</a>.
              </EnvVar>
              <EnvVar name="S3_ACCESS_KEY" required>
                S3 access key.
              </EnvVar>
              <EnvVar name="S3_SECRET_KEY" required>
                S3 secret key.
              </EnvVar>
              <EnvVar name="S3_BUCKET">
                Attachment bucket name (default:{" "}
                <code>exponential-attachments</code>).
              </EnvVar>
              <EnvVar name="S3_REGION">
                S3 region label your provider expects.
              </EnvVar>
              <EnvVar name="DOMAIN">
                Hostname Caddy serves with automatic HTTPS (default:{` `}
                <code>:80</code>, plain HTTP on localhost). Always set together
                with <code>APP_URL</code>.
              </EnvVar>
              <EnvVar name="APP_URL">
                The instance origin (e.g.{` `}
                <code>https://issues.yourcompany.com</code>) — becomes{` `}
                <code>BETTER_AUTH_URL</code> and the trusted origin.
              </EnvVar>
              <EnvVar name="IMAGE_TAG">
                Image tag for web + steer relay (default: <code>latest</code>,
                which tracks upstream master — pin a release tag to move
                deliberately).
              </EnvVar>
              <EnvVar name="AUTH_PASSWORD_ENABLED">
                Enable email/password login (default: <code>true</code>).
              </EnvVar>
              <EnvVar name="AUTH_SIGNUP_ENABLED">
                Allow public password sign-up. The image runs{` `}
                <code>NODE_ENV=production</code>, where this defaults{` `}
                <strong>off</strong> — the shipped compose sets it{` `}
                <code>true</code> so the first account works; flip to{` `}
                <code>false</code> once onboarded.
              </EnvVar>
              <EnvVar name="INITIAL_ADMIN_EMAILS">
                Comma-separated emails auto-promoted to instance admin at
                startup.
              </EnvVar>
              <EnvVar name="OIDC_PROVIDERS">
                JSON array of OIDC provider configs (Authentik, Keycloak,
                Zitadel, …). The redirect URI per provider is{` `}
                <code>{`\${APP_URL}/api/auth/oauth2/callback/<id>`}</code>.
              </EnvVar>
              <EnvVar name="GOOGLE_CLIENT_ID">Google OAuth client ID.</EnvVar>
              <EnvVar name="GOOGLE_CLIENT_SECRET">
                Google OAuth client secret.
              </EnvVar>
              <EnvVar name="GOOGLE_LOGIN_ENABLED">
                Show Google sign-in button (default: <code>false</code>).
              </EnvVar>
              <EnvVar name="GITHUB_APP_ID">
                GitHub App numeric ID — required to connect repositories (coding
                sessions and PRs).
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
              <EnvVar name="GITHUB_APP_CLIENT_ID">
                GitHub App OAuth client ID — optional. Enables the lightweight
                connect flow (a single GitHub consent screen); unset falls back
                to the install-page round-trip.
              </EnvVar>
              <EnvVar name="GITHUB_APP_CLIENT_SECRET">
                GitHub App OAuth client secret (generate it on the App page).
                The user token it mints is transient — used once to enumerate
                installations, never stored.
              </EnvVar>
              <EnvVar name="GITHUB_POLLING">
                Set to <code>true</code> to poll for PR merges instead — for
                servers behind NAT that webhooks can&apos;t reach.
              </EnvVar>
              <EnvVar name="SMTP_HOST">
                SMTP server for all outgoing mail (self-host transport). Unset
                together with <code>AWS_SES_REGION</code> ⇒ every send is a
                logged no-op.
              </EnvVar>
              <EnvVar name="SMTP_PORT">
                SMTP port (default: <code>587</code>).
              </EnvVar>
              <EnvVar name="SMTP_USER">
                SMTP username — optional, for unauthenticated relays.
              </EnvVar>
              <EnvVar name="SMTP_PASS">SMTP password.</EnvVar>
              <EnvVar name="SMTP_SECURE">
                Set to <code>true</code> for implicit TLS (port 465).
              </EnvVar>
              <EnvVar name="AWS_SES_REGION">
                Amazon SES region — setting it switches the sender to SES,
                which wins over SMTP when both are configured.
              </EnvVar>
              <EnvVar name="AWS_ACCESS_KEY_ID">
                AWS credentials for SES (IAM user allowed{` `}
                <code>ses:SendEmail</code>).
              </EnvVar>
              <EnvVar name="AWS_SECRET_ACCESS_KEY">AWS secret key for SES.</EnvVar>
              <EnvVar name="EMAIL_FROM">
                Sender address, e.g.{` `}
                <code>
                  {`Exponential <noreply@yourcompany.com>`}
                </code>
                {` `}
                — must be a verified identity on SES.
              </EnvVar>
              <EnvVar name="EMAIL_REPLY_TO">
                Monitored default Reply-To on every outbound email.
              </EnvVar>
              <EnvVar name="STEER_RELAY_URL">
                Steer relay WebSocket URL (e.g.{` `}
                <code>ws://your-host:4002</code>) — remote start and live
                watch/steer. Unset ⇒ the subsystem is off; local coding is
                unaffected.
              </EnvVar>
              <EnvVar name="STEER_RELAY_SECRET">
                Shared HS256 secret the web app signs steer tickets with — the
                compose profile hands the same value to the relay container.
                Both this and <code>STEER_RELAY_URL</code> are needed: with
                either missing the web app reports the subsystem as disabled,
                and a secretless relay answers <code>503</code>.
              </EnvVar>
              <EnvVar name="PUSH_RELAY_URL">
                Push relay URL — only meaningful with self-built mobile apps,
                see <a href="#push">Push notifications</a>.
              </EnvVar>
              <EnvVar name="PUSH_RELAY_SECRET">
                Shared secret between the web app and the push relay (sent as
                the <code>x-relay-secret</code> header) — must match the relay
                process&apos;s env.
              </EnvVar>
            </dl>
          </DocsSection>

          {/* ── 08 Updating ── */}
          <DocsSection id="updating" num="08" label="Updating">
            <h2>Updating</h2>
            <DocsCode language="shell">{`
docker compose pull && docker compose up -d
`}</DocsCode>
            <p>
              That&apos;s the whole procedure: the web image applies pending
              migrations and its custom trigger SQL at every boot, so there
              are no separate migration steps.
            </p>
            <p>
              By default (<code>IMAGE_TAG</code> unset ⇒ <code>latest</code>)
              this tracks upstream <code>master</code>. To move deliberately
              instead, pin a release in <code>.env</code> — e.g.{` `}
              <code>IMAGE_TAG=0.18.13</code> from the{` `}
              <a href={`${LINKS.github.repo}/tags`}>release tags</a> — and bump
              it when you choose; the same tag applies to the steer relay
              image.
            </p>
          </DocsSection>

          {/* ── 09 Licensing ── */}
          <DocsSection id="licensing" num="09" label="Licensing">
            <h2>Licensing</h2>
            <p>
              Exponential is <strong>fully open source</strong> under the{` `}
              <a href={`${LINKS.github.repo}/blob/master/LICENSE`}>
                Apache License 2.0
              </a>
              . Self-hosting is free for everyone — any company size, in
              production, forever. No seat caps, no commercial license, no
              phone home.
            </p>

            <h3>Enterprise Support — optional</h3>
            <p>
              If you want more than the community around the repo, add{` `}
              <strong>Enterprise Support</strong> — SLA, priority support,
              deployment help, and custom development, on one annual invoice.
              It&apos;s an add-on, never a requirement.{` `}
              <a href="/contact/">Contact sales</a> or email{` `}
              <a href="mailto:support@exponential.at">support@exponential.at</a>
              . Need SSO, SLA, or DPA? That conversation starts there too.
            </p>
          </DocsSection>
        </DocsLayout>
      </main>

      <SiteFooter />
    </>
  )
}
