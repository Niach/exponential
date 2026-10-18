import ExpCore
import ExpUI
import SwiftUI

/// EXP-1259 D12-D14: the iOS styleguide's run-thread cards (web
/// `apps/styleguide` entries `work-card`, `script-card`, `comment-call-card`;
/// the IDE styleguide and Android's screens draw the same specimens): THE Work
/// card (a workflow mid-review, a task list, a subagent), the Script card and
/// the Comment card, each from the REAL view over the shared models. Capture
/// only: the styleguide suite opens one with `-uiTestingStyleguide
/// <work-card|script-card|comment-call-card|…|ask-dock|plan-card>` (`sg_<name>`).
struct ThreadCardsScreen: View {
    let kind: String

    static let kinds: Set<String> = ["work-card", "script-card", "comment-call-card", "trace", "code-block", "thread-strip", "ask-dock", "plan-card"]

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    switch kind {
                    case "script-card": scriptCards
                    case "comment-call-card": commentCards
                    case "trace": traceSpecimens
                    case "code-block": codeBlockSpecimens
                    case "thread-strip": ThreadStripSpecimens()
                    case "ask-dock": AskDockSpecimens()
                    case "plan-card": PlanCardSpecimens()
                    default: workCards
                    }
                }
                .padding(16)
            }
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-cards-screen")
    }

    // MARK: - Work card (D12)

    private static let phases = [
        WorkflowCaptionPhase(index: 1, title: "Contract"),
        WorkflowCaptionPhase(index: 2, title: "Implement"),
        WorkflowCaptionPhase(index: 3, title: "Conformance"),
        WorkflowCaptionPhase(index: 4, title: "Review"),
        WorkflowCaptionPhase(index: 5, title: "Verify"),
    ]

    private static let agents = [
        WorkCardWorkflowAgent(index: 1, label: "contract", phaseIndex: 1, model: "opus", state: "done", durationMs: 600_000),
        WorkCardWorkflowAgent(index: 2, label: "impl:react", phaseIndex: 2, model: "opus", state: "done", durationMs: 1_200_000),
        WorkCardWorkflowAgent(index: 3, label: "impl:gpui", phaseIndex: 2, model: "opus", state: "done", durationMs: 1_500_000),
        WorkCardWorkflowAgent(index: 4, label: "conf:swift", phaseIndex: 3, model: "opus", state: "done", durationMs: 480_000),
        WorkCardWorkflowAgent(index: 5, label: "conf:compose", phaseIndex: 3, model: "opus", state: "done", durationMs: 300_000),
        WorkCardWorkflowAgent(index: 6, label: "review:catalog", phaseIndex: 4, model: "claude-opus-5-5", state: "done", durationMs: 509_000),
        WorkCardWorkflowAgent(index: 7, label: "fix:react", phaseIndex: 4, model: "opus", state: "done", durationMs: 1_900_000),
        WorkCardWorkflowAgent(index: 8, label: "recheck:core", phaseIndex: 4, model: "opus", state: "done", durationMs: 103_000),
        WorkCardWorkflowAgent(index: 9, label: "recheck:react", phaseIndex: 4, model: "opus", state: "running"),
    ]

    @ViewBuilder
    private var workCards: some View {
        let workflow = WorkCards.card(.workflow(
            name: "exponential-ui-renderer-hardening", status: "running",
            phases: Self.phases, agents: Self.agents, elapsedMs: 4_320_000
        ))
        WorkCardView(
            card: workflow,
            foldedPhases: WorkCards.foldedPhases(phases: Self.phases, agents: Self.agents, card: workflow),
            onOpenLane: { _, _ in }
        )
        WorkCardView(card: WorkCards.card(.tasks([
            WorkCardTask(content: "Survey", status: "completed"),
            WorkCardTask(content: "Draft", status: "completed"),
            WorkCardTask(content: "Fixtures", status: "completed"),
            WorkCardTask(content: "Wire the Review face", status: "in_progress"),
            WorkCardTask(content: "Desktop", status: "pending"),
            WorkCardTask(content: "iOS", status: "pending"),
            WorkCardTask(content: "Android", status: "pending"),
        ])))
        WorkCardView(
            card: WorkCards.card(.subagent(
                title: "Survey UI code", state: "done", toolCalls: 127, elapsedMs: 600_000
            )),
            onOpenSubagent: {}
        )
    }

    // MARK: - Trace (EXP-1324)

    @State private var todoOpen = true
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
                TaskQueueBlock(queue: queue, todoOpen: $todoOpen, completedOpen: $completedOpen)
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

    @ViewBuilder
    private var scriptCards: some View {
        let calls: [ScriptCall] =
            (0..<55).map { ScriptCall(tool: "exponential.exponential_issues_get", subject: "EXP-\(1253 + $0)", ok: true) }
            + (0..<11).map { ScriptCall(tool: "exponential.exponential_issues_update", subject: "EXT-\(1 + $0)", ok: true) }
            + (0..<13).map { ScriptCall(tool: "exponential.exponential_comments_create", subject: "MKT-\(6 + $0)", ok: true) }
            + [
                ScriptCall(tool: "exponential.exponential_issue_relations_add", subject: "EXT-15", ok: false),
                ScriptCall(tool: "computer.screenshot", ok: true),
            ]
        ScriptCardView(
            card: ScriptCards.card(ScriptCardInput(running: false, count: calls.count, calls: calls, elapsedMs: 3_040)),
            source: "const issues = await Promise.all(ids.map((id) => tools.exponential.issues_get({ id })))"
        )
        ScriptCardView(card: ScriptCards.card(ScriptCardInput(running: true)), running: true)
    }

    // MARK: - Comment card (D14)

    @ViewBuilder
    private var commentCards: some View {
        CommentCallCardView(
            author: "Danny",
            caption: "via MCP · 21:02",
            running: false,
            hasBody: true,
            onOpen: {}
        ) {
            IssueChip(
                identifier: "VAPP-84",
                title: "Exponential UI: an open generative-UI SDK",
                iconName: AppIcons.statusInProgress,
                statusColor: DesignTokens.Semantic.yellow,
                bodySize: DesignTokens.Transcript.bodySize
            )
        } mark: {
            FeedbackMark(size: 14, color: .white.opacity(TextOpacity.secondary), animated: false)
        } body: {
            VStack(alignment: .leading, spacing: 6) {
                Text("Status 2026-10-08, evening").font(.subheadline.weight(.semibold))
                Text(
                    "#VAPP-98 (renderer hardening) is open as PR #1007; the host API session #VAPP-91 is not based on "
                        + "the new contract yet, so I sent it a heads-up. The conformance lanes are green on all "
                        + "four renderers and the review wave starts tonight."
                )
                .font(.subheadline)
            }
            .foregroundStyle(.white)
        }
    }
}

