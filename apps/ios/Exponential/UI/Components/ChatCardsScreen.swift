import ExpCore
import ExpUI
import SwiftUI

/// EXP-1259 D12-D14: the iOS styleguide's chat cards (web
/// `apps/styleguide` entries `work-card`, `script-card`, `comment-call-card`;
/// the IDE styleguide and Android's screens draw the same specimens): THE Work
/// card (a workflow mid-review, a task list, a subagent), the Script card and
/// the Comment card, each from the REAL view over the shared models. Capture
/// only: the styleguide suite opens one with `-uiTestingStyleguide
/// <work-card|script-card|comment-call-card|…|ask-dock|dock-card|record-form-field-states|plan-card|mention-pill|voice-call|call-controls|call-pill|settings-voice|device-readiness|device-attention|work-screen|pr-checks|session-row>` (`sg_<name>`).
struct ChatCardsScreen: View {
    let kind: String

    @Environment(AppDependencies.self) private var deps

    static let kinds: Set<String> = ["record-form-field-states", "work-card", "script-card", "comment-call-card", "trace", "code-block", "chat-strip", "ask-dock", "dock-card", "plan-card", "chat-list", "phone-tab-bar", "phone-board-menu", "phone-team-menu", "team-avatar", "phone-page-header", "chat-header", "chat-transcript", "chat-menus", "schedule-picker", "mention-pill", "app-consent", "voice-call", "call-controls", "call-pill", "chat-selection", "needs-you", "results-tab", "results-tab-lightbox", "settings-voice", "settings-mcp", "device-readiness", "work-screen", "voice-meeting", "voice-meeting-consent", "sealed-locked", "pr-row", "pr-checks", "ci-run", "runner-settings", "session-row", "device-attention", "device-attention-menu"]

    var body: some View {
        if kind == "voice-meeting" || kind == "voice-meeting-consent" {
            // NEW-57: Settings → Voice with Meeting mode on (and THE consent
            // prompt open).
            VoiceMeetingSpecimens(consentOpen: kind == "voice-meeting-consent")
                .preferredColorScheme(.dark)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("chat-cards-screen")
        } else if kind == "settings-voice" {
            // NEW-71: THE Voice settings page over the seed's rows.
            VoiceSettingsSpecimen()
                .preferredColorScheme(.dark)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("chat-cards-screen")
        } else if kind == "settings-mcp" {
            // #VAPP-185: THE MCP settings page over sample rows.
            McpSettingsSpecimen()
                .preferredColorScheme(.dark)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("chat-cards-screen")
        } else if kind == "app-consent" {
            // VAPP-120: the consent card over a stand-in surface.
            AppConsentSpecimens()
        } else if kind == "chat-list" || kind == "chat-selection" {
            // A whole phone screen (its own nav bar and bottom bar);
            // #EXP-1388 `chat-selection` = the same list selecting two rows
            // over THE BAR.
            ChatListSpecimens(selection: kind == "chat-selection" ? ["b", "c"] : nil)
                .preferredColorScheme(.dark)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("chat-cards-screen")
        } else if kind == "needs-you" {
            // #FEED-191: THE entry row + THE list (needs-you.json `cases[1]`).
            NeedsYouSpecimens()
                .preferredColorScheme(.dark)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("chat-cards-screen")
        } else if kind == "results-tab" || kind == "results-tab-lightbox" {
            // #FEED-191: THE tab body (results-tab.json `tab.specimen`).
            ResultsTabSpecimens(lightbox: kind == "results-tab-lightbox")
                .preferredColorScheme(.dark)
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("chat-cards-screen")
        } else if kind == "phone-board-menu" || kind == "phone-team-menu" {
            // EXP-1554: a whole phone screen, so the panel grows from the
            // bottom strip where the live one does (Android's shot).
            ZStack {
                AppBackground()
                PhoneBoardMenuSpecimens(teamMenu: kind == "phone-team-menu")
            }
            .preferredColorScheme(.dark)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("chat-cards-screen")
        } else {
            cards
        }
    }

