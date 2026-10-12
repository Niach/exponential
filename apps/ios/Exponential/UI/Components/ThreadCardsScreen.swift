import ExpCore
import ExpUI
import SwiftUI

/// EXP-1259 D12-D14: the iOS styleguide's run-thread cards (web
/// `apps/styleguide` entries `work-card`, `script-card`, `comment-call-card`;
/// the IDE styleguide and Android's screens draw the same specimens): THE Work
/// card (a workflow mid-review, a task list, a subagent), the Script card and
/// the Comment card, each from the REAL view over the shared models. Capture
/// only: the styleguide suite opens one with `-uiTestingStyleguide
/// <work-card|script-card|comment-call-card>` (`sg_<name>`).
struct ThreadCardsScreen: View {
    let kind: String

    static let kinds: Set<String> = ["work-card", "script-card", "comment-call-card"]

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    switch kind {
                    case "script-card": scriptCards
                    case "comment-call-card": commentCards
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
