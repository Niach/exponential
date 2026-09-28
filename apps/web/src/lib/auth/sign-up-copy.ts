// EXP-1026: the name step of an email-code sign-up. A first sign-in with a
// one-time code for an address that has no account pauses on this step
// (server: `NAME_REQUIRED`, `lib/auth/ask-name.ts`) and resubmits the SAME
// code with the typed name. Byte-identical ×4: iOS, Android and the desktop
// mirror these literals, and `sign-up-copy.test.ts` locks them.
export const SIGN_UP_NAME_COPY = {
  title: `What should we call you?`,
  body: `This is how your teammates see you.`,
  fieldLabel: `Name`,
  placeholder: `Your name`,
  button: `Create account`,
  emptyNameError: `Enter your name to continue.`,
} as const

/** The request header a client sends to opt in to the name step. Without it
 *  the server keeps the mailbox-local-part fallback (API-driven/old clients). */
export const ASK_NAME_HEADER = `X-Exp-Ask-Name`

/** The Better Auth error `code` a name-less first sign-in answers with. */
export const NAME_REQUIRED_CODE = `NAME_REQUIRED`