    private var cards: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    switch kind {
                    case "script-card": scriptCards
                    case "comment-call-card": commentCards
                    case "trace": traceSpecimens
                    case "code-block": codeBlockSpecimens
                    case "chat-strip": SessionTabsSpecimens()
                    case "ask-dock": AskDockSpecimens()
                    case "dock-card": DockCardSpecimens()
                    case "record-form-field-states": RecordFormFieldStatesSpecimen()
                    case "plan-card": PlanCardSpecimens()
                    case "phone-tab-bar": PhoneTabBarSpecimens()
                    case "team-avatar": TeamAvatarSpecimens()
                    case "phone-page-header": PhonePageHeaderSpecimens()
                    case "chat-header": ChatHeaderSpecimens()
                    case "chat-transcript": ChatTranscriptSpecimens()
                    case "chat-menus": ChatMenuSpecimens()
                    case "schedule-picker": SchedulePickerSpecimens()
                    case "mention-pill": MentionPillSpecimens()
                    case "sealed-locked": SealedLockedSpecimens()
                    case "voice-call": VoiceCallSpecimens()
                    case "call-controls": CallControlsSpecimens()
                    case "call-pill": CallPillSpecimens()
                    case "device-readiness": DeviceReadinessSpecimens()
                    case "device-attention": DeviceAttentionSpecimens()
                    case "device-attention-menu": DeviceAttentionSpecimens(menuOpen: true)
                    case "work-screen": WorkScreenSpecimens()
                    case "pr-row": PrRowSpecimens()
                    case "session-row": SessionRowSpecimens()
                    case "pr-checks": PrChecksSpecimens()
                    // #EXP-1804: the CI run page and the CI runner section.
                    case "ci-run": CiRunSpecimens()
                    case "runner-settings": RunnerSettingsSpecimens()
                    default: workCards
                    }
                }
                .padding(16)
            }
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-cards-screen")
    }

    // MARK: - Work card (D12)

    // EXP-1553: the seeded demo feed's ONE Work card ×4 (apps/web/scripts/
    // screenshot-desktop.ts `wf-launch-review`), finished, nothing else.
    private static let phases = [
        WorkflowCaptionPhase(index: 1, title: "Review"),
        WorkflowCaptionPhase(index: 2, title: "Verify"),
    ]

    private static let agents = [
        WorkCardWorkflowAgent(index: 1, label: "review:correctness", phaseIndex: 1, model: "opus", state: "done", durationMs: 41_000),
        WorkCardWorkflowAgent(index: 2, label: "review:performance", phaseIndex: 1, model: "opus", state: "done", durationMs: 52_000),
        WorkCardWorkflowAgent(index: 3, label: "verify:benchmarks", phaseIndex: 2, model: "sonnet", state: "done", durationMs: 64_000),
    ]

    @ViewBuilder
    private var workCards: some View {
        let workflow = WorkCards.card(.workflow(
            name: "cold-start-review", status: "completed",
            phases: Self.phases, agents: Self.agents, elapsedMs: 116_000
        ))
        WorkCardView(
            card: workflow,
            foldedPhases: WorkCards.foldedPhases(phases: Self.phases, agents: Self.agents, card: workflow),
            onOpenLane: { _, _ in }
        )
    }

    // MARK: - Trace (EXP-1324)

    @State private var completedOpen = false

    private static let profileOutput = """
    > Task :app:startupProfile
    Pixel 6a     p50 740 ms   p90 812 ms
    Galaxy A54   p50 790 ms   p90 840 ms
    BUILD SUCCESSFUL in 41s
    """

    private static let benchmarkOutput = """
    StartupBenchmark > coldStart FAILED
        java.lang.AssertionError: p90 812 ms > 800 ms

    12 tests completed, 1 failed
    """

    private static let tsSource = """
    export function coldStart(board: Board): Promise<void> {
      // Paint the cached snapshot first, subscribe after the first frame.
      const cached = snapshots.read(board.id)
      return render(cached ?? [], { deferSubscribe: true })
    }
    """

    private static let ansiOutput = [
        "$ ./gradlew :app:startupProfile",
        "> Task :app:startupProfile",
        "\u{1B}[33mwarning:\u{1B}[0m baseline profile is 3 days old",
        "Pixel 6a     p50 740 ms   p90 812 ms",
        "\u{1B}[32mBUILD SUCCESSFUL\u{1B}[0m in 41s",
    ].joined(separator: "\n")

    /// The web island (apps/styleguide/src/entries/trace.tsx) line for line:
    /// the transcript's tail at sm, THE Queue, two lines at the default size.
    @ViewBuilder
    private var traceSpecimens: some View {
        let g = Trace.Geometry.sm
        VStack(alignment: .leading, spacing: 24) {
            TraceStack(size: .sm) {
                TraceLine(Trace.tool(
                    toolKind: "execute", name: "Bash", detail: "./gradlew :app:startupProfile",
                    settled: false, failed: false
                ), size: .sm)
                TraceLine(Trace.reasoning(running: false, durationMs: 5_200), size: .sm)
                TraceCollapsible(size: .sm, initiallyOpen: true) {
                    TraceLine(Trace.tool(
                        toolKind: "execute", name: "Bash", detail: "./gradlew :app:startupProfile",
                        settled: true, failed: false
                    ), size: .sm)
                } content: {
                    CodeBlockView(Self.profileOutput, language: .ansi, scrollToEnd: false)
                }
                let subagent = Trace.subagent(label: "Explore", running: false, failed: false, durationMs: 72_000)
                HStack(spacing: g.chevronGap) {
                    TraceLine(
                        title: subagent.title, status: subagent.status, size: .sm,
                        icon: { AppIcon(TraceIcons.glyph(subagent.icon), size: g.icon) },
                        accessory: {
                            if let badge = subagent.description { TraceBadge(badge, size: .sm) }
                        }
                    )
                    AppIcon(AppIcons.traceChevron, size: g.icon)
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                }
                if let failed = Trace.failedTests(Trace.parseTestResults(Self.benchmarkOutput)) {
                    TraceCollapsible(size: .sm) {
                        TraceLine(failed, size: .sm)
                    } content: {
                        CodeBlockView(Self.benchmarkOutput, language: .ansi, destructive: true, scrollToEnd: false)
                    }
                }
                let read = Trace.tool(toolKind: "read", name: "Read", detail: "README.md", settled: true, failed: false)
                TraceLine(TraceLineModel(icon: read.icon, title: read.title, description: read.description, status: .pending), size: .sm)
            }
            if let queue = TaskQueue.build(TaskQueueInput(
                tasks: [
                    WorkCardTask(content: "Profile the cold start", status: "completed"),
                    WorkCardTask(content: "Defer the snapshot cache warm-up", status: "completed"),
                    WorkCardTask(content: "Measuring on the Galaxy A54", status: "in_progress"),
                    WorkCardTask(content: "Open the PR with the numbers", status: "pending"),
                ],
                backgroundTasks: [TaskQueueInput.BackgroundTask(
                    id: "b1", kind: "shell", description: "Rerun the web and web-phone captures", startedAt: 0
                )],
                waits: [],
                now: 130_000
            )) {
                TaskQueueBlock(queue: queue, completedOpen: $completedOpen)
            }
            TraceStack {
                TraceLine(Trace.tool(toolKind: "search", name: "Grep", detail: "persistOnStop", settled: true, failed: false))
                TraceLine(Trace.tool(toolKind: "execute", name: "Bash", detail: "bun test", settled: true, failed: true))
            }
        }
    }

    // MARK: - CodeBlock (EXP-1324)

    /// The web island (apps/styleguide/src/entries/code-block.tsx): a titled
    /// plain block, an `ansi` Output, the destructive Output, all at sm.
    @ViewBuilder
    private var codeBlockSpecimens: some View {
        VStack(alignment: .leading, spacing: 16) {
            CodeBlockView(Self.tsSource, title: "startup.ts", scrollToEnd: false)
            CodeBlockView(Self.ansiOutput, language: .ansi, scrollToEnd: false)
            CodeBlockView(Self.benchmarkOutput, language: .ansi, destructive: true, scrollToEnd: false)
        }
    }

    // MARK: - Script card (D13)

    /// EXP-1553: the seeded demo feed's code-mode call ×4 (13 calls, 2.1 s,
    /// `issues_pr_files` on APP-9 failed), finished, nothing else.
    private static let scriptIds = ["APP-5", "APP-9", "APP-10", "APP-14", "APP-16", "APP-17"]

    private static let scriptSource = """
    const ids = ["APP-5", "APP-9", "APP-10", "APP-14", "APP-16", "APP-17"]
    const issues = await Promise.all(
      ids.map((id) => tools.exponential.exponential_issues_get({ id }))
    )
    const chats = await Promise.all(
      ids.map((issueId) => tools.exponential.exponential_comments_list({ issueId }))
    )
    const files = await tools.exponential.exponential_issues_pr_files({ issueId: "APP-9" })
    return issues.map((issue, i) => `${issue.identifier} ${chats[i].length} comments`)
    """

    @ViewBuilder
    private var scriptCards: some View {
        let calls: [ScriptCall] =
            Self.scriptIds.enumerated().map { index, id in
                ScriptCall(tool: "exponential.exponential_issues_get", subject: id, ok: true, ms: 180 + index * 20)
            }
            + Self.scriptIds.enumerated().map { index, id in
                ScriptCall(tool: "exponential.exponential_comments_list", subject: id, ok: true, ms: 140 + index * 15)
            }
            + [ScriptCall(tool: "exponential.exponential_issues_pr_files", subject: "APP-9", ok: false, ms: 95)]
        ScriptCardView(
            card: ScriptCards.card(ScriptCardInput(running: false, count: calls.count, calls: calls, elapsedMs: 2_140)),
            source: Self.scriptSource
        )
    }

    // MARK: - Comment card (D14)

    /// EXP-1553: the seeded `DEMO_AGENT_COMMENT_ID` comment ×4 (Alex Carter
    /// on APP-10, via MCP, 20 minutes ago), finished, nothing else.
    private static let commentBody = "From APP-5: the board snapshot cache now persists on `ON_STOP` "
        + "(`BoardSnapshotCache.persistOnStop`), so the LRU can reuse its directory and eviction hook instead "
        + "of opening a second one.\n\nCold start after the change: **740 ms** p50 on the Pixel 6a."

    @ViewBuilder
    private var commentCards: some View {
        CommentCallCardView(
            author: "Alex Carter",
            caption: "via MCP · 20 minutes ago",
            running: false,
            hasBody: true,
            onOpen: {}
        ) {
            IssueChip(
                identifier: "APP-10",
                title: "Migrate image cache to on-disk LRU",
                iconName: IssueStatus.backlog.iconName,
                statusColor: IssueStatus.backlog.color,
                bodySize: DesignTokens.Transcript.bodySize
            )
        } mark: {
            if let image = AgentBrandMark.image("claude") {
                image
                    .resizable()
                    .scaledToFit()
                    .frame(width: 14, height: 14)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        } body: {
            AgentMarkdownText(
                text: Self.commentBody,
                context: AgentMarkdownContext(baseURL: nil, accountId: "", httpClient: deps.httpClient),
                style: .document
            )
        }
    }
}

