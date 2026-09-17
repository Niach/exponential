import ExpCore
import ExpUI
import SwiftUI

/// EXP-909: the run's **Usage overlay** — ONE layout on all four clients.
///
/// It answers three questions in one column, top to bottom: which account is
/// this run spending (the header), how much of that account is left (the
/// windows), how full is the run's own context (the Context line) — and then
/// offers the other logins the machine holds, each with its own compact bars,
/// so "I am out of limit" and "use my other account" stay one thought.
///
/// What EXP-875 found and this replaces: three-line cards per window, a
/// full-width "Switch to this account" button on every row that cut the email,
/// two different bar primitives, and a plan string repeated on every row.
/// Now: two lines per window (title + countdown, then meter + `NN%`), ONE bar
/// primitive (`AgentUsageTrack`, ExpUI), the plan said ONCE in the header, and
/// an icon-only switch.
///
/// The device is still the only writer of the numbers (it probes its own CLIs
/// and reports on heartbeat); everything here reads the synced row through the
/// pure ×4 rules in `AgentUsagePresentation` / `SessionAccountSwitch`.

/// Every window the machine reported for ONE login, two lines each. No group
/// headers: the card titles already say "Current session" / "All models" /
/// "<Label> only", which is what the headers used to repeat.
///
/// Numbers are NEVER hidden for being old (EXP-909) — past the freshness
/// window, or flagged stale by a machine that could not refresh them, the block
/// dims and dates itself with `usageAge`.
struct UsageWindows: View {
    let usage: AgentUsage
    var now = Date()

    private var cards: [UsageCard] {
        AgentUsagePresentation.usageGroups(usage, now: now).flatMap(\.cards)
    }

    private var age: String? {
        AgentUsagePresentation.usageAge(usage, now: now)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(cards) { card in
                windowRow(card)
            }
            if let age {
                Text(age)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .opacity(age == nil ? 1 : 0.5)
    }

    /// Line 1: the window's title and its countdown (or the idle session
    /// window's "Starts when a message is sent"). Line 2: the meter and the
    /// percentage — `NN%`, without the word "used": the title already says
    /// what is being measured.
    private func windowRow(_ card: UsageCard) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text(card.title)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 0)
                if !card.caption.isEmpty {
                    Text(card.caption)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                }
            }
            HStack(spacing: 8) {
                AgentUsageTrack(percent: card.percent, severity: card.severity)
                Text(percentText(card.percent))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
        .accessibilityElement(children: .combine)
    }

    /// A window the machine reported without a number draws an empty rail and
    /// says so, rather than claiming 0%.
    private func percentText(_ percent: Double?) -> String {
        guard let percent else { return "—" }
        return "\(Int(percent.rounded()))%"
    }
}

/// EXP-909: the COMPACT form — up to three tiny meters on ONE line, each
/// `label · meter · NN%`, wearing the WIRE labels (`5h` / `Week` / `Fable`),
/// which is the only reason three fit. Empty for a login with no windows.
///
/// Deliberately standalone and reusable: this is the same piece EXP-872 mounts
/// as the account picker's rate-limit preview, and the Devices page draws it
/// under every login. Web `UsageMini`, desktop `usage_bar::render_usage_mini`,
/// Android `AgentUsageMini`.
struct AgentUsageMini: View {
    let usage: AgentUsage?
    /// EXP-944: pass a clock and the 5h and Week bars caption themselves with
    /// when they reset (`resets in 2h 14m`, the wording every client shares).
    /// Omitted = bars only, which is what the tight surfaces (the usage
    /// sheet's other accounts, the account picker's preview) want.
    var now: Date?

    private var windows: [UsageMiniWindow] {
        AgentUsagePresentation.miniWindows(usage)
    }

