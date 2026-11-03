import ExpUI
import ExpCore
import SwiftUI

/// The invite landing page (#EXP-1632, web `/invite/$token`, Android
/// `InviteAcceptScreen`): the invite is READ first and shown as web's card
/// (the users disc, "Team invite", "You've been invited to join", the team +
/// role in an inner card, a full-width Accept invite); nothing is accepted
/// until the caller taps it. A used or expired invite says so in place of
/// the button; an unreadable one is the "Invalid invite" card.
struct InviteAcceptView: View {
    let token: String
    /// Where a finished accept leads. Nil = dismiss (the pushed route); the
    /// onboarding wizard passes its own exit (the invite completes
    /// onboarding server-side).
    var onFinished: (() -> Void)? = nil

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var loading = true
    @State private var preview: InvitePreview?
    /// The invite could not be read: the "Invalid invite" card.
    @State private var loadError: String?
    @State private var accepting = false
    @State private var accepted = false
    // FEED-79: a failed accept is a toast; the card stays for a retry.
    @Environment(\.toaster) private var toaster
    /// EXP-1169: accepted, and the caller owns no machine, so the join step
    /// replaces the card until its advance.
    @State private var showDevicesStep = false

    var body: some View {
        ZStack {
            AppBackground()

            if showDevicesStep {
                ScrollView {
                    DevicesStepView(accountId: accountId) { finish() }
                        .padding(.horizontal, 32)
                        .padding(.vertical, 48)
                        .frame(maxWidth: .infinity)
                }
            } else if loading {
                // EXP-1275: skeleton rows, never a centred spinner.
                SkeletonRows(dot: false)
                    .frame(maxWidth: 448)
                    .padding(16)
            } else {
                // Centred vertically (web), scrolling only once it outgrows
                // the screen.
                GeometryReader { proxy in
                    ScrollView {
                        card
                            .frame(maxWidth: 448)
                            .padding(16)
                            .frame(maxWidth: .infinity, minHeight: proxy.size.height)
                    }
                    .scrollBounceBehavior(.basedOnSize)
                }
            }
        }
        .task { await load() }
    }

    // MARK: - Card

    private var isExpired: Bool {
        guard let expiresAt = preview?.expiresAt, let date = DevicesList.parseIso(expiresAt) else { return false }
        return date < Date()
    }

    private var isUsed: Bool { preview?.acceptedAt != nil }

    private var card: some View {
        VStack(spacing: 24) {
            header
            if let preview {
                VStack(spacing: 16) {
                    VStack(spacing: 4) {
                        Text(preview.teamName)
                            .font(.title3.weight(.semibold))
                            .foregroundStyle(.white)
                            .multilineTextAlignment(.center)
                        Text("Role: \(preview.role)")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                    .padding(16)
                    .frame(maxWidth: .infinity)
                    .overlay(
                        RoundedRectangle(cornerRadius: DesignTokens.Radius.lg)
                            .strokeBorder(GlassTokens.strokeCard, lineWidth: 1)
                    )

                    if isExpired {
                        notice("This invite has expired", tone: DesignTokens.Palette.destructive)
                    } else if isUsed && !accepted {
                        notice("This invite has already been used", tone: .white.opacity(TextOpacity.secondary))
                    } else {
                        GlassSubmitButton(
                            "Accept invite",
                            enabled: !accepted,
                            loading: accepting || accepted,
                            pendingWord: "Joining"
                        ) {
                            Task { await accept() }
                        }
                        .accessibilityIdentifier("invite-accept")
                    }
                }
            }
        }
        .padding(.vertical, 24)
        .padding(.horizontal, 24)
        .frame(maxWidth: .infinity)
        .glassCard()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("invite-accept-card")
    }

    /// Web's card header: the users disc over the title and its muted line.
    private var header: some View {
        VStack(spacing: 8) {
            Circle()
                .fill(GlassTokens.fillCard)
                .frame(width: 40, height: 40)
                .overlay {
                    AppIcon(AppIcons.uiTeam, size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                .accessibilityHidden(true)
                .padding(.bottom, 8)
            Text(loadError == nil ? "Team invite" : "Invalid invite")
                .font(.headline)
                .foregroundStyle(.white)
            if let loadError {
                subtitle(loadError)
            } else if preview != nil {
                subtitle("You've been invited to join")
            }
        }
        .frame(maxWidth: .infinity)
    }

    private func subtitle(_ text: String) -> some View {
        Text(text)
            .font(.subheadline)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .multilineTextAlignment(.center)
    }

    private func notice(_ text: String, tone: Color) -> some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
            Text(text)
                .font(.subheadline)
        }
        .foregroundStyle(tone)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: - Actions

    private func load() async {
        guard preview == nil, loadError == nil else { return }
        do {
            preview = try await deps.teamInvitesApi.getByToken(accountId: accountId, token: token)
        } catch {
            loadError = error.trpcUserMessage
        }
        loading = false
    }

    private func accept() async {
        guard !accepting, !accepted else { return }
        accepting = true
        do {
            try await deps.teamInvitesApi.accept(accountId: accountId, token: token)
            accepted = true
            // Own rows are local and survive the pipeline restart below,
            // so the read never waits on the network.
            var owns = await ownsDevice()
            // Membership just changed: every shape's server-derived where
            // clause rotated, and the in-flight live long-polls would keep
            // the OLD scope for up to ~60s. Relaunch the pipeline so the
            // joined team syncs in seconds (EXP-43 drain-lag fix).
            await deps.syncManager.restartPipeline(accountId: accountId)
            // A fresh install may not have its devices rows yet: give the
            // shape up to 2s before offering the join step (web waits 4s).
            var waited = 0
            while !owns && waited < 8 {
                try? await Task.sleep(for: .milliseconds(250))
                waited += 1
                owns = await ownsDevice()
            }
            accepting = false
            if owns {
                finish()
            } else {
                showDevicesStep = true
            }
        } catch {
            accepting = false
            toaster.error(
                FeedbackStates.Errors.title(verb: "accept", thing: "the invite"),
                description: error.trpcUserMessage
            )
        }
    }

    private func finish() {
        if let onFinished { onFinished() } else { dismiss() }
    }

    private func ownsDevice() async -> Bool {
        await DeviceQueries.ownsDevice(db: deps.db, accountId: accountId, userId: deps.auth.userId)
    }
}
