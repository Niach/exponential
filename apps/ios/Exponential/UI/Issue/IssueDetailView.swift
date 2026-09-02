import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// The SCREEN-level sheets (EXP-687): everything the issue detail itself
/// presents. The per-property pickers are `IssuePropertyChild` — they stack
/// OVER the Properties sheet now instead of dismissing and re-presenting it
/// (Android's `propertiesOpen` + `activeSheet` split).
enum IssueDetailSheet: String, Identifiable {
    case properties
    case moveBoard
    case startCoding

    var id: String { rawValue }
}

/// One editable property's picker. Presented over Properties when opened from
/// it, and directly from the chip box.
enum IssuePropertyChild: String, Identifiable {
    case status
    case priority
    case assignee
    case labels
    case dueDate
    case moveBoard
    case duplicateOf

    var id: String { rawValue }
}

struct IssueDetailView: View {
    let issueId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel: IssueDetailViewModel?
    @State private var showDeleteConfirm = false
    @State private var activeSheet: IssueDetailSheet?
    /// A property picker opened straight from the chip box (no Properties
    /// sheet under it). Its own node, so it never collides with `activeSheet`.
    @State private var directChild: IssuePropertyChild?
    /// A property picker stacked over the Properties sheet.
    @State private var propertyChild: IssuePropertyChild?
    /// A picker that has to hand off to ANOTHER sheet (the duplicate-status
    /// interception) parks its target here and it is promoted on dismiss — a
    /// sheet cannot present while its sibling is still animating away.
    @State private var pendingChild: IssuePropertyChild?
    // Candidates for the Start-coding sheet, loaded just before presenting.
    @State private var startCandidates: [StartCodingSheet.IssueOption] = []
    // The board picked in the move sheet, pending confirmation (EXP-57) —
    // non-nil drives the "Move issue" alert.
    @State private var moveTarget: BoardEntity?
    /// The Properties path's own confirm target: the alert has to hang off the
    /// Properties sheet, not the screen behind it.
    @State private var propertyMoveTarget: BoardEntity?
    /// Courier for both: the picked board, promoted once the picker dismissed.
    @State private var pendingMoveTarget: BoardEntity?
    /// The `…` toolbar menu (EXP-687): the popup is an in-view overlay on this
    /// screen's root, so the bar button only reports its frame and toggles.
    @State private var menuAnchor: CGRect = .zero
    @State private var menuOpen = false
    /// EXP-603: `ShareLink` cannot live inside a `GlassMenu` (its rows are
    /// plain buttons), so the menu item hands the URL to a host-level sheet.
    @State private var shareTarget: ShareTarget?
    /// EXP-536: consumed-once push into the run this screen just started —
    /// single AND batch (a batch row is issue-less, so the start circle can
    /// never reflect it).
    @State private var sessionTarget: StartedRunWatcher.StartedSession?
    /// EXP-592: the comment-edit editor lives up here, not inside the timeline,
    /// so the screen can mount ONE candidate menu above the keyboard for it and
    /// for the description alike. CommentThreadView re-seeds it per Edit tap.
    @State private var commentEditEditor = IssueEditorModel()
    @FocusState private var titleFocused: Bool