/// EXP-1553 `pr-row` (web `apps/styleguide` entry `pr-row` ×4): THE PrRow
/// in its two shapes — a PR TREE nested with tree guides, a linear STACK on
/// its rail down to the base branch (the current member washed, `stack` on
/// the top row) — and an unlinked PR with the external-link glyph.
private struct PrRowSpecimens: View {
    private static let tree: [(identifier: String, title: String, depth: Int)] = [
        ("EXP-1250", "opening tabs closes all at some point", 0),
        ("EXP-1252", "tabs per team on desktop", 1),
        ("EXP-1253", "inbox stepping reuses one tab", 1),
    ]

    private static let stack = [
        StackRailMember(id: "100", identifier: "VAPP-100", title: "SwiftUI parity with the round-1/2 contract", current: true),
        StackRailMember(id: "98", identifier: "VAPP-98", title: "Exponential UI renderer hardening round 1"),
        StackRailMember(id: "93", identifier: "VAPP-93", title: "ui.exponential.at: the Exponential UI site"),
        StackRailMember(id: "91", identifier: "VAPP-91", title: "Exponential UI host API ×4"),
    ]

    var body: some View {
        let guides = TreeGuides.compute(depths: Self.tree.map(\.depth))
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                GlassSectionBand("Exponential")
                ForEach(Array(Self.tree.enumerated()), id: \.offset) { index, row in
                    PrRow(identifier: row.identifier, title: row.title, guide: guides[index])
                }
                StackRail(members: Self.stack, baseBranch: "master", word: "stack", onOpen: { _ in }, onMergeThrough: { _ in })
            }
            VStack(alignment: .leading, spacing: 4) {
                GlassSectionBand("Not linked to an issue")
                PrRow(identifier: "#1013", title: "Dockerfiles: stub the exponential-ui workspace packages") {
                    AppIcon(AppIcons.uiExternalLink, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
        }
        .padding(.horizontal, -16)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("pr-row-specimen")
    }
}

/// Session 20b7fdc2 `session-row` (web `apps/styleguide` entry `session-row`,
/// desktop `session_row.rs`, row for row): THE SessionRow under the Running
/// caption — the APP-5 tree (needs you, a working child, an idle grandchild),
/// APP-14 working, an ended root; bold = unread, the first row lit. The lead
/// is the session's run mark (`ChatSessionMark`, the card state).
private struct SessionRowSpecimens: View {
    private static let rows: [(identifier: String?, title: String, depth: Int, state: SessionState, unread: Bool)] = [
        ("APP-5", "Reduce cold start below 800 ms", 0, .needsYou, true),
        (nil, "Profile the launch path", 1, .working, false),
        (nil, "Trace the splash screen", 2, .idle, true),
        ("APP-14", "Group board issues by status", 0, .working, false),
        (nil, "Pre-cleanup release", 0, .done, false),
    ]

    var body: some View {
        let guides = TreeGuides.compute(depths: Self.rows.map(\.depth))
        VStack(alignment: .leading, spacing: 4) {
            GlassSectionBand("Running")
            ForEach(Array(Self.rows.enumerated()), id: \.offset) { index, row in
                SessionRow(
                    identifier: row.identifier,
                    title: row.title,
                    guide: guides[index],
                    active: index == 0,
                    unread: row.unread
                ) {
                    ChatSessionMark(state: row.state)
                }
            }
        }
        .padding(.horizontal, -16)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("session-row-specimen")
    }
}

/// #EXP-1732 (session-screen.json; the `chat-strip` entry, its id kept):
/// THE session screen's header tabs (THE segmented control, each count muted
/// after its label; an app's six scroll sideways), the Issues tab (THE issue
/// rows led by the `+`, a sub-issue indented) and the Sessions tab (THE tree
/// rows) over one tree, then THE chat composer's tool row (the + alone,
/// S-3), the quick replies and an agent chat's "+" rows.
private struct SessionTabsSpecimens: View {
    private static let folded: [ChatTab] = [
        .add, .conversation,
        .issue(issueId: "i14", identifier: "APP-14", nested: [
            ChatTabNestedIssue(issueId: "i16", identifier: "APP-16", depth: 1),
        ]),
        .issue(issueId: "i21", identifier: "APP-21", nested: []),
        .session(sessionId: "s1", name: "Chats impl", markState: .needsYou, nested: [
            ChatTabNestedSession(sessionId: "s2", name: "Web lane", markState: .working, depth: 1),
            ChatTabNestedSession(sessionId: "s3", name: "Android lane", markState: .needsYou, depth: 1),
            ChatTabNestedSession(sessionId: "s4", name: "Emulator shots", markState: .done, depth: 2),
        ]),
        .session(sessionId: "s5", name: "Docs", markState: .idle, nested: []),
    ]

