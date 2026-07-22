// Whether an outbound email transport is configured (Amazon SES or SMTP).
//
// Lives in its own module — NOT in lib/email.ts — because this flag is
// CLIENT-REACHABLE: auth/config's buildAuthConfig feeds it to the login page
// (gates "Forgot password?"), and the account notification prefs surface
// reads it via tRPC. lib/email.ts statically imports the db (send-time
// suppression) and must never enter the browser bundle — the Postgres
// driver's node:util dependency black-screens the app (v0.18.10 outage).
export const emailEnabled = Boolean(
  process.env.AWS_SES_REGION || process.env.SMTP_HOST
)
