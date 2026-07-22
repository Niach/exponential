import { SiteShell } from "./components/SiteShell"

export function PrivacyPage() {
  return (
    <SiteShell>
      <section style={{ padding: `64px 0 96px` }}>
        <div className="shell" style={{ maxWidth: 760 }}>
          <h1
            style={{
              fontSize: 40,
              fontWeight: 600,
              letterSpacing: `-0.03em`,
              margin: `0 0 8px`,
            }}
          >
            Privacy Policy
          </h1>
          <p
            style={{
              fontFamily: `var(--font-mono)`,
              fontSize: 12,
              color: `var(--fg-dim)`,
              margin: `0 0 40px`,
            }}
          >
            Exponential · exponential.at · last updated 2026-07-21
          </p>

          <p style={prose}>
            This privacy policy describes what data the Exponential service and
            its apps (web, Android, iOS, and desktop — together “Exponential”,
            “the service”) collect, how it is used, stored, shared, and deleted.
            Exponential is operated by Dennis Strähhuber, Germany (“we”). It
            applies to the hosted cloud service at{` `}
            <code style={inlineCode}>app.exponential.at</code>. If you connect
            the apps to a self-hosted Exponential instance instead, the operator
            of that instance is responsible for the data it stores; this policy
            then applies only to what the apps themselves do on your device.
          </p>

          <h2 style={h2Style}>1. Data we collect</h2>
          <ul style={listStyle}>
            <li style={listItem}>
              <strong>Account data.</strong> When you sign in with Google we
              receive your name, email address, and profile picture URL via
              Google’s OAuth flow (scopes{` `}
              <code style={inlineCode}>openid</code>,{` `}
              <code style={inlineCode}>profile</code>,{` `}
              <code style={inlineCode}>email</code>). We request no other Google
              scopes and never access your contacts, mail, files, calendar, or
              location. When you sign in with Apple we receive your name and
              email address from Apple — if you chose “Hide My Email”, that
              address is an Apple private-relay address and we never see the
              real one. No other Apple account data is accessed.
            </li>
            <li style={listItem}>
              <strong>Content you create.</strong> Teams, boards, issues,
              comments, labels, and file attachments (including screenshots
              submitted through the feedback widget) are stored so the service
              can function. Issue and comment text may contain whatever you
              choose to write.
            </li>
            <li style={listItem}>
              <strong>Feedback widget submissions.</strong> If a site operator
              embeds our feedback widget and you submit feedback or a support
              request through it, we store what you send — your message and
              optional screenshot — plus the page URL you were on, your
              browser’s user-agent and viewport/screen size, and any email,
              name, or custom data the host site chooses to pass along with your
              submission. This lets the site operator triage and follow up on
              your report — the members of the operator’s team who handle
              feedback can see your email and message. Your email is never
              exposed outside that team.
            </li>
            <li style={listItem}>
              <strong>Push notification tokens.</strong> If you enable push
              notifications on Android or iOS, a Firebase Cloud Messaging device
              token is stored to deliver them. The token identifies the app
              install, not your physical device identity.
            </li>
            <li style={listItem}>
              <strong>GitHub integration data.</strong> If you connect the
              Exponential GitHub App, we store the installation reference and
              repository names you connect. Repository access tokens are
              short-lived and minted on demand; we do not store your GitHub
              password or personal access tokens.
            </li>
            <li style={listItem}>
              <strong>Billing data.</strong> Paid subscriptions are processed by
              Creem (merchant of record). We store your subscription state and a
              customer reference; payment card details never touch our servers.
            </li>
            <li style={listItem}>
              <strong>Technical logs.</strong> Standard server logs (IP address,
              request path, timestamps) are kept short-term for security and
              operations. We run no third-party analytics, no ad networks, and
              no tracking pixels.
            </li>
          </ul>

          <h2 style={h2Style}>2. How data is used</h2>
          <p style={prose}>
            Data is used solely to provide the service: authenticating you,
            syncing your boards in real time across your devices, sending the
            notifications you enabled, operating the GitHub and billing
            integrations you chose, and answering support requests. We do not
            use your data for advertising or profiling, we do not sell it, and
            we do not use it to train machine-learning models.
          </p>

          <h2 style={h2Style}>3. Sharing and processors</h2>
          <p style={prose}>
            We share data only with the processors required to run the service:
          </p>
          <ul style={listStyle}>
            <li style={listItem}>
              <strong>Hetzner Online GmbH</strong> (Germany) — servers and
              object storage for attachments.
            </li>
            <li style={listItem}>
              <strong>Google Firebase Cloud Messaging</strong> — delivery of
              push notifications (receives the device token and the notification
              payload).
            </li>
            <li style={listItem}>
              <strong>Amazon Web Services (Amazon SES)</strong> — transactional
              email (receives your email address and the message content, e.g.
              notification digests, team invitations, and support replies).
            </li>
            <li style={listItem}>
              <strong>Creem</strong> — subscription billing (merchant of record;
              receives your account email address to create the checkout
              session, plus the billing and payment details you enter with
              them).
            </li>
            <li style={listItem}>
              <strong>GitHub</strong> — only if you connect the GitHub App;
              repository operations happen through GitHub’s API on your behalf.
            </li>
          </ul>
          <p style={prose}>
            There are no data brokers, ad networks, or analytics providers.
            Feedback and support requests you submit through a widget are
            visible only to the members of the team that operates it.
          </p>

          <h2 style={h2Style}>4. Storage and protection</h2>
          <ul style={listStyle}>
            <li style={listItem}>
              All traffic between your devices and the service uses TLS (HTTPS).
            </li>
            <li style={listItem}>
              Data is stored in a PostgreSQL database and S3-compatible object
              storage on servers in Germany (Hetzner), reachable only from the
              application servers.
            </li>
            <li style={listItem}>
              Access is session-authenticated; a team’s data is only synced to
              members of that team. Server-side authorization enforces the same
              rules for every API call.
            </li>
          </ul>

          <h2 style={h2Style}>5. Retention and deletion</h2>
          <p style={prose}>
            Your data is retained while your account is active. You can delete
            issues, comments, attachments, boards, and teams yourself inside the
            app. Deleting a board moves it to a trash for 48 hours, during which
            the team owner can restore it from team settings → Boards; after
            that window it is purged permanently, including the attachments
            stored on it. All other deletions are immediate and propagate to all
            synced devices. You can also delete your entire account and all
            associated data directly in the product: on the web under Account →
            Notifications → Danger Zone, and in the mobile apps under Settings →
            your server → “Delete account”. Deletion is immediate and removes
            your account together with every team where you are the only member.
            If you are the sole owner of a team that still has other members,
            transfer ownership or remove those members first — we will not
            delete a shared team out from under the people still using it.
            Alternatively, email{` `}
            <a href="mailto:danny@exponential.at" style={linkStyle}>
              danny@exponential.at
            </a>
            {` `}from the address tied to your account; requests are honoured
            within 30 days. Revoking Google access is possible anytime at{` `}
            <a
              href="https://myaccount.google.com/permissions"
              style={linkStyle}
            >
              Google Account → Third-party access
            </a>
            ; Apple access under Settings → Apple ID → Sign-In &amp; Security →
            Sign in with Apple on your device, or at{` `}
            <a href="https://appleid.apple.com" style={linkStyle}>
              appleid.apple.com
            </a>
            .
          </p>

          <h2 style={h2Style}>6. Legal bases</h2>
          <p style={prose}>
            Under the GDPR we rely on three legal bases. Performance of a
            contract (Art. 6(1)(b)) covers everything needed to provide the
            service you signed up for — your account, your team’s content, and
            the paid subscription if you have one. Legitimate interests (Art.
            6(1)(f)) cover keeping the service secure, preventing abuse, and the
            short-lived operational logs that come with running servers. Consent
            (Art. 6(1)(a)) covers anything that asks for it first, such as the
            push notifications you switch on; you can withdraw it at any time by
            turning the feature off again.
          </p>

          <h2 style={h2Style}>7. International transfers</h2>
          <p style={prose}>
            The database and attachment storage stay on servers in Germany. Some
            processors are US companies or process data outside the EU/EEA —
            Google (Firebase Cloud Messaging) for push notifications, Amazon Web
            Services for transactional email, and GitHub for the optional
            repository integration. Those transfers rely on the EU–US Data
            Privacy Framework and/or the EU Standard Contractual Clauses.
          </p>

          <h2 style={h2Style}>8. Cookies and local storage</h2>
          <p style={prose}>
            We use only strictly necessary cookies and local storage: a session
            cookie to keep you signed in, and device-local preferences such as
            the team you last opened. There are no advertising cookies and no
            cross-site tracking.
          </p>

          <h2 style={h2Style}>9. Your rights</h2>
          <p style={prose}>
            You have the right to access the personal data we hold about you, to
            have it corrected or erased, to restrict or object to its
            processing, and to receive it in a portable format. Much of this you
            can do yourself in the app — edit or delete your content, or delete
            your account outright (see section 5). For anything else, email{` `}
            <a href="mailto:danny@exponential.at" style={linkStyle}>
              danny@exponential.at
            </a>
            {` `}from the address tied to your account; requests are honoured
            within 30 days. You also have the right to lodge a complaint with a
            data protection supervisory authority, typically the one where you
            live or work.
          </p>

          <h2 style={h2Style}>Limited Use disclosure</h2>
          <p style={prose}>
            Exponential’s use and transfer to any other app of information
            received from Google APIs adheres to the{` `}
            <a
              href="https://developers.google.com/terms/api-services-user-data-policy"
              style={linkStyle}
            >
              Google API Services User Data Policy
            </a>
            , including the Limited Use requirements.
          </p>

          <h2 style={h2Style}>Children</h2>
          <p style={prose}>
            The service is a professional productivity tool and is not directed
            at children under 16. We do not knowingly collect data from
            children.
          </p>

          <h2 style={h2Style}>Changes to this policy</h2>
          <p style={prose}>
            If this policy changes, the “last updated” date at the top of the
            page will be revised. Material changes that reduce user protections
            will be communicated to registered users by email before they take
            effect.
          </p>

          <h2 style={h2Style}>Contact</h2>
          <p style={prose}>
            Data controller: Dennis Strähhuber, Germany (
            <a href="/imprint/" style={linkStyle}>
              see the Imprint for the full postal address
            </a>
            ). Questions about this policy and all data-deletion requests:{` `}
            <a href="mailto:danny@exponential.at" style={linkStyle}>
              danny@exponential.at
            </a>
            .
          </p>
        </div>
      </section>
    </SiteShell>
  )
}

const prose = {
  fontSize: 15,
  lineHeight: 1.7,
  color: `var(--fg-muted)`,
  margin: `0 0 18px`,
} as const

const h2Style = {
  fontSize: 20,
  fontWeight: 500,
  letterSpacing: `-0.02em`,
  color: `var(--fg)`,
  margin: `40px 0 12px`,
} as const

const inlineCode = {
  fontFamily: `var(--font-mono)`,
  fontSize: `0.85em`,
  padding: `2px 7px`,
  background: `var(--bg-soft)`,
  borderRadius: 6,
  color: `var(--fg)`,
} as const

const linkStyle = {
  color: `var(--fg)`,
  textDecoration: `underline`,
  textUnderlineOffset: 3,
} as const

const listStyle = {
  fontSize: 15,
  lineHeight: 1.7,
  color: `var(--fg-muted)`,
  margin: `0 0 18px`,
  paddingLeft: 22,
} as const

const listItem = {
  margin: `0 0 8px`,
} as const