    @ViewBuilder
    var body: some View {
        if !windows.isEmpty {
            HStack(alignment: .top, spacing: 10) {
                ForEach(windows) { window in
                    VStack(spacing: 2) {
                        HStack(spacing: 4) {
                            Text(window.label)
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                            AgentUsageTrack(
                                percent: window.percent,
                                severity: AgentUsagePresentation.severity(window.percent),
                                height: 4
                            )
                            .frame(width: 24)
                            Text(window.percent.map { "\(Int($0.rounded()))%" } ?? "—")
                                .font(.caption2.monospacedDigit())
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                        // Centred under its own bar, and only where there is a
                        // reset to name (`miniWindowReset`).
                        if let reset = resetCaption(window) {
                            Text(reset)
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                                .lineLimit(1)
                        }
                    }
                }
                Spacer(minLength: 0)
            }
            .accessibilityElement(children: .combine)
        }
    }

    private func resetCaption(_ window: UsageMiniWindow) -> String? {
        guard let now else { return nil }
        return AgentUsagePresentation.miniWindowReset(window, now: now)
    }
}

/// The steering screen's "Usage" sheet — the composer ring's destination and
/// the rate-limit wall's "Switch account" pill.
///
/// Content-fitted (EXP-687): a machine reporting many windows grows the sheet
/// up to the shared 85 % cap, then scrolls. It hides once the run has ended
/// (the host's limits are then nobody's business) — its presenter decides that.
struct AgentUsageSheet: View {
    /// The run's agent — the header's brand mark.
    let agent: String?
    /// EXP-909: the login this run SPENDS, resolved by the ×4 order
    /// (`SessionAccountSwitch.activeAccountIndex`). Nil = unknown, which is
    /// not "the ambient one": the header then falls back to the machine's own
    /// account report.
    let runAccount: SessionAccountOption?
    /// The host machine's top-level report for this run's agent — the header's
    /// caption when no listed profile resolved.
    let account: AgentAccount?
    /// The RESOLVED account's own rate-limit windows (A2: the profile's
    /// `usage`, falling back to the machine's top-level map only when that
    /// profile is its active login, or the run's account is unknown).
    var usage: AgentUsage?
    /// EXP-746: this run's own context window and spend off the relay's
    /// latest-wins `usage` event.
    var sessionUsage: AgentSessionUsage? = nil
    /// EXP-849: EVERY login the host machine reports for this run's agent. The
    /// header one is dropped from the Accounts section — a row saying "switch
    /// to the account you are already on" is noise.
    var accounts: [SessionAccountOption] = []
    /// EXP-849: whether this run's agent can change login at all (claude). A
    /// codex run still lists its accounts — read-only, because switching there
    /// means starting the next run on the other one.
    var supportsSwitch: Bool = false
    /// EXP-849: why switching onto a given login would be refused right now
    /// (`AgentSessionModel.accountSwitchRefusal`, the ×4 rule).
    var switchRefusal: ((SessionAccountOption) -> String?)? = nil
    /// A switch is on the wire — every row's control waits.
    var switching: Bool = false
    var onSwitch: ((SessionAccountOption) -> Void)? = nil
    /// EXP-909: ONE usage refresh, requested as the overlay opens (no polling
    /// loop — the heartbeat delivers within 30 s). The model decides whether
    /// the machine may take it.
    var onOpen: (() -> Void)? = nil

    /// The other logins — never the one the header names.
    private var otherAccounts: [SessionAccountOption] {
        guard let runAccount else { return accounts }
        return accounts.filter { $0.profileId != runAccount.profileId }
    }

