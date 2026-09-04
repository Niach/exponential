// EXP-725: the first-run wizard runs the SAME steps in the SAME order with the
// SAME words on all four clients (team -> board -> invite -> devices), so the
// copy for steps 2-4 is ONE table and `onboarding-copy.test.ts` reads the
// native sources off disk to prove they still say the same thing:
//
//   desktop  crates/ui/src/onboarding.rs                     (ONBOARDING_COPY)
//   iOS      UI/Onboarding/OnboardingCopy.swift              (both tables)
//   Android  ui/onboarding/OnboardingCopy.kt                 (both tables)
//
// Step 1 (name the team / join) keeps its two style families on purpose: the
// web card is the desktop reference and the iOS TeamSetup sheet the mobile
// one (EXP-698), so those strings stay out of here. The devices step's two
// sub-cards reuse the getting-started `desktop` and `server` entries, which
// are already gated the same way. Keep every string free of quotes,
// apostrophes, backslashes and non-ASCII punctuation (no ellipsis, no em
// dash): the drift test matches them as literals inside Swift/Kotlin/Rust
// source, and in-flight labels like "Creating…" therefore stay inline.
export const ONBOARDING_COPY = {
  board: {
    title: `Create your first board`,
    subtitle: `Boards hold your issues. Connect a GitHub repository to code on them. Everything can be changed later.`,
    create: `Create board`,
  },
  invite: {
    title: `Invite your teammates`,
    subtitle: `Teammates share boards, reviews and the support inbox. You can also invite people later from team settings.`,
    generate: `Generate invite link`,
    copy: `Copy link`,
    copied: `Copied`,
  },
  devices: {
    title: `Set up your devices`,
    subtitle: `Coding sessions run on the desktop app or on a server with the Exponential CLI. Install one and sign your agents in. You can also do this later.`,
    yours: `Your devices`,
    none: `No devices yet. Sign in on the desktop app or a server and it shows up here.`,
  },
  nav: {
    skip: `Skip for now`,
    continue: `Continue`,
  },
} as const

/** Strings only the phones render: their wizard has a welcome page before
 * the team step and a done page after the devices step, and a persistent
 * sign-out escape. Gated against the two mobile copy files only. */
export const MOBILE_ONBOARDING_COPY = {
  share: `Share`,
  doneTitle: `All set`,
  doneBody: `Your first board is ready.`,
  doneButton: `Open Exponential`,
  retry: `Try again`,
  signOut: `Sign out`,
} as const

/** Every leaf string of a copy table, for the drift test. */
export function onboardingCopyStrings(
  table: Record<string, string | Record<string, string>>
): string[] {
  return Object.values(table).flatMap((value) =>
    typeof value === `string` ? [value] : Object.values(value)
  )
}
