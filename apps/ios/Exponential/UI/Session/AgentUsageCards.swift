import ExpCore
import ExpUI
import SwiftUI

/// EXP-909: the session's **Usage overlay** — ONE layout on all four clients.
///
/// It answers three questions in one column, top to bottom: how full is the
/// session's own context (the Context block), which account is it spending
/// (the header) and how much of that account is left (the windows). Two
/// lines per window (title + countdown, then meter + `NN%`), ONE bar
/// primitive (`AgentUsageTrack`, ExpUI), the plan said ONCE in the header.
///
/// The device is still the only writer of the numbers (it probes its own CLIs
/// and reports on heartbeat); everything here reads the synced row through the
/// pure ×4 rules in `AgentUsagePresentation` / `AgentAccountsRows`.

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

/// The session screen's "Usage" sheet — the composer ring's destination.
///
/// Content-fitted (EXP-687): a machine reporting many windows grows the sheet
/// up to the shared 85 % cap, then scrolls. It hides once the run has ended
/// (the host's limits are then nobody's business) — its presenter decides that.
struct AgentUsageSheet: View {
    /// The run's agent — the header's brand mark.
    let agent: String?
    /// EXP-909: the login this session SPENDS (`AgentSessionModel.runAccount`).
    /// Nil = unknown, which is not "the ambient one": the header then falls
    /// back to the machine's own account report.
    let runAccount: AgentProfileUsageRow?
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
    /// EXP-1051: what is IN that window — the layers the device attributed
    /// before the first turn, off the latest-wins `context_layout` event. Nil
    /// (or empty) still draws the block: the conversation and the free rest
    /// are derived from the usage alone.
    var contextLayout: [ContextSegment]? = nil
    /// EXP-909: ONE usage refresh, requested as the overlay opens (no polling
    /// loop — the heartbeat delivers within 30 s). The model decides whether
    /// the machine may take it.
    var onOpen: (() -> Void)? = nil

    /// EXP-1051: the context window, folded ×4. Nil while the engine has
    /// published no usage (or a window of unknown size) — there is then no
    /// scale to draw the layers against.
    private var contextWindow: ContextWindowView? {
        ContextLayoutPresentation.contextWindowView(
            usage: sessionUsage, segments: contextLayout
        )
    }

    var body: some View {
        GlassSheetChrome(title: "Usage") {
            VStack(alignment: .leading, spacing: 12) {
                // EXP-1051: FIRST, above the account header — the run's own
                // window is what the composer ring was reporting when it was
                // tapped, and the machine's rate-limit windows are the second
                // question.
                if let contextWindow {
                    ContextWindowBlock(
                        view: contextWindow,
                        cost: AgentUsagePresentation.formatUsageCost(sessionUsage?.costUsd)
                    )
                    GlassDivider()
                }
                header
                GlassDivider()
                windowsBlock
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
            if let badge = runAccount.flatMap(AgentAccountsRows.healthBadge) {
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
        runAccount.map(AgentAccountsRows.loginLabel) ?? AgentUsagePresentation.accountCaption(account)
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
}

/// EXP-1051: the run's context window — the headline the old
/// `SessionContextBlock` printed, over a STACKED bar that says where the
/// window went, and a legend that names every layer.
///
/// Closed it is one line and one bar (what a glance wants: how full, and how
/// much of that was there before the first turn). Open it lists the layers,
/// `≈`-marked where the device estimated rather than measured them. Every
/// string and every percent comes from `ContextLayoutPresentation`, the
/// fixture-locked ×4 fold — nothing here formats a number itself.
struct ContextWindowBlock: View {
    let view: ContextWindowView
    /// EXP-746: the run's spend, which used to ride the context line and still
    /// has nowhere better to sit.
    var cost: String? = nil

    @State private var open = false
    @Environment(\.motion) private var motion

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button {
                // EXP-1243: the shared expand/collapse token, not a literal.
                withAnimation(motion.standard) { open.toggle() }
            } label: {
                HStack(spacing: 8) {
                    Text(ContextLayoutPresentation.title.uppercased())
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Spacer(minLength: 0)
                    if let cost {
                        Text(cost)
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                    Text(view.headline)
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                    AppIcon(open ? AppIcons.uiChevronDown : AppIcons.uiChevronRight, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("session-context-window")
            SegmentedTrack(slices: view.bar, ticks: view.ticks)
            if open {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(view.legend) { row in
                        legendRow(row)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Swatch · label (· what the device found there) · tokens · share. The
    /// `≈` is the whole of the measured/estimated distinction: a device that
    /// counted a layer says `21k`, one that divided its characters says
    /// `≈21k`.
    private func legendRow(_ row: ContextLegendRow) -> some View {
        HStack(alignment: .top, spacing: 8) {
            RoundedRectangle(cornerRadius: 3, style: .continuous)
                .fill(SegmentedTrack.tone(row.tone))
                .frame(width: 10, height: 10)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 1) {
                Text(row.label)
                    .font(.caption)
                    .foregroundStyle(.white)
                // Today only `project` names what it loaded (`CLAUDE.md,
                // ~/.claude/CLAUDE.md`), but any layer that does gets to say
                // so.
                if let detail = row.detail {
                    Text(detail)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 0)
            Text("\(row.estimated ? "≈" : "")\(row.tokens)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text(row.percent)
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .frame(width: 44, alignment: .trailing)
        }
        .accessibilityElement(children: .combine)
    }
}
