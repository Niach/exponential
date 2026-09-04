import ExpCore
import ExpUI
import SwiftUI

/// First-run wizard (shared onboarding spec, EXP-8 + EXP-188 + EXP-725): the
/// SAME four steps in the SAME order on all four clients, wrapped on mobile by
/// a welcome page and a done page.
///
///   0 welcome  — app name + one-line value prop + "Get started"
///   1 team     — create-or-join (signups get NO auto-created team; create →
///                owner, join → paste an invite link and exit the wizard)
///   2 board    — name + optional repository with inline GitHub connect
///   3 invite   — mint ONE invite link, or skip (`InviteLinkCreator`; the
///                control is absent entirely at the seat cap — 3.1.1)
///   4 devices  — where coding sessions run, or skip. LAST because acting on
///                it means leaving for another machine.
///   5 done     — drops into the app
///
/// `onboarding.complete` is called as soon as the first BOARD exists (steps 3
/// and 4 are both skippable, so the account must not stay half-onboarded if
/// the user quits on one of them); the final page only flips the local
/// `needsOnboarding` flag so the nav gate in AppNavigator stops showing this
/// screen. The join path is flipped by `teamInvites.accept` server-side
/// (mirrored locally). The server also backfills onboardingCompletedAt on
/// session reads for users who already have a board in a team
/// (lib/auth/onboarding.ts), so a stale account self-heals via
/// reconcileWithServer before the user ever creates anything.
///
/// Copy for steps 2-4 lives in `OnboardingCopy` and is byte-identical across
/// the four clients; step 1 keeps the mobile wording of `TeamSetupView`.
struct OnboardingView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.motion) private var motion

    private static let welcomePage = 0
    private static let teamPage = 1
    private static let boardPage = 2
    private static let invitePage = 3
    private static let devicesPage = 4
    private static let donePage = 5

    @State private var page = OnboardingView.welcomePage
    @State private var teamId: String?
    @State private var resolvingTeam = true
    @State private var teamError: String?
    /// Step 3: a link was minted, so the trailing button reads "Continue".
    @State private var invitedSomeone = false
    /// Step 4: at least one of the caller's OWN machines is registered.
    @State private var hasOwnDevice = false
    // Deliberately sticky once set: flipping needsOnboarding swaps this view out.
    @State private var finishing = false

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(spacing: 0) {
                    // EXP-523: steps push in from the trailing edge and leave
                    // to the leading one, the same direction the app's own
                    // navigation pushes — the wizard used to cut between
                    // pages with no indication it had moved forward.
                    // `.id(page)` is what makes SwiftUI treat each step as a
                    // distinct view so the transition has something to run on.
                    Group {
                        switch page {
                        case Self.welcomePage: welcomeStep
                        case Self.teamPage: teamStep
                        case Self.boardPage: boardStep
                        case Self.invitePage: inviteStep
                        case Self.devicesPage: devicesStep
                        default: doneStep
                        }
                    }
                    .id(page)
                    .transition(
                        .asymmetric(
                            insertion: .move(edge: .trailing).combined(with: .opacity),
                            removal: .move(edge: .leading).combined(with: .opacity)
                        )
                    )

                    // The wizard is the FIRST authed surface, so a session the
                    // server has invalidated (deleted account, revoked session)
                    // strands the user on whichever page they reached — the
                    // escape is persistent rather than bolted onto the team
                    // step's error state.
                    Spacer().frame(height: 32)
                    Button(OnboardingCopy.signOut) { signOut() }
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .buttonStyle(.plain)
                }
                .padding(.horizontal, 32)
                .padding(.vertical, 48)
                .frame(maxWidth: .infinity)
                .animation(motion.standard, value: page)
            }
        }
        .task { await reconcileWithServer() }
    }

    // MARK: - Step headers

    private func stepHeader(_ title: String, _ subtitle: String) -> some View {
        VStack(spacing: 0) {
            Text(title)
                .font(.system(size: 24, weight: .bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Spacer().frame(height: 8)

            Text(subtitle)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)

            Spacer().frame(height: 24)
        }
    }

    /// The ONE trailing control the two skippable steps share: "Skip for now"
    /// until the step was actually used, "Continue" after.
    private func advanceButton(done: Bool, action: @escaping () -> Void) -> some View {
        VStack(spacing: 0) {
            Spacer().frame(height: 20)
            GlassSubmitButton(
                done ? OnboardingCopy.continueLabel : OnboardingCopy.skip,
                action: action
            )
        }
    }

    // MARK: - Welcome

    private var welcomeStep: some View {
        VStack(spacing: 0) {
            Spacer().frame(height: 64)

            Text("Exponential")
                .font(.system(size: 34, weight: .bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Spacer().frame(height: 12)

            Text("Track issues and ship with your team.")
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)

            Spacer().frame(height: 48)

            GlassSubmitButton("Get started") {
                withAnimation(motion.standard) { page = Self.teamPage }
            }
        }
    }

    // MARK: - Step 1: Create or join a team

    private var teamStep: some View {
        VStack(spacing: 0) {
            Text("Set up your team")
                .font(.system(size: 24, weight: .bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Spacer().frame(height: 8)

            Text("Create a team, or join one with an invite link from a teammate.")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)

            Spacer().frame(height: 24)

            Group {
                if resolvingTeam {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small).tint(.white.opacity(0.6))
                        Text("Checking your teams…")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                    .padding(.vertical, 32)
                } else if let teamError {
                    VStack(spacing: 12) {
                        Text(teamError)
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .multilineTextAlignment(.center)
                        GlassSubmitButton(OnboardingCopy.retry) {
                            Task { await resolveTeam() }
                        }
                    }
                    .padding(24)
                    .glassCard()
                } else {
                    TeamSetupView(
                        onCreated: { team in
                            teamId = team.id
                            withAnimation(motion.standard) { page = Self.boardPage }
                        },
                        onJoined: {
                            // teamInvites.accept stamps onboardingCompletedAt
                            // server-side; mirror it locally so the nav gate
                            // exits the wizard — joiners land in the team they
                            // just joined, no board step.
                            deps.auth.markOnboardingCompleted(
                                ISO8601DateFormatter().string(from: Date())
                            )
                        }
                    )
                }
            }
        }
        .task { await resolveTeam() }
    }

    // MARK: - Step 2: Create your first board

    private var boardStep: some View {
        VStack(spacing: 0) {
            stepHeader(OnboardingCopy.boardTitle, OnboardingCopy.boardSubtitle)

            if let teamId {
                CreateBoardForm(
                    accountId: deps.auth.activeAccountId ?? "",
                    teamId: teamId,
                    minimal: true,
                    onCreated: { _ in
                        // The account is onboarded the moment a board exists:
                        // both remaining steps are skippable, and quitting on
                        // one of them must not re-open the wizard.
                        Task { await completeOnboarding() }
                        withAnimation(motion.standard) { page = Self.invitePage }
                    }
                )
                .padding(24)
                .glassCard()
            } else {
                // Unreachable in practice — the team step always sets teamId
                // before advancing here.
                ProgressView().tint(.white.opacity(0.6)).padding(.vertical, 32)
            }
        }
    }

    // MARK: - Step 3: Invite your teammates

    private var inviteStep: some View {
        VStack(spacing: 0) {
            stepHeader(OnboardingCopy.inviteTitle, OnboardingCopy.inviteSubtitle)

            if let teamId {
                InviteLinkCreator(
                    accountId: deps.auth.activeAccountId ?? "",
                    teamId: teamId,
                    onMinted: { invitedSomeone = true }
                )
            }

            advanceButton(done: invitedSomeone) {
                withAnimation(motion.standard) { page = Self.devicesPage }
            }
        }
        // `contain` keeps the creator's own controls queryable inside the
        // page container the capture lane waits on.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("onboarding-invite-step")
    }

    // MARK: - Step 4: Set up your devices

    private var devicesStep: some View {
        VStack(spacing: 0) {
            stepHeader(OnboardingCopy.devicesTitle, OnboardingCopy.devicesSubtitle)

            OnboardingDevicesStep(
                accountId: deps.auth.activeAccountId ?? "",
                onDevicesChanged: { hasOwnDevice = $0 }
            )

            advanceButton(done: hasOwnDevice) {
                withAnimation(motion.standard) { page = Self.donePage }
            }
        }
    }

    // MARK: - Done

    private var doneStep: some View {
        VStack(spacing: 0) {
            Spacer().frame(height: 64)

            AppIcon(AppIcons.uiSuccess, size: AppIcon.Size.xlarge)
                .foregroundStyle(DesignTokens.Semantic.green)

            Spacer().frame(height: 20)

            Text(OnboardingCopy.doneTitle)
                .font(.system(size: 28, weight: .bold))
                .foregroundStyle(.white)
                .multilineTextAlignment(.center)

            Spacer().frame(height: 12)

            Text(OnboardingCopy.doneBody)
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .multilineTextAlignment(.center)

            Spacer().frame(height: 48)

            GlassSubmitButton(
                finishing ? "Opening…" : OnboardingCopy.doneButton,
                enabled: !finishing
            ) {
                Task { await finish() }
            }
        }
    }

    // MARK: - Actions

    /// Local-only sign-out of the active account — the same teardown the
    /// dead-session gate performs (SessionGate → SyncManager): drop the token,
    /// stop this account's sync, keep the record and its cache. No server-side
    /// revocation: the session this escapes from may be one the server already
    /// refuses, so the call would only 401.
    private func signOut() {
        guard let accountId = deps.auth.activeAccountId else { return }
        Task {
            await deps.syncManager.signOut(accountId: accountId)
            deps.auth.signOutLocally(accountId: accountId)
        }
    }

    /// The server backfills onboardingCompletedAt on session reads for users
    /// who already have a board in a team (the unified rule in
    /// lib/auth/onboarding.ts). Re-read the session on appear so an account
    /// whose flag was still null at login self-heals here instead of showing
    /// this screen again.
    private func reconcileWithServer() async {
        guard let accountId = deps.auth.activeAccountId,
              let user = await deps.authApi.fetchSession(accountId: accountId),
              let completedAt = user.onboardingCompletedAt
        else { return }
        deps.auth.markOnboardingCompleted(completedAt)
    }

    /// Resolve an existing default team (teams.getDefault NEVER creates —
    /// EXP-188). A user who already has a membership (e.g. re-running a
    /// half-finished wizard) skips straight to the board step; a fresh signup
    /// gets the create-or-join choice.
    private func resolveTeam() async {
        guard teamId == nil, let accountId = deps.auth.activeAccountId else {
            resolvingTeam = false
            return
        }
        resolvingTeam = true
        teamError = nil
        do {
            if let team = try await deps.teamsApi.getDefault(accountId: accountId) {
                teamId = team.id
                withAnimation(motion.standard) { page = capturePage() ?? Self.boardPage }
            }
        } catch {
            teamError = error.trpcUserMessage
        }
        resolvingTeam = false
    }

    /// Screenshot hook (capture only, EXP-725): under `-uiTesting`, the launch
    /// argument `-uiTestingOnboardingStep invite|devices` parks the wizard on
    /// step 3 or 4 once a team has resolved, so the two capture suites can
    /// photograph them without creating a team and burning the seeded invite.
    /// The VALUE follows the key in `ProcessInfo.arguments`.
    private func capturePage() -> Int? {
        let arguments = ProcessInfo.processInfo.arguments
        guard arguments.contains("-uiTesting"),
              let keyIndex = arguments.firstIndex(of: "-uiTestingOnboardingStep"),
              arguments.index(after: keyIndex) < arguments.endIndex
        else { return nil }
        switch arguments[arguments.index(after: keyIndex)] {
        case "invite": return Self.invitePage
        case "devices": return Self.devicesPage
        default: return nil
        }
    }

    /// Flips the SERVER flag. Called once the first board exists, not on the
    /// done page — everything after the board is skippable.
    private func completeOnboarding() async {
        guard let accountId = deps.auth.activeAccountId else { return }
        try? await deps.onboardingApi.complete(accountId: accountId)
    }

    private func finish() async {
        guard !finishing else { return }
        finishing = true
        deps.auth.markOnboardingCompleted(ISO8601DateFormatter().string(from: Date()))
    }
}
