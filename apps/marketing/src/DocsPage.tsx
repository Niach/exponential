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
  { id: `cloud-or-self-host`, num: `01`, label: `Cloud or self-host?` },
  { id: `installation`, num: `02`, label: `Installation` },
  { id: `push`, num: `03`, label: `Push notifications` },
  { id: `integrations`, num: `04`, label: `Integrations` },
  { id: `mobile`, num: `05`, label: `Mobile apps` },
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
        <DocsSection
          id="cloud-or-self-host"
          num="01"
          label="Cloud or self-host?"
        >
          <h2>Cloud or self-host?</h2>
          <p>
            Same code on both sides — pick whichever fits.
          </p>
          <p>
            <strong>Free cloud at{` `}
              <a href="https://app.exponential.at">app.exponential.at</a>.
            </strong>{` `}
            Sign in with Google, create a workspace, invite teammates. No
            install, no operator burden. Limited to Google sign-in for now.
          </p>
          <p>
            <strong>Self-host.</strong> One <code>docker compose up</code>{` `}
            on a Linux box and you have the full stack on your network —
            Postgres, Electric, S3-compatible attachment storage. Your
            choice of sign-in method, your data on your disks.
          </p>
          <p>
            Either way, there is one shared{` `}
            <a href="https://app.exponential.at/feedback">public feedback workspace</a>
            {` `}
            on the cloud where you can file bugs and feature requests. Self-
            hosted instances expose a "Send feedback" button that deep-links
            here, so issues stay in one place no matter how you run it.
          </p>
        </DocsSection>

        <DocsSection id="installation" num="02" label="Installation">
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

        <DocsSection id="push" num="03" label="Push notifications">
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

        <DocsSection id="integrations" num="04" label="Integrations">
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

        <DocsSection id="mobile" num="05" label="Mobile apps">
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
