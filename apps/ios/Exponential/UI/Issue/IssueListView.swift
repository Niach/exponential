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
    // and the floating selection bar acts on the whole selection. The steer
    // state backing the bar's Start coding action (relay enabled + online
    // desktops) resolves lazily on entry, mirroring AgentsView.
    @State private var selectionActive = false
    @State private var selectedIds: Set<String> = []
    @State private var steerEnabled: Bool?
    @State private var steerDevices: [SteerDevice]?
    @State private var showStartSheet = false
    // Transient feedback under/instead of the bar: start sent / failed /
    // no desktop online. Auto-clears (errors included — the bar is modal
    // enough that a sticky error would just block the list).
    @State private var startNotice: StartNotice?
    /// Identifier column floor — fits "EXP-999" in .caption.monospaced at
    /// default Dynamic Type and scales with the user's text size (EXP-24).
    @ScaledMetric(relativeTo: .caption) private var identifierMinWidth: CGFloat = 60

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
        // Floating selection bar + transient start feedback, above the
        // floating tab bar's zone (EXP-239).
        .overlay(alignment: .bottom) {
            VStack(spacing: 8) {
                if let notice = startNotice {
                    noticeCapsule(notice)
                }
                if selectionActive, let vm = viewModel {
                    selectionBar(vm)
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, showsTabBarClearance ? 92 : 16)
        }
        // Haptic tick when multi-select engages (long-press confirmation).
        .sensoryFeedback(.impact(weight: .medium), trigger: selectionActive) { _, entered in entered }
        .navigationTitle(viewModel?.board?.name ?? "Issues")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
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
        .onAppear {
            if viewModel == nil {
                let vm = IssueListViewModel(
                    accountId: accountId,
                    boardId: boardId,
                    db: deps.db,
                    issuesApi: deps.issuesApi,
                    boardsApi: deps.boardsApi,
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

            // Filter bar (search lives in the Search tab, not the issue list)
            filterBar(vm)
                .padding(.horizontal, 16)
                .padding(.vertical, 8)

            if !vm.filters.isEmpty {
                activeFilterPills(vm)
                    .padding(.horizontal, 16)
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

    @ViewBuilder
    private func filterBar(_ vm: IssueListViewModel) -> some View {
        HStack(spacing: 8) {
            // Filter sheet trigger with active-count badge (Android parity).
            Button {
                showFilterSheet = true
            } label: {
                Image(systemName: "line.3.horizontal.decrease")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white.opacity(vm.filters.isEmpty ? TextOpacity.secondary : 1.0))
                    // 38pt circle, matching Android's CircleIconButton — equal
                    // width/height turns the glassButton capsule into a circle.
                    .frame(width: 38, height: 38)
            }
            .glassButton(isActive: !vm.filters.isEmpty)
            // Badge OUTSIDE glassButton's Capsule clip, so the count isn't cut
            // off. The bar's vertical padding gives the -4pt offset headroom.
            .overlay(alignment: .topTrailing) {
                if vm.filters.count > 0 {
                    Text("\(vm.filters.count)")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(.white)
                        .frame(minWidth: 15, minHeight: 15)
                        .background(Accent.indigo, in: Circle())
                        .offset(x: 4, y: -4)
                        // The badge sits above the button; it must not swallow
                        // taps meant for the filter control underneath it.
                        .allowsHitTesting(false)
                }
            }

            // Only the three tabs scroll here — they fit at rest on iPhone
            // widths. The Clear affordance lives in the active-filter-pills
            // row below as "Clear all" (web parity; appears exactly when
            // filters are active). Both earlier EXP-47 layouts clipped a chip
            // at rest: Clear pinned outside the scroller squeezed "Backlog",
            // and Clear inside the scroller pushed itself off-screen.
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(FilterTab.allCases) { tab in
                        Button {
                            vm.setTab(tab)
                        } label: {
                            Text(tab.label)
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.white.opacity(vm.activeTab == tab ? 1.0 : TextOpacity.secondary))
                                .padding(.horizontal, 14)
                                .padding(.vertical, 8)
                        }
                        .glassButton(isActive: vm.activeTab == tab)
                    }
                }
                // Trailing content margin so the last chip's capsule stroke rests
                // fully inside the scroll clip (visible when scrolled to the end)
                // instead of being shaved by the edge.
                .padding(.trailing, 8)
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
            NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                issueRowContent(issue: issue, vm: vm, selected: nil)
            }
            .buttonStyle(.plain)
            // Long-press enters multi-select (EXP-239); simultaneous so the
            // link's plain tap keeps navigating.
            .simultaneousGesture(
                LongPressGesture(minimumDuration: 0.35).onEnded { _ in
                    guard vm.permissions.isMember else { return }
                    enterSelection(with: issue.id, vm: vm)
                }
            )
            .accessibilityIdentifier("issue-row-\(issue.identifier ?? issue.id)")
        }
    }

    @ViewBuilder
    private func issueRowContent(issue: IssueEntity, vm: IssueListViewModel, selected: Bool?) -> some View {
            HStack(spacing: 10) {
                // Multi-select indicator (EXP-239) — same glyphs as the
                // Start-coding picker so "selected" reads identically.
                if let selected {
                    Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                        .font(.body)
                        .foregroundStyle(selected ? Accent.indigo : .white.opacity(TextOpacity.tertiary))
                }
                // Priority icon (16pt column, Android parity)
                Image(systemName: IssuePriority.from(issue.priority).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssuePriority.from(issue.priority).color)
                    .frame(width: 16)

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
                Image(systemName: IssueStatus.from(issue.status).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssueStatus.from(issue.status).color)
                    .frame(width: 16)

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

                // Assignee avatar (pseudonym initial when the user row isn't synced)
                if let assigneeId = issue.assigneeId {
                    userAvatar(vm.userFor(id: assigneeId), id: assigneeId, size: 22)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            // Selected rows get an unmistakable indigo wash + hairline on top
            // of the glass (the check glyph alone is easy to miss mid-scroll).
            .overlay {
                if selected == true {
                    RoundedRectangle(cornerRadius: 10)
                        .fill(Accent.indigo.opacity(0.12))
                        .overlay(
                            RoundedRectangle(cornerRadius: 10)
                                .stroke(Accent.indigo.opacity(0.45), lineWidth: 1)
                        )
                        .allowsHitTesting(false)
                }
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
        HStack(spacing: 2) {
            Button {
                exitSelection()
            } label: {
                Image(systemName: "xmark")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(width: 34, height: 34)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Clear selection")

            Text("\(selectedIds.count) selected")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)

            Spacer(minLength: 8)

            Button {
                bulkSet(vm, .done)
            } label: {
                Image(systemName: "checkmark.circle.fill")
                    .font(.body)
                    .foregroundStyle(IssueStatus.done.color)
                    .frame(width: 34, height: 34)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Mark done")

            Button {
                bulkSet(vm, .backlog)
            } label: {
                Image(systemName: "circle.dashed")
                    .font(.body)
                    // Matches the leading swipe action's backlog tint.
                    .foregroundStyle(.orange)
                    .frame(width: 34, height: 34)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Move to backlog")

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
                                .tint(.white)
                        } else {
                            Image(systemName: "play.fill")
                                .font(.caption)
                        }
                        Text("Start coding")
                            .font(.subheadline.weight(.medium))
                    }
                    .foregroundStyle(.white)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(Accent.indigo, in: Capsule())
                    .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .padding(.leading, 6)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .glassCard(cornerRadius: 24)
        .transition(.move(edge: .bottom).combined(with: .opacity))
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
            .transition(.move(edge: .bottom).combined(with: .opacity))
    }

    private func bulkSet(_ vm: IssueListViewModel, _ status: IssueStatus) {
        let ids = Array(selectedIds)
        exitSelection()
        Task { await vm.bulkSetStatus(issueIds: ids, status: status) }
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

