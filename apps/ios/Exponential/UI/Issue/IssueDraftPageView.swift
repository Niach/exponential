import ExpUI
import ExpCore
import SwiftUI

/// EXP-1170 — the New issue page: a DRAFT in the phone issue face's layout
/// (`IssueFaceView` as hosted by `WorkScreen`), identical ×4. The differences
/// are the whole spec: the header's identifier slot reads
/// `IssueDraftPage.header` (collapsing over the typed title), the trailing
/// cluster is `Create` + an `×` labelled `Discard draft` (EXP-1191), and the body is
/// title → property chips → description → Files, nothing else. The draft
/// autosaves (`IssueDraftViewModel`). EXP-1212: a draft WITH content never
/// goes silently: `×` confirms (`IssueDraftPage.DiscardConfirm`), and Back or
/// any navigator path change (held through `IssueDraftLeaveGuard`) asks
/// `IssueDraftPage.Leave`. With the system back button hidden, the
/// interactive swipe-back is off, so the custom Back is the only pop.
struct IssueDraftPageView: View {
    /// The created issue's id — the host replaces this page with it.
    let onCreated: (String) -> Void
    /// Leave the page (Back, Discard, a board that is gone).
    let onClose: () -> Void
    private let draftId: String

    @State private var vm: IssueDraftViewModel

    /// The board picker's title: the draft PICKS its board, it never moves.
    static let boardPickerTitle = "Board"

    init(
        draftId: String,
        boardId: String,
        statusId: String? = nil,
        parentId: String? = nil,
        onCreated: @escaping (String) -> Void,
        onClose: @escaping () -> Void
    ) {
        self.onCreated = onCreated
        self.onClose = onClose
        self.draftId = draftId
        _vm = State(initialValue: IssueDraftViewModel(
            draftId: draftId, boardId: boardId, statusId: statusId, parentId: parentId
        ))
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.issueDraftLeaveGuard) private var leaveGuard
    @FocusState private var titleFocused: Bool
    /// The picker a chip opened (the face's direct path).
    @State private var child: IssuePropertyChild?
    /// EXP-1162: the header title's collapse, flipped only on the edge.
    @State private var titleScrolledAway = false
    @State private var titleEdges = TitleCollapseTracker()
    /// EXP-1212: `×` on a draft with content asks first.
    @State private var confirmDiscard = false
    /// EXP-1212: the navigation the leave dialog holds, and the dialog.
    @State private var heldLeave: IssueDraftLeaveGuard.Held?
    @State private var leavePresented = false
    /// Set by EVERY leave-dialog button (Cancel too) before the dialog's
    /// `isPresented` flip is observed, so a flip without it = dismissed.
    @State private var leaveAnswered = false
    /// A Create / Keep answer is running: nothing more is held meanwhile.
    @State private var leaveAnswerInFlight = false
    /// The page is on screen: an async answer that lands after the page
    /// was left (another navigation went through meanwhile) never replays.
    @State private var onScreen = false

