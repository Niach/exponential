import ExpCore
import ExpUI
import SwiftUI

/// EXP-981 — one workflow: what it is (the name, the shared shape line and the
/// machine it runs on), its GRAPH, its merge train, its metrics, and what its
/// status lets it do — a draft starts (or says why it cannot), gets planned and
/// deleted; a live one pauses, resumes or is cancelled (EXP-982).
///
/// EXP-1014: the screen CONFIGURES nothing about the run. The launch (EXP-1029:
/// one agent, a cheap model and a strong one) is picked where a workflow is
/// planned and carried from there; the only control left is the runner machine,
/// which a draft needs before it can start, and it sits in the header.
///
/// The graph is the phone form (`WorkflowGraphView`): waves as bands, nodes as
/// issue chips. Tapping a node opens its panel — a SHEET here, where web and
/// the IDE put a right-hand panel.
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
    @State private var showCancelConfirm = false

    var body: some View {
        ZStack {
            AppBackground()

            if let model {
                if model.workflow != nil {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 0) {
                            header(model)
                            graph(model)
                            mergeTrain(model)
                            metrics(model)
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
                    nodes: model.nodes,
                    issues: model.issues,
                    launch: model.launch,
                    isDraft: model.isDraft,
                    busy: model.busy,
                    onUpdate: { patch in
                        model.updateNode(issueId: node.issueId, patch: patch)
                    },
                    onApprove: { approved in
                        model.approveNode(node.id, approved: approved)
                    },
                    onResolve: { action in
                        model.resolveNode(node.id, action: action)
                    },
                    onAdmit: { admit in
                        model.admitNode(node.id, admit: admit)
                    },
                    onOpenIssue: { issueId in
                        selectedNodeId = nil
                        pushRoute(.issue(accountId: accountId, id: issueId))
                    },
                    onOpenRun: { sessionId in
                        selectedNodeId = nil
                        pushRoute(.agentSession(accountId: accountId, sessionId: sessionId))
                    },
                    onOpenChanges: { issueId in
                        selectedNodeId = nil
                        pushRoute(.changes(accountId: accountId, issueId: issueId))
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
        .confirmationDialog(
            WorkflowView.cancelLabel,
            isPresented: $showCancelConfirm,
            titleVisibility: .visible
        ) {
            Button(WorkflowView.cancelLabel, role: .destructive) { model?.cancel() }
            Button("Keep running", role: .cancel) {}
        } message: {
            Text(WorkflowView.cancelConfirm)
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

            runner(model)

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

    /// EXP-1014 — the ONE thing this screen still sets: WHICH machine runs the
    /// workflow. A draft needs one bound before it can start, and once it has
    /// started the pick is history, so the row goes inert rather than bouncing
    /// on submit. Everything else about the run (agent, models) is carried on
    /// the stored launch and is not edited here.
    ///
    /// EXP-1030: the SHARED `DevicePicker`; the row is only its trigger, so a
    /// machine is picked here exactly as it is in the composer and the
    /// automation editor — its own glyph, its owner under the name. The
    /// leading row unbinds the runner again while the plan is still a draft.
    @ViewBuilder
    private func runner(_ model: WorkflowDetailModel) -> some View {
        DevicePicker(
            devices: [DevicePickerDevice(id: "", name: "No machine")]
                + model.devices.map(DevicePickerDevice.init),
            value: model.workflow?.deviceId ?? "",
            onChange: { model.setDevice($0.isEmpty ? nil : $0) },
            trigger: {
                GlassPickerRowLabel(
                    "Runs on",
                    value: model.devices.first { $0.deviceId == model.workflow?.deviceId }
                        .map(LaunchVocabulary.deviceCaption) ?? "No machine",
                    enabled: model.isDraft
                )
            }
        )
        // Once it has started the pick is history: the row goes inert rather
        // than bouncing on submit.
        .disabled(!model.isDraft)
        .font(.caption)
        .accessibilityIdentifier("workflow-runner-row")
    }

    // MARK: - Graph

    @ViewBuilder
    private func graph(_ model: WorkflowDetailModel) -> some View {
        WorkflowGraphView(
            nodes: model.nodes,
            edges: model.edges,
            workflowStatus: model.status,
            issues: model.issues,
            finalPrCaption: model.finalPrCaption,
            finalPrUrl: model.workflow?.finalPrUrl,
            finalPrState: model.workflow?.finalPrState,
            runs: model.runs,
            busy: model.busy,
            onSelect: { selectedNodeId = WorkflowNodeTarget(id: $0.id) },
            onOpenRun: { pushRoute(.agentSession(accountId: accountId, sessionId: $0)) },
            onMergeFinalPr: { model.mergeFinalPr() }
        )
    }

    // MARK: - Merge train

    /// EXP-982 — what is queued to land on the integration branch, in landing
    /// order. Web and the IDE draw a horizontal strip; a phone gets the short
    /// list. Hidden on a draft: nothing can be waiting there.
    @ViewBuilder
    private func mergeTrain(_ model: WorkflowDetailModel) -> some View {
        if !model.isDraft {
            let train = model.mergeTrain
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand(WorkflowView.mergeTrainTitle)
                    .padding(.top, 12)

                if train.isEmpty {
                    Text(WorkflowView.mergeTrainEmpty)
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .flatRow()
                } else {
                    ForEach(train, id: \.id) { entry in
                        if let node = model.nodes.first(where: { $0.id == entry.id }) {
                            trainRow(node: node, entry: entry, model: model)
                        }
                    }
                }
            }
            .accessibilityIdentifier("workflow-merge-train")
        }
    }

    @ViewBuilder
    private func trainRow(
        node: WorkflowNodeEntity, entry: WorkflowView.TrainEntry, model: WorkflowDetailModel
    ) -> some View {
        Button { selectedNodeId = WorkflowNodeTarget(id: node.id) } label: {
            HStack(spacing: 8) {
                Text(WorkflowGraphView.nodeIdentifier(
                    node, issue: model.issues[node.issueId]
                ))
                .font(.caption.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
                Spacer(minLength: 8)
                Text(WorkflowView.trainStepLabel(entry.step))
                    .font(.caption2)
                    .foregroundStyle(Self.trainStepColor(entry.step))
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .flatRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("workflow-train-row")
    }

    /// The step's paint: amber only where a PERSON is needed, the same rule the
    /// node tones follow.
    private static func trainStepColor(_ step: WorkflowView.TrainStep) -> Color {
        switch step {
        case .next, .updating: DesignTokens.Semantic.green
        case .needsApproval: DesignTokens.Semantic.yellow
        case .queued: .white.opacity(TextOpacity.tertiary)
        }
    }

    // MARK: - Metrics

    /// EXP-984 — what the run cost and caught, the LAST section of a started
    /// workflow's detail. Flat label/value rows; hidden on a draft, which has
    /// no run to count.
    @ViewBuilder
    private func metrics(_ model: WorkflowDetailModel) -> some View {
        if !model.isDraft {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand(WorkflowView.metricsTitle)
                    .padding(.top, 12)

                ForEach(model.metricRows, id: \.label) { row in
                    HStack(spacing: 8) {
                        Text(row.label)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        Spacer(minLength: 8)
                        Text(row.value)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .flatRow()
                }
            }
            .accessibilityIdentifier("workflow-metrics")
        }
    }

    // MARK: - Actions

    /// EXP-982 — what a workflow can do, by status: a draft starts (or says
    /// why it cannot), gets planned and deleted; a live one pauses or resumes
    /// and can be cancelled; a finished one is only ever deleted.
    @ViewBuilder
    private func actions(_ model: WorkflowDetailModel) -> some View {
        let blocker = model.startBlocker
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                switch model.status {
                case DomainContract.wfStatusDraft:
                    GlassPill(
                        WorkflowView.startLabel,
                        icon: AppIcons.codingRunning,
                        size: .md,
                        mode: .action { model.start() },
                        primary: true,
                        enabled: blocker == nil && !model.busy
                    )
                    .accessibilityIdentifier("workflow-start-button")

                    GlassPill(
                        WorkflowView.planLabel,
                        icon: ActionIconDisplay.iconName(for: "layers"),
                        size: .md,
                        mode: .action { plan(model) },
                        enabled: model.workflow != nil
                    )
                    .accessibilityIdentifier("workflow-plan-button")
                case DomainContract.wfStatusRunning:
                    GlassPill(
                        WorkflowView.pauseLabel,
                        icon: AppIcons.uiStop,
                        size: .md,
                        mode: .action { model.pause() },
                        enabled: !model.busy
                    )
                    .accessibilityIdentifier("workflow-pause-button")
                case DomainContract.wfStatusPaused:
                    GlassPill(
                        WorkflowView.resumeLabel,
                        icon: AppIcons.codingRunning,
                        size: .md,
                        mode: .action { model.resume() },
                        primary: true,
                        enabled: !model.busy
                    )
                    .accessibilityIdentifier("workflow-resume-button")
                default:
                    EmptyView()
                }

                Spacer(minLength: 0)

                // Cancelling is the destructive end of a live run; deleting is
                // the destructive end of everything else. Never both.
                if model.status == DomainContract.wfStatusRunning
                    || model.status == DomainContract.wfStatusPaused {
                    destructiveButton(WorkflowView.cancelLabel) { showCancelConfirm = true }
                        .accessibilityIdentifier("workflow-cancel-button")
                } else {
                    destructiveButton(WorkflowView.deleteLabel) { showDeleteConfirm = true }
                        .accessibilityIdentifier("workflow-delete-button")
                }
            }

            // Why Start is disabled, in the server's own sentence.
            if model.isDraft, let blocker {
                Text(blocker)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .accessibilityIdentifier("workflow-start-blocker")
            }
        }
        .padding(.top, 16)
    }

    private func destructiveButton(
        _ label: String, action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Text(label)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(DesignTokens.Palette.destructive)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
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
/// gets a sheet. EXP-1014 pares it to exactly what a reader decides on: the
/// node's issue chip, what the plan declares (Kind — draft-only — and Risk,
/// which raises a node onto the strong model at ANY status), the model its run
/// takes, its state and the actions that state allows, the sub-issues a
/// compound node runs as one batch, the contract it published and the siblings
/// it merges in first, the way out to the Work faces, and the latest agent
/// review. The planner's `touches` globs stay off it: nobody acts on a glob.
struct WorkflowNodeSheet: View {
    let node: WorkflowNodeEntity
    /// EXP-983 — the workflow's other nodes: what `after_node_ids` names is a
    /// NODE, so `Merges in first` resolves its chips through these.
    let nodes: [WorkflowNodeEntity]
    let issues: [String: IssueEntity]
    /// The workflow's stored launch, CARRIED (EXP-1029): the panel says which
    /// of its two models this node's run takes and never writes it back.
    let launch: WorkflowLaunch
    /// The KIND shapes the plan, so the server takes it on a draft only; risk
    /// rides at any status. A draft's nodes also keep the issue's own status
    /// glyph, whatever their state says.
    let isDraft: Bool
    /// A write is in flight; the run controls go inert rather than double-fire.
    let busy: Bool
    let onUpdate: (WorkflowNodePatch) -> Void
    let onApprove: (Bool) -> Void
    let onResolve: (WorkflowNodeResolution) -> Void
    /// EXP-984 — a `proposed` node: admit it into the run, or dismiss it.
    let onAdmit: (Bool) -> Void
    let onOpenIssue: (String) -> Void
    let onOpenRun: (String) -> Void
    let onOpenChanges: (String) -> Void

    @State private var showSkipConfirm = false
    /// The review's findings are folded to a few lines while they are long.
    @State private var findingsExpanded = false

    var body: some View {
        GlassSheetChrome(
            title: WorkflowGraphView.nodeIdentifier(node, issue: issues[node.issueId])
        ) {
            VStack(alignment: .leading, spacing: 10) {
                subject
                planRows
                runControls
                bookkeeping
                faces
                agentReview
            }
            .padding(.horizontal, GlassSheetTokens.headerHPadding)
            .padding(.bottom, 16)
        }
        .accessibilityIdentifier("workflow-node-sheet")
        .confirmationDialog(
            WorkflowView.skipNodeLabel,
            isPresented: $showSkipConfirm,
            titleVisibility: .visible
        ) {
            Button(WorkflowView.skipNodeLabel, role: .destructive) { onResolve(.skip) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(WorkflowView.skipNodeConfirm)
        }
    }

    // MARK: - The node

    /// The node's own issue, as the chip the graph row draws — stacked when the
    /// node is a compound one. The CHIP is the way into the issue; the faces
    /// below are the way into its run and its changes.
    @ViewBuilder
    private var subject: some View {
        Button { onOpenIssue(node.issueId) } label: {
            VStack(alignment: .leading, spacing: 6) {
                chip
                if let title = issues[node.issueId]?.title {
                    Text(title)
                        .font(.subheadline)
                        .foregroundStyle(.white)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("workflow-node-issue")
    }

    /// The node's chip, drawn by the graph's own rules: the state's glyph in
    /// its tone once the state has one, the issue's status glyph before that,
    /// the workflow ring around a proposed or cyclic node, and the stack behind
    /// a compound one.
    @ViewBuilder
    private var chip: some View {
        let issue = issues[node.issueId]
        // No row, no status glyph; a draft's nodes have not run, so they keep
        // the issue's own. The same rule ×4.
        let status = issue.map { IssueStatus.from($0.status) }
        let stateIcon = isDraft ? nil : WorkflowGraphView.stateIcon(node.state)
        let face = IssueChip(
            identifier: WorkflowGraphView.nodeIdentifier(node, issue: issue),
            title: WorkflowGraphView.nodeChipTitle(issue),
            iconName: stateIcon ?? status?.iconName,
            statusColor: stateIcon == nil ? status?.color : Self.color(tone)
        )
        .overlay { WorkflowGraphView.ring(node) }
        if node.memberIssueIds.isEmpty {
            face
        } else {
            IssueChipStack { face }
        }
    }

    /// What the plan declares about the node, and what that costs: the KIND,
    /// which shapes the plan and is therefore a draft's to set, the RISK, which
    /// is a pick at any status (the server only draft-gates kind and touches,
    /// and `risk: high` is the one lever that puts a node on the workflow's
    /// strong model), then that model, read-only. EXP-1029: the launch is set
    /// where a workflow is planned; a node only says which half of it applies.
    @ViewBuilder
    private var planRows: some View {
        // EXP-994: one grouped card, hairline-separated rows.
        VStack(spacing: 0) {
            GlassPickerRow(
                "Kind",
                selection: Binding(
                    get: { node.kind },
                    set: { onUpdate(WorkflowNodePatch(kind: $0)) }
                ),
                options: DomainContract.wfNodeKindValues,
                label: WorkflowView.nodeKindLabel,
                enabled: isDraft
            )
            .padding(.horizontal, 12)
            .padding(.vertical, 12)

            GlassDivider()

            GlassPickerRow(
                "Risk",
                selection: Binding(
                    get: { node.risk },
                    set: { onUpdate(WorkflowNodePatch(risk: $0)) }
                ),
                options: DomainContract.wfRiskValues,
                label: { $0.prefix(1).uppercased() + $0.dropFirst() },
                enabled: !busy
            )
            .padding(.horizontal, 12)
            .padding(.vertical, 12)

            GlassDivider()

            HStack(spacing: 8) {
                Text(WorkflowView.nodeModelLabel)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                Spacer(minLength: 8)
                Text(LaunchVocabulary.modelLabel(
                    WorkflowView.modelForNode(launch, kind: node.kind, risk: node.risk)
                ))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 12)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("workflow-node-model")
        }
        .glassSection()
    }

    // MARK: - What else the node carries (EXP-983)

    /// The rest of what the node IS, under the decisions: the sub-issues a
    /// compound node runs as ONE batch, the contract it has published (its
    /// dependents may already be building on it), and the siblings it merges in
    /// first — the engine's serialization edges, without which a node waiting
    /// on a sibling it does not depend on cannot be explained at all. Web and
    /// the IDE keep all three in the panel beside the graph; the phone keeps
    /// them here.
    @ViewBuilder
    private var bookkeeping: some View {
        let members = node.memberIssueIds.compactMap { issues[$0] }
        let mergesFirst = node.afterNodeIds.compactMap { id in
            nodes.first { $0.id == id }
        }
        if !members.isEmpty {
            FlowLayout(spacing: 6) {
                ForEach(members, id: \.id) { member in
                    IssueChip(
                        identifier: member.identifier,
                        title: member.title,
                        status: IssueStatus.from(member.status),
                        onTap: { onOpenIssue(member.id) }
                    )
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityIdentifier("workflow-node-members")
        }
        if let stamp = node.checkpointAt, !stamp.isEmpty {
            Text(Self.contractPublishedLine(stamp))
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityIdentifier("workflow-node-checkpoint")
        }
        if !mergesFirst.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                Text(WorkflowView.mergesInFirstLabel)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                FlowLayout(spacing: 6) {
                    ForEach(mergesFirst, id: \.id) { from in
                        let issue = issues[from.issueId]
                        IssueChip(
                            identifier: WorkflowGraphView.nodeIdentifier(from, issue: issue),
                            title: WorkflowGraphView.nodeChipTitle(issue),
                            iconName: issue.map { IssueStatus.from($0.status).iconName },
                            statusColor: issue.map { IssueStatus.from($0.status).color },
                            onTap: { onOpenIssue(from.issueId) }
                        )
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityIdentifier("workflow-node-merges-first")
        }
    }

    /// `Contract published · 2 hr ago` — the label is byte-locked ×4, the stamp
    /// is the app's own relative-date idiom (and drops out when the wire stamp
    /// is unreadable).
    private static func contractPublishedLine(_ stamp: String) -> String {
        let relative = AgentUsagePresentation.relativeDate(stamp)
        return relative.isEmpty
            ? WorkflowView.contractPublishedLabel
            : "\(WorkflowView.contractPublishedLabel) · \(relative)"
    }

    // MARK: - The Work faces (EXP-1024)

    /// Issue · Run · Changes for this node, as the Work screen's own faces — a
    /// node reads exactly like the screen it opens into. Which faces exist is
    /// the shared rule (`WorkFaces.availableFaces`): no run, no Run; no pull
    /// request, no Changes. Under two there is nothing to choose, and the chip
    /// above is already the way into the issue.
    @ViewBuilder
    private var faces: some View {
        let available = WorkFaces.availableFaces(
            hasIssue: true,
            hasRun: node.sessionId != nil,
            hasChanges: issues[node.issueId]?.prNumber != nil,
            hasResults: false
        )
        if available.count > 1 {
            HStack(spacing: 8) {
                ForEach(available, id: \.self) { face in
                    GlassPill(
                        WorkFaces.faceLabel(face),
                        icon: WorkFaceSwitcher.icon(.face(face)),
                        mode: .action { open(face) }
                    )
                }
            }
            .accessibilityIdentifier("workflow-node-faces")
        }
    }

    private func open(_ face: WorkFaceKind) {
        switch face {
        case .run:
            if let sessionId = node.sessionId { onOpenRun(sessionId) }
        case .changes:
            onOpenChanges(node.issueId)
        case .issue, .results:
            onOpenIssue(node.issueId)
        }
    }

    // MARK: - Agent review (EXP-984)

    /// The latest verdict an agent reviewer submitted: the one line, its
    /// findings (folded while they are long), and the check it actually RAN —
    /// an opinion is advisory, a passing oracle is evidence.
    @ViewBuilder
    private var agentReview: some View {
        if let review = node.parsedReview {
            let approved = review.verdict == DomainContract.wfReviewVerdictApprove
            VStack(alignment: .leading, spacing: 6) {
                Text(WorkflowView.agentReviewTitle)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text(WorkflowView.reviewLine(review, nodeApproved: node.approvedAt != nil))
                    .font(.caption.weight(.medium))
                    .foregroundStyle(
                        approved
                            ? DesignTokens.Semantic.green
                            : DesignTokens.Palette.destructive
                    )
                    .accessibilityIdentifier("workflow-node-review-line")
                if !review.findings.isEmpty {
                    Text(review.findings)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(findingsExpanded ? nil : Self.findingsLines)
                        .accessibilityIdentifier("workflow-node-review-findings")
                    if Self.foldable(review.findings) {
                        Button(findingsExpanded ? "Show less" : "Show more") {
                            findingsExpanded.toggle()
                        }
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .buttonStyle(.plain)
                    }
                }
                if let command = review.oracle?.command, !command.isEmpty {
                    Text(command)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                        .accessibilityIdentifier("workflow-node-review-oracle")
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .accessibilityIdentifier("workflow-node-review")
        }
    }

    /// The fold: a few lines, then "Show more" — the session view's own rule
    /// for a long body.
    private static let findingsLines = 4
    private static let findingsChars = 280

    private static func foldable(_ findings: String) -> Bool {
        findings.count > findingsChars
            || findings.filter { $0 == "\n" }.count >= findingsLines
    }

    // MARK: - State and its actions (EXP-982)

    /// Where the node stands, and the only things a person can do about it:
    /// admit or dismiss a proposal, retry or skip a failure, approve a PR or
    /// take that approval back.
    @ViewBuilder
    private var runControls: some View {
        VStack(alignment: .leading, spacing: 10) {
            // The state in full, in its own tone — the graph row's caption,
            // said where the decisions are made.
            HStack(spacing: 6) {
                if let icon = WorkflowGraphView.stateIcon(node.state) {
                    AppIcon(icon, size: AppIcon.Size.small)
                        .foregroundStyle(Self.color(tone))
                }
                Text(WorkflowView.nodeStateLabel(node.state))
                    .font(.caption.weight(.medium))
                    .foregroundStyle(Self.color(tone))
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityIdentifier("workflow-node-state")

            // The engine's own words for a `failed` / `waiting` node.
            if let note = node.note, !note.isEmpty {
                Text(note)
                    .font(.caption)
                    .foregroundStyle(Self.color(tone))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("workflow-node-note")
            }

            // EXP-984: a follow-up filed mid-run that was not plainly additive
            // waits for a member. It is not part of the run yet, so none of the
            // run's own controls apply to it — only Admit and Dismiss.
            if node.isProposed {
                Text(WorkflowView.proposedNodeNote)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("workflow-node-proposed-note")
                HStack(spacing: 8) {
                    GlassPill(
                        WorkflowView.admitNodeLabel,
                        icon: AppIcons.uiCheck,
                        mode: .action { onAdmit(true) },
                        primary: true,
                        enabled: !busy
                    )
                    .accessibilityIdentifier("workflow-node-admit")
                    GlassPill(
                        WorkflowView.dismissNodeLabel,
                        icon: AppIcons.uiClose,
                        mode: .action { onAdmit(false) },
                        enabled: !busy
                    )
                    .accessibilityIdentifier("workflow-node-dismiss")
                }
            }

            // A failed node's two ways out.
            if !node.isProposed, node.state == DomainContract.wfNodeStateFailed {
                HStack(spacing: 8) {
                    GlassPill(
                        WorkflowView.retryNodeLabel,
                        icon: AppIcons.uiRefresh,
                        mode: .action { onResolve(.retry) },
                        enabled: !busy
                    )
                    .accessibilityIdentifier("workflow-node-retry")
                    GlassPill(
                        WorkflowView.skipNodeLabel,
                        icon: AppIcons.uiClose,
                        mode: .action { showSkipConfirm = true },
                        enabled: !busy
                    )
                    .accessibilityIdentifier("workflow-node-skip")
                }
            }

            // A node with its PR up waits for an approval: the agent review's,
            // or a person's by hand, which can be taken back until it lands.
            if !node.isProposed {
                if node.approvedAt != nil {
                    if node.state != DomainContract.wfNodeStateLanded {
                        GlassPill(
                            WorkflowView.withdrawApprovalLabel,
                            icon: AppIcons.uiClose,
                            mode: .action { onApprove(false) },
                            enabled: !busy
                        )
                        .accessibilityIdentifier("workflow-node-withdraw")
                    }
                } else if node.state == DomainContract.wfNodeStateInReview {
                    GlassPill(
                        WorkflowView.approveNodeLabel,
                        icon: AppIcons.uiCheck,
                        mode: .action { onApprove(true) },
                        primary: true,
                        enabled: !busy
                    )
                    .accessibilityIdentifier("workflow-node-approve")
                }
            }
        }
    }

    private var tone: WorkflowView.Tone { WorkflowView.nodeTone(node.state) }

    /// The node tones in this app's own state vocabulary — the graph's table.
    private static func color(_ tone: WorkflowView.Tone) -> Color {
        WorkflowGraphView.color(tone)
    }
}
