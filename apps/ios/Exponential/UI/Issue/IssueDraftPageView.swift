import ExpUI
import ExpCore
import SwiftUI

/// EXP-1170 — the New issue page: a DRAFT in the phone issue face's layout
/// (`IssueFaceView` as hosted by `WorkScreen`), identical ×4. The differences
/// are the whole spec: the header's identifier slot reads
/// `IssueDraftPage.header` (collapsing over the typed title), the trailing
/// cluster is `Create` + a `…` holding only `Discard draft`, and the body is
/// title → property chips → description → Files, nothing else. The draft
/// autosaves (`IssueDraftViewModel`); Back never asks.
struct IssueDraftPageView: View {
    /// The created issue's id — the host replaces this page with it.
    let onCreated: (String) -> Void
    /// Leave the page (Back, Discard, a board that is gone).
    let onClose: () -> Void

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
        _vm = State(initialValue: IssueDraftViewModel(
            draftId: draftId, boardId: boardId, statusId: statusId, parentId: parentId
        ))
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @Environment(\.scenePhase) private var scenePhase
    @FocusState private var titleFocused: Bool
    /// The picker a chip opened (the face's direct path).
    @State private var child: IssuePropertyChild?
    @State private var menuAnchor: CGRect = .zero
    @State private var menuOpen = false
    /// EXP-1162: the header title's collapse, flipped only on the edge.
    @State private var titleScrolledAway = false
    @State private var titleEdges = TitleCollapseTracker()

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
        .glassMenuOverlay(isPresented: $menuOpen, anchor: menuAnchor, presentation: .inline) {
            GlassMenuItem(IssueDraftPage.discard, icon: AppIcons.uiDelete, destructive: true) {
                vm.discard()
                onClose()
            }
        }
        // Lifecycle on the always-present root (never an empty Group).
        .onAppear {
            vm.start(deps: deps, accountId: accountId)
            // Back from a route pushed over the page: editing resumes.
            vm.resume()
            titleFocused = true
        }
        .onDisappear { vm.leave() }
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
                vm.leave()
                onClose()
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
        ToolbarItem(placement: .topBarTrailing) {
            GlassMenuBarButton(
                icon: AppIcons.uiMore,
                accessibilityLabel: "More",
                anchor: $menuAnchor,
                isPresented: $menuOpen
            )
        }
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
