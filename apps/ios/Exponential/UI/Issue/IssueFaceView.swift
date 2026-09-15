import ExpUI
import ExpCore
import SwiftUI

/// The SCREEN-level sheets (EXP-687): everything the issue face itself
/// presents. The per-property pickers are `IssuePropertyChild` — they stack
/// OVER the Properties sheet now instead of dismissing and re-presenting it
/// (Android's `propertiesOpen` + `activeSheet` split). EXP-893: the `…`
/// menu's Move-to-board picker moved up to the Work screen with the menu.
enum IssueDetailSheet: String, Identifiable {
    case properties

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
    /// EXP-736: the two-stage "Add relation" picker (kind, then issue).
    case addRelation

    var id: String { rawValue }
}

/// EXP-893: the Work screen's ISSUE face — today's issue body (big title,
/// props chip box, description, PR row, files, activity) and its floating
/// bar. Everything around it — the nav bar, the `…` menu, share, delete, the
/// view model's lifetime — is the Work screen's; the face is swapped in and
/// out as screen STATE, so it must be re-mountable without losing the issue.
struct IssueFaceView<Switcher: View>: View {
    let vm: IssueDetailViewModel
    let issue: IssueEntity
    /// The bar's trailing circle: Start coding while there is no own run,
    /// the face switcher once there is one.
    let barTrailing: IssueBarTrailing
    let onStartCoding: () -> Void
    /// The PR / branch row switches the screen to its Changes face.
    let onOpenChanges: () -> Void
    @ViewBuilder let switcher: () -> Switcher

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
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
    // The board picked in a direct move sheet, pending confirmation (EXP-57)
    // — non-nil drives the "Move issue" alert.
    @State private var moveTarget: BoardEntity?
    /// The Properties path's own confirm target: the alert has to hang off the
    /// Properties sheet, not the screen behind it.
    @State private var propertyMoveTarget: BoardEntity?
    /// Courier for both: the picked board, promoted once the picker dismissed.
    @State private var pendingMoveTarget: BoardEntity?
    /// EXP-592: the comment-edit editor lives up here, not inside the timeline,
    /// so the screen can mount ONE candidate menu above the keyboard for it and
    /// for the description alike. CommentThreadView re-seeds it per Edit tap.
    @State private var commentEditEditor = IssueEditorModel()
    /// EXP-741: the reply the docked composer is composing — set by the
    /// thread's "Leave a reply…" row, cleared by the bar.
    @State private var commentReplyTarget: CommentReplyTarget?
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

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // Origin chip: issues filed through the embeddable feedback
                // widget (source='widget') or by a coding agent over MCP
                // (source='agent', EXP-496) carry no user creator — surface
                // that provenance read-only. The row renders only when there
                // IS a chip: an empty one would just be a gap above the title.
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
                    duplicateBanner(duplicateOfId: duplicateOfId)
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

                // EXP-893: no "Coding now" row any more — the run is the
                // screen's Run face, one switch away, and its state rides
                // the nav-bar title dot.

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

                // PR status rows (EXP-156): GitHub-style PR + branch
                // chips → the Changes face. Renders nothing when there's
                // nothing to show.
                AgentPrCard(issue: issue, onOpenChanges: onOpenChanges)

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
                    editEditor: $commentEditEditor,
                    replyTarget: $commentReplyTarget
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
        // EXP-698: the toolbar is `.ultraThinMaterial`, so scrolled
        // prose used to be sliced through its letterforms right at the
        // header's edge — the same 24pt wash the steering feed wears.
        .stickyHeaderFade()
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
                if let editor = scrollerAutocompleteEditor {
                    EditorAutocompleteMenu(model: editor)
                        .padding(.horizontal, 12)
                }