    var body: some View {
        ZStack {
            AppBackground()
            content
                // The detail chrome's header band, with no face tabs.
                .workHeaderBand(onBottom: { bottom in
                    titleEdges.headerBottom = bottom
                    titleEdgesChanged()
                }) { EmptyView() }
                .onPreferenceChange(IssueTitleRowBottomKey.self) { bottom in
                    titleEdges.titleBottom = bottom
                    titleEdgesChanged()
                }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.hidden, for: .navigationBar)
        // The page owns its Back: leaving runs the draft's final write.
        .navigationBarBackButtonHidden(true)
        .toolbar { toolbarContent }
        // Lifecycle on the always-present root (never an empty Group).
        .onAppear {
            vm.start(deps: deps, accountId: accountId)
            // Back from a route pushed over the page: editing resumes.
            vm.resume()
            titleFocused = true
            onScreen = true
            leaveGuard.register(draftId: draftId) { held in hold(held) }
        }
        .onDisappear {
            onScreen = false
            leaveGuard.unregister(draftId: draftId)
            vm.leave()
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .background { vm.saveNow() }
        }
        .onChange(of: vm.error) { _, message in
            guard let message else { return }
            toaster.error(message)
            vm.error = nil
        }
        .onChange(of: vm.loadFailed) { _, failed in
            if failed { onClose() }
        }
        // Presenting a picker over a focused editor kept it first responder
        // (EXP-246): resign before the picker lands.
        .onChange(of: child) { _, shown in
            if shown != nil { UIApplication.endEditing() }
        }
        // The description's blur flushes the autosave.
        .onChange(of: vm.editor.focusedBlockId) { _, focused in
            if focused == nil { vm.saveNow() }
        }
        .alert(IssueDraftPage.DiscardConfirm.title, isPresented: $confirmDiscard) {
            Button(IssueDraftPage.DiscardConfirm.confirm, role: .destructive) { discardAndClose() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(IssueDraftPage.DiscardConfirm.body)
        }
        // The stack-merge choice's three-answer dialog (`WorkMergePill`).
        .confirmationDialog(
            IssueDraftPage.Leave.title,
            isPresented: $leavePresented,
            titleVisibility: .visible
        ) {
            // A sub-issue draft offers no Keep (`IssueDraftPage.leaveChoices`).
            ForEach(vm.leaveChoices, id: \.self) { choice in
                switch choice {
                case .create:
                    Button(IssueDraftPage.Leave.create) { answerLeave(.create) }
                        .disabled(!IssueDraftPage.leaveCreateEnabled(title: vm.title, creating: vm.creating))
                case .keep:
                    Button(IssueDraftPage.Leave.keep) { answerLeave(.keep) }
                case .discard:
                    Button(IssueDraftPage.Leave.discard, role: .destructive) { answerLeave(.discard) }
                }
            }
            Button("Cancel", role: .cancel) { answerLeave(nil) }
        } message: {
            Text(IssueDraftPage.Leave.body)
        }
        // Gone WITHOUT an answer (a tap outside that ran no button, the page
        // torn down under it): the held navigation is dropped, the page
        // stays. Every button sets `leaveAnswered` synchronously in the tap
        // that also flips `isPresented`; `onChange` is delivered on the
        // following view update, so it always sees the flag.
        .onChange(of: leavePresented) { _, presented in
            guard !presented else { return }
            defer { leaveAnswered = false }
            guard !leaveAnswered, let held = heldLeave else { return }
            heldLeave = nil
            held.dropped()
        }
        .sheet(item: issueViewChild($child)) { target in
            childSheet(target)
        }
        .background {
            IssuePropertyPickers(
                model: vm,
                child: $child,
                boardPickerTitle: Self.boardPickerTitle,
                onSelectBoard: { board in vm.setBoard(board.id) },
                onDismiss: {}
            )
        }
    }

    // MARK: - Header

    private func titleEdgesChanged() {
        let away = titleEdges.collapsed
        if away != titleScrolledAway { titleScrolledAway = away }
    }

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .principal) {
            WorkTitle(
                text: IssueDraftPage.header,
                issueTitle: vm.trimmedTitle.isEmpty ? IssueDraftPage.untitled : vm.trimmedTitle,
                collapsed: titleScrolledAway
            )
        }
        ToolbarItem(placement: .topBarLeading) {
            Button {
                let held = IssueDraftLeaveGuard.Held(
                    proceed: {
                        vm.leave()
                        onClose()
                    },
                    dropped: {}
                )
                if !hold(held) { held.proceed() }
            } label: {
                AppIcon(AppIcons.uiBack, size: AppIcon.Size.medium, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: 32, height: 32)
                    .contentShape(Circle().inset(by: -GlassMenuTokens.triggerHitInset))
            }
            .buttonStyle(.plain)
            // A close mid-create would race the draft against the issue.
            .disabled(vm.creating)
            .accessibilityLabel("Back")
        }
        ToolbarItem(placement: .topBarTrailing) {
            Button(IssueDraftPage.create) {
                Task {
                    UIApplication.endEditing()
                    if let id = await vm.create() { onCreated(id) }
                }
            }
            .fontWeight(.semibold)
            .disabled(!vm.canCreate)
            .accessibilityIdentifier("issue-draft-create")
        }
        // EXP-1191: Discard is the draft's only action — a bare `×`, not a
        // one-item `…` menu; its label (and pointer tooltip) says what it does.
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                if vm.prompt(for: .discard) == .discardConfirm {
                    confirmDiscard = true
                } else {
                    discardAndClose()
                }
            } label: {
                AppIcon(AppIcons.uiClose, size: AppIcon.Size.medium, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: 32, height: 32)
                    .contentShape(Circle().inset(by: -GlassMenuTokens.triggerHitInset))
            }
            .buttonStyle(.plain)
            .disabled(vm.creating)
            .help(IssueDraftPage.discard)
            .accessibilityLabel(IssueDraftPage.discard)
            .accessibilityIdentifier("issue-draft-discard")
        }
    }

    // MARK: - Leaving (EXP-1212)

    /// Hold `held` behind the leave dialog when the draft has content; false
    /// = nothing to ask, the caller goes now. Nothing is held while a Create
    /// is in flight (`vm.prompt` is `.none`) or a Create/Keep answer runs:
    /// that navigation goes now, and the answer, landing off screen, drops
    /// its own held one instead of replaying it.
    private func hold(_ held: IssueDraftLeaveGuard.Held) -> Bool {
        guard !leaveAnswerInFlight, vm.prompt(for: .leave) == .leave else { return false }
        UIApplication.endEditing()
        // A second hold while the dialog is up replaces the first, which is
        // dropped (for a link that consumes it off the bus).
        heldLeave?.dropped()
        heldLeave = held
        leaveAnswered = false
        leavePresented = true
        return true
    }

    /// A leave-dialog button; nil = Cancel.
    private func answerLeave(_ answer: IssueDraftPage.LeaveChoice?) {
        leaveAnswered = true
        guard let held = heldLeave else { return }
        // Claimed here, so the dismissal that follows never drops it.
        heldLeave = nil
        // R2: a Create that started meanwhile owns the draft.
        guard let answer, !vm.creating else {
            held.dropped()
            return
        }
        switch answer {
        case .create:
            // The page's Create; on success the held navigation continues
            // INSTEAD of opening the new issue. A failure toasts and stays.
            leaveAnswerInFlight = true
            Task {
                let created = await vm.create() != nil
                leaveAnswerInFlight = false
                if created, onScreen { held.proceed() } else { held.dropped() }
            }
        case .keep:
            // A failed save toasts (the page's save error) and stays.
            leaveAnswerInFlight = true
            Task {
                let kept = await vm.flushForKeep()
                leaveAnswerInFlight = false
                if kept, onScreen { held.proceed() } else { held.dropped() }
            }
        case .discard:
            vm.discard()
            held.proceed()
        }
    }

    /// Discard draft, the page's own exit: never the leave dialog, and never
    /// while a Create is in flight (it owns the draft).
    private func discardAndClose() {
        guard !vm.creating else { return }
        vm.discard()
        onClose()
    }

    // MARK: - Body

    private var content: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 8) {
                    // Parent mode (EXP-1097): the line the face draws.
                    if let parent = vm.parentIssue {
                        IssueParentLine(
                            parent: IssueRelationsView.Row(
                                id: parent.id,
                                identifier: parent.identifier ?? "",
                                title: parent.title,
                                status: parent.status,
                                open: true
                            ),
                            status: IssueStatusResolver.resolve(parent, team: vm.teamStatuses),
                            onOpen: nil
                        )
                    }

                    IssueTitleField(
                        text: Binding(get: { vm.title }, set: { vm.setTitle($0) }),
                        placeholder: IssueDraftPage.titlePlaceholder,
                        focused: $titleFocused,
                        accessibilityIdentifier: "issue-title-field",
                        onBlur: { vm.saveNow() },
                        onSubmit: { vm.saveNow() }
                    )
                }

                IssuePropertyChipsBox(
                    subject: .init(
                        priority: vm.priority.rawValue,
                        assigneeId: vm.assigneeId,
                        dueDate: vm.dueDate
                    ),
                    status: vm.status,
                    assignee: vm.assignee,
                    assignedLabels: vm.assignedLabels,
                    singleMemberTeam: vm.singleMemberTeam,
                    estimationType: DomainContract.issueEstimationNone,
                    isModerator: vm.permissions.isModerator,
                    board: vm.showsBoardChip ? vm.board : nil,
                    showsUnsetDueDate: true,
                    backgroundOpensProperties: false,
                    labelsBeforeDueDate: true,
                    onTapProperty: { child = $0 },
                    // No Properties sheet behind a draft: "+" adds a label.
                    onOpenProperties: { child = .labels }
                )

                MarkdownEditor(
                    model: vm.editor,
                    placeholder: IssueDraftPage.descriptionPlaceholder,
                    baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                    accountId: accountId,
                    httpClient: deps.httpClient,
                    mentionMembers: vm.mentionMembers,
                    minHeight: 200,
                    onAttachFile: { url in vm.ingestFile(url) }
                )
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("issue-description")

                DraftFilesSection(files: vm.attachments) { file in
                    Task { await vm.removeAttachment(file) }
                }
            }
            .padding(20)
            // Tap-outside keyboard dismissal (EXP-246): a catcher BEHIND the
            // content, so only dead-space taps reach it.
            .background {
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture { UIApplication.endEditing() }
            }
        }
        .scrollDismissesKeyboard(.interactively)
        // EXP-592: the description's `@`/`#`/`:` menu rides above the keyboard.
        .safeAreaInset(edge: .bottom) {
            if vm.editor.showsAutocompleteMenu {
                EditorAutocompleteMenu(model: vm.editor)
                    .padding(.horizontal, 20)
                    .padding(.bottom, 8)
            }
        }
    }

    /// The property children that present a view of their own — on a draft
    /// only the due date (no estimate, no relations).
    @ViewBuilder
    private func childSheet(_ target: IssuePropertyChild) -> some View {
        switch target {
        case .dueDate:
            DueDateSheet(
                date: vm.dueDate.flatMap { AppDateFormatters.yyyyMMdd.date(from: $0) },
                onDateChange: { vm.setDueDate($0) }
            )
        default:
            EmptyView()
        }
    }
}
