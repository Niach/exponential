import ExpUI
import ExpCore
import SwiftUI
import GRDB

struct IssueListView: View {
    let boardId: String
    /// False when pushed on a bar-less surface (where MainNavigator hides the
    /// floating tab bar): no clearance then.
    var showsTabBarClearance = true

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var viewModel: IssueListViewModel?
    @State private var showFilterSheet = false
    // Multi-select mode (EXP-239): long-press a row to enter, tap toggles,
    // and the selection bar (in-flow at the top of the list, EXP-251) acts on
    // the whole selection. The steer state backing the bar's Start coding
    // action (relay enabled + online desktops) resolves lazily on entry,
    // mirroring AgentsView.
    @State private var selectionActive = false
    @State private var selectedIds: Set<String> = []
    @State private var steerEnabled: Bool?
    @State private var steerDevices: [SteerDevice]?
    @State private var showStartSheet = false
    // Which bulk-property picker the selection bar is presenting (EXP-247).
    @State private var bulkSheet: BulkSheet?
    // Inline status/priority editing straight from a row's icon (EXP-247) —
    // non-selection rows only, moderator-gated.
    @State private var inlineEdit: InlineEdit?
    // Transient feedback under/instead of the bar: start sent / failed /
    // no desktop online. Auto-clears (errors included — the bar is modal
    // enough that a sticky error would just block the list).
    @State private var startNotice: StartNotice?
    /// Identifier column floor — fits "EXP-999" in .caption.monospaced at
    /// default Dynamic Type and scales with the user's text size (EXP-24).
    @ScaledMetric(relativeTo: .caption) private var identifierMinWidth: CGFloat = 60
    /// Row content height floor. Pins the row so its height never depends on
    /// WHICH optional glyphs happen to be present — the selection checkmark,
    /// the assignee avatar, an inline-edit tap target. Without it, entering
    /// selection mode changes the tallest element and every row below re-flows
    /// vertically, which reads as the list jumping (EXP-251). 22 = the avatar,
    /// the tallest thing a row has ever contained; larger Dynamic Type still
    /// grows rows, equally in both modes.
    @ScaledMetric(relativeTo: .subheadline) private var rowContentMinHeight: CGFloat = 22

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                VStack(spacing: 0) {
                    if let board = vm.board, board.repositoryId != nil {
                        HStack {
                            RepoNameChip(
                                accountId: accountId,
                                teamId: board.teamId,
                                repositoryId: board.repositoryId
                            )
                            Spacer()
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .background(Color.white.opacity(0.04))
                    }
                    issueListContent(vm)
                }
            }
        }
        // Haptic tick when multi-select engages (long-press confirmation).
        .sensoryFeedback(.impact(weight: .medium), trigger: selectionActive) { _, entered in entered }
        .navigationTitle(viewModel?.board?.name ?? "Issues")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        // Filter trigger in the nav bar (EXP-251 — replaces the removed
        // inline filter/tab bar). In Root mode SwiftUI merges this trailing
        // item next to IssuesHomeView's Settings gear; pushed boards show it
        // as the sole trailing item.
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                if let vm = viewModel {
                    filterToolbarButton(vm)
                }
            }
        }
        .sheet(isPresented: $showFilterSheet) {
            if let vm = viewModel {
                IssueFilterSheet(vm: vm)
                    .presentationDetents([.medium, .large])
                    .presentationBackground(.ultraThinMaterial)
            }
        }
        .sheet(isPresented: $showStartSheet) {
            if let vm = viewModel {
                StartCodingSheet(
                    devices: steerDevices ?? [],
                    issues: vm.startCodingCandidates(),
                    preselectedIds: selectedIds
                ) { device, issueIds, options in
                    startCoding(on: device, issueIds: issueIds, options: options)
                }
            }
        }
        .sheet(item: $bulkSheet) { sheet in
            if let vm = viewModel {
                bulkSheetContent(sheet, vm: vm)
            }
        }
        .sheet(item: $inlineEdit) { edit in
            if let vm = viewModel {
                inlineEditContent(edit, vm: vm)
            }
        }
        .onAppear {
            if viewModel == nil {
                let vm = IssueListViewModel(
                    accountId: accountId,
                    boardId: boardId,
                    db: deps.db,
                    issuesApi: deps.issuesApi,
                    boardsApi: deps.boardsApi,
                    labelsApi: deps.labelsApi,
                    auth: deps.auth
                )
                viewModel = vm
                vm.startObserving()
            }
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
    }

    @ViewBuilder
    private func issueListContent(_ vm: IssueListViewModel) -> some View {
        VStack(spacing: 0) {
            if vm.permissionsPending {
                syncingBanner
            }

            // In-flow start feedback + selection bar, pinned above the list
            // in the space the removed filter tabs used to occupy (EXP-251 —
            // sticky at the top, no longer a floating bottom overlay).
            if let notice = startNotice {
                noticeCapsule(notice)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
            }
            if selectionActive {
                selectionBar(vm)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
            }

            if !vm.filters.isEmpty {
                activeFilterPills(vm)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .padding(.bottom, 8)
            }

            if IssueStatus.displayOrder.allSatisfy({ vm.issuesForStatus($0).isEmpty }) {
                // Android parity: an empty (or fully filtered-out) board says
                // so instead of rendering a blank list.
                VStack {
                    Text("No issues yet")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.top, 64)
                    Spacer(minLength: 0)
                }
                .frame(maxWidth: .infinity)
            } else {
                List {
                    ForEach(IssueStatus.displayOrder, id: \.self) { status in
                        let statusIssues = vm.issuesForStatus(status)
                        if !statusIssues.isEmpty {
                            Section {
                                if !vm.collapsedStatuses.contains(status) {
                                    ForEach(statusIssues, id: \.id) { issue in
                                        issueRow(issue: issue, vm: vm)
                                            .listRowBackground(Color.clear)
                                            .listRowSeparator(.hidden)
                                            .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                                            .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                                                // Swipes pause while multi-select is active — the
                                                // bar owns bulk mutations then (EXP-239).
                                                if !selectionActive, vm.permissions.canMutateIssue(creatorId: issue.creatorId) {
                                                    Button {
                                                        Task { await vm.setStatus(issueId: issue.id, status: .done) }
                                                    } label: {
                                                        Label("Done", systemImage: "checkmark.circle.fill")
                                                    }
                                                    // Track done's status color (EXP-120: now blue).
                                                    .tint(IssueStatus.done.color)

                                                    Button {
                                                        Task { await vm.setStatus(issueId: issue.id, status: .cancelled) }
                                                    } label: {
                                                        Label("Cancel", systemImage: "xmark.circle.fill")
                                                    }
                                                    .tint(.gray)
                                                }
                                            }
                                            .swipeActions(edge: .leading, allowsFullSwipe: true) {
                                                if !selectionActive, vm.permissions.canMutateIssue(creatorId: issue.creatorId) {
                                                    Button {
                                                        Task { await vm.setStatus(issueId: issue.id, status: .backlog) }
                                                    } label: {
                                                        Label("Backlog", systemImage: "circle.dashed")
                                                    }
                                                    .tint(.orange)
                                                }
                                            }
                                    }
                                }
                            } header: {
                                statusHeader(status: status, count: statusIssues.count, vm: vm)
                                    .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                                    .listRowBackground(Color.clear)
                            }
                        }
                    }
                }
                .listStyle(.plain)
                // Zero the List's own default horizontal content margins so the
                // 16pt listRowInsets alone govern the gutter (Android parity) — the
                // default extra margin made rows sit noticeably inboard of the bar.
                .contentMargins(.horizontal, 0, for: .scrollContent)
                // …and the top margin: the default put ~40pt of dead space between
                // the filter chips and the first section header (Android: 8dp bar
                // padding + 3dp flow + the header's own 8dp = ~19dp total).
                .contentMargins(.top, 0, for: .scrollContent)
                // Kill List's implicit 44pt minimum row height: Android rows are
                // content-hugging (~40dp) with 3dp gaps, and the floor made every
                // iOS row visibly chunkier than its Android twin (EXP-24 redux).
                .environment(\.defaultMinListRowHeight, 0)
                // Sections flow like Android's single 3dp-spaced column — without
                // this, plain List inserts its own inter-section band.
                .listSectionSpacing(0)
                .scrollContentBackground(.hidden)
                .background(Color.clear)
                .refreshable {
                    await vm.refresh()
                }
                // Clearance for the floating tab bar (EXP-36) — the bar is an
                // ancestor overlay, so the List must reserve the space itself.
                .tabBarBottomInset(showsTabBarClearance)
            }
        }
    }

    // Shown while team membership is still syncing, so a signed-in viewer
    // sees "we're catching up" rather than silently-disabled controls.
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
        .padding(.horizontal, 16)
        .padding(.top, 8)
    }

    /// Nav-bar filter-sheet trigger with active-count badge (EXP-251 — the
    /// inline filter bar and its tab presets are gone; the trigger sits next
    /// to the Settings gear in Root mode, matching its 32pt style).
    @ViewBuilder
    private func filterToolbarButton(_ vm: IssueListViewModel) -> some View {
        Button {
            showFilterSheet = true
        } label: {
            Image(systemName: "line.3.horizontal.decrease")
                .font(.body)
                .foregroundStyle(.white.opacity(vm.filters.isEmpty ? TextOpacity.secondary : 1.0))
                .frame(width: 32, height: 32)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Filters")
        .overlay(alignment: .topTrailing) {
            if vm.filters.count > 0 {
                Text("\(vm.filters.count)")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(minWidth: 15, minHeight: 15)
                    .background(Accent.indigo, in: Circle())
                    .offset(x: 2, y: -2)
                    // The badge sits above the button; it must not swallow
                    // taps meant for the filter control underneath it.
                    .allowsHitTesting(false)
            }
        }
    }

    /// Removable pills for every active filter (web/Android parity).
    @ViewBuilder
    private func activeFilterPills(_ vm: IssueListViewModel) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(IssueStatus.displayOrder.filter { vm.filters.statuses.contains($0) }, id: \.self) { status in
                    filterPill(icon: status.sfSymbol, iconColor: status.color, text: status.label) {
                        vm.toggleStatus(status)
                    }
                }
                ForEach(IssuePriority.displayOrder.filter { vm.filters.priorities.contains($0) }, id: \.self) { priority in
                    filterPill(icon: priority.sfSymbol, iconColor: priority.color, text: priority.label) {
                        vm.togglePriority(priority)
                    }
                }
                ForEach(vm.teamLabels.filter { vm.filters.labelIds.contains($0.id) }, id: \.id) { label in
                    filterPill(dotColor: Color(hex: label.color) ?? .gray, text: label.name) {
                        vm.toggleLabel(label.id)
                    }
                }

                // "Clear all" closes the pills row, mirroring the web's
                // ActiveFilterPills — this row exists exactly when filters are
                // active, so Clear needs no spot in the (space-tight) tab row.
                Button {
                    vm.clearFilters()
                } label: {
                    Text("Clear all")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                }
                .glassButton()
            }
        }
    }

    @ViewBuilder
    private func filterPill(
        icon: String? = nil,
        iconColor: Color = .white,
        dotColor: Color? = nil,
        text: String,
        onRemove: @escaping () -> Void
    ) -> some View {
        Button(action: onRemove) {
            HStack(spacing: 5) {
                if let icon {
                    Image(systemName: icon)
                        .font(.caption2)
                        .foregroundStyle(iconColor)
                }
                if let dotColor {
                    Circle()
                        .fill(dotColor)
                        .frame(width: 7, height: 7)
                }
                Text(text)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Image(systemName: "xmark")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
        }
        .glassButton()
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func statusHeader(status: IssueStatus, count: Int, vm: IssueListViewModel) -> some View {
        Button {
            vm.toggleStatusCollapsed(status)
        } label: {
            HStack(spacing: 8) {
                Image(systemName: vm.collapsedStatuses.contains(status) ? "chevron.right" : "chevron.down")
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .frame(width: 12)

                Image(systemName: status.sfSymbol)
                    .font(.caption)
                    .foregroundStyle(status.color)

                Text(status.label)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                Text("\(count)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                Spacer()
            }
            .padding(.vertical, 6)
            .padding(.horizontal, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .textCase(nil)
    }

    @ViewBuilder
    private func issueRow(issue: IssueEntity, vm: IssueListViewModel) -> some View {
        if selectionActive {
            Button {
                toggleSelection(issue.id)
            } label: {
                issueRowContent(issue: issue, vm: vm, selected: selectedIds.contains(issue.id))
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("issue-row-\(issue.identifier ?? issue.id)")
        } else {
            // Inline status/priority editing (EXP-247): tap the icons to open a
            // picker — only when the viewer may mutate this issue. In selection
            // mode taps keep toggling selection, so these stay nil there.
            let canMutate = vm.permissions.canMutateIssue(creatorId: issue.creatorId)
            let onLongPress: () -> Void = {
                guard vm.permissions.isMember else { return }
                enterSelection(with: issue.id, vm: vm)
            }
            NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                issueRowContent(
                    issue: issue,
                    vm: vm,
                    selected: nil,
                    onTapStatus: canMutate ? { inlineEdit = InlineEdit(kind: .status, issue: $0) } : nil,
                    onTapPriority: canMutate ? { inlineEdit = InlineEdit(kind: .priority, issue: $0) } : nil,
                    onIconLongPress: onLongPress
                )
            }
            .buttonStyle(.plain)
            // Long-press enters multi-select (EXP-239); simultaneous so the
            // link's plain tap keeps navigating.
            .simultaneousGesture(
                LongPressGesture(minimumDuration: 0.35).onEnded { _ in onLongPress() }
            )
            .accessibilityIdentifier("issue-row-\(issue.identifier ?? issue.id)")
        }
    }

    @ViewBuilder
    private func issueRowContent(
        issue: IssueEntity,
        vm: IssueListViewModel,
        selected: Bool?,
        onTapStatus: ((IssueEntity) -> Void)? = nil,
        onTapPriority: ((IssueEntity) -> Void)? = nil,
        onIconLongPress: (() -> Void)? = nil
    ) -> some View {
            HStack(spacing: 10) {
                // Multi-select indicator (EXP-239) — same glyphs as the
                // Start-coding picker so "selected" reads identically.
                if let selected {
                    Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                        .font(.body)
                        .foregroundStyle(selected ? DesignTokens.Palette.primary : .white.opacity(TextOpacity.tertiary))
                }
                // Priority icon (16pt column, Android parity)
                inlineEditableIcon(
                    systemName: IssuePriority.from(issue.priority).sfSymbol,
                    color: IssuePriority.from(issue.priority).color,
                    onTap: onTapPriority.map { tap in { tap(issue) } },
                    onLongPress: onIconLongPress
                )

                // Identifier — leading-aligned min width so the status icon
                // and title don't shift horizontally with digit count for
                // typical identifiers (EXP-24), while longer prefixes / 4+
                // digit numbers grow instead of ellipsizing into ambiguity.
                Text(issue.identifier ?? "")
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .frame(minWidth: identifierMinWidth, alignment: .leading)

                // Status icon
                inlineEditableIcon(
                    systemName: IssueStatus.from(issue.status).sfSymbol,
                    color: IssueStatus.from(issue.status).color,
                    onTap: onTapStatus.map { tap in { tap(issue) } },
                    onLongPress: onIconLongPress
                )

                // Title — the ONLY flexible element (Android parity): it
                // truncates under pressure so the due date never wraps.
                Text(issue.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .layoutPriority(1)

                Spacer()

                // Labels
                HStack(spacing: 4) {
                    let issueLabels = vm.labelsFor(issueId: issue.id)
                    ForEach(issueLabels.prefix(3), id: \.id) { label in
                        Circle()
                            .fill(Color(hex: label.color) ?? .gray)
                            .frame(width: 8, height: 8)
                    }
                }

                // Due date — never wraps mid-word ("Tomor-row"); it holds its
                // intrinsic width and the title truncates instead (EXP-55).
                if let dueDate = issue.dueDate {
                    HStack(spacing: 3) {
                        Image(systemName: "calendar")
                            .font(.caption2)
                        Text(formatDueDate(dueDate))
                            .font(.caption)
                            .lineLimit(1)
                    }
                    .fixedSize(horizontal: true, vertical: false)
                    .foregroundStyle(dueDateColor(dueDate))
                }

                // Assignee avatar (pseudonym initial when the user row isn't
                // synced) — hidden on solo teams, where every issue is the
                // sole member's (EXP-247).
                if !vm.singleMemberTeam, let assigneeId = issue.assigneeId {
                    userAvatar(vm.userFor(id: assigneeId), id: assigneeId, size: 22)
                }
            }
            .frame(minHeight: rowContentMinHeight)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            // Selected rows get an unmistakable neutral wash + hairline on top
            // of the glass (the check glyph alone is easy to miss mid-scroll).
            .overlay {
                if selected == true {
                    RoundedRectangle(cornerRadius: 10)
                        .fill(DesignTokens.Palette.primary.opacity(0.12))
                        .overlay(
                            RoundedRectangle(cornerRadius: 10)
                                .stroke(DesignTokens.Palette.primary.opacity(0.45), lineWidth: 1)
                        )
                        .allowsHitTesting(false)
                }
            }
    }

    /// A row's status/priority glyph. Plain 16pt-wide by default; when `onTap`
    /// is supplied it takes 6pt of padding on every side for a ~28pt tap
    /// target, then gives all of it back with matching negative padding so the
    /// glyph occupies exactly the same layout box as a non-editable one
    /// (EXP-247). The padding must be symmetric: reclaiming only the
    /// horizontal 12pt left the box 28pt TALL, which made every row outside
    /// selection mode taller than the same row inside it (EXP-251). The row's
    /// long-press is re-attached here so a press starting over the icon still
    /// enters selection despite the tap gesture.
    @ViewBuilder
    private func inlineEditableIcon(
        systemName: String,
        color: Color,
        onTap: (() -> Void)?,
        onLongPress: (() -> Void)?
    ) -> some View {
        let glyph = Image(systemName: systemName)
            .font(.caption)
            .foregroundStyle(color)
            .frame(width: 16)
        if let onTap {
            glyph
                .padding(6)
                .contentShape(Rectangle())
                .onTapGesture { onTap() }
                .simultaneousGesture(
                    LongPressGesture(minimumDuration: 0.35).onEnded { _ in onLongPress?() }
                )
                .padding(-6)
        } else {
            glyph
        }
    }

    // MARK: - Multi-select (EXP-239)

    private func enterSelection(with issueId: String, vm: IssueListViewModel) {
        guard !selectionActive else { return }
        withAnimation(.snappy(duration: 0.2)) {
            selectionActive = true
            selectedIds = [issueId]
        }
        // Resolve relay + device presence while the user is still picking, so
        // the bar's Start coding is ready by the time they tap it (repo-backed
        // boards only — the button is absent otherwise).
        if vm.board?.repositoryId != nil, steerDevices == nil {
            Task { await loadSteer() }
        }
    }

    private func toggleSelection(_ issueId: String) {
        if selectedIds.contains(issueId) {
            selectedIds.remove(issueId)
        } else {
            selectedIds.insert(issueId)
        }
        // Deselecting the last row leaves selection mode — same as the web
        // bulk bar disappearing at zero.
        if selectedIds.isEmpty {
            exitSelection()
        }
    }

    private func exitSelection() {
        withAnimation(.snappy(duration: 0.2)) {
            selectionActive = false
            selectedIds = []
        }
    }

    private func loadSteer() async {
        let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
        steerEnabled = config.enabled
        guard config.enabled else {
            steerDevices = []
            return
        }
        steerDevices = (try? await deps.steerApi.myDevices(accountId: accountId)) ?? []
    }

    @ViewBuilder
    private func selectionBar(_ vm: IssueListViewModel) -> some View {
        // Bare count (not "N selected") so ✕ + four property buttons + the
        // Start-coding capsule never clip at ~375pt (EXP-247).
        HStack(spacing: 0) {
            Button {
                exitSelection()
            } label: {
                Image(systemName: "xmark")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Clear selection")

            Text("\(selectedIds.count)")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
                .lineLimit(1)
                .padding(.trailing, 2)

            Spacer(minLength: 4)

            // Status — the shared status glyph when the selection agrees,
            // else a neutral checklist mark.
            barIconButton(
                systemName: sharedStatus(vm)?.sfSymbol ?? "checklist",
                color: sharedStatus(vm)?.color ?? .white.opacity(TextOpacity.secondary),
                accessibility: "Set status"
            ) {
                bulkSheet = .status
            }

            // Priority — shared priority glyph, else the neutral "no priority"
            // glyph.
            barIconButton(
                systemName: sharedPriority(vm)?.sfSymbol ?? IssuePriority.none.sfSymbol,
                color: sharedPriority(vm)?.color ?? .white.opacity(TextOpacity.secondary),
                accessibility: "Set priority"
            ) {
                bulkSheet = .priority
            }

            // Assignee — only meaningful on multi-member teams.
            if !vm.singleMemberTeam {
                barIconButton(
                    systemName: "person.circle",
                    color: .white.opacity(TextOpacity.secondary),
                    accessibility: "Set assignee"
                ) {
                    bulkSheet = .assignee
                }
            }

            // Labels — tri-state toggle sheet that stays open.
            barIconButton(
                systemName: "tag",
                color: .white.opacity(TextOpacity.secondary),
                accessibility: "Edit labels"
            ) {
                bulkSheet = .labels
            }

            // Start coding — the bar's raison d'être (EXP-239). Only on
            // repo-backed boards, and only while the relay isn't known-off.
            if vm.board?.repositoryId != nil, steerEnabled != false {
                Button {
                    startCodingTapped()
                } label: {
                    HStack(spacing: 6) {
                        if steerDevices == nil {
                            ProgressView()
                                .controlSize(.small)
                                .tint(DesignTokens.Palette.primaryForeground)
                        } else {
                            Image(systemName: "play.fill")
                                .font(.caption)
                        }
                        Text("Start coding")
                            .font(.subheadline.weight(.medium))
                    }
                    .foregroundStyle(DesignTokens.Palette.primaryForeground)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(DesignTokens.Palette.primary, in: Capsule())
                    .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .padding(.leading, 4)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .glassCard(cornerRadius: 24)
        .transition(.move(edge: .top).combined(with: .opacity))
    }

    /// One 32pt property button in the selection bar (EXP-247).
    @ViewBuilder
    private func barIconButton(
        systemName: String,
        color: Color,
        accessibility: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.body)
                .foregroundStyle(color)
                .frame(width: 32, height: 32)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibility)
    }

    // MARK: - Selection-bar bulk pickers (EXP-247)

    private func selectedIssues(_ vm: IssueListViewModel) -> [IssueEntity] {
        vm.issues.filter { selectedIds.contains($0.id) }
    }

    /// The status shared by every selected issue, or nil when they differ.
    private func sharedStatus(_ vm: IssueListViewModel) -> IssueStatus? {
        let statuses = Set(selectedIssues(vm).map { IssueStatus.from($0.status) })
        return statuses.count == 1 ? statuses.first : nil
    }

    /// The priority shared by every selected issue, or nil when they differ.
    private func sharedPriority(_ vm: IssueListViewModel) -> IssuePriority? {
        let priorities = Set(selectedIssues(vm).map { IssuePriority.from($0.priority) })
        return priorities.count == 1 ? priorities.first : nil
    }

    /// The assignee shared by every selected issue, or nil when they differ /
    /// are unassigned — the picker's pre-selection.
    private func sharedAssigneeId(_ vm: IssueListViewModel) -> String? {
        let assignees = Set(selectedIssues(vm).map { $0.assigneeId })
        return assignees.count == 1 ? (assignees.first ?? nil) : nil
    }

    @ViewBuilder
    private func bulkSheetContent(_ sheet: BulkSheet, vm: IssueListViewModel) -> some View {
        switch sheet {
        case .status:
            // No duplicate: bulk marking has no canonical-issue picker (web parity).
            GlassPickerSheet(
                title: "Status",
                items: IssueStatus.allCases.filter { $0 != .duplicate },
                selectedID: sharedStatus(vm)?.id,
                idFor: { $0.id },
                onSelect: { selected in bulkSetStatus(vm, selected) }
            ) { status in
                Label {
                    Text(status.label)
                } icon: {
                    Image(systemName: status.sfSymbol)
                        .foregroundStyle(status.color)
                }
            }
        case .priority:
            GlassPickerSheet(
                title: "Priority",
                items: IssuePriority.allCases,
                selectedID: sharedPriority(vm)?.id,
                idFor: { $0.id },
                onSelect: { selected in bulkSetPriority(vm, selected) }
            ) { priority in
                Label {
                    Text(priority.label)
                } icon: {
                    Image(systemName: priority.sfSymbol)
                        .foregroundStyle(priority.color)
                }
            }
        case .assignee:
            AssigneeSheet(
                users: vm.users,
                selectedId: sharedAssigneeId(vm),
                onSelect: { userId in bulkSetAssignee(vm, userId) }
            )
        case .labels:
            BulkLabelsSheet(
                labels: vm.teamLabels.sorted {
                    $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
                },
                stateFor: { labelId in labelToggleState(vm, labelId: labelId) },
                onToggle: { labelId, add in
                    Task { await vm.bulkToggleLabel(issueIds: Array(selectedIds), labelId: labelId, add: add) }
                }
            )
        }
    }

    /// Tri-state assignment of one label across the current selection.
    private func labelToggleState(_ vm: IssueListViewModel, labelId: String) -> LabelToggleState {
        let ids = selectedIds
        guard !ids.isEmpty else { return .none }
        let assigned = ids.filter { issueId in
            vm.issueLabels.contains { $0.issueId == issueId && $0.labelId == labelId }
        }.count
        if assigned == 0 { return .none }
        if assigned == ids.count { return .all }
        return .some
    }

    @ViewBuilder
    private func inlineEditContent(_ edit: InlineEdit, vm: IssueListViewModel) -> some View {
        switch edit.kind {
        case .status:
            GlassPickerSheet(
                title: "Status",
                items: IssueStatus.allCases.filter { $0 != .duplicate },
                selectedID: IssueStatus.from(edit.issue.status).id,
                idFor: { $0.id },
                onSelect: { selected in
                    Task { await vm.setStatus(issueId: edit.issue.id, status: selected) }
                }
            ) { status in
                Label {
                    Text(status.label)
                } icon: {
                    Image(systemName: status.sfSymbol)
                        .foregroundStyle(status.color)
                }
            }
        case .priority:
            GlassPickerSheet(
                title: "Priority",
                items: IssuePriority.allCases,
                selectedID: IssuePriority.from(edit.issue.priority).id,
                idFor: { $0.id },
                onSelect: { selected in
                    Task { await vm.setPriority(issueId: edit.issue.id, priority: selected) }
                }
            ) { priority in
                Label {
                    Text(priority.label)
                } icon: {
                    Image(systemName: priority.sfSymbol)
                        .foregroundStyle(priority.color)
                }
            }
        }
    }

    private func bulkSetStatus(_ vm: IssueListViewModel, _ status: IssueStatus) {
        let ids = Array(selectedIds)
        exitSelection()
        Task { await vm.bulkSetStatus(issueIds: ids, status: status) }
    }

    private func bulkSetPriority(_ vm: IssueListViewModel, _ priority: IssuePriority) {
        let ids = Array(selectedIds)
        exitSelection()
        Task { await vm.bulkSetPriority(issueIds: ids, priority: priority) }
    }

    private func bulkSetAssignee(_ vm: IssueListViewModel, _ assigneeId: String?) {
        let ids = Array(selectedIds)
        exitSelection()
        Task { await vm.bulkSetAssignee(issueIds: ids, assigneeId: assigneeId) }
    }

    @ViewBuilder
    private func noticeCapsule(_ notice: StartNotice) -> some View {
        Text(notice.message)
            .font(.caption)
            .foregroundStyle(notice.isError ? DesignTokens.Semantic.red : .white.opacity(TextOpacity.secondary))
            .multilineTextAlignment(.center)
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            .glassCard(cornerRadius: 14)
            .transition(.move(edge: .top).combined(with: .opacity))
    }

    private func startCodingTapped() {
        guard let devices = steerDevices else { return } // presence still resolving
        guard steerEnabled == true, !devices.isEmpty else {
            showNotice(
                steerEnabled == true
                    ? "No desktop online — open the Exponential desktop app to run here."
                    : "Remote start isn't available on this server.",
                isError: true
            )
            return
        }
        showStartSheet = true
    }

    /// Mirror of AgentsView.start — single vs batch overloads of
    /// steer.startSession, with the outcome surfaced as a transient notice.
    private func startCoding(on device: SteerDevice, issueIds: [String], options: SteerStartOptions) {
        guard !issueIds.isEmpty else { return }
        let isBatch = issueIds.count > 1
        let label = device.deviceLabel.isEmpty ? "your desktop" : device.deviceLabel
        exitSelection()
        Task {
            do {
                if isBatch {
                    try await deps.steerApi.startSession(
                        accountId: accountId,
                        issueIds: issueIds,
                        deviceId: device.deviceId,
                        options: options
                    )
                } else {
                    try await deps.steerApi.startSession(
                        accountId: accountId,
                        issueId: issueIds[0],
                        deviceId: device.deviceId,
                        options: options
                    )
                }
                showNotice(
                    isBatch
                        ? "Batch start sent to \(label) — watch it in the Agents tab."
                        : "Start sent to \(label) — watch it in the Agents tab.",
                    isError: false
                )
            } catch {
                showNotice(error.localizedDescription, isError: true)
            }
        }
    }

    private func showNotice(_ message: String, isError: Bool) {
        withAnimation(.snappy(duration: 0.2)) {
            startNotice = StartNotice(message: message, isError: isError)
        }
        Task {
            try? await Task.sleep(for: .seconds(6))
            withAnimation(.snappy(duration: 0.2)) {
                if startNotice?.message == message {
                    startNotice = nil
                }
            }
        }
    }

    @ViewBuilder
    private func userAvatar(_ user: UserEntity?, id: String, size: CGFloat) -> some View {
        let initial = memberDisplayName(user, id: id).prefix(1).uppercased()
        Text(initial)
            .font(.system(size: size * 0.45, weight: .medium))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Color.white.opacity(0.15))
            .clipShape(Circle())
    }

    private func formatDueDate(_ dateString: String) -> String {
        guard let date = AppDateFormatters.yyyyMMdd.date(from: dateString) else { return dateString }
        let calendar = Calendar.current
        if calendar.isDateInToday(date) { return "Today" }
        if calendar.isDateInTomorrow(date) { return "Tomorrow" }
        return AppDateFormatters.MMMd.string(from: date)
    }

    private func dueDateColor(_ dateString: String) -> Color {
        guard let date = AppDateFormatters.yyyyMMdd.date(from: dateString) else { return .white.opacity(TextOpacity.tertiary) }
        // Due-today must win over overdue: the date parses to local midnight, which is already past.
        if Calendar.current.isDateInToday(date) { return DesignTokens.Semantic.orange }
        if date < Date() { return DesignTokens.Semantic.red }
        return .white.opacity(TextOpacity.tertiary)
    }
}

/// Transient outcome of a selection-bar action (EXP-239).
private struct StartNotice: Equatable {
    let message: String
    let isError: Bool
}

/// Which bulk-property picker the selection bar is presenting (EXP-247).
private enum BulkSheet: String, Identifiable {
    case status
    case priority
    case assignee
    case labels

    var id: String { rawValue }
}

/// An inline status/priority edit opened from a single row's icon (EXP-247).
private struct InlineEdit: Identifiable {
    enum Kind: String { case status, priority }
    let kind: Kind
    let issue: IssueEntity

    var id: String { "\(kind.rawValue)-\(issue.id)" }
}

/// Assignment of one label across a multi-issue selection (EXP-247).
private enum LabelToggleState {
    case all
    case some
    case none
}

/// Tri-state bulk label sheet (EXP-247): each row shows a full checkmark when
/// ALL selected issues carry the label, a `minus` when only SOME do, and
/// nothing otherwise. Tapping removes the label from all when every issue has
/// it, else adds it to the ones missing it. The sheet STAYS open across
/// toggles (dismiss by swipe) — chrome/row styling mirrors the detail
/// `LabelsSheet`.
private struct BulkLabelsSheet: View {
    let labels: [LabelEntity]
    let stateFor: (String) -> LabelToggleState
    let onToggle: (String, Bool) -> Void

    @State private var searchText = ""

    private var trimmedQuery: String {
        searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var filtered: [LabelEntity] {
        guard !trimmedQuery.isEmpty else { return labels }
        return labels.filter { $0.name.localizedCaseInsensitiveContains(trimmedQuery) }
    }

    var body: some View {
        GlassSheetChrome(title: "Labels", detents: [.medium, .large]) {
            GlassSheetSearchField(placeholder: "Search labels", text: $searchText)
            ScrollView {
                VStack(spacing: 2) {
                    ForEach(filtered, id: \.id) { label in
                        let state = stateFor(label.id)
                        Button {
                            onToggle(label.id, state != .all)
                        } label: {
                            HStack(spacing: 10) {
                                Circle()
                                    .fill(Color(hex: label.color) ?? .gray)
                                    .frame(width: 10, height: 10)
                                    .frame(width: 24)
                                Text(label.name)
                                    .font(.subheadline)
                                    .foregroundStyle(.white)
                                    .lineLimit(1)
                                Spacer(minLength: 0)
                                switch state {
                                case .all:
                                    Image(systemName: "checkmark")
                                        .font(.subheadline.weight(.semibold))
                                        .foregroundStyle(Accent.indigo)
                                case .some:
                                    Image(systemName: "minus")
                                        .font(.subheadline.weight(.semibold))
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                case .none:
                                    EmptyView()
                                }
                            }
                            .padding(.horizontal, 14)
                            .frame(minHeight: 44)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }

                    if filtered.isEmpty {
                        Text("No labels yet.")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(.top, 16)
                    }
                }
                .padding(.horizontal, 6)
                .padding(.bottom, 16)
            }
        }
    }
}

