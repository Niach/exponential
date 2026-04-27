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
            Exponential · self-hosted issue tracker · last updated 2026-04-27
          </p>

          <p style={prose}>
            This privacy policy describes how the Exponential application
            (“Exponential”, “the app”) handles data when you connect a Google
            account to use the Google Calendar integration. Exponential is
            operated by Daniel Strähhuber as a personal, self-hosted issue
            tracker. There is no commercial service, no shared backend, and no
            central database aggregating user data across installs.
          </p>

          <h2 style={h2Style}>What data the app accesses</h2>
          <p style={prose}>
            When you connect your Google account, the app requests a single
            Google OAuth scope:{` `}
            <code style={inlineCode}>
              https://www.googleapis.com/auth/calendar.events
            </code>
            . This scope is used solely to create, update, and delete calendar
            events that mirror issue due dates that you set inside Exponential.
            The app does not read events that it did not create. The app does
            not access your contacts, mail, drive, or any other Google service.
          </p>

          <h2 style={h2Style}>How that data is used</h2>
          <p style={prose}>
            For each Exponential issue you give a due date, the app inserts one
            all-day event into your primary Google Calendar. The event title is
            the issue’s identifier and title. The event date is the issue’s due
            date. When you change the due date, the event is updated. When you
            mark the issue as done or cancelled, archive it, clear the due
            date, or delete the issue, the event is deleted. No other use is
            made of your Google data.
          </p>

          <h2 style={h2Style}>Where data is stored</h2>
          <p style={prose}>
            Your OAuth access and refresh tokens are stored on the self-hosted
            Exponential instance you connected to, inside its private
            PostgreSQL database. Tokens are not transmitted to any third party.
            The instance is operated by the user or organization who deployed
            it, not by a central service.
          </p>

          <h2 style={h2Style}>Retention and deletion</h2>
          <p style={prose}>
            You may revoke the app’s access at any time from{` `}
            <a href="https://myaccount.google.com/permissions" style={linkStyle}>
              Google Account → Third-party access
            </a>
            , or by clicking “Disconnect” on the{` `}
            <code style={inlineCode}>/account/integrations</code> page inside
            Exponential. Disconnecting deletes the stored OAuth tokens
            immediately. Calendar events the app previously created remain in
            your calendar until you delete them, or until the underlying issue
            is deleted/done/cancelled while the integration is still connected.
          </p>

          <h2 style={h2Style}>Sharing</h2>
          <p style={prose}>
            The app does not share, sell, transfer, or transmit any Google user
            data to third parties. Data flows only between your browser, the
            self-hosted Exponential server, and Google’s own APIs.
          </p>

          <h2 style={h2Style}>Limited Use disclosure</h2>
          <p style={prose}>
            Exponential’s use and transfer of information received from Google
            APIs adheres to the{` `}
            <a
              href="https://developers.google.com/terms/api-services-user-data-policy"
              style={linkStyle}
            >
              Google API Services User Data Policy
            </a>
            , including the Limited Use requirements.
          </p>

          <h2 style={h2Style}>Security</h2>
          <p style={prose}>
            OAuth tokens are stored in the application database and transmitted
            only over TLS. The self-hosted instance is access-controlled by the
            operator who deployed it.
          </p>

          <h2 style={h2Style}>Contact</h2>
          <p style={prose}>
            Questions about this policy or the app’s data handling:{` `}
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