    private static var items: ChatTreeItems { ChatTreeItems(tabs: folded) }

    private static func screen(_ kind: SessionScreen.Kind, _ tab: SessionScreen.Tab) -> SessionScreen.Screen {
        SessionScreen.screen(
            kind: kind, requested: tab, open: nil,
            counts: SessionScreen.Counts(issues: items.issues.count, sessions: items.sessions.count, results: 4)
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            caption("A chat: Chat · Issues · Sessions · Results, the counts muted")
            tabs(Self.screen(.session, .chat))
            caption("An agent: the same tabs, Sessions active")
            tabs(Self.screen(.agent, .sessions))
            caption("An app: six tabs scroll sideways")
            tabs(Self.screen(.app, .preview))
            caption("Issues: the + leads, a sub-issue indented")
            ChatIssuesTab(
                issues: Self.items.issues,
                issueStatus: { id in
                    let ids = Self.items.issues.map(\.issueId)
                    let key: IssueStatus = (ids.firstIndex(of: id) ?? 0) % 2 == 0 ? .inProgress : .backlog
                    return IssueStatusResolver.builtinFallbackTeam.first { $0.builtinKey == key }
                },
                onAdd: {},
                onOpen: { _ in }
            )
            .frame(height: 200)
            .background(GlassTokens.fillCard.opacity(0.4), in: RoundedRectangle(cornerRadius: 12))
            caption("Sessions: THE tree rows of every session under the root")
            ChatSessionsTab(sessions: Self.items.sessions) { _ in }
                .frame(height: 220)
                .background(GlassTokens.fillCard.opacity(0.4), in: RoundedRectangle(cornerRadius: 12))
            caption("THE composer: + · the field · the ring beside Send")
            composerRow()
            caption("EXP-1211: the agent's quick replies above the composer")
            VStack(spacing: 0) {
                // fixtures/session-chat.json `repliesCases[1].expected`.
                ComposerRepliesRow(
                    replies: ComposerReplies(messageId: "m2", items: [
                        ComposerReply(id: "a", text: "Raise the version numbers"),
                        ComposerReply(id: "b", text: "Open the PR"),
                    ]),
                    onSend: { _ in },
                    onDismiss: {}
                )
                FloatingComposer(
                    placeholder: SessionTree.ChatCopy.placeholder,
                    open: false,
                    collapsedLabel: SessionTree.ChatCopy.placeholder,
                    collapsedIdentifier: "chat-composer-collapsed",
                    onExpand: {}
                ) {
                    EmptyView()
                }
            }
            .padding(.horizontal, -16)
            // S-3: an agent chat's "+" (composer-menu.json surface `agent`).
            ChatComposerPlusMenu(
                conditions: ComposerMenu.Conditions(model: true),
                surface: .agent,
                agent: "claude",
                modelValue: "opus"
            ) { _ in }
                .background(GlassTokens.fillCard, in: RoundedRectangle(cornerRadius: 16))
        }
        .accessibilityIdentifier("chat-strip-specimens")
    }

    private func tabs(_ screen: SessionScreen.Screen) -> some View {
        SessionScreenTabs(tabs: screen.tabs, selection: screen.active) { _ in }
            .padding(.horizontal, -16)
    }

    /// #EXP-1762: THE flat one-line composer card: the `+` left of the
    /// field, the ring beside Send right (`SessionComposerCard`'s layout).
    private func composerRow() -> some View {
        let controls = SessionComposerFooter.Controls(
            modelValue: "opus", agent: "claude", onPickModel: { _ in },
            usageFraction: 0.42, usageSeverity: .normal, onUsage: {}
        )
        return GlassComposer(isOpaque: true, isFlat: true) {
            Text("Type / for commands")
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .frame(minHeight: GlassComposerTokens.toolHitSize)
        } strip: {
            EmptyView()
        } tools: {
            SessionComposerFooter(
                controls: controls, attachEnabled: true, onCamera: {}, onPhoto: {}, onFile: {},
                attachMenuOpen: .constant(false), onPlus: {}, part: .plus
            )
        } submit: {
            SessionComposerFooter(
                controls: controls, attachEnabled: true, onCamera: {}, onPhoto: {}, onFile: {},
                attachMenuOpen: .constant(false), part: .ring
            )
            GlassComposerSubmitButton(AppIcons.uiSubmit, accessibilityLabel: "Send", action: {})
        }
    }

    private func caption(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }
}

/// N-12: THE Dock specimen (`sg_ask-dock`, web `AskDockSpecimen`, the IDE's
/// `styleguide:ask-dock`; web `packages/ui/src/ask-dock-specimen.tsx` is THE
/// canonical one), line for line: one waiting plan (a single DockCard), the
/// pile (the queued messages, THE Queue, the suggested tasks, a plan, a
/// question: the front card whole, the others peeking out above it), the
/// pile fanned, and the plan's sheet (page 0 of the stepped plan: the plan,
/// Next in the footer, in the sheet's static frame). Real `DockCard` /
/// `DockPile` / `DockSheetSpecimenFrame`.
private struct AskDockSpecimens: View {
    private static let longPlan = [
        "## Plan: one Dock ×4",
        "",
        "1. Add `fixtures/dock.json` + the TS model, mirrored ×4.",
        "2. Float the waiting cards above the composer; the transcript is the one scroll.",
        "3. Pile several cards like the toasts; a tap opens one as a sheet.",
        "4. Share THE plan options: Yes · Yes, and start with a fresh context · No.",
        "5. Styleguide entry + scoped shots on every platform.",
        "6. Open the PR with the Guide.",
    ].joined(separator: "\n")