    var body: some View {
        GlassSheetChrome(title: "Usage") {
            VStack(alignment: .leading, spacing: 12) {
                header
                GlassDivider()
                windowsBlock
                if let sessionUsage {
                    GlassDivider()
                    SessionContextBlock(usage: sessionUsage)
                }
                accountsBlock
            }
            .padding(.horizontal, 16)
            .padding(.top, 4)
            .padding(.bottom, 24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .onAppear { onOpen?() }
    }

    // MARK: - Header

    /// The brand mark, the run's account (truncating), and ONE trailing slot:
    /// the health badge when the login needs attention, else the plan — said
    /// here and nowhere else, so the rows below stay identities.
    private var header: some View {
        HStack(spacing: 8) {
            if let agent, let mark = AgentBrandMark.image(agent) {
                mark
                    .resizable()
                    .scaledToFit()
                    .frame(width: 16, height: 16)
            }
            Text(headerCaption)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 0)
            if let badge = runAccount?.health.badgeLabel {
                Text(badge)
                    .font(.caption2)
                    .foregroundStyle(DesignTokens.Semantic.yellow)
                    .lineLimit(1)
            } else if let plan = headerPlan {
                Text(plan)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("session-usage-header")
    }

    private var headerCaption: String {
        runAccount?.caption ?? AgentUsagePresentation.accountCaption(account)
    }

    /// The plan rides the trailing slot only when the caption is the EMAIL —
    /// otherwise the caption already IS the plan and printing it twice reads
    /// as a bug.
    private var headerPlan: String? {
        if let runAccount {
            guard runAccount.email != nil else { return nil }
            return runAccount.plan
        }
        guard let account, account.email?.isEmpty == false else { return nil }
        return account.plan.flatMap { $0.isEmpty ? nil : $0 }
    }

    // MARK: - Windows

    /// The resolved login's windows — or, while the machine has reported the
    /// login but nothing has probed its limits yet, the one word that says so.
    /// "No usage reported" would make a machine that is simply still working
    /// read as broken.
    @ViewBuilder
    private var windowsBlock: some View {
        if let usage, !(usage.windows ?? []).isEmpty {
            UsageWindows(usage: usage)
        } else {
            Text("Checking…")
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
    }

    // MARK: - Accounts

    /// EXP-849/EXP-909: the OTHER logins the host machine holds for this run's
    /// agent — each an identity line with its compact bars, and the icon-only
    /// switch. The footer says the one thing that is true of all of them: the
    /// run-level refusal, or what a switch costs.
    @ViewBuilder
    private var accountsBlock: some View {
        if !otherAccounts.isEmpty {
            GlassDivider()
            VStack(alignment: .leading, spacing: 10) {
                Text(SessionAccountSwitch.sectionTitle.uppercased())
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                ForEach(otherAccounts) { option in
                    SessionAccountRow(
                        option: option,
                        showsSwitch: supportsSwitch && onSwitch != nil,
                        refusal: rowRefusal(option),
                        switching: switching,
                        onSwitch: { onSwitch?(option) }
                    )
                }
                if let footer {
                    Text(footer)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityIdentifier("session-accounts")
        }
    }

    /// A refusal that is about the RUN (wrong agent, mid-turn, offline …) is
    /// said ONCE in the footer; only a row-specific one rides its row.
    private func rowRefusal(_ option: SessionAccountOption) -> String? {
        let refusal = switchRefusal?(option)
        return refusal == globalBlocker ? nil : refusal
    }

    private var globalBlocker: String? {
        SessionAccountSwitch.globalSwitchBlocker(otherAccounts.map { switchRefusal?($0) })
    }

    /// The run-level refusal when there is one, else the one-time cost of a
    /// switch — stated BEFORE the tap, because the relaunch re-reads the
    /// transcript on the account moved to.
    private var footer: String? {
        if let globalBlocker { return globalBlocker }
        return supportsSwitch ? SessionAccountSwitch.costNote : nil
    }
}

/// EXP-849: one OTHER login of the run's host machine — who it is, its health,
/// its own compact bars, and the switch. The control STAYS when a switch would
/// be refused and the reason sits under the row: a vanished button teaches
/// nothing. Mirrors Android's `SessionAccountRow` field for field.
struct SessionAccountRow: View {
    let option: SessionAccountOption
    /// Whether this run's agent supports switching at all (claude). A codex run
    /// lists its accounts without controls.
    let showsSwitch: Bool
    /// Why a switch onto this login is refused right now, nil when it is not.
    let refusal: String?
    let switching: Bool
    let onSwitch: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text(option.caption)
                    .font(.subheadline)
                    .foregroundStyle(option.signedIn ? Color.white : DesignTokens.Semantic.yellow)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let badge = option.health.badgeLabel {
                    Text(badge)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.yellow)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                if showsSwitch {
                    switchControl
                }
            }
            AgentUsageMini(usage: option.usage)
            if let age = AgentUsagePresentation.usageAge(option.usage) {
                Text(age)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            // The refusal rides the ROW the control is on, so it is read where
            // the tap was meant to happen.
            if let refusal {
                Text(refusal)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityIdentifier("session-account-\(option.profileId)")
    }

    /// EXP-909: icon-only. The labelled pill cut the email on every row it sat
    /// beside; the label survives as the control's accessibility name, which
    /// is the only place it was load-bearing.
    @ViewBuilder
    private var switchControl: some View {
        if switching {
            ProgressView()
                .controlSize(.small)
                .tint(.white)
                .accessibilityLabel("Switching account")
        } else {
            CircleIconButton(
                AppIcons.uiSwap,
                accessibilityLabel: SessionAccountSwitch.switchLabel,
                size: 28,
                glyphSize: AppIcon.Size.small,
                enabled: refusal == nil,
                action: onSwitch
            )
            .accessibilityIdentifier("switch-account-\(option.profileId)")
        }
    }
}

/// EXP-746/EXP-909: the run's own context window and spend on ONE line —
/// `Context · 124k / 200k (62%) · $1.24` — with the shared meter under it. The
/// strings come from the ×4-locked pure rules; nothing here formats a number
/// itself.
struct SessionContextBlock: View {
    let usage: AgentSessionUsage

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text(AgentUsagePresentation.contextSectionTitle)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(
                    AgentUsagePresentation.formatContextUsage(
                        used: usage.contextUsed, size: usage.contextSize
                    ) ?? "—"
                )
                .font(.subheadline.weight(.medium).monospacedDigit())
                .foregroundStyle(.white)
                .lineLimit(1)
                Spacer(minLength: 0)
                if let cost = AgentUsagePresentation.formatUsageCost(usage.costUsd) {
                    Text(cost)
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
            AgentUsageTrack(
                percent: usage.percent.map(Double.init),
                severity: AgentUsagePresentation.severity(usage.percent.map(Double.init))
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}
