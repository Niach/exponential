import ExpUI
import ExpCore
import SwiftUI

/// EXP-1170 — the New issue page: a DRAFT in the phone issue face's layout
/// (`IssueFaceView` as hosted by `IssueScreen`), identical ×4. The differences
/// are the whole spec: the header's identifier slot reads
/// `IssueDraftPage.header` (collapsing over the typed title), the trailing
/// cluster is `Create` alone (EXP-1239: no `×` beside the Back), and the body is
/// title → property chips → description → Files, nothing else. The draft
/// autosaves (`IssueDraftViewModel`). EXP-1212: a draft WITH content never
/// goes silently: Back or any navigator path change (held through `IssueDraftLeaveGuard`) asks
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
        threadId: String? = nil,
        onCreated: @escaping (String) -> Void,
        onClose: @escaping () -> Void
    ) {
        self.onCreated = onCreated
        self.onClose = onClose
        self.draftId = draftId
        _vm = State(initialValue: IssueDraftViewModel(
            draftId: draftId, boardId: boardId, statusId: statusId, parentId: parentId,
            threadId: threadId
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
        // EXP-1231: another client consumed the draft. Created elsewhere =
        // this page becomes that issue at once, exactly like its own Create
        // (no prompt, no toast); discarded elsewhere = a toast and Back. Both
        // are the page's own exits: never held, and any open prompt (with
        // the navigation it held) is dropped.
        .onChange(of: vm.consumedElsewhere) { _, consumed in
            guard let consumed else { return }
            dropPrompts()
            switch consumed {
            case let .created(issueId):
                onCreated(issueId)
            case .discarded:
                toaster.info(IssueDraftPage.discardedElsewhere)
                onClose()
            }
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
        // EXP-1212: the app's own centred alert card (`GlassAlert`), never
        // the system alert, on a zero-size node of its own (EXP-240):
        // stacked with the child sheet on one node SwiftUI drops it.
        // The leave dialog: Discard (quiet, leading) … Save draft (plain) ·
        // Create issue (the default, primary, trailing); a sub-issue draft
        // offers no Keep (`IssueDraftPage.leaveChoices`). Create without a
        // title shows disabled, never hidden, and Return moves to Save
        // draft (`IssueDraftPage.leaveDefault`). A scrim tap is the
        // no-answer path (Cancel).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(
                    isPresented: $leavePresented,
                    title: IssueDraftPage.Leave.title,
                    actions: leaveActions,
                    onDismiss: { answerLeave(nil) }
                )
        }
        // Gone WITHOUT an answer (the page torn down under it; a scrim tap
        // already answers nil above): the held navigation is dropped, the page
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
            // A close mid-create would race the draft against the issue; a
            // second Back during a Create/Keep answer would close the page
            // under the in-flight save (EXP-1272: a failed save = lost edit).
            .disabled(vm.creating || leaveAnswerInFlight)
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
    }

    // MARK: - Leaving (EXP-1212)

    /// Hold `held` behind the leave dialog when the draft has content; false
    /// = nothing to ask, the caller goes now. Nothing is held while a Create
    /// is in flight (`vm.prompt` is `.none`): that navigation goes now.
    /// While a Create/Keep answer runs, a second navigation is SWALLOWED
    /// (held and dropped): letting it go would close the page under the
    /// in-flight save, and a failed save would lose the edit (EXP-1272);
    /// the answer continues its own held navigation when it lands.
    /// EXP-1272: a reopened draft is never asked: it runs Keep's save-then-
    /// continue at once (a failed save stays with the save error, as Keep
    /// does). Back and every navigator-driven change come through here.
    private func hold(_ held: IssueDraftLeaveGuard.Held) -> Bool {
        guard !leaveAnswerInFlight else {
            held.dropped()
            return true
        }
        switch vm.prompt(for: .leave) {
        case .none:
            return false
        case .keep:
            UIApplication.endEditing()
            keepThenContinue(held)
            return true
        case .leave:
            break
        }
        UIApplication.endEditing()
        // A second hold while the dialog is up replaces the first, which is
        // dropped (for a link that consumes it off the bus).
        heldLeave?.dropped()
        heldLeave = held
        leaveAnswered = false
        leavePresented = true
        return true
    }

    /// The leave dialog's buttons in reading order (`leaveChoices`): Discard
    /// (leading), then Save draft, then Create issue (primary). Create is the
    /// default answer; disabled, it hands Return to Save draft.
    private var leaveActions: [GlassAlertAction] {
        let choices = vm.leaveChoices
        let createEnabled = IssueDraftPage.leaveCreateEnabled(title: vm.title, creating: vm.creating)
        let defaultChoice = IssueDraftPage.leaveDefault(
            canKeep: choices.contains(.keep),
            createEnabled: createEnabled
        )
        return choices.map { choice in
            switch choice {
            case .create:
                GlassAlertAction(
                    IssueDraftPage.Leave.create,
                    role: .primary,
                    enabled: createEnabled,
                    isDefault: defaultChoice == .create,
                    id: "leave-create"
                ) { answerLeave(.create) }
            case .keep:
                GlassAlertAction(
                    IssueDraftPage.Leave.keep,
                    role: .outline,
                    isDefault: defaultChoice == .keep,
                    id: "leave-keep"
                ) { answerLeave(.keep) }
            case .discard:
                GlassAlertAction(IssueDraftPage.Leave.discard, role: .quietDestructive, id: "leave-discard") {
                    answerLeave(.discard)
                }
            }
        }
    }

    /// A leave-dialog button; nil = dismissed without an answer (scrim tap).
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
            keepThenContinue(held)
        case .discard:
            vm.discard()
            held.proceed()
        }
    }

    /// Keep: save, then continue `held`. A failed save toasts (the page's
    /// save error) and stays. EXP-1272: also a reopened draft's silent exit.
    private func keepThenContinue(_ held: IssueDraftLeaveGuard.Held) {
        leaveAnswerInFlight = true
        Task {
            let kept = await vm.flushForKeep()
            leaveAnswerInFlight = false
            if kept, onScreen { held.proceed() } else { held.dropped() }
        }
    }

    /// EXP-1231: the draft is gone from under the prompt: close it, and
    /// drop the navigation the leave dialog held (marked answered, so its
    /// dismissal never drops it twice).
    private func dropPrompts() {
        if let held = heldLeave {
            heldLeave = nil
            leaveAnswered = true
            held.dropped()
        }
        leavePresented = false
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

                    // EXP-1284: a "New issue in this thread" draft names
                    // its thread (kept across leave + reopen from Drafts).
                    if let thread = vm.thread {
                        HStack(spacing: 6) {
                            Text(draftInThreadPrefix)
                                .font(.footnote)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .fixedSize()
                            IssueThreadChip(thread: thread) {}
                                .allowsHitTesting(false)
                            Spacer(minLength: 0)
                        }
                        .accessibilityElement(children: .combine)
                        .accessibilityLabel("\(draftInThreadPrefix) \(thread.title)")
                        .accessibilityIdentifier("issue-draft-thread")
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

/// EXP-1284: the lead word of a draft's `In <thread>` line (web + Android
/// read the same word).
private let draftInThreadPrefix = "In"