    // Shown while team membership is still syncing, so a signed-in viewer
    // sees "we're catching up" instead of a silently read-only issue.
    private var syncingBanner: some View {
        HStack(spacing: 8) {
            ProgressView()
                .controlSize(.small)
                .tint(.white)
            Text("Syncing team…")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .glassRow()
    }

    // Shown once the bounded load clock gives up (EXP-264): the issue is
    // either still syncing or genuinely out of reach — say so, and offer
    // another attempt instead of an endless spinner.
    private func unavailableState(vm: IssueDetailViewModel) -> some View {
        VStack(spacing: 12) {
            Text("This issue isn't available yet")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("It may still be syncing, or you may not have access to it.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
            GlassPill("Try again", size: .md, mode: .action { vm.retryLoad() })
        }
        .padding(24)
    }

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel, let issue = vm.issue {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        // Header: provenance only (actions live in the nav bar's
                        // menu). EXP-327 dropped the backing-repo chip — the PR
                        // row is the link to the code, and the chip only
                        // repeated what the board already says. EXP-568 dropped
                        // the identifier chip too: the nav bar title IS the
                        // identifier now, and the chip only said it twice.
                        // Origin chip: issues filed through the embeddable
                        // feedback widget (source='widget') or by a coding
                        // agent over MCP (source='agent', EXP-496) carry no
                        // user creator — surface that provenance read-only.
                        // The row renders only when there IS a chip: an empty
                        // one would just be a gap above the title.
                        if issue.source == DomainContract.issueSourceWidget
                            || issue.source == DomainContract.issueSourceAgent {
                            let isAgent = issue.source == DomainContract.issueSourceAgent
                            HStack(spacing: 6) {
                                GlassPill(
                                    isAgent ? "Agent" : "Feedback widget",
                                    icon: isAgent ? AppIcons.uiAgentSource : AppIcons.uiWidget
                                )
                                Spacer()
                            }
                        }

                        if vm.permissionsPending {
                            syncingBanner
                        }

                        // Canonical-issue banner when marked as a duplicate:
                        // tap-through to the canonical issue + Unmark (§5e).
                        if let duplicateOfId = issue.duplicateOfId {
                            duplicateBanner(vm: vm, duplicateOfId: duplicateOfId)
                        }

                        // Title (editable)
                        TextField("Title", text: Binding(
                            get: { vm.editingTitle },
                            set: { vm.editingTitle = $0 }
                        ))
                        .font(.title2.weight(.semibold))
                        .textFieldStyle(.plain)
                        .foregroundStyle(.white)
                        .focused($titleFocused)
                        .onSubmit { Task { await vm.saveTitle() } }
                        .onChange(of: titleFocused) { _, focused in
                            if !focused { Task { await vm.saveTitle() } }
                        }

                        // Property chip box (EXP-240) — replaces the old
                        // properties / times / labels sections.
                        IssuePropertyChipsBox(
                            issue: issue,
                            status: vm.resolvedStatus,
                            assignee: vm.assignee(),
                            assignedLabels: vm.assignedLabels,
                            singleMemberTeam: vm.singleMemberTeam,
                            isModerator: vm.permissions.isModerator,
                            onTapProperty: { directChild = $0 },
                            onOpenProperties: { activeSheet = .properties }
                        )

                        // A remote edit arrived while editing locally — offer
                        // a non-blocking reload (field-level last-write-wins).
                        if vm.editor.pendingRemoteMarkdown != nil {
                            Button {
                                vm.reloadRemoteDescription()
                            } label: {
                                Label {
                                    Text("Updated by someone else. Reload")
                                } icon: {
                                    AppIcon(AppIcons.uiRefresh, size: AppIcon.Size.small)
                                }
                                .font(.caption)
                                .foregroundStyle(.white)
                                .padding(.horizontal, 12)
                                .padding(.vertical, 8)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(Color.blue.opacity(0.25), in: RoundedRectangle(cornerRadius: 8))
                            }
                            .buttonStyle(.plain)
                        }

                        // Description (block-based markdown editor with images)
                        MarkdownEditor(
                            model: vm.editor,
                            baseURL: instanceBaseURL,
                            accountId: accountId,
                            httpClient: deps.httpClient,
                            mentionMembers: vm.mentionMembers,
                            onIssueRefTap: { issueId in
                                // Route through the deep-link bus — MainNavigator
                                // observes it and pushes the issue route.
                                deps.deepLinkBus.navigateToIssue(issueId)
                            },
                            // EXP-327: the description editor is the ONE attach
                            // affordance; non-image picks land in the Files
                            // section below.
                            // EXP-655 (Android parity): a tappable band below
                            // the description focuses its end.
                            minHeight: 200,
                            onAttachFile: vm.permissions.isModerator
                                ? { url in vm.uploadFile(from: url) }
                                : nil
                        )
                        // EXP-642: the store slide's pop-out rect is measured
                        // off this block (`PopRects`). `contain` keeps the
                        // editor's own elements queryable.
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier("issue-description")

                        // Coding + PR status card (EXP-156): "Coding now" /
                        // GitHub-style PR + branch chips → diff page. Remote
                        // start moved into the bottom bar (EXP-240). Renders
                        // nothing when there's nothing to show.
                        AgentPrCard(
                            issue: issue,
                            runningSessions: vm.runningSessions,
                            permissions: vm.permissions,
                            users: vm.users,
                            config: vm.steerConfig,
                            currentUserId: deps.auth.userId
                        )

                        // Widget/agent submission metadata (EXP-496):
                        // expandable card, default collapsed; renders nothing
                        // for issues without a submission row.
                        if let submission = vm.widgetSubmission {
                            WidgetSubmissionCard(submission: submission, source: issue.source)
                        }

                        // Non-image attachments (EXP-297): rendered from the
                        // synced attachment rows, not from the markdown.
                        IssueFilesSection(viewModel: vm)

                        // Error
                        if let error = vm.error {
                            Text(error)
                                .font(.callout)
                                .foregroundStyle(.red)
                        }

                        // Activity timeline (comments + events)
                        CommentThreadView(
                            issue: issue,
                            singleMemberTeam: vm.singleMemberTeam,
                            editEditor: $commentEditEditor
                        )
                    }
                    .padding(20)
                    // Tap-outside keyboard dismissal (EXP-246): a catcher
                    // BEHIND the content, so it only receives taps on dead
                    // space (gaps, padding) — interactive children and the
                    // UIKit editors keep winning hit-testing and are never
                    // double-handled.
                    .background {
                        Color.clear
                            .contentShape(Rectangle())
                            .onTapGesture { UIApplication.endEditing() }
                    }
                }
                .scrollDismissesKeyboard(.interactively)
                // The floating bottom bar (EXP-240): reserves scroll clearance
                // and rides the keyboard automatically. ALWAYS mounted so the
                // composer draft (bar-owned @State) survives; the bar renders
                // itself zero-height while another editor (title, description,
                // or a comment edit) owns the keyboard, so it never stacks
                // over the markdown toolbar — Android parity:
                // barVisible = composerExpanded || !imeVisible.
                .safeAreaInset(edge: .bottom) {
                    VStack(spacing: 8) {
                        // EXP-592: the `@`/`#`/`:` menu for the two editors that
                        // live in the SCROLLER — the description and the comment
                        // being edited. Riding the safe area is what keeps it
                        // above the keyboard; inside the editor it landed under
                        // the end of a full-length description, off-screen. The
                        // bar keeps its own for its comment composer: that
                        // editor is inside the bar's card, and only one editor
                        // can hold the keyboard, so the two never both render.
                        if let editor = scrollerAutocompleteEditor(vm: vm) {
                            EditorAutocompleteMenu(model: editor)
                                .padding(.horizontal, 12)
                        }

                        IssueDetailBottomBar(
                            issue: issue,
                            mentionMembers: vm.mentionMembers,
                            singleMemberTeam: vm.singleMemberTeam,
                            isModerator: vm.permissions.isModerator,
                            startUi: startCircleUi(vm: vm, issue: issue),
                            onOpenProperties: { activeSheet = .properties },
                            onStartCoding: { presentStartSheet(vm: vm) }
                        )
                    }
                }
                // Relay config + device presence for the start circle — keyed
                // on session presence AND membership (mirrors the old
                // AgentPrCard task): when a session ends the circle must
                // (re)load presence, and the load must re-run once the members
                // shape syncs and isMember flips true. EXP-432 adds the board's
                // team: the device list is team-scoped now, so it must reload
                // once the board (hence the team) resolves.
                .task(id: "\(accountId)|\(issue.id)|\(vm.runningSessions.isEmpty)|\(vm.permissions.isMember)|\(vm.board?.teamId ?? "")") {
                    await vm.refreshSteer()
                }
                // EXP-496: the submission metadata card's one-shot fetch.
                .task(id: "widget-submission-\(issue.id)") {
                    await vm.loadWidgetSubmission()
                }
                .sheet(item: $activeSheet, onDismiss: { promoteMoveTarget(to: .screen) }) { sheet in
                    sheetContent(sheet, vm: vm, issue: issue)
                }
                // Presenting a sheet over a focused editor kept the editor
                // first responder — its keyboard-accessory strip then floated
                // over the sheet (EXP-246). Resign before the sheet lands.
                .onChange(of: activeSheet) { _, newSheet in
                    if newSheet != nil { UIApplication.endEditing() }
                }
                // The chip box presents its pickers directly (EXP-687), so
                // that path needs the same resign.
                .onChange(of: directChild) { _, child in
                    if child != nil { UIApplication.endEditing() }
                }
                .moveBoardConfirm(
                    target: $moveTarget,
                    identifier: issue.identifier,
                    onConfirm: { target in Task { await vm.moveToBoard(target.id) } }
                )
                // EXP-327: one `…` and nothing else — share and the subscribe
                // toggle moved inside it (with words, so the bell's state is
                // readable instead of guessed), next to Move to board. The MENU
                // is available to everyone; only the mutating items are
                // moderator-gated (parity with Android).
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        GlassMenuBarButton(
                            icon: AppIcons.uiMore,
                            accessibilityLabel: "More",
                            anchor: $menuAnchor,
                            isPresented: $menuOpen
                        )
                    }
                }
            } else if let vm = viewModel, vm.loadTimedOut {
                // The row never arrived: it may be outside this account's
                // synced scope entirely (EXP-264 — an endless spinner used to
                // be the only answer here).
                unavailableState(vm: vm)
            } else {
                ProgressView().tint(.white)
            }
        }
        // EXP-687: the `…` popup rides an in-view overlay on THIS root, not a
        // presentation launched from inside the UIKit bar item — that dropped
        // taps, slid in from the bottom and landed off the button.
        .glassMenuOverlay(isPresented: $menuOpen, anchor: menuAnchor, presentation: .inline) {
            toolbarMenuItems
        }
        // Each presentation lives on its OWN node (EXP-240): a second `.sheet`
        // in the same chain silently loses to the first.
        .background {
            Color.clear
                .sheet(item: $directChild, onDismiss: {
                    promoteChild(to: .screen)
                    promoteMoveTarget(to: .screen)
                }) { child in
                    if let vm = viewModel, let issue = vm.issue {
                        childSheet(child, vm: vm, issue: issue)
                    }
                }
        }
        .background {
            Color.clear
                .sheet(item: $shareTarget) { target in
                    ActivityShareSheet(items: [target.text, target.url])
                }
        }
        // The identifier IS the title (EXP-568) — it used to be a chip in the
        // content, one line below a nav bar that just said "Issue".
        .navigationTitle(viewModel?.issue?.identifier ?? "")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .alert("Delete Issue", isPresented: $showDeleteConfirm) {
            Button("Delete", role: .destructive) {
                Task {
                    if await viewModel?.deleteIssue() == true {
                        dismiss()
                    }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This action cannot be undone.")
        }
        .onAppear {
            if viewModel == nil {
                let vm = IssueDetailViewModel(
                    accountId: accountId,
                    issueId: issueId,
                    db: deps.db,
                    issuesApi: deps.issuesApi,
                    attachmentsApi: deps.attachmentsApi,
                    labelsApi: deps.labelsApi,
                    subscriptionsApi: deps.subscriptionsApi,
                    steerApi: deps.steerApi,
                    widgetsApi: deps.widgetsApi,
                    auth: deps.auth
                )
                viewModel = vm
                // Opening an issue clears its inbox notifications (EXP-92) —
                // push taps and universal links never pass through the inbox's
                // own mark-read. Fire-and-forget: a failure just leaves the
                // notifications unread.
                let notificationsApi = deps.notificationsApi
                let accountId = accountId
                let issueId = issueId
                Task {
                    try? await notificationsApi.markReadByIssue(
                        accountId: accountId,
                        issueId: issueId
                    )
                }
            }
            // Re-arm on every appear: pushing a child screen stops the
            // observations (onDisappear), popping back must resume them.
            viewModel?.startObserving()
        }
        .onDisappear {
            // Belt-and-braces with EditorTextView.willMove(toWindow:) — no
            // first responder may outlive this screen (EXP-246).
            UIApplication.endEditing()
            if let vm = viewModel {
                // Stop synchronously: deferring it behind the async saves
                // could cancel the observers a quick pop-back just re-armed.
                vm.stopObserving()
                vm.startWatcher.stop()
                Task {
                    await vm.saveTitle()
                    await vm.commitDescription()
                }
            }
        }
        // The desktop picked the start up — push the live steer screen ONCE
        // (the same destination the .agentSession route arm builds).
        .onChange(of: viewModel?.startWatcher.startedSession) { _, started in
            if let started {
                viewModel?.startWatcher.startedSession = nil
                viewModel?.startPending = false
                sessionTarget = started
            }
        }
        .navigationDestination(item: $sessionTarget) { target in
            AgentSessionRouteView(sessionId: target.sessionId)
                .environment(\.accountId, accountId)
        }
    }

    // MARK: - Autocomplete

    /// The scroller-hosted editor whose candidate menu is currently open, if
    /// any. Both are gated on FOCUS as well as candidates: a candidate set
    /// outlives the blur that hands the keyboard to the other editor, so
    /// without that the previous editor's menu would stay up over the new one.
    private func scrollerAutocompleteEditor(vm: IssueDetailViewModel) -> IssueEditorModel? {
        if vm.editor.showsAutocompleteMenu { return vm.editor }
        if commentEditEditor.showsAutocompleteMenu { return commentEditEditor }
        return nil
    }

    // MARK: - Toolbar menu

    @ViewBuilder
    private var toolbarMenuItems: some View {
        if let vm = viewModel, let issue = vm.issue {
            if let shareURL = vm.shareURL {
                GlassMenuItem("Share", icon: AppIcons.uiShare) {
                    shareTarget = ShareTarget(url: shareURL, text: vm.shareText)
                }
            }
            GlassMenuItem(
                vm.isSubscribed ? "Unsubscribe" : "Subscribe",
                icon: vm.isSubscribed ? AppIcons.uiUnsubscribe : AppIcons.uiSubscribe
            ) {
                Task { await vm.toggleSubscribe() }
            }
            if vm.permissions.isModerator {
                // Duplicate = status interception (L27): unmark is the only
                // duplicate action here; marking happens via the `duplicate`
                // status picker.
                if issue.duplicateOfId != nil {
                    GlassMenuItem("Unmark duplicate", icon: AppIcons.statusDuplicate) {
                        Task { await vm.unmarkDuplicate() }
                    }
                }
                // Move to another board in the same team (EXP-57) — hidden
                // when there's nowhere to go.
                if !vm.moveTargetBoards.isEmpty {
                    GlassMenuItem("Move to board", icon: AppIcons.navBoards) {
                        activeSheet = .moveBoard
                    }
                }
                GlassMenuItem("Delete issue", icon: AppIcons.uiDelete, destructive: true) {
                    showDeleteConfirm = true
                }
            }
        }
    }

    // MARK: - Sheets

    /// Which "Move issue" alert a promoted target belongs to — the screen's
    /// own, or the one hanging off the Properties sheet.
    private enum MoveConfirmHost {
        case screen
        case properties
    }

    private func promoteMoveTarget(to host: MoveConfirmHost) {
        guard let target = pendingMoveTarget else { return }
        pendingMoveTarget = nil
        switch host {
        case .screen: moveTarget = target
        case .properties: propertyMoveTarget = target
        }
    }

    private func promoteChild(to host: MoveConfirmHost) {
        guard let next = pendingChild else { return }
        pendingChild = nil
        switch host {
        case .screen: directChild = next
        case .properties: propertyChild = next
        }
    }

    @ViewBuilder
    private func sheetContent(_ sheet: IssueDetailSheet, vm: IssueDetailViewModel, issue: IssueEntity) -> some View {
        switch sheet {
        case .properties:
            IssuePropertiesSheet(
                issue: issue,
                status: vm.resolvedStatus,
                assignee: vm.assignee(),
                labels: vm.teamLabels,
                assignedIds: vm.assignedLabelIds,
                singleMemberTeam: vm.singleMemberTeam,
                board: vm.board,
                hasMoveTargets: !vm.moveTargetBoards.isEmpty,
                onToggleLabel: { labelId in
                    Task { await vm.toggleLabel(labelId) }
                },
                activeChild: $propertyChild,
                onChildDismiss: {
                    promoteChild(to: .properties)
                    promoteMoveTarget(to: .properties)
                },
                child: { child in
                    childSheet(child, vm: vm, issue: issue)
                }
            )
            // The confirm hangs off the Properties ROOT — a different node
            // from the child `.sheet` inside it (EXP-240).
            .moveBoardConfirm(
                target: $propertyMoveTarget,
                identifier: issue.identifier,
                onConfirm: { target in Task { await vm.moveToBoard(target.id) } }
            )
        case .moveBoard:
            moveBoardPicker(vm: vm, issue: issue)
        case .startCoding:
            // EXP-642: `teamId` + `onRunAction` are what light up the sheet's
            // Actions and Chat tabs — without them the issue detail offered
            // Issues-only, unlike every other host.
            StartCodingSheet(
                devices: vm.steerDevices ?? [],
                issues: startCandidates,
                preselectedIds: [issue.id],
                teamId: vm.board?.teamId,
                onStart: { device, issueIds, options in
                    vm.startCoding(on: device, issueIds: issueIds, options: options)
                },
                onRunAction: { device, action, options, inputs in
                    vm.runAction(on: device, action: action, options: options, inputs: inputs)
                }
            )
        }
    }

    /// The per-property pickers. The SAME builder feeds the chip box's direct
    /// sheet and the ones Properties stacks over itself.
    @ViewBuilder
    private func childSheet(_ child: IssuePropertyChild, vm: IssueDetailViewModel, issue: IssueEntity) -> some View {
        switch child {
        case .status:
            GlassPickerSheet(
                title: "Status",
                // The team's own statuses in render order — the ONE picker
                // vocabulary (REV2-85, EXP-314).
                items: vm.teamStatuses,
                selectedID: vm.resolvedStatus.id,
                idFor: { $0.id },
                onSelect: { selected in
                    // Duplicate CATEGORY = status interception (L27): picking
                    // it opens the canonical-issue picker instead of writing
                    // the status directly; markDuplicate sets duplicateOfId +
                    // status='duplicate' atomically. Cancelling the picker
                    // leaves the status untouched. The hand-off is promoted on
                    // THIS picker's dismiss, never on a timer.
                    if selected.category == .duplicate {
                        pendingChild = .duplicateOf
                    } else {
                        Task { await vm.setStatus(selected) }
                    }
                }
            ) { status in
                Label {
                    Text(status.name)
                } icon: {
                    AppIcon(status.iconName, size: AppIcon.Size.medium)
                        .foregroundStyle(status.color)
                }
            }
        case .priority:
            GlassPickerSheet(
                title: "Priority",
                items: IssuePriority.displayOrder,
                selectedID: IssuePriority.from(issue.priority).id,
                idFor: { $0.id },
                onSelect: { selected in
                    Task { await vm.setPriority(selected) }
                }
            ) { priority in
                Label {
                    Text(priority.label)
                } icon: {
                    AppIcon(priority.iconName, size: AppIcon.Size.medium)
                        .foregroundStyle(priority.color)
                }
            }
        case .assignee:
            AssigneeSheet(
                users: vm.teamUsers,
                selectedId: issue.assigneeId,
                onSelect: { userId in
                    Task { await vm.setAssignee(userId) }
                }
            )
        case .labels:
            LabelsSheet(
                labels: vm.teamLabels,
                assignedIds: vm.assignedLabelIds,
                onToggle: { labelId in
                    Task { await vm.toggleLabel(labelId) }
                },
                onCreate: { name in
                    Task { await vm.createAndAssignLabel(name: name, color: autoLabelColor(for: name)) }
                }
            )
        case .dueDate:
            DueDateSheet(
                date: parseDate(issue.dueDate),
                onDateChange: { date in Task { await vm.setDueDate(date) } }
            )
        case .moveBoard:
            moveBoardPicker(vm: vm, issue: issue)
        case .duplicateOf:
            DuplicatePickerSheet(
                loadCandidates: { await vm.duplicateCandidates() },
                onSelect: { canonical in
                    Task { await vm.markDuplicate(of: canonical) }
                }
            )
        }
    }

    /// Move to board (EXP-57): pick a same-team target, then confirm — the
    /// issue is renumbered in the target board, so the move deserves an
    /// explicit yes before it fires. The pick is parked and promoted once this
    /// picker finished dismissing.
    private func moveBoardPicker(vm: IssueDetailViewModel, issue: IssueEntity) -> some View {
        GlassPickerSheet(
            title: "Move to board",
            items: vm.moveTargetBoards,
            selectedID: issue.boardId,
            idFor: { $0.id },
            onSelect: { target in pendingMoveTarget = target }
        ) { board in
            Label {
                Text(board.name)
            } icon: {
                // Board glyph tinted with the board color — same idiom as
                // the board switcher sheet (EXP-449).
                AppIcon(BoardTypeDisplay.iconName(for: board), size: 16)
                    .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
            }
        }
    }

    // MARK: - Start circle state

    private func startCircleUi(vm: IssueDetailViewModel, issue: IssueEntity) -> StartCircleUi {
        guard vm.steerConfig?.enabled == true,
              vm.permissions.isMember,
              vm.board?.repositoryId != nil else { return .hidden }
        // Multi-window desktops can run several sessions on one issue —
        // surface the caller's most recent OWN session (EXP-312: live
        // sessions are owner-only; a teammate's run shows in the AgentPrCard
        // badge, and the circle falls through to Start coding instead).
        let ownSessions = vm.runningSessions.filter { $0.userId == deps.auth.userId }
        if let session = ownSessions.max(by: { $0.startedAt < $1.startedAt }) {
            return .session(
                CodingSessionDisplayState.of(session: session, prState: issue.prState),
                sessionId: session.id
            )
        }
        if vm.startPending { return .sending }
        guard let devices = vm.steerDevices else { return .hidden }
        return devices.isEmpty ? .noDevices : .start
    }

    private func presentStartSheet(vm: IssueDetailViewModel) {
        Task {
            startCandidates = await vm.startCodingCandidates()
            activeSheet = .startCoding
        }
    }

    private var instanceBaseURL: URL? {
        deps.auth.instanceBaseURL(forAccountId: accountId)
    }

    /// "Duplicate of {IDENTIFIER}" — the identifier pill pushes the canonical
    /// issue's detail; Unmark clears the FK and restores a working status.
    @ViewBuilder
    private func duplicateBanner(vm: IssueDetailViewModel, duplicateOfId: String) -> some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.statusDuplicate, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Duplicate of")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            NavigationLink(value: AppRoute.issue(accountId: accountId, id: duplicateOfId)) {
                GlassPill(vm.duplicateOf?.identifier ?? vm.duplicateOf?.title ?? "issue")
                    .contentShape(Capsule())
            }
            .buttonStyle(.plain)
            Spacer()
            if vm.permissions.isModerator {
                GlassPill("Unmark", mode: .action { Task { await vm.unmarkDuplicate() } })
            }
        }
        .padding(10)
        // A single banner, not a group of rows: it keeps a border of its own.
        .glassCard()
    }

    private func parseDate(_ dateString: String?) -> Date? {
        guard let dateString else { return nil }
        return AppDateFormatters.yyyyMMdd.date(from: dateString)
    }
}

/// The "Move issue" confirmation (EXP-57), reusable so BOTH paths — the `…`
/// menu's picker and the one Properties stacks over itself — confirm with the
/// exact same words on their own host node (EXP-687).
private struct MoveBoardConfirm: ViewModifier {
    @Binding var target: BoardEntity?
    let identifier: String?
    let onConfirm: (BoardEntity) -> Void

    func body(content: Content) -> some View {
        content.alert(
            "Move issue",
            isPresented: Binding(
                get: { target != nil },
                set: { if !$0 { target = nil } }
            ),
            presenting: target
        ) { board in
            Button("Move") { onConfirm(board) }
            Button("Cancel", role: .cancel) {}
        } message: { board in
            // Byte-shared with web, desktop and Android (EXP-426).
            Text("Move \(identifier ?? "this issue") to \"\(board.name)\"? The issue will get a new identifier in that board.")
        }
    }
}

extension View {
    func moveBoardConfirm(
        target: Binding<BoardEntity?>,
        identifier: String?,
        onConfirm: @escaping (BoardEntity) -> Void
    ) -> some View {
        modifier(MoveBoardConfirm(target: target, identifier: identifier, onConfirm: onConfirm))
    }
}
