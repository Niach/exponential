import Foundation

/// Every string the first-run wizard's shared steps render (EXP-725).
///
/// One place per platform, byte-identical across all four: web's
/// `apps/web/src/components/onboarding/onboarding-copy.ts` is the SOURCE, and
/// the IDE's `crates/ui/src/onboarding.rs`, Android's `OnboardingCopy.kt` and
/// this file mirror it. A drift test
/// (`apps/web/src/components/onboarding/onboarding-copy.test.ts`) reads the
/// native sources off disk and asserts each value appears verbatim, so every
/// literal below is a PLAIN double-quoted single-line string — no
/// interpolation, no concatenation, no escapes, no ellipsis.
///
/// Step 1 (name the team / join) keeps its own mobile wording (`TeamSetupView`,
/// EXP-698) and stays out of the shared table. The devices step's two
/// sub-cards REUSE `GettingStartedCopy.desktop*` / `.server*` — the same
/// entries the getting-started checklist already gates — so they are not
/// duplicated here.
enum OnboardingCopy {
    // MARK: - Step 2: first board

    static let boardTitle = "Create your first board"
    static let boardSubtitle = "Boards hold your issues. Connect a GitHub repository to code on them. Everything can be changed later."
    static let boardCreate = "Create board"

    // MARK: - Step 3: invite teammates

    static let inviteTitle = "Invite your teammates"
    static let inviteSubtitle = "Teammates share boards, reviews and the support inbox. You can also invite people later from team settings."
    static let inviteGenerate = "Generate invite link"
    static let inviteCopy = "Copy link"
    static let inviteCopied = "Copied"

    // MARK: - Step 4: devices

    static let devicesTitle = "Set up your devices"
    static let devicesSubtitle = "Coding sessions run on the desktop app or on a server with the Exponential CLI. Install one and sign your agents in. You can also do this later."
    static let devicesYours = "Your devices"
    static let devicesNone = "No devices yet. Sign in on the desktop app or a server and it shows up here."

    // MARK: - Navigation

    static let skip = "Skip for now"
    static let continueLabel = "Continue"

    // MARK: - Mobile-only

    /// The phones wrap the minted link in the system share sheet; the web and
    /// the IDE only copy it.
    static let share = "Share"
    /// The welcome page's counterpart at the end of the wizard — web and the
    /// IDE drop straight into the app instead.
    static let doneTitle = "All set"
    static let doneBody = "Your first board is ready."
    static let doneButton = "Open Exponential"
    static let retry = "Try again"
    /// The persistent escape under every page: the wizard is the FIRST authed
    /// surface, so a session the server has invalidated must have a way back
    /// to LoginView.
    static let signOut = "Sign out"
}
