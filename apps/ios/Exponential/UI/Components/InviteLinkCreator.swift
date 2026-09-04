import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// The invite-LINK creator (EXP-725) — the one place the app mints an invite,
/// shared by the first-run wizard's step 3 and team settings → Members.
///
/// App Store 3.1.1: the control is shown ONLY while the team has free seats.
/// At the cap it renders NOTHING — no hint, no pointer to the web, not even
/// the neutral plan message. Seats come from `teams.inviteCapacity` (members
/// plus PENDING invites vs. the plan's seats; nil = unlimited), refreshed from
/// the synced membership/invite rows rather than polled: one GRDB observation
/// over the two counts wakes a debounced re-fetch, so accepting the link on
/// another device removes the control here without a manual reload.
///
/// A capacity read that FAILS resolves to "unlimited" on purpose — the server
/// still gates the mint (`PRECONDITION_FAILED`, `isPlanLimitError`), and a
/// transient failure must not hide a control the user is entitled to. The
/// refusal itself is what flips the capacity to zero, silently.
struct InviteLinkCreator: View {
    let accountId: String
    let teamId: String
    /// Fired once a link exists — the wizard's trailing button swaps from
    /// "Skip for now" to "Continue" on it.
    var onMinted: () -> Void = {}

    @Environment(AppDependencies.self) private var deps

    /// Seats left. `.loading` renders nothing yet (the control must not blink
    /// in and out); `.known(nil)` is unlimited.
    private enum Capacity: Equatable {
        case loading
        case known(Int?)
    }

    @State private var capacity: Capacity = .loading
    @State private var inviteURL: URL?
    @State private var generating = false
    @State private var copied = false
    @State private var shareTarget: ShareTarget?
    /// Non-plan failures only — a plan refusal has no copy on iOS.
    @State private var errorText: String?
    @State private var observationTask: Task<Void, Never>?
    @State private var copyFlashTask: Task<Void, Never>?

    private var atCapacity: Bool {
        capacity == .known(0)
    }

    var body: some View {
        // `.loading` and `.known(0)` both render nothing: the first because
        // the answer is not in yet, the second because there is nothing to
        // offer. A VStack, not a Group: a Group whose only child is a false
        // conditional collapses to EmptyView, and modifiers on an EmptyView
        // never fire — the capacity task would never run and the card would
        // stay hidden forever.
        VStack(spacing: 0) {
            if case let .known(remaining) = capacity, remaining != 0 {
                card
            }
        }
        .task(id: teamId) { await refreshCapacity() }
        .onAppear { startObserving() }
        .onDisappear { stopObserving() }
        .sheet(item: $shareTarget) { target in
            ActivityShareSheet(items: [target.url])
        }
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let inviteURL {
                Text(inviteURL.absoluteString)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .textSelection(.enabled)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .frame(maxWidth: .infinity, alignment: .leading)

                HStack(spacing: 8) {
                    GlassPill(
                        copied ? OnboardingCopy.inviteCopied : OnboardingCopy.inviteCopy,
                        icon: copied ? AppIcons.uiSelected : AppIcons.uiCopy,
                        mode: .action { copy(inviteURL) }
                    )
                    GlassPill(
                        OnboardingCopy.share,
                        icon: AppIcons.uiShare,
                        mode: .action {
                            shareTarget = ShareTarget(
                                url: inviteURL, text: inviteURL.absoluteString
                            )
                        }
                    )
                    Spacer(minLength: 0)
                }
            } else {
                GlassSubmitButton(
                    OnboardingCopy.inviteGenerate,
                    enabled: !generating,
                    loading: generating
                ) {
                    Task { await generate() }
                }
                .accessibilityIdentifier("invite-generate")
            }

            if let errorText {
                Text(errorText)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("invite-link-creator")
    }

    // MARK: - Actions

    private func generate() async {
        guard !generating else { return }
        generating = true
        errorText = nil
        defer { generating = false }
        do {
            let token = try await deps.teamInvitesApi.create(
                accountId: accountId, teamId: teamId
            )
            let instanceUrl = deps.auth.accounts.first { $0.id == accountId }?.instanceUrl
            guard let url = WebLinks.invite(instanceUrl: instanceUrl, token: token) else {
                errorText = "Could not build the invite link."
                return
            }
            inviteURL = url
            copied = false
            onMinted()
            await refreshCapacity()
        } catch {
            if error.isPlanLimitError {
                // The seat cap, discovered on the mint: remove the control
                // outright rather than explain it (App Store 3.1.1). No error
                // copy — the server's message carries purchase language.
                capacity = .known(0)
                inviteURL = nil
                errorText = nil
            } else {
                errorText = error.trpcUserMessage
            }
        }
    }

    private func copy(_ url: URL) {
        Platform.copyToPasteboard(url.absoluteString)
        copied = true
        copyFlashTask?.cancel()
        copyFlashTask = Task {
            try? await Task.sleep(for: .seconds(2))
            if !Task.isCancelled { copied = false }
        }
    }

    private func refreshCapacity() async {
        do {
            capacity = .known(
                try await deps.teamsApi.inviteCapacity(accountId: accountId, teamId: teamId)
            )
        } catch {
            // Unknown, not zero: the server still gates the mint.
            capacity = .known(nil)
        }
    }

    // MARK: - Observation

    /// ONE restartable observation over the two counts that move capacity:
    /// accepted members and still-pending invites. `removeDuplicates` keeps
    /// unrelated row churn (a rename, a role change) from re-fetching, and the
    /// debounce collapses the burst a pipeline restart produces.
    private func startObserving() {
        stopObserving()
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let teamId = teamId
        observationTask = Task {
            let observation = ValueObservation
                .tracking { db -> [Int] in
                    let members = try TeamMemberEntity
                        .filter(Column("team_id") == teamId)
                        .fetchCount(db)
                    let pending = try TeamInviteEntity
                        .filter(Column("team_id") == teamId)
                        .filter(Column("accepted_at") == nil)
                        .fetchCount(db)
                    return [members, pending]
                }
                .removeDuplicates()
            do {
                for try await _ in observation.values(in: pool) {
                    try? await Task.sleep(for: .milliseconds(300))
                    if Task.isCancelled { return }
                    await refreshCapacity()
                }
            } catch {}
        }
    }

    private func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
        copyFlashTask?.cancel()
        copyFlashTask = nil
    }
}
