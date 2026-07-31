import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// Every sheet the issue detail can present, driving ONE `.sheet(item:)`
/// (EXP-240 — replaces six independent Bools so sheet hand-offs are just a
/// deferred item swap).
enum IssueDetailSheet: String, Identifiable {
    case status
    case priority
    case assignee
    case labels
    case dueDate
    case properties
    case moveBoard
    case duplicateOf
    case startCoding

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
    // Candidates for the Start-coding sheet, loaded just before presenting.
    @State private var startCandidates: [StartCodingSheet.IssueOption] = []
    // The board picked in the move sheet, pending confirmation (EXP-57) —
    // non-nil drives the "Move issue" alert.
    @State private var moveTarget: BoardEntity?
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
            Button {
                vm.retryLoad()
            } label: {
                Text("Try again")
                    .font(.callout)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
            }
            .glassButton()
            .buttonStyle(.plain)
        }
        .padding(24)
    }

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel, let issue = vm.issue {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        // Header: identifier only (actions live in the nav bar's
                        // menu). EXP-327 dropped the backing-repo chip — the PR
                        // row is the link to the code, and the chip only
                        // repeated what the board already says.
                        HStack(spacing: 6) {
                            if let identifier = issue.identifier {
                                Text(identifier)
                                    .font(.caption.monospaced().weight(.medium))
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 4)
                                    .glassButton()
                            }
                            // Origin chip: issues filed through the embeddable
                            // feedback widget carry source='widget' and no user
                            // creator — surface that provenance read-only.
                            if issue.source == DomainContract.issueSourceWidget {
                                HStack(spacing: 6) {
                                    AppIcon(AppIcons.uiWidget, size: 11)
                                    Text("Feedback widget")
                                        .font(.caption)
                                        .lineLimit(1)
                                }
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .glassButton()
                            }
                            Spacer()
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
                            onTap: { activeSheet = $0 }
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
                            showsMentionButton: !vm.singleMemberTeam,
                            // EXP-327: the description editor is the ONE attach
                            // affordance; non-image picks land in the Files
                            // section below.
                            onAttachFile: vm.permissions.isModerator
                                ? { url in vm.uploadFile(from: url) }
                                : nil
                        )

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
                        CommentThreadView(issue: issue, singleMemberTeam: vm.singleMemberTeam)
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
                // Relay config + device presence for the start circle — keyed
                // on session presence AND membership (mirrors the old
                // AgentPrCard task): when a session ends the circle must
                // (re)load presence, and the load must re-run once the members
                // shape syncs and isMember flips true.
                .task(id: "\(accountId)|\(issue.id)|\(vm.runningSessions.isEmpty)|\(vm.permissions.isMember)") {
                    await vm.refreshSteer()
                }
                .sheet(item: $activeSheet) { sheet in
                    sheetContent(sheet, vm: vm, issue: issue)
                }
                // Presenting a sheet over a focused editor kept the editor
                // first responder — its keyboard-accessory strip then floated
                // over the sheet (EXP-246). Resign before the sheet lands.
                .onChange(of: activeSheet) { _, newSheet in
                    if newSheet != nil { UIApplication.endEditing() }
                }
                // Batch starts insert an issue-LESS session row that never
                // syncs into this issue's runningSessions, so the start circle
                // can't reflect them — confirm explicitly instead (parity with
                // Android's batch-Sent snackbar).
                .alert(
                    "Batch start sent",
                    isPresented: Binding(
                        get: { vm.batchStartNotice != nil },
                        set: { if !$0 { vm.batchStartNotice = nil } }
                    ),
                    presenting: vm.batchStartNotice
                ) { _ in
                    Button("OK", role: .cancel) {}
                } message: { notice in
                    Text(notice)
                }
                .alert(
                    "Move issue",
                    isPresented: Binding(
                        get: { moveTarget != nil },
                        set: { if !$0 { moveTarget = nil } }
                    ),
                    presenting: moveTarget
                ) { target in
                    Button("Move") {
                        Task { await vm.moveToBoard(target.id) }
                    }
                    Button("Cancel", role: .cancel) {}
                } message: { target in
                    Text("\(issue.identifier ?? "This issue") will move to \(target.name) and get a new identifier there.")
                }
                // EXP-327: one `…` and nothing else — share and the subscribe
                // toggle moved inside it (with words, so the bell's state is
                // readable instead of guessed), next to Move to board. The MENU
                // is available to everyone; only the mutating items are
                // moderator-gated (parity with Android).
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        Menu {
                            if let shareURL = vm.shareURL {
                                ShareLink(
                                    item: shareURL,
                                    subject: Text(vm.shareText),
                                    message: Text(vm.shareText)
                                ) {
                                    Label("Share", appIcon: AppIcons.uiShare)
                                }
                            }
                            Button {
                                Task { await vm.toggleSubscribe() }
                            } label: {
                                Label(
                                    vm.isSubscribed ? "Unsubscribe" : "Subscribe",
                                    appIcon: vm.isSubscribed ? AppIcons.uiUnsubscribe : AppIcons.uiSubscribe
                                )
                            }
                            if vm.permissions.isModerator {
                                // Duplicate = status interception (L27): unmark is
                                // the only duplicate action here; marking happens via
                                // the `duplicate` status picker.
                                if issue.duplicateOfId != nil {
                                    Button {
                                        Task { await vm.unmarkDuplicate() }
                                    } label: {
                                        Label("Unmark duplicate", appIcon: AppIcons.statusDuplicate)
                                    }
                                }
                                // Move to another board in the same team
                                // (EXP-57) — hidden when there's nowhere to go.
                                if !vm.moveTargetBoards.isEmpty {
                                    Button {
                                        activeSheet = .moveBoard
                                    } label: {
                                        Label("Move to board", appIcon: AppIcons.navBoards)
                                    }
                                }
                                Button(role: .destructive) {
                                    showDeleteConfirm = true
                                } label: {
                                    Label("Delete issue", appIcon: AppIcons.uiDelete)
                                }
                            }
                        } label: {
                            AppIcon(AppIcons.uiMore, size: AppIcon.Size.large)
                        }
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
        .navigationTitle("Issue")
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
                    issueImagesApi: deps.issueImagesApi,
                    attachmentsApi: deps.attachmentsApi,
                    labelsApi: deps.labelsApi,
                    subscriptionsApi: deps.subscriptionsApi,
                    steerApi: deps.steerApi,
                    auth: deps.auth
                )
                viewModel = vm
                vm.startObserving()
                // Opening an issue clears its inbox notifications (EXP-92) —
                // push taps and universal links never pass through the inbox's
                // own mark-read. Fire-and-forget; also tolerates older
                // self-hosted servers without the mutation.
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
        }
        .onDisappear {
            // Belt-and-braces with EditorTextView.willMove(toWindow:) — no
            // first responder may outlive this screen (EXP-246).
            UIApplication.endEditing()
            if let vm = viewModel {
                Task {
                    await vm.saveTitle()
                    await vm.commitDescription()
                    vm.stopObserving()
                }
            }
        }
    }

    // MARK: - Sheets

    @ViewBuilder
    private func sheetContent(_ sheet: IssueDetailSheet, vm: IssueDetailViewModel, issue: IssueEntity) -> some View {
        switch sheet {
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
                    // leaves the status untouched.
                    if selected.category == .duplicate {
                        handOff(to: .duplicateOf)
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
                users: vm.users,
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
        case .properties:
            IssuePropertiesSheet(
                issue: issue,
                status: vm.resolvedStatus,
                assignee: vm.assignee(),
                labels: vm.teamLabels,
                assignedIds: vm.assignedLabelIds,
                singleMemberTeam: vm.singleMemberTeam,
                boardName: vm.board?.name,
                hasMoveTargets: !vm.moveTargetBoards.isEmpty,
                onNavigate: { handOff(to: $0) },
                onToggleLabel: { labelId in
                    Task { await vm.toggleLabel(labelId) }
                }
            )
        case .moveBoard:
            // Move to board (EXP-57): pick a same-team target, then
            // confirm — the issue is renumbered in the target board, so
            // the move deserves an explicit yes before it fires.
            // Deliberately still the stock PickerSheet.
            PickerSheet(
                title: "Move to board",
                items: vm.moveTargetBoards,
                selectedID: issue.boardId,
                idFor: { $0.id },
                onSelect: { target in
                    // Defer so this sheet finishes dismissing before
                    // the confirmation alert presents.
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
                        moveTarget = target
                    }
                }
            ) { board in
                Label {
                    Text(board.name)
                } icon: {
                    Circle()
                        .fill(Color(hex: board.color) ?? .gray)
                        .frame(width: 10, height: 10)
                }
            }
        case .duplicateOf:
            DuplicatePickerSheet(
                loadCandidates: { await vm.duplicateCandidates() },
                onSelect: { canonical in
                    Task { await vm.markDuplicate(of: canonical) }
                }
            )
            .presentationBackground(.ultraThinMaterial)
        case .startCoding:
            StartCodingSheet(
                devices: vm.steerDevices ?? [],
                issues: startCandidates,
                preselectedIds: [issue.id]
            ) { device, issueIds, options in
                vm.startCoding(on: device, issueIds: issueIds, options: options)
            }
        }
    }

    /// Dismiss the current sheet and present `target` once the dismissal
    /// animation finished (the same trick the duplicate-status interception
    /// has always used).
    private func handOff(to target: IssueDetailSheet) {
        activeSheet = nil
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
            activeSheet = target
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
                Text(vm.duplicateOf?.identifier ?? vm.duplicateOf?.title ?? "issue")
                    .font(.caption.monospaced().weight(.medium))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .glassButton(isActive: true)
            }
            .buttonStyle(.plain)
            Spacer()
            if vm.permissions.isModerator {
                Button {
                    Task { await vm.unmarkDuplicate() }
                } label: {
                    Text("Unmark")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                }
                .glassButton()
                .buttonStyle(.plain)
            }
        }
        .padding(10)
        .glassSection()
    }

    private func parseDate(_ dateString: String?) -> Date? {
        guard let dateString else { return nil }
        return AppDateFormatters.yyyyMMdd.date(from: dateString)
    }
}