    private static let questions = [
        ChatPlanQuestion(id: "q-scope", prompt: "Which cards move into the Dock first?", custom: true, options: [
            TreeQuestionOption(label: "Plan and question cards", description: "The rest follows as its own issue"),
            TreeQuestionOption(label: "Everything", description: "Plan, question, tasks, queued messages, banners"),
        ]),
        ChatPlanQuestion(id: "q-eui", prompt: "Should DockCard also go into Exponential UI?", custom: false, options: [
            TreeQuestionOption(label: "Yes, as DockCard"),
            TreeQuestionOption(label: "Styleguide only for now"),
        ]),
    ]

    private static let asks: [Dock.Source] = [
        .plan(id: "p1", text: longPlan, sessionName: "Ask dock", questions: 2, createdAt: 10),
        .question(id: "q1", text: "Ship both fixes in one PR, or split them?", sessionName: "Dock polish", createdAt: 30),
    ]

    private static let state: [Dock.Source] = [
        .queued(["also check the watchdog once the root is idle", "then open the PR"]),
        .tasks(Dock.QueueSummary(
            todoTitle: "3 to do",
            items: [
                .init(label: "Measuring on the Galaxy A54", description: "2m 35s"),
                .init(label: "Rerun the web captures", description: nil),
                .init(label: "Open the PR with the numbers", description: nil),
            ],
            completedTitle: "2 completed"
        )),
        .suggestions(
            id: "m4", label: "3 suggested tasks", caption: "1 started",
            title: "Lazy-load the markdown editor", open: 2
        ),
    ]

    var body: some View {
        let single = Dock.rows([Self.asks[0]])
        let rows = Dock.rows(Self.state + Self.asks)
        VStack(alignment: .leading, spacing: 24) {
            section("One waiting plan") {
                DockCard(row: single[0], onOpen: {})
            }
            section("A plan, a question, the suggested tasks, the Queue and the queued messages, piled") {
                DockPile(rows: rows, expanded: .constant(false), onOpen: { _ in })
            }
            section("The pile fanned (hover or a tap)") {
                DockPile(rows: rows, expanded: .constant(true), onOpen: { _ in })
            }
            section("The plan's sheet") {
                PlanSheetSpecimen(row: single[0], card: ChatPlanRow(
                    id: "p1", sessionId: "a", plan: Self.longPlan, questions: Self.questions
                ))
            }
        }
        .accessibilityIdentifier("ask-dock-specimen")
    }

    private func section(_ title: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            content()
        }
    }
}

/// N-12: THE DockCard (`sg_dock-card`, web styleguide `dock-card`, the IDE's
/// `dock_card.rs`), line for line: one card (a question), a banner with its
/// action, five cards piled, fanned, and the sheet's static frame (the
/// pinned header, the scrolling body, the pinned footer). The ask-dock
/// specimen shows the plan and question cards on it; this one is the
/// primitive. Real `DockCard` / `DockPile` / `DockSheetSpecimenFrame`.
private struct DockCardSpecimens: View {
    private static let sources: [Dock.Source] = [
        .queued(["also check the watchdog once the root is idle", "then open the PR"]),
        .tasks(Dock.QueueSummary(
            todoTitle: "3 to do",
            items: [
                .init(label: "Measuring on the Galaxy A54", description: "2m 35s"),
                .init(label: "Rerun the web captures", description: nil),
                .init(label: "Open the PR with the numbers", description: nil),
            ],
            completedTitle: "2 completed"
        )),
        .suggestions(
            id: "m4", label: "3 suggested tasks", caption: "1 started",
            title: "Lazy-load the markdown editor", open: 2
        ),
        .banner(
            id: "wall", icon: "ui-usage", label: "Usage limit",
            text: "Claude's 5-hour window is used up until 14:00", tone: .warning, action: "Switch account"
        ),
        .question(id: "q1", text: "Ship both fixes in one PR, or split them?", sessionName: "Dock polish", createdAt: 30),
    ]

    /// EXP-1374: the session's own state cards (session-card.json `banner`,
    /// web `STATE_ROWS`): background tasks and (#EXP-1405) queued, waiting
    /// for its device. S-3: no interrupted card.
    private static let stateRows: [Dock.Row] = Dock.rows(["background", "queued"].map { id in
        .banner(
            id: id, icon: SessionTree.CardCopy.bannerIcons[id] ?? "",
            label: SessionTree.CardCopy.bannerLabels[id] ?? "",
            text: SessionTree.CardCopy.banner[id] ?? "", tone: .muted, action: nil
        )
    })

    private static func action(_ row: Dock.Row) -> AnyView? {
        guard row.kind == .banner, row.id == "banner:wall" else { return nil }
        return AnyView(GlassPill("Switch account", mode: .action {}))
    }

    var body: some View {
        let rows = Dock.rows(Self.sources)
        let question = rows.first { $0.kind == .question }!
        let banner = rows.first { $0.kind == .banner }!
        VStack(alignment: .leading, spacing: 12) {
            caption("One card: the label, the caption, the meta, the title, the chevron")
            DockCard(row: question, onOpen: {})
            caption("A banner: never opens; its action word trails")
            DockCard(row: banner, action: Self.action(banner))
            caption("The session's own state: background tasks, queued")
            ForEach(Self.stateRows, id: \.id) { row in
                DockCard(row: row)
            }
            caption("Five cards piled: the front one whole, the rest peeking out above it")
            DockPile(rows: rows, expanded: .constant(false), onOpen: { _ in }, renderAction: Self.action)
            caption("The pile fanned (hover or a tap): every card, 6px apart")
            DockPile(rows: rows, expanded: .constant(true), onOpen: { _ in }, renderAction: Self.action)
            caption("The sheet: the pinned header, the scrolling body, the pinned footer")
            DockSheetSpecimenFrame(row: question, height: 300) {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Ship both fixes in one PR, or split them?")
                        .font(.subheadline)
                        .foregroundStyle(.white)
                    Text(PlanCard.Copy.chooseOne)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            } footer: {
                GlassPill(PlanCard.Copy.answer, mode: .action {}, primary: true)
            }
        }
        .accessibilityIdentifier("dock-card-specimen")
    }

    private func caption(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }
}

/// The plan sheet at rest: page 0 of the stepped plan (the plan, no meta,
/// Next in the footer; web `ChatPlanSheet` at its first page), the
/// sheet's face without presenting.
private struct PlanSheetSpecimen: View {
    let row: Dock.Row
    let card: ChatPlanRow

