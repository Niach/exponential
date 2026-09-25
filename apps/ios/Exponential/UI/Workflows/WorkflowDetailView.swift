import ExpCore
import ExpUI
import SwiftUI

/// EXP-1086 — one workflow on a phone: the open question (only while one is
/// open), the caption line with the ONE primary action and an overflow, the
/// chip strip (one row per wave, `All` first), and under it the Work screen's
/// faces. `All` draws the workflow's own faces — its issues nested (Issue),
/// its runs as the session tree (Run), the final pull request (Changes) and
/// every run's screenshots (Results) — behind the SAME face switcher; one
/// picked node is that issue's Work screen, in place.
///
/// What the page says is the shared view model (`WorkflowView`, ×4); which
/// chip is picked is `WorkflowSelection`.
struct WorkflowDetailView: View {
    let workflowId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    @State private var model: WorkflowDetailModel?
    /// The page's face — `All`'s, and the one a picked node's Work screen
    /// opens on; a face switched inside that screen lifts back here, so a
    /// step to the next node keeps it.
    @State private var face: WorkFaceKind = .issue
    @State private var switcherAnchor: CGRect = .zero
    @State private var switcherOpen = false
    @State private var menuAnchor: CGRect = .zero
    @State private var menuOpen = false
    @State private var devicePickerOpen = false
    @State private var collapsedRuns: Set<String> = []
    /// The chip whose mini-graph is up (long-press).
    @State private var graphNodeId: String?
    @State private var skipNodeId: String?
    @State private var dismissNodeId: String?
    /// One draft per asking run.
    @State private var answerDrafts: [String: String] = [:]
    @State private var decisionsOpen = false
    @State private var nameDraft = ""
    @State private var seededName = false
    @FocusState private var nameFocused: Bool
    @State private var showDeleteConfirm = false
    @State private var showStopConfirm = false
    @State private var showMergeConfirm = false