/// EXP-1334: the thread header's never-scrolling strip (`sg_thread-strip`):
/// everything fitting, then folded (issue chips identifier-only with a
/// sub-issue chevron and `› APP-16` open, the session chips with a folded
/// subtree, each zone's "+N"), the subtree sheet's rows, and THE thread
/// composer's options line + "+" rows.
private struct ThreadStripSpecimens: View {
    private static let fits: [ThreadTab] = [
        .add, .conversation,
        .issue(issueId: "i14", identifier: "APP-14", nested: []),
        .session(sessionId: "s1", name: "Web lane", markState: .working, nested: []),
        .guide(dot: true),
    ]

    private static let folded: [ThreadTab] = [
        .add, .conversation,
        .issue(issueId: "i14", identifier: "APP-14", nested: [
            ThreadTabNestedIssue(issueId: "i16", identifier: "APP-16", depth: 1),
        ]),
        .issue(issueId: "i21", identifier: "APP-21", nested: []),
        .issue(issueId: "i22", identifier: "APP-22", nested: []),
        .issue(issueId: "i23", identifier: "APP-23", nested: []),
        .session(sessionId: "s1", name: "Threads impl", markState: .needsYou, nested: [
            ThreadTabNestedSession(sessionId: "s2", name: "Web lane", markState: .working, depth: 1),
            ThreadTabNestedSession(sessionId: "s3", name: "Android lane", markState: .needsYou, depth: 1),
            ThreadTabNestedSession(sessionId: "s4", name: "Emulator shots", markState: .ended, depth: 2),
        ]),
        .session(sessionId: "s5", name: "Docs", markState: .waiting, nested: []),
        .session(sessionId: "s6", name: "Review", markState: .ended, nested: []),
        .guide(dot: false),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            caption("Everything fits")
            strip(Self.fits, selection: .conversation)
            caption("Folded: identifiers only, each zone's +N")
            strip(Self.folded, selection: .conversation)
            caption("A sub-issue open: <chip> › <open one>")
            strip(Self.folded, selection: .issue("i16"))
            if let fold = ThreadStripFold.subtree(Self.folded[6]) {
                caption("A folded chip's tap: THE tree rows")
                ThreadFoldSheet(fold: fold) { _ in }
                    .background(GlassTokens.fillCard, in: RoundedRectangle(cornerRadius: 16))
            }
            caption("THE composer: options under the card, the \"+\" rows")
            SessionComposerOptionsLine(
                controls: SessionComposerFooter.Controls(
                    planModeActive: false, modelValue: "opus", agent: "claude", onPickModel: { _ in },
                    usageFraction: 0.42, usageSeverity: .normal, onUsage: {},
                    devicePin: Self.devicePin(moveTo: nil)
                ),
                plan: .constant(true)
            )
            caption("EXP-1252: moving to another device")
            SessionComposerOptionsLine(
                controls: SessionComposerFooter.Controls(
                    planModeActive: false, modelValue: "opus", agent: "claude", onPickModel: { _ in },
                    usageFraction: 0.42, usageSeverity: .normal, onUsage: {},
                    devicePin: Self.devicePin(moveTo: "studio")
                ),
                plan: .constant(true)
            )
            ThreadComposerPlusMenu(
                conditions: ComposerMenu.Conditions(subagentModel: false, ultracode: true, mcp: true, computerUse: true),
                agent: "claude",
                effortValue: "High",
                mcpValue: "2",
                ultracode: .constant(false),
                computerUse: .constant(true)
            ) { _ in }
                .background(GlassTokens.fillCard, in: RoundedRectangle(cornerRadius: 16))
        }
        .accessibilityIdentifier("thread-strip-specimens")
    }

    /// EXP-1252: the Device pin specimen — this laptop, a shared studio
    /// machine (refused: shared differently) and an offline one.
    private static func devicePin(moveTo: String?) -> SessionDevicePin {
        let devices = [
            SteerDevice(deviceId: "laptop", deviceLabel: "MacBook Pro", agents: ["claude"], kind: "desktop", online: true),
            SteerDevice(deviceId: "studio", deviceLabel: "Mac Studio", agents: ["claude"], kind: "desktop", online: true),
            SteerDevice(deviceId: "build", deviceLabel: "Build server", agents: ["claude"], kind: "server", online: true, sharedTeamIds: ["t1"]),
            SteerDevice(deviceId: "mini", deviceLabel: "Mac mini", agents: ["claude"], kind: "desktop", online: false),
        ]
        let rows = SessionDeviceMove.rows(
            devices: devices, currentDeviceId: "laptop", sessionTeamId: "t1", sessionShared: false, agent: "claude"
        )
        return SessionDevicePin(
            label: SessionDeviceMove.pinLabel(currentDeviceName: "MacBook Pro", moveToDeviceId: moveTo, devices: devices),
            icon: DeviceIconDisplay.iconName(for: devices[0]),
            rows: rows.map(DevicePickerDevice.init(moveRow:)),
            currentDeviceId: "laptop",
            blockedReason: nil,
            onPick: { _ in }
        )
    }

    private func caption(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }

    private func strip(_ tabs: [ThreadTab], selection: ThreadPage) -> some View {
        ThreadStripTabs(tabs: tabs, selection: selection, onSelect: { _ in }, onAdd: {}, onOpenSession: { _ in })
            .background(GlassTokens.fillCard.opacity(0.4), in: RoundedRectangle(cornerRadius: 12))
    }
}

