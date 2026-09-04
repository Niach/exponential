package com.exponential.app.ui.onboarding

/**
 * The first-run wizard's words, byte-identical to the other three clients
 * (EXP-725). Web owns the canonical tables in
 * `apps/web/src/components/onboarding/onboarding-copy.ts` and its
 * `onboarding-copy.test.ts` reads THIS file, asserting every string appears
 * here verbatim — so each value must stay a plain, single-line, double-quoted
 * literal with no escapes and no non-ASCII punctuation.
 *
 * The wizard runs the SAME four steps in the SAME order everywhere: name the
 * team (or join with an invite link) -> first board -> invite teammates
 * (skippable) -> set up devices (skippable, LAST because finishing it means
 * leaving for another machine).
 *
 * Step 1's strings are deliberately absent: the two style families are
 * intentional (EXP-698) and [TeamSetupForm] owns the mobile one. The devices
 * step's two sub-cards reuse `GettingStartedCopy.DESKTOP_*` / `SERVER_*`
 * rather than duplicating those literals.
 */
object OnboardingCopy {
    const val BOARD_TITLE = "Create your first board"
    const val BOARD_SUBTITLE =
        "Boards hold your issues. Connect a GitHub repository to code on them. Everything can be changed later."
    const val BOARD_CREATE = "Create board"

    const val INVITE_TITLE = "Invite your teammates"
    const val INVITE_SUBTITLE =
        "Teammates share boards, reviews and the support inbox. You can also invite people later from team settings."
    const val INVITE_GENERATE = "Generate invite link"
    const val INVITE_COPY = "Copy link"
    const val INVITE_COPIED = "Copied"

    const val DEVICES_TITLE = "Set up your devices"
    const val DEVICES_SUBTITLE =
        "Coding sessions run on the desktop app or on a server with the Exponential CLI. Install one and sign your agents in. You can also do this later."
    const val DEVICES_YOURS = "Your devices"
    const val DEVICES_NONE =
        "No devices yet. Sign in on the desktop app or a server and it shows up here."

    const val SKIP = "Skip for now"
    const val CONTINUE = "Continue"

    // ── Mobile-only: the phones' wizard has a welcome page before the team
    // step, a done page after the devices step, and a persistent sign-out. ──

    const val SHARE = "Share"
    const val DONE_TITLE = "All set"
    const val DONE_BODY = "Your first board is ready."
    const val DONE_BUTTON = "Open Exponential"
    const val RETRY = "Try again"
    const val SIGN_OUT = "Sign out"
}