    var body: some View {
        ZStack {
            AppBackground()
            if let model {
                if model.workflow != nil {
                    page(model)
                } else if model.loaded {
                    missingState
                } else {
                    ProgressView().tint(.white)
                }
            } else {
                ProgressView().tint(.white)
            }
        }
        .navigationTitle(model?.workflow?.name ?? WorkflowView.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            // The name with the workflow's status glyph beside it (the chip
            // vocabulary, as web's header).
            ToolbarItem(placement: .principal) {
                HStack(spacing: 6) {
                    if let model, model.workflow != nil {
                        let display = WorkflowView.statusGlyph(status: model.status)
                        AppIcon(Self.glyph(display), size: AppIcon.Size.small)
                            .foregroundStyle(Self.color(display))
                            .accessibilityIdentifier("workflow-status-glyph")
                    }
                    if model?.workflow != nil {
                        // The name saves on commit or blur (`rename`).
                        GlassTextField("Name", text: $nameDraft, bordered: false) {
                            EmptyView()
                        } trailing: {
                            EmptyView()
                        }
                        .font(.headline)
                        .focused($nameFocused)
                        .onSubmit { model?.rename(nameDraft) }
                        .accessibilityIdentifier("workflow-name-field")
                    } else {
                        Text(WorkflowView.title)
                            .font(.headline)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                    }
                }
            }
        }
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("workflow-detail")
        .onAppear {
            if model == nil {
                model = WorkflowDetailModel(workflowId: workflowId, accountId: accountId, deps: deps)
            }
            model?.observe()
        }
        .onChange(of: model?.workflow?.name, initial: true) { _, name in
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
        .noticeToast(
            Binding(get: { model?.error }, set: { model?.error = $0 }), isError: true
        )
        .glassMenuOverlay(isPresented: $menuOpen, anchor: menuAnchor, presentation: .inline) {
            overflowItems
        }
        .glassMenuOverlay(isPresented: $switcherOpen, anchor: switcherAnchor, presentation: .inline) {
            switcherItems
        }
        .background {
            if let model {
                DevicePicker(
                    devices: model.runnerChoices.map(DevicePickerDevice.init),
                    value: model.workflow?.deviceId,
                    onChange: { model.setDevice($0) },
                    title: WorkflowView.runsOnLabel,
                    open: $devicePickerOpen,
                    hideTrigger: true
                ) { EmptyView() }
            }
        }
        .confirmationDialog(
            WorkflowView.deleteLabel, isPresented: $showDeleteConfirm, titleVisibility: .visible
        ) {
            Button(WorkflowView.deleteLabel, role: .destructive) { model?.delete { dismiss() } }
            Button("Cancel", role: .cancel) {}
        }
        .confirmationDialog(
            "Stop this workflow?", isPresented: $showStopConfirm, titleVisibility: .visible
        ) {
            Button(WorkflowView.stopWorkflowLabel, role: .destructive) { model?.cancel() }
            Button("Keep running", role: .cancel) {}
        } message: {
            Text(WorkflowView.cancelConfirm)
        }
        .confirmationDialog(
            WorkflowView.skipNodeLabel,
            isPresented: Binding(get: { skipNodeId != nil }, set: { if !$0 { skipNodeId = nil } }),
            titleVisibility: .visible
        ) {
            Button(WorkflowView.skipNodeLabel, role: .destructive) {
                if let id = skipNodeId { model?.perform(.skip, on: id) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(WorkflowView.skipNodeConfirm)
        }
        .confirmationDialog(
            WorkflowView.dismissNodeLabel,
            isPresented: Binding(get: { dismissNodeId != nil }, set: { if !$0 { dismissNodeId = nil } }),
            titleVisibility: .visible
        ) {
            Button(WorkflowView.dismissNodeLabel, role: .destructive) {
                if let id = dismissNodeId { model?.perform(.dismiss, on: id) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(WorkflowView.dismissNodeConfirm)
        }
        .alert(WorkflowView.mergeFinalPrLabel, isPresented: $showMergeConfirm) {
            Button(WorkflowView.mergeFinalPrLabel) { model?.mergeFinalPr() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(WorkflowView.mergeFinalPrConfirm)
        }
    }

    // MARK: - Page

    private func page(_ model: WorkflowDetailModel) -> some View {
        VStack(spacing: 0) {
            questionBanner(model)
            header(model)
            strip(model)
            stepper(model)
            if let node = model.selectedNode {
                // One node = its Work screen, in place: the issue's, or a
                // batch node's own run of mine (an issue subject cannot find
                // an issue-less run). It opens on the page's face and reports
                // a switch back, so a step keeps the face (a face the node
                // lacks falls back).
                let work = model.work(for: node)
                WorkScreen(
                    subject: Self.workSubject(work.subject),
                    initialFace: WorkFaces.fallbackFace(shown: face, available: work.faces)
                        ?? work.faces.first ?? .issue,
                    onFaceChange: { face = $0 }
                )
                .id("\(node.id)|\(work.subject)")
            } else {
                allFace(model)
            }
        }
        .onChange(of: availableFaces(model)) { _, faces in
            // `All`'s faces only: a picked node's screen falls back itself.
            if model.selection.isAll, !faces.contains(face), let next = WorkFaces.fallbackFace(shown: face, available: faces) {
                face = next
            }
        }
    }

    private static func workSubject(_ subject: WorkflowNodeSubject) -> WorkSubject {
        switch subject {
        case let .issue(id): .issue(id: id)
        case let .session(id): .session(id: id)
        }
    }

    // MARK: - The open question

    /// EVERY open question; the answer field only on the ones whose run is
    /// mine (`isAnswerable`).
    @ViewBuilder
    private func questionBanner(_ model: WorkflowDetailModel) -> some View {
        let questions = model.openQuestions
        if !questions.isEmpty {
            VStack(spacing: 8) {
                ForEach(questions, id: \.sessionId) { question in
                    questionCard(question, model: model)
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)
        }
    }

    private func questionCard(_ question: WorkflowOpenQuestion, model: WorkflowDetailModel) -> some View {
        let draft = Binding(
            get: { answerDrafts[question.sessionId] ?? "" },
            set: { answerDrafts[question.sessionId] = $0 }
        )
        return VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                SessionStateDot(tone: .needsInput)
                if let node = model.node(question.nodeId) {
                    Text(model.identifier(of: node))
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            Text(question.question)
                .font(.subheadline)
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity, alignment: .leading)
            if model.isAnswerable(question) {
                GlassTextField("Answer", text: draft, lines: 1...4) {
                    EmptyView()
                } trailing: {
                    Button {
                        if model.answer(draft.wrappedValue, to: question.sessionId) {
                            answerDrafts[question.sessionId] = nil
                        }
                    } label: {
                        AppIcon(AppIcons.uiSend, size: AppIcon.Size.medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                    .buttonStyle(.plain)
                    .disabled(draft.wrappedValue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityLabel("Send answer")
                }
                .accessibilityIdentifier("workflow-question-answer")
            }
        }
        .padding(12)
        .glassCard()
        .accessibilityIdentifier("workflow-question")
    }

    // MARK: - Header

    private func header(_ model: WorkflowDetailModel) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                Text(model.caption)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(2)
                    .accessibilityIdentifier("workflow-caption")
                Spacer(minLength: 8)
                primaryButton(model)
                GlassMenuBarButton(
                    icon: AppIcons.uiMore,
                    accessibilityLabel: "More",
                    anchor: $menuAnchor,
                    isPresented: $menuOpen
                )
                .accessibilityIdentifier("workflow-overflow")
            }
            if model.primaryAction == .start, let blocker = model.startBlocker {
                Text(blocker)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Palette.destructive)
                    .accessibilityIdentifier("workflow-start-blocker")
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private func primaryButton(_ model: WorkflowDetailModel) -> some View {
        switch model.primaryAction {
        case .pickDevice:
            GlassPill(
                WorkflowView.pickDeviceLabel,
                icon: AppIcons.uiDevice,
                size: .md,
                mode: .action { openDevicePicker(model) },
                primary: true,
                enabled: !model.busy
            )
            .accessibilityIdentifier("workflow-pick-device")
        case .start:
            GlassPill(
                WorkflowView.startLabel,
                icon: AppIcons.actionRun,
                size: .md,
                mode: .action { model.start() },
                primary: true,
                enabled: model.startBlocker == nil && !model.busy
            )
            .accessibilityIdentifier("workflow-start-button")
        case .pause:
            GlassPill(
                WorkflowView.pauseLabel,
                icon: AppIcons.runPause,
                size: .md,
                mode: .action { model.pause() },
                enabled: !model.busy
            )
            .accessibilityIdentifier("workflow-pause-button")
        case .resume:
            GlassPill(
                WorkflowView.resumeLabel,
                icon: AppIcons.runResume,
                size: .md,
                mode: .action { model.resume() },
                primary: true,
                enabled: !model.busy
            )
            .accessibilityIdentifier("workflow-resume-button")
        case .reviewFinalPr:
            GlassPill(
                WorkflowView.reviewFinalPrLabel,
                icon: AppIcons.navReviews,
                size: .md,
                // The final PR row (with Merge) lives on All × Changes.
                mode: .action {
                    model.selection.click(nil, in: model.order)
                    face = .changes
                },
                primary: true
            )
            .accessibilityIdentifier("workflow-review-button")
        case nil:
            EmptyView()
        }
    }

    /// The shared overflow rule (`WorkflowView.overflowMenu`, ×4).
    @ViewBuilder
    private var overflowItems: some View {
        if let model {
            ForEach(WorkflowView.overflowMenu(status: model.status), id: \.self) { item in
                switch item {
                case .plan:
                    GlassMenuItem(WorkflowView.planLabel, icon: AppIcons.actionRun) { plan(model) }
                case .runsOn:
                    GlassMenuItem(WorkflowView.runsOnLabel, icon: AppIcons.uiDevice) {
                        openDevicePicker(model)
                    }
                case .stop:
                    GlassMenuItem(
                        WorkflowView.stopWorkflowLabel, icon: AppIcons.uiStop, destructive: true
                    ) { showStopConfirm = true }
                case .delete:
                    GlassMenuItem(
                        WorkflowView.deleteLabel, icon: AppIcons.uiDelete, destructive: true
                    ) { showDeleteConfirm = true }
                }
            }
        }
    }

    private func openDevicePicker(_ model: WorkflowDetailModel) {
        Task {
            await model.loadDevices()
            devicePickerOpen = true
        }
    }

    /// The planner run: the Agent page composer with the hidden Plan-workflow
    /// builtin and THIS workflow named.
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

    // MARK: - Chip strip

    private func strip(_ model: WorkflowDetailModel) -> some View {
        let waves = model.strip
        return ScrollView(.horizontal, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(waves.enumerated()), id: \.element.wave) { index, wave in
                    HStack(spacing: 8) {
                        if index == 0 { allChip(model) }
                        ForEach(wave.nodes, id: \.id) { chip in
                            nodeChip(chip, model: model)
                        }
                    }
                }
                if waves.isEmpty { allChip(model) }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 6)
        }
        .accessibilityIdentifier("workflow-strip")
    }

    /// One node picked: step along the strip in DAG order, All at position 0
    /// (`WorkflowSelection.step`). The Work screen's own switcher belongs to
    /// the issue, so the stepping sits here, under the strip.
    @ViewBuilder
    private func stepper(_ model: WorkflowDetailModel) -> some View {
        let order = model.order
        if !model.selection.isAll, !order.isEmpty {
            HStack(spacing: 8) {
                GlassPill(
                    "Previous",
                    icon: AppIcons.uiChevronLeft,
                    mode: .action { model.selection.step(-1, in: order) }
                )
                .accessibilityIdentifier("workflow-step-previous")
                Spacer(minLength: 8)
                Text("\(model.selection.position(in: order)) of \(order.count)")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Spacer(minLength: 8)
                GlassPill(
                    "Next",
                    icon: AppIcons.uiChevronRight,
                    mode: .action { model.selection.step(1, in: order) },
                    enabled: model.selection.position(in: order) < order.count
                )
                .accessibilityIdentifier("workflow-step-next")
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 6)
        }
    }

    private func allChip(_ model: WorkflowDetailModel) -> some View {
        GlassPill(
            WorkflowView.allNodesLabel,
            mode: .select(isSelected: model.selection.isAll) {
                model.selection.click(nil, in: model.order)
            }
        )
        .accessibilityIdentifier("workflow-chip-all")
    }

    @ViewBuilder
    private func nodeChip(_ chip: NodeChip, model: WorkflowDetailModel) -> some View {
        let node = model.node(chip.id)
        let menu = node.map { WorkflowView.nodeChipMenu(state: $0.state) } ?? []
        let dimmed = !model.selection.isAll && !model.selection.ids.contains(chip.id)
        let chipView = chipFace(chip)
            .overlay(alignment: .topTrailing) { chipDots(chip) }
            .opacity(dimmed ? 0.55 : 1)
            .contentShape(Rectangle())
            // A tap picks exactly this node; the picked chip stays picked.
            .onTapGesture { model.selection.click(chip.id, in: model.order) }
            .popover(
                isPresented: Binding(
                    get: { graphNodeId == chip.id },
                    set: { if !$0 { graphNodeId = nil } }
                )
            ) {
                if let node {
                    IssueGraphPopover(
                        graph: model.blockGraph(for: node),
                        issues: Array(model.issues.values),
                        onOpenIssue: { issueId in
                            graphNodeId = nil
                            if let target = model.node(coveringIssue: issueId) {
                                model.selection.click(target.id, in: model.order)
                            }
                        }
                    )
                    .presentationCompactAdaptation(.popover)
                }
            }
            .accessibilityIdentifier("workflow-chip")
        if menu.isEmpty {
            chipView.onLongPressGesture { graphNodeId = chip.id }
        } else {
            chipView.contextMenu {
                ForEach(menu, id: \.self) { action in
                    Button(role: action == .skip || action == .dismiss ? .destructive : nil) {
                        if action == .skip {
                            skipNodeId = chip.id
                        } else if action == .dismiss {
                            dismissNodeId = chip.id
                        } else {
                            model.perform(action, on: chip.id)
                        }
                    } label: {
                        Text(Self.menuLabel(action))
                    }
                }
                Button { graphNodeId = chip.id } label: {
                    Text("Blocks")
                }
            }
        }
    }

    @ViewBuilder
    private func chipFace(_ chip: NodeChip) -> some View {
        let issueChip = IssueChip(
            identifier: chip.title,
            title: chip.caption,
            iconName: Self.glyph(chip.display),
            statusColor: Self.color(chip.display)
        )
        if chip.stacked {
            IssueChipStack { issueChip }
        } else {
            issueChip
        }
    }

    /// The live dot while the node's agent is in a turn, the amber one while
    /// it waits on a person.
    @ViewBuilder
    private func chipDots(_ chip: NodeChip) -> some View {
        HStack(spacing: 2) {
            if chip.live { SessionStateDot(tone: .running, pulsing: true, size: 7) }
            if chip.needsYou {
                SessionStateDot(tone: .needsInput, size: 7)
                    .accessibilityLabel(WorkflowView.needsYouLabel)
            }
        }
        .offset(x: 3, y: -3)
    }

    static func glyph(_ display: WorkflowNodeDisplayState) -> String {
        switch display {
        case .queued: AppIcons.uiQueued
        case .running: AppIcons.codingRunning
        case .done: AppIcons.prMerged
        case .failed: AppIcons.uiError
        case .skipped: AppIcons.statusCancelled
        }
    }

    static func color(_ display: WorkflowNodeDisplayState) -> Color {
        switch display {
        case .queued, .skipped: .white.opacity(TextOpacity.tertiary)
        case .running: DesignTokens.Semantic.green
        case .done: DesignTokens.Semantic.blue
        case .failed: DesignTokens.Semantic.red
        }
    }

    static func menuLabel(_ action: NodeChipAction) -> String {
        switch action {
        case .retry: WorkflowView.retryNodeLabel
        case .skip: WorkflowView.skipNodeLabel
        case .admit: WorkflowView.admitNodeLabel
        case .dismiss: WorkflowView.dismissNodeLabel
        }
    }

    // MARK: - All: the workflow's own faces

    private func availableFaces(_ model: WorkflowDetailModel) -> [WorkFaceKind] {
        WorkFaces.availableFaces(
            hasIssue: true,
            // Every node's runs, changes and results live here, so each face
            // exists before its first row does (`No runs yet` / `No changes
            // yet` / `No results yet`, like web).
            hasRun: true,
            hasChanges: true,
            hasResults: true
        )
    }

    private func switcherTargets(_ model: WorkflowDetailModel) -> [WorkFaces.SwitcherTarget] {
        WorkFaces.switcherTargets(
            faces: availableFaces(model), shown: face, runIds: [], shownRunId: nil, offerStart: false
        )
    }

    @ViewBuilder
    private var switcherItems: some View {
        if let model {
            ForEach(switcherTargets(model), id: \.self) { target in
                if case let .face(next) = target {
                    GlassMenuItem(
                        WorkFaces.faceLabel(next, multipleRuns: model.sessions.count > 1),
                        icon: WorkFaceSwitcher.icon(target)
                    ) { face = next }
                }
            }
        }
    }

    private func switcher(_ model: WorkflowDetailModel) -> some View {
        WorkFaceSwitcher(
            mode: WorkFaces.switcherMode(switcherTargets(model)),
            badge: nil,
            badgePulsing: false,
            anchor: $switcherAnchor,
            menuOpen: $switcherOpen,
            onSelect: { target in
                if case let .face(next) = target { face = next }
            }
        )
    }

    @ViewBuilder
    private func allFace(_ model: WorkflowDetailModel) -> some View {
        let shown = availableFaces(model).contains(face) ? face : .issue
        switch shown {
        case .results:
            if model.resultGroups.isEmpty {
                barred(model) { emptyNote(WorkflowView.noResultsLabel, id: "workflow-results-empty") }
            } else {
                SessionResultsFace(groups: model.resultGroups) { switcher(model) }
            }
        case .run:
            barred(model) { runsFace(model) }
        case .changes:
            barred(model, center: { mergeButton(model) }) { changesFace(model) }
        case .issue:
            barred(model) { issuesFace(model) }
        }
    }

    private func barred<Content: View, Center: View>(
        _ model: WorkflowDetailModel,
        @ViewBuilder center: () -> Center = { EmptyView() },
        @ViewBuilder content: () -> Content
    ) -> some View {
        content()
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .safeAreaInset(edge: .bottom) {
                FloatingBottomBar {
                    EmptyView()
                } center: {
                    center()
                } trailing: {
                    switcher(model)
                }
            }
    }

    /// A face with nothing in scope yet: one quiet line.
    private func emptyNote(_ text: String, id: String) -> some View {
        Text(text)
            .font(.subheadline)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.vertical, 24)
            .accessibilityIdentifier(id)
    }

    /// Every covered issue, nested; a tap picks its node.
    private func issuesFace(_ model: WorkflowDetailModel) -> some View {
        let rows = model.nestedIssues
        let guides = TreeGuides.compute(depths: rows.map(\.depth))
        return ScrollView {
            LazyVStack(spacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.element.issue.id) { index, row in
                    issueRow(row.issue, model: model)
                        .treeGuides(guides[index])
                }
                decisions(model)
            }
            .padding(.vertical, 8)
        }
        .accessibilityIdentifier("workflow-issues")
    }

    private func issueRow(_ issue: IssueEntity, model: WorkflowDetailModel) -> some View {
        let status = IssueStatus.from(issue.status)
        return Button {
            if let node = model.node(coveringIssue: issue.id) {
                model.selection.click(node.id, in: model.order)
            }
        } label: {
            HStack(spacing: 10) {
                AppIcon(status.iconName, size: AppIcon.Size.small)
                    .foregroundStyle(status.color)
                    .frame(width: 16)
                Text(issue.identifier ?? "")
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                Text(issue.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .flatRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    /// The workflow's runs as the session tree, then its event log.
    private func runsFace(_ model: WorkflowDetailModel) -> some View {
        let rows = SessionTree.visibleRows(
            SessionTree.sessionTree(
                model.sessions,
                context: SessionTree.Context(
                    workflows: [WorkflowEntity](),
                    workflowNodes: [WorkflowNodeEntity](),
                    issues: Array(model.issues.values)
                )
            ),
            collapsed: collapsedRuns
        )
        let guides = TreeGuides.compute(depths: rows.map(\.depth))
        return ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if rows.isEmpty {
                    emptyNote(WorkflowView.noRunsLabel, id: "workflow-runs-empty")
                }
                ForEach(Array(rows.enumerated()), id: \.element.key) { index, entry in
                    runRow(entry, model: model)
                        .treeGuides(guides[index])
                }
                WorkflowEventList(events: model.events)
            }
            .padding(.vertical, 8)
        }
        .accessibilityIdentifier("workflow-runs")
    }

    @ViewBuilder
    private func runRow(_ entry: SessionTree.FlatRow, model: WorkflowDetailModel) -> some View {
        let expanded = !collapsedRuns.contains(entry.key)
        let onToggle = {
            if collapsedRuns.contains(entry.key) {
                collapsedRuns.remove(entry.key)
            } else {
                collapsedRuns.insert(entry.key)
            }
        }
        switch entry.node {
        case let .session(node):
            let session = node.session
            let issue = session.issueId.flatMap { model.issues[$0] }
            let batch = model.batchIssues(session)
            RunningSessionRow(
                session: session,
                identifier: sessionRowIdentifier(issue: issue, session: session, batchIssues: batch),
                title: sessionRowTitle(issue: issue, session: session, batchIssues: batch),
                state: CodingSessionDisplayState.of(
                    session: session, prState: issue?.prState ?? session.prState
                ),
                device: model.devicePresentation(session),
                open: .route(.agentSession(accountId: accountId, sessionId: session.id)),
                expandable: entry.hasChildren,
                expanded: expanded,
                onToggle: onToggle
            )
        case let .stack(group):
            SessionGroupRow(
                glyph: AppIcons.prStack,
                title: SessionTree.stackGroupLabel,
                count: group.children.count,
                open: .none,
                key: entry.key,
                expanded: expanded,
                onToggle: onToggle
            )
        case .workflow:
            EmptyView()
        }
    }

    /// The collapsed decisions log under the nested issues; empty = hidden.
    @ViewBuilder
    private func decisions(_ model: WorkflowDetailModel) -> some View {
        let text = model.workflow?.decisions.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !text.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                Button {
                    decisionsOpen.toggle()
                } label: {
                    GlassSectionBand(WorkflowView.decisionsLabel) {
                        AppIcon(
                            decisionsOpen ? AppIcons.uiChevronDown : AppIcons.uiChevronRight,
                            size: 12
                        )
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    } trailing: {
                        EmptyView()
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("workflow-decisions")
                if decisionsOpen {
                    Text(text)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .accessibilityIdentifier("workflow-decisions-body")
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 12)
        }
    }

    /// All × Changes: EVERY node in scope keeps its row (chip + PR state) —
    /// one with a PR or pushed branch opens its own Changes face in place,
    /// one without reads `No changes yet` — then the final PR (Merge sits in
    /// the bar).
    private func changesFace(_ model: WorkflowDetailModel) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(model.orderedNodes, id: \.id) { node in
                    nodeChangesRow(node, model: model)
                }
                if model.workflow?.finalPrUrl?.isEmpty == false {
                    finalPrRow(model)
                        .padding(.top, 12)
                }
            }
            .padding(16)
        }
        .accessibilityIdentifier("workflow-changes")
    }

    private func nodeChangesRow(_ node: WorkflowNodeEntity, model: WorkflowDetailModel) -> some View {
        let issue = model.issues[node.issueId]
        let pushed = issue?.prUrl?.isEmpty == false || issue?.branch?.isEmpty == false
        let display = WorkflowView.nodeDisplayState(node.state)
        return Button {
            guard pushed else { return }
            model.selection.click(node.id, in: model.order)
            face = .changes
        } label: {
            HStack(spacing: 10) {
                IssueChip(
                    identifier: WorkflowView.nodeTitle(
                        identifier: model.identifier(of: node), memberCount: node.memberIssueIds.count
                    ),
                    title: issue?.title ?? WorkflowView.nodeUnsyncedTitle,
                    iconName: Self.glyph(display),
                    statusColor: Self.color(display)
                )
                Spacer(minLength: 8)
                Text(Self.changesLabel(issue))
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                if pushed {
                    AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .flatRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!pushed)
        .accessibilityIdentifier("workflow-changes-node")
    }

    /// A node's PR state (`#12 · Open`), its pushed branch, or nothing yet.
    static func changesLabel(_ issue: IssueEntity?) -> String {
        guard let issue else { return WorkflowView.noChangesLabel }
        if issue.prUrl?.isEmpty == false {
            let state = switch issue.prState {
            case DomainContract.prStateMerged: "Merged"
            case DomainContract.prStateClosed: "Closed"
            default: "Open"
            }
            return issue.prNumber.map { "#\($0) · \(state)" } ?? state
        }
        if let branch = issue.branch, !branch.isEmpty { return branch }
        return WorkflowView.noChangesLabel
    }

    /// The ONE final pull request: its state and GitHub.
    private func finalPrRow(_ model: WorkflowDetailModel) -> some View {
        let workflow = model.workflow
        let caption = WorkflowView.finalPrCaption(
            states: model.nodes.map(\.state),
            finalPrState: workflow?.finalPrState,
            finalPrNumber: workflow?.finalPrNumber
        ) ?? workflow?.finalPrNumber.map { "#\($0)" } ?? ""
        return VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(WorkflowView.finalPrTitle)
            Button {
                if let url = workflow?.finalPrUrl.flatMap(URL.init(string:)) { openURL(url) }
            } label: {
                HStack(spacing: 10) {
                    AppIcon(
                        workflow?.finalPrState == DomainContract.prStateMerged
                            ? AppIcons.prMerged : AppIcons.prOpen,
                        size: AppIcon.Size.small
                    )
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Text(caption)
                        .font(.subheadline)
                        .foregroundStyle(.white)
                    Spacer(minLength: 0)
                    AppIcon(AppIcons.uiGithub, size: AppIcon.Size.small)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .flatRow()
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(DomainContract.diffUiOpenOnGithub)
        }
        .accessibilityIdentifier("workflow-final-pr")
    }

    @ViewBuilder
    private func mergeButton(_ model: WorkflowDetailModel) -> some View {
        let state = model.workflow?.finalPrState
        if model.workflow?.finalPrNumber != nil,
           state != DomainContract.prStateMerged, state != DomainContract.prStateClosed,
           model.status != DomainContract.wfStatusCancelled {
            GlassPill(
                WorkflowView.mergeFinalPrLabel,
                icon: AppIcons.prMerged,
                size: .md,
                mode: .action { showMergeConfirm = true },
                primary: true,
                enabled: !model.busy
            )
            .accessibilityIdentifier("workflow-merge-final-pr")
        }
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