/// THE ask dock specimen (`sg_ask-dock`, web styleguide `ask-dock`, the IDE's
/// `styleguide:ask-dock`): one plan = THE big card; two plans + a question
/// of three sessions = the stack; the same tree with one card focused = the
/// big card with the back arrow. Real `AskDockView` + `ThreadAskCard`.
private struct AskDockSpecimens: View {
    // The web `AskDockSpecimen` rows (packages/ui/src/ask-dock-specimen.tsx), line for line.
    private static let longPlan = [
        "## Plan: one ask dock ×4",
        "",
        "1. Add `fixtures/ask-dock.json` + the TS model, mirrored ×4.",
        "2. Move waiting cards out of the transcript into the composer's slot.",
        "3. Stack several cards; a tap grows one into the big card.",
        "4. Share THE plan options: Yes · Yes, and start with a fresh context · No.",
        "5. Styleguide entry + scoped shots on every platform.",
        "6. Open the PR with the Guide.",
    ].joined(separator: "\n")

    private static let single: [ThreadRow] = [
        .plan(ThreadPlanRow(id: "p1", sessionId: "c1", plan: longPlan)),
    ]

    private static let stack: [ThreadRow] = [
        .plan(ThreadPlanRow(id: "p1", sessionId: "c1", plan: longPlan)),
        .plan(ThreadPlanRow(
            id: "p2", sessionId: "c2",
            plan: "1. Group the thread list by state\n2. Drop the per-row counters"
        )),
        .question(ThreadQuestionRow(
            id: "q1", sessionId: "c3", question: "Ship both fixes in one PR, or split them?",
            options: [TreeQuestionOption(label: "One PR"), TreeQuestionOption(label: "Split them")],
            custom: true, answered: false
        )),
    ]

