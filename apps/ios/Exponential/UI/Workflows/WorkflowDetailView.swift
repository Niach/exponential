import ExpCore
import ExpUI
import SwiftUI

/// EXP-981 — one workflow: what it is (the name and the shared shape line), its
/// GRAPH, how it runs, and the two things a draft can do — plan it with an
/// agent, or delete it. P2 ships DRAFT workflows: there is no Start button yet.
///
/// The graph is the phone form (`WorkflowGraphView`): waves as bands, nodes as
/// flat rows, blockers as chips. Tapping a node opens its panel — a SHEET here,
/// where web and the IDE put a right-hand panel.
struct WorkflowDetailView: View {
    let workflowId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.dismiss) private var dismiss

    @State private var model: WorkflowDetailModel?
    /// The name is the one TYPED field, so the one with a draft; it saves on
    /// return and when the field gives up focus, like the issue title.
    @State private var nameDraft = ""
    @State private var seededName = false
    @FocusState private var nameFocused: Bool
    /// The node whose panel is up (by node id — the sheet reads the live row).
    @State private var selectedNodeId: WorkflowNodeTarget?
    @State private var showDeleteConfirm = false

    var body: some View {
        ZStack {
            AppBackground()

            if let model {
                if model.workflow != nil {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 0) {
                            header(model)
                            graph(model)
                            howItRuns(model)
                            actions(model)
                        }
                        .padding()
                    }
                } else if model.loaded {
                    // The row is gone (deleted elsewhere) or has never synced.
                    missingState
                } else {
                    ProgressView().tint(.white)
                }
            } else {
                ProgressView().tint(.white)
            }
        }
        .navigationTitle(WorkflowView.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("workflow-detail")
        .onAppear {
            if model == nil {
                model = WorkflowDetailModel(
                    workflowId: workflowId, accountId: accountId, deps: deps
                )
            }
            model?.observe()
        }
        .onChange(of: model?.workflow?.name) { _, name in
            // Seed once, then leave the field alone: a synced echo must never
            // stomp what is being typed.
            guard let name, !seededName else { return }
            seededName = true
            nameDraft = name
        }
        .onChange(of: nameFocused) { _, focused in
            if !focused { model?.rename(nameDraft) }
        }
        .onDisappear {
            model?.rename(nameDraft)
            model?.stop()
        }
        .sheet(item: $selectedNodeId) { target in
            if let model, let node = model.nodes.first(where: { $0.id == target.id }) {
                WorkflowNodeSheet(
                    node: node,
                    issues: model.issues,
                    enabled: model.isDraft,
                    onUpdate: { patch in
                        model.updateNode(issueId: node.issueId, patch: patch)
                    },
                    onOpenIssue: { issueId in
                        selectedNodeId = nil
                        pushRoute(.issue(accountId: accountId, id: issueId))
                    }
                )
            }
        }
        .confirmationDialog(
            WorkflowView.deleteLabel,
            isPresented: $showDeleteConfirm,
            titleVisibility: .visible
        ) {
            Button(WorkflowView.deleteLabel, role: .destructive) {
                model?.delete { dismiss() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This removes the workflow and its plan. The issues stay where they are.")
        }
    }

    // MARK: - Header

    @ViewBuilder
    private func header(_ model: WorkflowDetailModel) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            GlassTextField("Name", text: $nameDraft, bordered: false) {
                EmptyView()
            } trailing: {
                EmptyView()
            }
            .focused($nameFocused)
            .onSubmit { model.rename(nameDraft) }
            .accessibilityIdentifier("workflow-name-field")

            Text(WorkflowView.shapeLine(model.metrics))
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            // A cyclic plan cannot start, so the note is destructive, not a
            // hint — and it names the issues to unlink.
            if let note = WorkflowView.cycleNote(model.metrics) {
                Text(note)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Palette.destructive)
                    .accessibilityIdentifier("workflow-cycle-note")
            }
            if let error = model.error {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .accessibilityIdentifier("workflow-error")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .glassCard()
        .padding(.bottom, 12)
    }

    // MARK: - Graph

    @ViewBuilder
    private func graph(_ model: WorkflowDetailModel) -> some View {
        WorkflowGraphView(
            nodes: model.nodes,
            edges: model.edges,
            workflowStatus: model.workflow?.status ?? DomainContract.wfStatusDraft,
            issues: model.issues,
            onSelect: { selectedNodeId = WorkflowNodeTarget(id: $0.id) }
        )
    }

    // MARK: - How it runs

    /// The start configuration, persisted field by field through
    /// `workflows.update`. DRAFT-only server-side: once a workflow is running
    /// the rows go inert rather than bouncing on submit.
    @ViewBuilder
    private func howItRuns(_ model: WorkflowDetailModel) -> some View {
        let enabled = model.isDraft
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand("How it runs")
                .padding(.top, 12)

            optionRow {
                GlassPickerRow(
                    "Runs on",
                    selection: Binding(
                        get: { model.workflow?.deviceId ?? "" },
                        set: { model.setDevice($0.isEmpty ? nil : $0) }
                    ),
                    options: [""] + model.devices.map(\.deviceId),
                    label: { id in
                        model.devices.first { $0.deviceId == id }
                            .map(LaunchVocabulary.deviceCaption) ?? "No machine"
                    },
                    enabled: enabled
                )
            }

            optionRow {
                GlassPickerRow(
                    "Agent",
                    selection: Binding(
                        get: { model.agent },
                        set: { model.setAgent($0) }
                    ),
                    options: model.availableAgents,
                    label: { LaunchVocabulary.agentLabel($0) },
                    enabled: enabled
                )
            }

            optionRow {
                GlassPickerRow(
                    "Model",
                    selection: launchBinding(
                        value: model.launch.model, set: { model.setModel($0) }
                    ),
                    // A binding offers "CLI default" for every agent: a blank
                    // stores NULL and lets the machine decide.
                    options: [LaunchVocabulary.cliDefault]
                        + LaunchVocabulary.automationModelValues(for: model.agent),
                    label: { LaunchVocabulary.modelLabel($0) },
                    enabled: enabled
                )
            }

            // EXP-981: claude only, exactly like the composer's row.
            if LaunchVocabulary.supportsSubagentModel(model.agent) {
                optionRow {
                    GlassPickerRow(
                        "Subagent model",
                        selection: launchBinding(
                            value: model.launch.subagentModel,
                            set: { model.setSubagentModel($0) }
                        ),
                        options: LaunchVocabulary.subagentModelValues(),
                        label: { LaunchVocabulary.subagentModelLabel($0) },
                        enabled: enabled
                    )
                }
            }

            optionRow {
                GlassPickerRow(
                    LaunchVocabulary.effortTitle(for: model.agent),
                    selection: launchBinding(
                        value: model.launch.effort, set: { model.setEffort($0) }
                    ),
                    options: [LaunchVocabulary.cliDefault]
                        + LaunchVocabulary.effortValues(for: model.agent),
                    label: { value in
                        value == LaunchVocabulary.cliDefault
                            ? "CLI default"
                            : LaunchVocabulary.effortLabel(value)
                    },
                    enabled: enabled
                )
            }

            // The login the nodes run under — offered only where it is a
            // choice (the composer's rule: two or more profiles).
            if model.accountProfiles.count >= 2 {
                optionRow {
                    GlassPickerRow(
                        "Account",
                        selection: Binding(
                            get: { model.launch.account ?? "" },
                            set: { model.setAccount($0.isEmpty ? nil : $0) }
                        ),
                        options: [""] + model.accountProfiles.map(\.id),
                        label: { id in
                            model.accountProfiles.first { $0.id == id }
                                .map(accountLabel) ?? "Active login"
                        },
                        enabled: enabled
                    )
                }
            }

            optionRow {
                GlassPickerRow(
                    "Max parallel",
                    selection: Binding(
                        get: { model.maxParallel },
                        set: { model.setMaxParallel($0) }
                    ),
                    options: Array(1...Self.maxParallelCap),
                    label: { "\($0)" },
                    enabled: enabled
                )
            }

            optionRow {
                GlassPickerRow(
                    "Gate",
                    selection: Binding(
                        get: { model.workflow?.gate ?? DomainContract.wfGateHuman },
                        set: { model.setGate($0) }
                    ),
                    options: DomainContract.wfGateValues,
                    label: Self.gateLabel,
                    enabled: enabled
                )
            }

            optionRow {
                GlassPickerRow(
                    "Start",
                    selection: Binding(
                        get: { model.workflow?.startOn ?? DomainContract.wfStartOnContract },
                        set: { model.setStartOn($0) }
                    ),
                    options: DomainContract.wfStartOnValues,
                    label: Self.startOnLabel,
                    enabled: enabled
                )
            }
        }
    }

    /// The server's own cap (`WORKFLOW_MAX_PARALLEL_CAP`) — how many nodes the
    /// engine may run at once.
    private static let maxParallelCap = 8

    private static func gateLabel(_ value: String) -> String {
        switch value {
        case DomainContract.wfGateNone: "No gate"
        case DomainContract.wfGateAgent: "Agent review"
        default: "Human review"
        }
    }

    private static func startOnLabel(_ value: String) -> String {
        switch value {
        case DomainContract.wfStartOnPrOpen: "On PR open"
        case DomainContract.wfStartOnLanded: "When landed"
        default: "On contract"
        }
    }

    /// One login's name, with the health badge a refused credential earns —
    /// the composer's own label rule.
    private func accountLabel(_ profile: AgentAccountProfile) -> String {
        let name = profile.email ?? profile.label ?? profile.id
        guard let badge = AgentAccountHealth.of(profile).badgeLabel else { return name }
        return "\(name) · \(badge.lowercased())"
    }

    /// A launch field the "CLI default" sentinel stands in for: the picker
    /// speaks the sentinel, the row stores NULL.
    private func launchBinding(
        value: String?, set: @escaping (String?) -> Void
    ) -> Binding<String> {
        Binding(
            get: { (value?.isEmpty ?? true) ? LaunchVocabulary.cliDefault : value! },
            set: { set($0 == LaunchVocabulary.cliDefault ? nil : $0) }
        )
    }

    @ViewBuilder
    private func optionRow<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        content()
            .padding(.horizontal, 12)
            .padding(.vertical, 12)
            .flatRow()
    }

    // MARK: - Actions

    /// P2: plan it, or delete it. The engine (and the Start button) arrive with
    /// the next change.
    @ViewBuilder
    private func actions(_ model: WorkflowDetailModel) -> some View {
        HStack(spacing: 10) {
            GlassPill(
                WorkflowView.planLabel,
                icon: ActionIconDisplay.iconName(for: "layers"),
                size: .md,
                mode: .action { plan(model) },
                primary: true,
                enabled: model.workflow != nil
            )
            .accessibilityIdentifier("workflow-plan-button")

            Spacer(minLength: 0)

            Button { showDeleteConfirm = true } label: {
                Text(WorkflowView.deleteLabel)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(DesignTokens.Palette.destructive)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("workflow-delete-button")
        }
        .padding(.top, 16)
    }

    /// The planner run: the Agent page composer with the hidden Plan-workflow
    /// builtin as its subject and THIS workflow named — its free text is the
    /// optional "anything the plan should respect".
    private func plan(_ model: WorkflowDetailModel) {
        guard let workflow = model.workflow else { return }
        pushRoute(.agent(
            accountId: accountId,
            seed: AgentComposerSeed(
                actionId: DomainContract.builtinPlanWorkflowId,
                workflowId: workflow.id,
                teamId: workflow.teamId
            )
        ))
    }

    private var missingState: some View {
        VStack(spacing: 12) {
            AppIcon(AppIcons.navWorkflows, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("This workflow is gone.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .padding(.horizontal, 40)
        .accessibilityIdentifier("workflow-missing")
    }
}

/// The node whose panel is up (`.sheet(item:)`); the sheet reads the LIVE row
/// behind this id rather than a snapshot.
struct WorkflowNodeTarget: Identifiable {
    let id: String
}

/// EXP-981 — one node's panel. Web and the IDE put it beside the graph; a phone
/// gets a sheet. It names the node's issue, its members when it is a compound
/// one, what the plan declares about it (Kind / Risk) and the `touches` globs
/// the planner wrote, and it opens the issue.
struct WorkflowNodeSheet: View {
    let node: WorkflowNodeEntity
    let issues: [String: IssueEntity]
    /// Kind shapes the PLAN, so the server takes it on a draft only.
    let enabled: Bool
    let onUpdate: (WorkflowNodePatch) -> Void
    let onOpenIssue: (String) -> Void

    var body: some View {
        GlassSheetChrome(
            title: WorkflowView.nodeTitle(
                identifier: issues[node.issueId]?.identifier ?? node.issueId,
                memberCount: node.memberIssueIds.count
            )
        ) {
            VStack(alignment: .leading, spacing: 10) {
                subject

                if !node.memberIssueIds.isEmpty {
                    // A compound node runs as ONE batch: its members are part
                    // of the same session, branch and pull request.
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Runs together with")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 4) {
                                ForEach(node.memberIssueIds, id: \.self) { chip($0) }
                            }
                        }
                    }
                }

                VStack(spacing: 2) {
                    GlassPickerRow(
                        "Kind",
                        selection: Binding(
                            get: { node.kind },
                            set: { onUpdate(WorkflowNodePatch(kind: $0)) }
                        ),
                        options: DomainContract.wfNodeKindValues,
                        label: WorkflowView.nodeKindLabel,
                        enabled: enabled
                    )
                    .padding(.horizontal, 12)
                    .padding(.vertical, 12)
                    .glassRow()

                    GlassPickerRow(
                        "Risk",
                        selection: Binding(
                            get: { node.risk },
                            set: { onUpdate(WorkflowNodePatch(risk: $0)) }
                        ),
                        options: DomainContract.wfRiskValues,
                        label: { $0.prefix(1).uppercased() + $0.dropFirst() }
                    )
                    .padding(.horizontal, 12)
                    .padding(.vertical, 12)
                    .glassRow()
                }

                // What the node expects to change — the planner's own globs,
                // read-only here.
                if !node.touches.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Touches")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        ForEach(node.touches, id: \.self) { glob in
                            Text(glob)
                                .font(.caption.monospaced())
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .lineLimit(1)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .glassRow()
                }

                GlassPill(
                    "Open issue",
                    icon: AppIcons.uiExternalLink,
                    mode: .action { onOpenIssue(node.issueId) }
                )
                .accessibilityIdentifier("workflow-node-open-issue")
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding)
            .padding(.bottom, 16)
        }
        .accessibilityIdentifier("workflow-node-sheet")
    }

    /// The node's own issue, with the caption the graph row carries.
    @ViewBuilder
    private var subject: some View {
        VStack(alignment: .leading, spacing: 6) {
            chip(node.issueId)
            if let title = issues[node.issueId]?.title {
                Text(title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    @ViewBuilder
    private func chip(_ issueId: String) -> some View {
        let issue = issues[issueId]
        let status = IssueStatus.from(issue?.status)
        IssueChip(
            identifier: issue?.identifier ?? issueId,
            title: issue?.title,
            iconName: status.iconName,
            statusColor: node.onCycle ? DesignTokens.Semantic.red : status.color
        )
    }
}
