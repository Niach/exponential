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
            Exponential · self-hosted issue tracker · last updated 2026-04-30
          </p>

          <p style={prose}>
            This privacy policy describes how the Exponential application
            (“Exponential”, “the app”) accesses, uses, stores, shares, retains,
            and protects Google user data when you choose to sign in with
            Google or connect your Google Calendar. Exponential is operated by
            Daniel Strähhuber as a personal, self-hosted issue tracker. There
            is no commercial service, no shared backend, and no central
            database aggregating user data across installs.
          </p>

          <h2 style={h2Style}>1. Data accessed</h2>
          <p style={prose}>
            When you sign in with Google or link your Google account, the app
            requests the following OAuth scopes:
          </p>
          <ul style={listStyle}>
            <li style={listItem}>
              <code style={inlineCode}>openid</code> — used by Google’s OAuth
              flow to issue an ID token that identifies your Google account to
              the app. No additional data is read.
            </li>
            <li style={listItem}>
              <code style={inlineCode}>profile</code> — used to read your
              Google profile name and (where available) profile picture URL,
              shown only inside your own Exponential session as the
              signed-in user.
            </li>
            <li style={listItem}>
              <code style={inlineCode}>email</code> — used to read the email
              address of your Google account so the app can identify your
              user record and display it in the user menu.
            </li>
            <li style={listItem}>
              <code style={inlineCode}>
                https://www.googleapis.com/auth/calendar.events
              </code>
              {` `}— requested only when you explicitly enable the Google
              Calendar integration. Used to create, update, and delete calendar
              events on your primary calendar that mirror issue due dates set
              inside Exponential.
            </li>
          </ul>
          <p style={prose}>
            The app does <strong>not</strong> request or access your contacts,
            mail, Drive files, photos, location, search history, or any Google
            service other than the scopes listed above. The app does not read
            calendar events it did not itself create.
          </p>

          <h2 style={h2Style}>2. Data usage</h2>
          <p style={prose}>
            Google user data is used only to provide the features the user has
            chosen, in the following specific ways:
          </p>
          <ul style={listStyle}>
            <li style={listItem}>
              <strong>Account identification.</strong> Your Google name, email,
              and profile picture URL are written to the app’s{` `}
              <code style={inlineCode}>users</code> table to associate your
              session with an account record. The email is used as your unique
              identifier inside the app.
            </li>
            <li style={listItem}>
              <strong>Calendar event sync.</strong> For each Exponential issue
              you give a due date (and that is not done, cancelled, or
              archived), the app inserts one all-day event into your primary
              Google Calendar. The event title is the issue’s identifier and
              title. The event date is the issue’s due date. When you change
              the due date, the event is updated. When you mark the issue as
              done or cancelled, archive it, clear the due date, or delete the
              issue, the event is deleted. The app stores the resulting
              calendar event ID so it can update or remove the event later.
            </li>
          </ul>
          <p style={prose}>
            Google user data is <strong>never</strong> used to train,
            fine-tune, or evaluate machine-learning or AI models; it is not
            used for advertising, profiling, or any analytics; it is not
            disclosed to humans for review except where required by law or
            with the user’s explicit prior consent (e.g. user-initiated
            support).
          </p>

          <h2 style={h2Style}>3. Data sharing</h2>
          <p style={prose}>
            The app does not share, sell, rent, lease, transfer, or transmit
            Google user data to any third party. There are no analytics
            providers, ad networks, data brokers, or marketing partners
            involved. Data flows only between your browser, the self-hosted
            Exponential server you connected to, and Google’s own APIs (
            <code style={inlineCode}>accounts.google.com</code> for OAuth and
            {` `}
            <code style={inlineCode}>www.googleapis.com</code> for Calendar
            calls). No subprocessors handle Google user data.
          </p>

          <h2 style={h2Style}>4. Data storage and protection</h2>
          <p style={prose}>
            All Google user data handled by the app is stored on the
            self-hosted Exponential instance you connected to, in its private
            PostgreSQL database:
          </p>
          <ul style={listStyle}>
            <li style={listItem}>
              <strong>OAuth tokens</strong> (access token, refresh token,
              expiry, granted scopes, account ID) are stored in the{` `}
              <code style={inlineCode}>accounts</code> table managed by the
              Better Auth library. Tokens are scoped to the user that
              authorized them and are not visible to other users of the
              instance.
            </li>
            <li style={listItem}>
              <strong>Profile fields</strong> (name, email, profile picture
              URL) are stored in the <code style={inlineCode}>users</code>{` `}
              table.
            </li>
            <li style={listItem}>
              <strong>Calendar event IDs</strong> created by the sync are
              stored on the corresponding issue row so the app can update or
              delete the event later.
            </li>
          </ul>
          <p style={prose}>
            Protections in place:
          </p>
          <ul style={listStyle}>
            <li style={listItem}>
              All network traffic between your browser, the Exponential
              server, and Google’s APIs is transmitted over TLS (HTTPS).
            </li>
            <li style={listItem}>
              The application enforces session-based authentication; only the
              authenticated owner of an account can read or modify their own
              tokens, profile, or issues. There is no admin UI that exposes
              other users’ Google data.
            </li>
            <li style={listItem}>
              The PostgreSQL database is reachable only from the application
              server inside the deployment’s private network. Database
              credentials are stored in environment variables on the server.
            </li>
            <li style={listItem}>
              The OAuth client secret and database URL are kept server-side
              only and are never sent to the browser.
            </li>
            <li style={listItem}>
              The deployment server runs on hardened Linux infrastructure
              under the operator’s control with full-disk encryption at rest.
            </li>
          </ul>

          <h2 style={h2Style}>5. Data retention and deletion</h2>
          <p style={prose}>
            <strong>Retention.</strong> OAuth tokens and profile data are
            retained only for as long as you keep the integration connected
            and your account active. There is no separate analytical or
            archival store of Google user data; the only copies are the live
            rows in the operational database.
          </p>
          <p style={prose}>
            <strong>Deletion paths.</strong> You can remove your data at any
            time through any of the following:
          </p>
          <ul style={listStyle}>
            <li style={listItem}>
              Click <em>Disconnect</em> on the{` `}
              <code style={inlineCode}>/account/integrations</code> page
              inside Exponential. The associated{` `}
              <code style={inlineCode}>accounts</code> row (containing the
              access and refresh tokens) is deleted from the database
              immediately.
            </li>
            <li style={listItem}>
              Revoke access from{` `}
              <a
                href="https://myaccount.google.com/permissions"
                style={linkStyle}
              >
                Google Account → Third-party access
              </a>
              . Existing tokens stop working immediately on Google’s side; the
              local copy will be removed on next disconnect or by request
              (below).
            </li>
            <li style={listItem}>
              Email{` `}
              <a href="mailto:danny@straehhuber.com" style={linkStyle}>
                danny@straehhuber.com
              </a>
              {` `}from the address tied to your Google account to request
              deletion of your user record (profile, tokens, issues, and any
              related rows). Requests are honoured within 30 days.
            </li>
          </ul>
          <p style={prose}>
            Calendar events the app previously created remain in your Google
            Calendar after disconnect (because the app no longer has tokens
            to remove them); you can delete them inside Google Calendar at
            any time. While the integration is connected, marking an issue
            done/cancelled, archiving it, clearing its due date, or deleting
            the issue causes the corresponding event to be deleted from your
            calendar automatically.
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

          <h2 style={h2Style}>Changes to this policy</h2>
          <p style={prose}>
            If this policy changes, the “last updated” date at the top of the
            page will be revised. Material changes that reduce user
            protections will be communicated to connected users by email
            before they take effect.
          </p>

          <h2 style={h2Style}>Contact</h2>
          <p style={prose}>
            Questions about this policy or the app’s data handling, and all
            data-deletion requests:{` `}
            <a href="mailto:danny@straehhuber.com" style={linkStyle}>
              danny@straehhuber.com
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
  color: `var(--accent)`,
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