    private static let names = ["c1": "Ask dock", "c2": "Thread list", "c3": "Dock polish"]

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            section("One waiting plan", rows: Self.single)
            section("Two plans and a question", rows: Self.stack)
            section("Focused out of the stack", rows: Self.stack, focused: "p2")
        }
    }

    private func section(_ title: String, rows: [ThreadRow], focused: String? = nil) -> some View {
        let pending = AskDock.threadPendingAsks(rows) { Self.names[$0] }
        return VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            AskDockView(pending: pending, initialFocusedId: focused) { id, onBack, cap in
                if let row = rows.first(where: { $0.id == id }) {
                    ThreadAskCard(
                        row: row,
                        onBack: onBack,
                        bodyMaxHeight: cap
                    )
                }
            }
        }
    }
}

/// EXP-1280: THE rich plan / question card (`sg_plan-card`, web styleguide
/// `plan-card`, the IDE's `styleguide:plan-card`): session-thread.json's case
/// "EXP-1280: a rich plan card…" through the REAL row model
/// (`ThreadRow.askCard`) and `ThreadAskCard`: the pending plan (a picture in
/// the body, the inline surface, a picture-tile question + a multiple one),
/// the answered question card (several picks, closed) and the approved plan
/// with its feedback.
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

    private static func row(
        _ id: String, _ card: String, createdAt: String, answeredAt: String?, consumedAt: String?
    ) -> ThreadRow? {
        ThreadRow.askCard(TreeMessage(
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
    private func section(_ title: String, _ row: ThreadRow?) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            if let row {
                ThreadAskCard(row: row)
            }
        }
    }
}