    var body: some View {
        DockSheetSpecimenFrame(row: row, height: 420, onOpenFull: {}) {
            AgentMarkdownText(
                text: PlanCard.renderableMarkdown(card.plan),
                context: nil,
                style: .document.autolinked.planCard
            )
        } footer: {
            PlanCardFooter(onNext: {})
        }
    }
}

/// EXP-1280: THE rich plan / question card (`sg_plan-card`, web styleguide
/// `plan-card`, the IDE's `styleguide:plan-card`): session-chat.json's case
/// "EXP-1280: a rich plan card…" through the REAL row model
/// (`SessionChatRow.askCard`) and `ChatAskCard`: #EXP-1662 the pending plan's ONE
/// row, the answered question card (several picks, closed) and the approved
/// plan's row.
private struct PlanCardSpecimens: View {
    private static let pending = #"""
    {"kind":"plan","plan":"# Plan\n\n1. Lift the partition\n\n![before](/api/attachments/att-1)\n\n![after](upload:after)",
     "pictures":[{"label":"before","attachmentId":"att-1","width":1440,"height":900,"caption":"Before"},
                 {"label":"after","attachmentId":null,"width":null,"height":null,"caption":null},
                 {"label":"two","attachmentId":"att-2","width":800,"height":500},
                 {"label":"one","attachmentId":null,"width":null,"height":null}],
     "questions":[{"id":"layout","prompt":"Which layout?","multi":false,"custom":false,
                   "options":[{"label":"Two columns","description":"Pictures beside the text","picture":"two"},
                              {"label":"One column","picture":"one"},{"label":"Neither"}]},
                  {"id":"platforms","prompt":"Platforms","multi":true,
                   "options":[{"label":"iOS"},{"label":"Android","description":""}]},
                  {"id":"broken","options":[]},"junk"],
     "surface":{"root":{"id":"s","component":"Stack","props":{"direction":"vertical","gap":"sm"},
                        "children":[{"id":"t","component":"Text","props":{"text":"Pick the accent"}},
                                    {"id":"c","component":"ChoiceTiles","props":{"name":"accent",
                                     "options":[{"label":"Blue","value":"blue"},{"label":"Green","value":"green"}],
                                     "value":{"path":"/accent"}}}]},
                "data":{"accent":"blue"}},
     "decision":null,"fresh":null,"feedback":null,"answers":null,"surfaceData":null}
    """#

    private static let answered = #"""
    {"kind":"question","question":"Which platforms first?","options":["Web","iOS","Android"],
     "optionDescriptions":["The styleguide first",null,""],"optionPictures":["web",null,"ghost"],
     "pictures":[{"label":"web","attachmentId":"att-3","width":1200,"height":750}],
     "multi":true,"custom":false,"surface":null,"answer":"Web, Android","answers":["Web","Android"],
     "surfaceData":{"size":"lg"}}
    """#

    private static let approved = #"""
    {"kind":"plan","plan":"1. Old","decision":"approved","fresh":false,"feedback":"Go.",
     "answers":{"q":["Yes",7]},
     "questions":[{"id":"q","prompt":"Go?","multi":false,"custom":true,"options":[{"label":"Yes"}]}],
     "surface":{"nope":true}}
    """#

    /// #EXP-1561: session-chat.json case "#EXP-1561: a dismissed
    /// question…" — a question and a plan a person dismissed.
    private static let dismissedQuestion = #"""
    {"kind":"question","question":"Which first?","options":["Lazy-load","Skip"],"dismissed":true}
    """#

    private static let dismissedPlan = #"""
    {"kind":"plan","plan":"1. One","decision":"dismissed","fresh":false,"dismissed":true}
    """#

    private static func row(
        _ id: String, _ card: String, createdAt: String, answeredAt: String?, consumedAt: String?
    ) -> SessionChatRow? {
        SessionChatRow.askCard(TreeMessage(
            id: id,
            sessionId: "r2",
            authorKind: "agent",
            authorId: nil,
            body: "",
            card: TreeMessageCard.parse(card),
            answeredAt: answeredAt,
            consumedAt: consumedAt,
            createdAt: createdAt,
            authorSessionId: nil
        ))
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    var body: some View {
        let _ = Self.seedPictures(baseURL: deps.auth.instanceBaseURL(forAccountId: accountId))
        VStack(alignment: .leading, spacing: 24) {
            section(
                PlanCard.Copy.Specimen.pending,
                Self.row("n1", Self.pending, createdAt: "2026-10-13T10:01:00.000Z", answeredAt: nil, consumedAt: nil)
            )
            section(
                PlanCard.Copy.Specimen.answered,
                Self.row(
                    "n2", Self.answered, createdAt: "2026-10-13T10:02:00.000Z",
                    answeredAt: "2026-10-13T10:03:00.000Z", consumedAt: "2026-10-13T10:03:05.000Z"
                )
            )
            section(
                PlanCard.Copy.Specimen.approved,
                Self.row(
                    "n3", Self.approved, createdAt: "2026-10-13T10:04:00.000Z",
                    answeredAt: "2026-10-13T10:04:00.000Z", consumedAt: "2026-10-13T10:04:00.000Z"
                )
            )
            // #EXP-1561 (waiting-cards.json): THE dismissed card lines.
            VStack(alignment: .leading, spacing: 8) {
                Text(PlanCard.Copy.Specimen.dismissed)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                VStack(alignment: .leading, spacing: 14) {
                    ForEach([
                        Self.row(
                            "o2", Self.dismissedQuestion, createdAt: "2026-10-13T10:02:00.000Z",
                            answeredAt: "2026-10-13T10:05:00.000Z", consumedAt: "2026-10-13T10:00:01.000Z"
                        ),
                        Self.row(
                            "o3", Self.dismissedPlan, createdAt: "2026-10-13T10:03:00.000Z",
                            answeredAt: "2026-10-13T10:06:00.000Z", consumedAt: "2026-10-13T10:00:02.000Z"
                        ),
                    ].compactMap { $0 }, id: \.id) { row in
                        ChatAskCard(row: row)
                    }
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(PlanCard.TestIds.card)
    }

    // MARK: - Stand-in pictures (THE palette ×4)

    /// web `plan-card-specimen.tsx` `TONES` (desktop + Android draw the same):
    /// one diagonal gradient per attachment id, a 35% white bar, a 12% white
    /// panel, 800×500.
    private static let tones: [(id: String, from: UInt32, to: UInt32)] = [
        ("att-1", 0x3F3F46, 0x52525B),
        ("att-2", 0x1E3A5F, 0x3B82F6),
        ("att-3", 0x14532D, 0x22C55E),
    ]

    /// Seeds the markdown image cache under the exact keys the loader reads
    /// (`AttachmentURL.resolve`: the relative path signed out, else the
    /// instance's absolute url), so the body's picture and the tiles
    /// (`PlanCardPicture`) paint without a network.
    private static func seedPictures(baseURL: URL?) {
        for tone in tones {
            let path = "/api/attachments/\(tone.id)"
            let keys = Set([path, AttachmentURL.resolve(path, baseURL: baseURL)?.absoluteString].compactMap { $0 })
            guard keys.contains(where: { MarkdownImageCache.shared.image(for: $0) == nil }) else { continue }
            let image = standIn(from: tone.from, to: tone.to)
            for key in keys { MarkdownImageCache.shared.store(image, for: key) }
        }
    }

    private static func standIn(from: UInt32, to: UInt32) -> UIImage {
        let size = CGSize(width: 800, height: 500)
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.opaque = true
        return UIGraphicsImageRenderer(size: size, format: format).image { context in
            let cg = context.cgContext
            let colors = [uiColor(from).cgColor, uiColor(to).cgColor] as CFArray
            if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: colors, locations: [0, 1]) {
                cg.drawLinearGradient(gradient, start: .zero, end: CGPoint(x: size.width, y: size.height), options: [])
            }
            UIColor.white.withAlphaComponent(0.35).setFill()
            UIBezierPath(roundedRect: CGRect(x: 48, y: 48, width: 320, height: 28), cornerRadius: 6).fill()
            UIColor.white.withAlphaComponent(0.12).setFill()
            UIBezierPath(roundedRect: CGRect(x: 48, y: 100, width: 704, height: 352), cornerRadius: 12).fill()
        }
    }

    private static func uiColor(_ hex: UInt32) -> UIColor {
        UIColor(
            red: CGFloat((hex >> 16) & 0xFF) / 255,
            green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255,
            alpha: 1
        )
    }

    @ViewBuilder
    private func section(_ title: String, _ row: SessionChatRow?) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            if let row {
                ChatAskCard(row: row)
            }
        }
    }
}

/// S-3: the `voice-call` entry (voice-call.json `surfaceSpecimens`): THE
/// phone-call surface the live call screen draws (`VoiceCallSurface`), all
/// five rows stacked full width in one column (web-mobile), each in a
/// phone-shaped frame with its `name` captioned under it, static (rings at
/// their rest phase).
private struct VoiceCallSpecimens: View {
    /// The frame's height (web `h-[600px]`).
    private static let height: CGFloat = 600

    var body: some View {
        VStack(spacing: 16) {
            ForEach(Array(VoiceCall.surfaceSpecimens.enumerated()), id: \.offset) { _, specimen in
                VStack(spacing: 8) {
                    VoiceCallSurface(
                        view: specimen.view,
                        callee: VoiceCallCallee(name: specimen.agent),
                        timer: .fixed(specimen.timer),
                        level: specimen.level,
                        bands: specimen.bands,
                        animated: false,
                        onMinimize: {}
                    )
                    .frame(maxWidth: .infinity)
                    .frame(height: Self.height)
                    .background(DesignTokens.Palette.background)
                    .clipShape(RoundedRectangle(cornerRadius: 28, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 28, style: .continuous).stroke(DesignTokens.Palette.border, lineWidth: 1))
                    Text(specimen.name)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                }
            }
        }
        .frame(maxWidth: .infinity)
    }
}

/// S-3: the `call-controls` entry (eUI `CallButton`), web's gallery
/// (`CallControlsSpecimen`) with its captions: the round buttons in every
/// tone (neutral, pressed, speaker, speaker off, destructive, ringing with
/// Cancel under the red end), then the rows (phones in call; web + desktop
/// in call, muted; ringing), then the compact row (EXP-1681: the floating
/// call pill, `call-pill`).
private struct CallControlsSpecimens: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 32) {
            FlowLayout(spacing: 32, lineSpacing: 32) {
                labelled("neutral") {
                    CallButton(icon: AppIcons.callMute, label: VoiceCall.Copy.mute, action: {})
                }
                labelled("pressed") {
                    CallButton(icon: AppIcons.callUnmute, label: VoiceCall.Copy.unmute, toggled: true, action: {})
                }
                labelled("speaker") {
                    CallButton(icon: AppIcons.callSpeaker, label: VoiceCall.Copy.speaker, action: {})
                }
                labelled("speaker off") {
                    CallButton(icon: AppIcons.callSpeakerOff, label: VoiceCall.Copy.speakerOff, action: {})
                }
                labelled("destructive") {
                    CallButton(icon: AppIcons.callEnd, label: VoiceCall.Copy.endCall, tone: .destructive, action: {})
                }
                labelled("ringing: label under") {
                    CallButton(
                        icon: AppIcons.callEnd, label: VoiceCall.Copy.cancel, tone: .destructive,
                        caption: VoiceCall.Copy.cancel, action: {}
                    )
                }
            }
            FlowLayout(spacing: 48, lineSpacing: 32) {
                labelled("phones in call") {
                    CallControls {
                        CallButton(icon: AppIcons.callMute, label: VoiceCall.Copy.mute, action: {})
                        CallButton(icon: AppIcons.callSpeaker, label: VoiceCall.Copy.speaker, action: {})
                        CallButton(icon: AppIcons.callEnd, label: VoiceCall.Copy.endCall, tone: .destructive, action: {})
                    }
                }
                labelled("web + desktop in call, muted") {
                    CallControls {
                        CallButton(icon: AppIcons.callUnmute, label: VoiceCall.Copy.unmute, toggled: true, action: {})
                        CallButton(icon: AppIcons.callEnd, label: VoiceCall.Copy.endCall, tone: .destructive, action: {})
                    }
                }
                labelled("ringing") {
                    CallControls {
                        CallButton(
                            icon: AppIcons.callEnd, label: VoiceCall.Copy.cancel, tone: .destructive,
                            caption: VoiceCall.Copy.cancel, action: {}
                        )
                    }
                }
            }
            if let inCall = VoiceCall.surfaceSpecimens.first(where: { $0.name == "in call, speaking" }) {
                labelled("compact row: the floating call pill") {
                    VoiceCallPill(
                        view: inCall.view, callee: VoiceCallCallee(name: inCall.agent), timer: .fixed(inCall.timer),
                        level: inCall.level, bands: inCall.bands, width: CallPillSpecimens.width, animated: false
                    )
                }
                .frame(maxWidth: .infinity)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Web `Labelled`: the control over its caption, centred (`gap-3`,
    /// the control in an 80 pt tall slot).
    private func labelled(_ label: String, @ViewBuilder _ content: () -> some View) -> some View {
        VStack(spacing: 12) {
            content()
                .frame(minHeight: 80, alignment: .top)
            Text(label)
                .font(.caption)
                .foregroundStyle(DesignTokens.Palette.mutedForeground)
        }
    }
}

/// EXP-1681: the `call-pill` entry (voice-call.json `surfaceSpecimens`):
/// THE floating call pill a minimized call draws, one per row, static.
private struct CallPillSpecimens: View {
    /// `pill.width`, inside the 16 pt page margins.
    static var width: CGFloat { min(CGFloat(VoiceCall.Pill.width), UIScreen.main.bounds.width - 32) }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            ForEach(Array(VoiceCall.surfaceSpecimens.enumerated()), id: \.offset) { _, specimen in
                VoiceCallPill(
                    view: specimen.view,
                    callee: VoiceCallCallee(name: specimen.agent),
                    timer: .fixed(specimen.timer),
                    level: specimen.level,
                    bands: specimen.bands,
                    width: Self.width,
                    animated: false
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityIdentifier("call-pill-specimen")
    }
}

/// EXP-1375: the `device-readiness` entry's low-disk case (device-doctor.json,
/// the last `case`) as a phone draws them (always another device): the
/// block in Device settings, then the composer's low-disk row
/// (`DeviceReadiness.warningRow`, a warning with no pill).
private struct DeviceReadinessSpecimens: View {
    private static let cases: [(name: String, json: String)] = [
        ("EXP-1375: low disk with two kept worktrees: a warning, never a blocker", #"""
        {"checkedAt":"2026-10-28T09:00:00.000Z","items":[{"key":"git","group":"required","state":"ok","detail":"2.55.0"},{"key":"claude","group":"agents","state":"ok","detail":"2.1.289"},{"key":"codex","group":"agents","state":"missing","detail":"Signed out","action":"sign_in"},{"key":"computer_use","group":"computer_use","state":"off"},{"key":"disk","group":"storage","state":"action","detail":"6.2 GB free"},{"key":"worktree-1","group":"storage","parent":"disk","label":"exp-session-9497089a","state":"off","detail":"Uncommitted changes"},{"key":"worktree-2","group":"storage","parent":"disk","label":"exp-EXP-1375","state":"off","detail":"Unpushed commits"}]}
        """#),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            ForEach(Array(Self.cases.enumerated()), id: \.offset) { _, specimen in
                if let doctor = DeviceDoctor.decode(json: specimen.json) {
                    VStack(alignment: .leading, spacing: 6) {
                        caption(specimen.name)
                        DeviceReadinessView(groups: DeviceReadiness.groups(doctor, remote: true), onAction: { _ in })
                        if let row = DeviceReadiness.warningRow(doctor, remote: true) {
                            caption("Composer")
                            DeviceReadinessView(row: row, onAction: { _ in })
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func caption(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }
}

/// NEW-59: device settings' computer-use card with the Work screen row
/// (computer use on, two reported displays, display 2 picked), THE real
/// readiness block over a doctor report.
private struct WorkScreenSpecimens: View {
    @State private var computerUse = true
    @State private var model = DomainContract.deviceComputerUseDefaultsModel
    @State private var display: String? = "display-2"

    private static let doctor = DeviceDoctor(
        items: [
            .init(key: "computer_use", group: "computer_use", state: "ok"),
            .init(key: "screen_recording", group: "computer_use", parent: "computer_use", state: "ok", detail: "Granted"),
            .init(key: "accessibility", group: "computer_use", parent: "computer_use", state: "ok", detail: "Granted"),
        ],
        displays: [
            WorkScreen.Display(id: "display-1", number: 1, width: 2560, height: 1440, primary: false),
            WorkScreen.Display(id: "display-2", number: 2, width: 3024, height: 1964, primary: true),
        ]
    )

    var body: some View {
        DeviceReadinessView(
            groups: DeviceReadiness.groups(Self.doctor, remote: true, computerUseOn: computerUse),
            computerUse: $computerUse,
            computerUseModel: $model,
            workScreen: $display,
            workScreenOptions: WorkScreen.options(displays: Self.doctor.displays, stored: display),
            onAction: { _ in }
        )
    }
}

/// NEW-57: Settings → Voice, the tray group with Meeting mode on (Zoom and
/// Slack picked on a per-app voice device), optionally with THE consent
/// prompt open. THE real page over fixed inputs.
private struct VoiceMeetingSpecimens: View {
    let consentOpen: Bool

    private static let stamp = "2026-10-28T10:00:00.000Z"
    private static let apps = [Meeting.AudioApp(id: "us.zoom.xos", name: "Zoom"), Meeting.AudioApp(id: "com.tinyspeck.slackmacgap", name: "Slack")]
    private static let doctor = #"{"items":[],"desktopAudio":{"capability":"per_app","apps":[{"id":"us.zoom.xos","name":"Zoom"},{"id":"com.tinyspeck.slackmacgap","name":"Slack"},{"id":"com.spotify.client","name":"Spotify"}]}}"#

    var body: some View {
        NavigationStack {
            VoiceSettingsPage(accountId: "specimen", specimen: VoiceSettingsPage.Specimen(
                userId: "u",
                settings: Voice.Settings(
                    voiceDeviceId: "d-mint",
                    agentId: "a-ops",
                    pttShortcut: "ctrl+space",
                    wakeWord: true,
                    desktopAudio: .include,
                    desktopAudioApps: Self.apps,
                    meetingStartedAt: consentOpen ? nil : Self.stamp
                ),
                devices: [DeviceEntity(
                    id: "d-mint", userId: "u", deviceId: "d-mint", label: "mint", kind: "desktop",
                    doctor: Self.doctor, lastSeenAt: Self.stamp
                )],
                agents: [AgentDefinitionEntity(id: "a-ops", teamId: "t", name: "Ops", createdAt: Self.stamp, updatedAt: Self.stamp)],
                teams: [TeamEntity(id: "t", name: "Acme", slug: "acme", iconUrl: nil, createdAt: Self.stamp, updatedAt: Self.stamp)],
                consentOpen: consentOpen
            ))
        }
    }
}