                IssueDetailBottomBar(
                    issue: issue,
                    mentionMembers: vm.mentionMembers,
                    singleMemberTeam: vm.singleMemberTeam,
                    isModerator: vm.permissions.isModerator,
                    trailing: barTrailing,
                    onOpenProperties: { activeSheet = .properties },
                    onStartCoding: onStartCoding,
                    replyTarget: $commentReplyTarget,
                    switcher: switcher
                )
            }
        }
        // Relay config + device presence for the start circle — keyed
        // on session presence AND membership: when a session ends the circle
        // must (re)load presence, and the load must re-run once the members
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
            sheetContent(sheet)
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
        // Each presentation lives on its OWN node (EXP-240): a second `.sheet`
        // in the same chain silently loses to the first.
        .background {
            Color.clear
                .sheet(item: $directChild, onDismiss: {
                    promoteChild(to: .screen)
                    promoteMoveTarget(to: .screen)
                }) { child in
                    childSheet(child)
                }
        }
        .onDisappear {
            // Belt-and-braces with EditorTextView.willMove(toWindow:) — no
            // first responder may outlive this face (EXP-246). The screen
            // keeps observing; a face switch only has to SAVE.
            UIApplication.endEditing()
            Task {
                await vm.saveTitle()
                await vm.commitDescription()
            }
        }
    }

    // MARK: - Autocomplete

    /// The scroller-hosted editor whose candidate menu is currently open, if
    /// any. Both are gated on FOCUS as well as candidates: a candidate set
    /// outlives the blur that hands the keyboard to the other editor, so
    /// without that the previous editor's menu would stay up over the new one.
    private var scrollerAutocompleteEditor: IssueEditorModel? {
        if vm.editor.showsAutocompleteMenu { return vm.editor }
        if commentEditEditor.showsAutocompleteMenu { return commentEditEditor }
        return nil
    }

    // MARK: - Sheets

    /// Which "Move issue" alert a promoted target belongs to — the face's
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
    private func sheetContent(_ sheet: IssueDetailSheet) -> some View {
        switch sheet {
        case .properties:
            IssuePropertiesSheet(
                issue: issue,
                status: vm.resolvedStatus,
                assignee: vm.assignee(),
                labels: vm.teamLabels,
                assignedIds: vm.assignedLabelIds,
                relations: vm.relationRows,
                singleMemberTeam: vm.singleMemberTeam,
                board: vm.board,
                hasMoveTargets: !vm.moveTargetBoards.isEmpty,
                onToggleLabel: { labelId in
                    Task { await vm.toggleLabel(labelId) }
                },
                onRemoveRelation: { relation in
                    Task { await vm.removeRelation(relation) }
                },
                activeChild: $propertyChild,
                onChildDismiss: {
                    promoteChild(to: .properties)
                    promoteMoveTarget(to: .properties)
                },
                child: { child in
                    childSheet(child)
                }
            )
            // The confirm hangs off the Properties ROOT — a different node
            // from the child `.sheet` inside it (EXP-240).
            .moveBoardConfirm(
                target: $propertyMoveTarget,
                identifier: issue.identifier,
                onConfirm: { target in Task { await vm.moveToBoard(target.id) } }
            )
        }
    }

    /// The per-property pickers. The SAME builder feeds the chip box's direct
    /// sheet and the ones Properties stacks over itself.
    @ViewBuilder
    private func childSheet(_ child: IssuePropertyChild) -> some View {
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
            MoveBoardPickerSheet(
                boards: vm.moveTargetBoards,
                selectedId: issue.boardId,
                onSelect: { target in pendingMoveTarget = target }
            )
        case .duplicateOf:
            DuplicatePickerSheet(
                loadCandidates: { await vm.duplicateCandidates() },
                onSelect: { canonical in
                    Task { await vm.markDuplicate(of: canonical) }
                }
            )
        case .addRelation:
            RelationPickerSheet(
                loadCandidates: { await vm.relationCandidates() },
                onSelect: { pick, other in
                    Task { await vm.addRelation(pick, other: other) }
                }
            )
        }
    }

    private var instanceBaseURL: URL? {
        deps.auth.instanceBaseURL(forAccountId: accountId)
    }

    /// "Duplicate of {IDENTIFIER}" — the identifier pill pushes the canonical
    /// issue's Work screen; Unmark clears the FK and restores a working status.
    @ViewBuilder
    private func duplicateBanner(duplicateOfId: String) -> some View {
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
