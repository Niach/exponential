import ExpUI
import ExpCore
import PhotosUI
import SwiftUI
import GRDB
import UniformTypeIdentifiers

/// #EXP-1381 (fixtures/settings-nav.json): the Team page's phone rows, each
/// opening its OWN page (the row's label as the title, the native back) that
/// shows and observes only that category. The page of every section stacked
/// (and the scroll-to inside it) is gone.
enum TeamSettingsSection: String, Hashable, CaseIterable {
    case general, members, labels, boards, agents, repositories, prompts, mcp, skills, devices

    init?(_ id: SettingsNav.Id) { self.init(rawValue: id.rawValue) }

    /// The generated row (label = the page's title, icon concept, count).
    var row: SettingsNav.Row { SettingsNav.rows.first { $0.id == rawValue }! }
}

/// The page a `teamSettingsSection` route opens.
struct TeamSettingsPage: View {
    let teamId: String
    let section: TeamSettingsSection

    var body: some View {
        switch section {
        case .general: TeamGeneralPage(teamId: teamId)
        case .members: TeamMembersPage(teamId: teamId)
        case .labels: TeamLabelsPage(teamId: teamId)
        case .boards: TeamBoardsPage(teamId: teamId)
        case .agents: TeamAgentsPage(teamId: teamId)
        case .repositories: TeamRepositoriesPage(teamId: teamId)
        case .prompts: TeamPromptsPage(teamId: teamId)
        // #VAPP-185: Settings › MCP (settings-mcp.json).
        case .mcp: McpSettingsPage(teamId: teamId)
        // #EXP-1661: Settings › Skills (skills.json `list`), read-only.
        case .skills: SkillsSettingsPage(teamId: teamId)
        // The devices page moved here from the phone tab bar (chats.json
        // `shell`): the same screen, titled "Devices" by itself.
        case .devices: AgentsView()
        }
    }
}

// MARK: - Shared

/// One settings page's chrome: the app background, the scrolling column, the
/// row's label as the inline title.
struct TeamSettingsPageChrome<Content: View>: View {
    let section: TeamSettingsSection
    @ViewBuilder let content: Content

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    content
                }
                .padding(16)
            }
        }
        .navigationTitle(section.row.label)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .accessibilityIdentifier(SettingsNav.pageTestId)
    }
}

enum TeamPageQueries {
    /// The viewer OWNS the team (settings-nav.json `owner`).
    static func isOwner(_ db: Database, teamId: String, userId: String?) throws -> Bool {
        guard let userId else { return false }
        return try TeamMemberEntity
            .filter(Column("team_id") == teamId && Column("user_id") == userId)
            .filter(Column("role") == DomainContract.teamRoleOwner)
            .fetchCount(db) > 0
    }
}

/// Runs ONE tracked fetch (a page's whole data) and hands every distinct
/// value to `apply` until the calling `.task` is cancelled (the page left).
@MainActor
private func observePage<T: Equatable & Sendable>(
    _ deps: AppDependencies,
    accountId: String,
    _ fetch: @escaping @Sendable (Database) throws -> T,
    apply: (T) -> Void
) async {
    guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
    do {
        for try await value in ValueObservation.tracking(fetch).removeDuplicates().values(in: pool) {
            apply(value)
        }
    } catch {}
}

// MARK: - General

/// #EXP-1645 (team-image.json): the General row (web `TeamImageRow`): the
/// label, the mark, the Upload pill opening the composers' Photo | File choice
/// (a phone's library and its Files app are two pickers), and, only with an
/// image, the icon-only Remove.
struct TeamImageRow: View {
    let iconUrl: String?
    var busy = false
    var enabled = true
    var onPick: (PendingCommentAttachment) -> Void = { _ in }
    var onPickFailed: (String?) -> Void = { _ in }
    var onRemove: () -> Void = {}

    @State private var choiceOpen = false
    @State private var choiceAnchor: CGRect = .zero
    @State private var photoPicker = false
    @State private var photoItem: PhotosPickerItem?
    @State private var fileImporter = false

    private var canAct: Bool { enabled && !busy }

    private var hasImage: Bool {
        if case .image = TeamImage.mark(iconUrl: iconUrl) { return true }
        return false
    }

    var body: some View {
        HStack(spacing: 12) {
            Text(TeamImage.Copy.row)
                .foregroundStyle(.white.opacity(TextOpacity.primary))
                .lineLimit(1)
            Spacer(minLength: 8)
            TeamAvatar(iconUrl: iconUrl, size: CGFloat(TeamImage.Row.markSize))
                .foregroundStyle(.white)
            uploadPill
            if hasImage {
                Button(action: onRemove) {
                    AppIcon(AppIcons.uiDelete, size: 16)
                        .foregroundStyle(.white.opacity(canAct ? TextOpacity.secondary : TextOpacity.quaternary))
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(!canAct)
                .accessibilityLabel(TeamImage.Copy.remove)
                .accessibilityIdentifier(TeamImage.TestIds.remove)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(TeamImage.TestIds.row)
        .background {
            Color.clear
                .photosPicker(isPresented: $photoPicker, selection: $photoItem, matching: .images)
        }
        .background {
            Color.clear
                .fileImporter(
                    isPresented: $fileImporter,
                    allowedContentTypes: TeamImage.Upload.accept.compactMap { UTType(mimeType: $0) }
                ) { result in
                    guard case let .success(url) = result else { return }
                    Task { deliver(await AttachmentPicks.readPickedFile(at: url)) }
                }
        }
        .onChange(of: photoItem) { _, item in
            guard let item else { return }
            Task {
                deliver(await AttachmentPicks.ingestPhotoItem(item))
                photoItem = nil
            }
        }
    }

    private var uploadPill: some View {
        GlassPill(
            TeamImage.Copy.upload,
            mode: .action {
                var transaction = Transaction()
                transaction.disablesAnimations = true
                withTransaction(transaction) { choiceOpen = true }
            },
            enabled: canAct
        ) {
            if busy {
                ProgressView()
                    .controlSize(.mini)
                    .tint(.white)
            }
        }
        .fixedSize()
        .accessibilityIdentifier(TeamImage.TestIds.upload)
        .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .global) }) { frame in
            choiceAnchor = frame
        }
        .steerAttachChoiceMenu(
            isPresented: $choiceOpen,
            anchor: choiceAnchor,
            onPhoto: { photoPicker = true },
            onFile: { fileImporter = true }
        )
    }

    private func deliver(_ outcome: AttachmentPickOutcome) {
        if let attachment = outcome.attachment {
            onPick(attachment)
        } else {
            onPickFailed(outcome.failure)
        }
    }
}

/// General ×4 (web `general-section.tsx`): the General band over the Name
/// row (label left, value right, saves on submit and on leaving the field),
/// the Team image row (#EXP-1645), the time zone, the Merge mode (#EXP-1710), the Team prompt (raw monospace field, byte counter, autosaves),
/// then the Danger zone (delete team, owner only; EXP-364: an owner may
/// delete ANY team).
private struct TeamGeneralPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var team: TeamEntity?
        var isOwner = false
    }

    private enum Field: Hashable { case name, prompt }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var data = Data()
    @State private var showDeleteTeam = false
    @State private var deletingTeam = false
    @FocusState private var focused: Field?
    @State private var name = ""
    @State private var savingName = false
    /// #EXP-1710: the Merge mode pick while its write is in flight.
    @State private var mergeModePending: String?
    /// VAPP-201: the zone pick while its write is in flight (`.some(nil)` =
    /// clearing it).
    @State private var zonePending: String??
    /// #EXP-1645: the team image's upload or removal is in flight.
    @State private var imageBusy = false
    /// nil = still loading (the field stays a skeleton, so a blur can never
    /// save an empty draft over a prompt that has not arrived).
    @State private var prompt: String?
    @State private var savedPrompt = ""
    @State private var promptLoadFailed = false
    @State private var promptSaving = false
    @State private var promptJustSaved = false
    @State private var promptSaveTask: Task<Void, Never>?

    private var promptOver: Bool { TeamPrompt.bytes(prompt ?? "") > TeamPrompt.maxBytes }
    private var promptDirty: Bool {
        guard let prompt else { return false }
        return prompt.trimmingTrailingWhitespace != savedPrompt
    }

    var body: some View {
        TeamSettingsPageChrome(section: .general) {
            if let team = data.team {
                VStack(alignment: .leading, spacing: 4) {
                    GlassSectionBand("General")
                    VStack(spacing: 0) {
                        nameRow(team)
                        Rectangle()
                            .fill(GlassTokens.strokeCard)
                            .frame(height: GlassTokens.hairline)
                        TeamImageRow(
                            iconUrl: team.iconUrl,
                            busy: imageBusy,
                            enabled: data.isOwner,
                            onPick: { uploadImage($0) },
                            onPickFailed: { failedImage(verb: "upload", description: $0) },
                            onRemove: { removeImage() }
                        )
                        Rectangle()
                            .fill(GlassTokens.strokeCard)
                            .frame(height: GlassTokens.hairline)
                        zoneRow(team)
                    }
                    .glassSection()
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("team-settings-identity")

                // #EXP-1710 (merge-train.json MODE): the yolo switch became
                // ONE Merge select row (Automatic | After approval), owners only.
                mergeModeRow(team)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                    .glassSection()
                    .accessibilityIdentifier(MergeTrain.TestIds.settingsRow)

                promptSection

                if data.isOwner {
                    dangerZone
                }
            }
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            let userId = deps.auth.userId
            await observePage(deps, accountId: accountId, { db in
                Data(
                    team: try TeamEntity.fetchOne(db, key: teamId),
                    isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId)
                )
            }, apply: { next in
                if focused != .name, !savingName, next.team?.name != data.team?.name {
                    name = next.team?.name ?? ""
                }
                if let pending = mergeModePending, next.team?.mergeMode == pending { mergeModePending = nil }
                if let pending = zonePending, let team = next.team, team.timeZone == pending { zonePending = nil }
                data = next
            })
        }
        .task(id: "prompt/\(accountId)/\(teamId)") { await loadPrompt(retried: false) }
        .onChange(of: focused) { old, new in
            if old == .name, new != .name { saveName() }
            if old == .prompt, new != .prompt { savePrompt() }
        }
        // Leaving the page inside the debounce window never drops the edit.
        .onDisappear { savePrompt() }
        // EXP-1215: the app's own alert card (`GlassAlert`) on a zero-size
        // node of its own (EXP-240).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showDeleteTeam) {
                    GlassAlert(
                        prompt: Prompts.DeleteTeam.copy(name: data.team?.name ?? "this team"),
                        enabled: ["delete": !deletingTeam],
                        handlers: ["delete": { Task { await deleteTeam() } }]
                    )
                }
        }
    }

    /// EXP-818: the Linear settings row, saving itself (no Save button).
    private func nameRow(_ team: TeamEntity) -> some View {
        HStack(spacing: 8) {
            Text("Name")
                .foregroundStyle(.white.opacity(TextOpacity.primary))
            Spacer(minLength: 8)
            GlassTextField("Name", text: Binding(
                get: { name },
                set: { name = String($0.prefix(255)) }
            ), bordered: false, accessibilityIdentifier: "team-name") {
                EmptyView()
            } trailing: {
                if savingName { InlineStatus(FeedbackStates.Inline.saving) }
            }
            .multilineTextAlignment(.trailing)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .submitLabel(.done)
            .focused($focused, equals: .name)
            .onSubmit { saveName() }
            .disabled(!data.isOwner)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .onAppear { if name.isEmpty { name = team.name } }
    }

    /// EXP-1025: a RAW monospace field (a model reads it, so the bytes shown
    /// are the bytes it gets); the counter is the footer.
    private var promptSection: some View {
        VStack(alignment: .leading, spacing: 4) {
            GlassSectionBand(TeamPrompt.title) {
                if promptSaving {
                    InlineStatus(FeedbackStates.Inline.saving)
                } else if promptJustSaved {
                    InlineStatus(FeedbackStates.Inline.saved, busy: false)
                }
            }
            VStack(spacing: 0) {
                if let current = prompt {
                    GlassTextField(TeamPrompt.placeholder, text: Binding(
                        get: { current },
                        set: { prompt = $0; schedulePromptSave() }
                    ), lines: 10...40, bordered: false, accessibilityIdentifier: "team-agent-prompt")
                    .font(.expMono(.footnote))
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .focused($focused, equals: .prompt)
                    .frame(minHeight: 180, alignment: .topLeading)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                    .disabled(!data.isOwner)
                } else if promptLoadFailed {
                    VStack(spacing: 12) {
                        Text("Prompt unavailable")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        GlassPill(FeedbackStates.Errors.retry, icon: AppIcons.uiRefresh, mode: .action {
                            Task { await loadPrompt(retried: true) }
                        })
                    }
                    .frame(maxWidth: .infinity, minHeight: 180)
                } else {
                    SkeletonRows(rows: 2, rowHeight: 22, dot: false, dividers: false)
                        .padding(.vertical, 12)
                }
                Rectangle()
                    .fill(GlassTokens.strokeCard)
                    .frame(height: GlassTokens.hairline)
                Text(TeamPrompt.counter(prompt ?? ""))
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(promptOver ? DesignTokens.Semantic.red : .white.opacity(TextOpacity.tertiary))
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .accessibilityIdentifier("team-agent-prompt-counter")
            }
            .glassSection()
            if promptOver {
                Text(TeamPrompt.tooLong(prompt ?? ""))
                    .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .padding(.top, 4)
            }
        }
    }

    private func saveName() {
        guard let team = data.team, data.isOwner, !savingName else { return }
        let draft = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !draft.isEmpty else {
            name = team.name
            return
        }
        guard draft != team.name else { return }
        savingName = true
        Task {
            defer { savingName = false }
            do {
                try await deps.teamsApi.update(accountId: accountId, UpdateTeamInput(teamId: teamId, name: draft))
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the team name"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    /// #EXP-1645 (team-image.json `upload`): the server normalises the pick and
    /// the shape delivers the new `icon_url`.
    private func uploadImage(_ file: PendingCommentAttachment) {
        guard data.isOwner, !imageBusy else { return }
        imageBusy = true
        Task {
            defer { imageBusy = false }
            do {
                _ = try await deps.attachmentsApi.uploadTeamImage(
                    accountId: accountId,
                    teamId: teamId,
                    data: file.data,
                    filename: file.filename,
                    contentType: file.contentType
                )
            } catch {
                failedImage(verb: "upload", description: error.trpcUserMessage)
            }
        }
    }

    private func removeImage() {
        guard data.isOwner, !imageBusy else { return }
        imageBusy = true
        Task {
            defer { imageBusy = false }
            do {
                try await deps.teamsApi.update(accountId: accountId, UpdateTeamInput(teamId: teamId, iconUrl: .some(nil)))
            } catch {
                failedImage(verb: "remove", description: error.trpcUserMessage)
            }
        }
    }

    private func failedImage(verb: String, description: String?) {
        toaster.error(FeedbackStates.Errors.title(verb: verb, thing: "the team image"), description: description)
    }

    /// VAPP-201 (team-zone.json): THE schedule zone picker, `Device time
    /// zone` (= nil) first; an owner's pick saves at once.
    private func zoneRow(_ team: TeamEntity) -> some View {
        ScheduleZonePicker(
            title: TeamZone.Copy.row,
            value: zonePending ?? team.timeZone,
            nullLabel: TeamZone.Copy.device,
            disabled: !data.isOwner || zonePending != nil,
            onChange: { saveZone($0) }
        )
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .accessibilityIdentifier(TeamZone.TestIds.row)
    }

    private func saveZone(_ next: String?) {
        guard let team = data.team, data.isOwner, next != team.timeZone else { return }
        zonePending = .some(next)
        Task {
            do {
                try await deps.teamsApi.update(accountId: accountId, UpdateTeamInput(teamId: teamId, timeZone: .some(next)))
            } catch {
                zonePending = nil
                toaster.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the time zone"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    /// #EXP-1710: THE select row of the page (the zone row's look): Merge,
    /// Automatic | After approval; an owner's pick saves at once.
    private func mergeModeRow(_ team: TeamEntity) -> some View {
        GlassPickerRow(
            MergeTrain.Copy.Settings.label,
            selection: Binding(
                get: { mergeModePending ?? team.mergeMode },
                set: { saveMergeMode($0) }
            ),
            options: DomainContract.teamMergeModeValues,
            label: { mode in
                mode == DomainContract.teamMergeModeApproval
                    ? MergeTrain.Copy.Settings.approval
                    : MergeTrain.Copy.Settings.auto
            },
            enabled: data.isOwner && mergeModePending == nil
        )
    }

    /// #EXP-1710: `teams.update {mergeMode}`, owner-only.
    private func saveMergeMode(_ next: String) {
        guard let team = data.team, data.isOwner, next != team.mergeMode else { return }
        mergeModePending = next
        Task {
            do {
                try await deps.teamsApi.update(accountId: accountId, UpdateTeamInput(teamId: teamId, mergeMode: next))
            } catch {
                mergeModePending = nil
                toaster.error(
                    FeedbackStates.Errors.title(verb: "update", thing: "the merge mode"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    /// FEED-79: the load runs on open, so a failure is quiet (the Retry
    /// state); only a failed Retry tap toasts.
    private func loadPrompt(retried: Bool) async {
        promptLoadFailed = false
        do {
            let text = try await deps.teamsApi.agentPrompt(accountId: accountId, teamId: teamId)
            prompt = text
            savedPrompt = text.trimmingTrailingWhitespace
        } catch {
            guard !Task.isCancelled else { return }
            promptLoadFailed = true
            if retried {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "load", thing: "the team prompt"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    /// EXP-1275 (D9): autosave a beat after the last keystroke.
    private func schedulePromptSave() {
        promptSaveTask?.cancel()
        guard promptDirty, !promptOver else { return }
        promptSaveTask = Task {
            // feedback-states.json `autosave.debounceMs`.
            try? await Task.sleep(for: .milliseconds(800))
            guard !Task.isCancelled else { return }
            savePrompt()
        }
    }

    private func savePrompt() {
        promptSaveTask?.cancel()
        promptSaveTask = nil
        guard data.isOwner, let text = prompt, promptDirty, !promptOver else { return }
        let account = accountId
        let team = teamId
        promptSaving = true
        promptJustSaved = false
        Task {
            do {
                try await deps.teamsApi.update(accountId: account, UpdateTeamInput(teamId: team, agentPrompt: text))
                savedPrompt = text.trimmingTrailingWhitespace
                promptSaving = false
                promptJustSaved = true
                // `autosave.savedHoldMs`.
                try? await Task.sleep(for: .milliseconds(2000))
                promptJustSaved = false
            } catch {
                promptSaving = false
                toaster.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the team prompt"),
                    description: error.trpcUserMessage
                )
            }
        }
    }

    /// ×4: the Danger zone heading is a filled band with its label in the
    /// danger red; its ONE row = the muted consequence + THE destructive
    /// button trailing (EXP-1554, web's row).
    private var dangerZone: some View {
        VStack(alignment: .leading, spacing: 8) {
            GlassSectionBand("Danger zone", danger: true)

            HStack(spacing: 12) {
                Text("Permanently delete this team and all its data.")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .frame(maxWidth: .infinity, alignment: .leading)
                DestructiveButton("Delete team") { showDeleteTeam = true }
                    .accessibilityIdentifier("team-delete")
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .glassSection()
        }
    }

    private func deleteTeam() async {
        deletingTeam = true
        defer { deletingTeam = false }
        do {
            // An owner may delete ANY of their teams, including the last one
            // (EXP-188) — a team-less account lands on the zero-team state.
            try await deps.teamsApi.delete(accountId: accountId, teamId: teamId)
            // Membership changed, so every shape's where clause rotated —
            // relaunch the pipeline so all shapes re-scope immediately.
            await deps.syncManager.restartPipeline(accountId: accountId)
            await MainActor.run {
                // Pop the whole stack to root — parent views may still
                // target the deleted team.
                NotificationCenter.default.post(name: .teamDeleted, object: nil)
            }
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "delete", thing: "the team"),
                description: error.trpcUserMessage
            )
        }
    }
}

private extension String {
    /// The server's normalisation (`trimEnd`).
    var trimmingTrailingWhitespace: String {
        var copy = self
        while let last = copy.last, last.isWhitespace { copy.removeLast() }
        return copy
    }
}

// MARK: - Members

/// Members (view/manage only — inviting by email stays web-only, EXP-216).
private struct TeamMembersPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var teamName: String?
        var members: [TeamMemberEntity] = []
        var users: [UserEntity] = []
        var invites: [TeamInviteEntity] = []
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var data = Data()

    private var isOwner: Bool {
        guard let me = deps.auth.userId else { return false }
        return data.members.contains { $0.userId == me && $0.role == DomainContract.teamRoleOwner }
    }

    var body: some View {
        TeamSettingsPageChrome(section: .members) {
            TeamMembersSection(
                accountId: accountId,
                teamId: teamId,
                members: data.members,
                users: data.users,
                invites: data.invites,
                currentUserId: deps.auth.userId,
                membersApi: deps.teamMembersApi,
                isOwner: isOwner,
                teamName: data.teamName
            )
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            await observePage(deps, accountId: accountId, { db in
                // Members ×4 list in JOIN order.
                let members = try TeamMemberEntity.filter(Column("team_id") == teamId)
                    .order(Column("created_at"), Column("id")).fetchAll(db)
                let userIds = members.map(\.userId)
                return Data(
                    teamName: try TeamEntity.fetchOne(db, key: teamId)?.name,
                    members: members,
                    users: try UserEntity.filter(userIds.contains(Column("id"))).fetchAll(db),
                    invites: try TeamInviteEntity.filter(Column("team_id") == teamId).fetchAll(db)
                )
            }, apply: { data = $0 })
        }
    }
}

// MARK: - Labels

private struct TeamLabelsPage: View {
    let teamId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var labels: [LabelEntity] = []

    var body: some View {
        TeamSettingsPageChrome(section: .labels) {
            TeamLabelsSection(
                accountId: accountId,
                teamId: teamId,
                labels: labels,
                labelsApi: deps.labelsApi
            )
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            await observePage(deps, accountId: accountId, { db in
                try LabelEntity.filter(Column("team_id") == teamId).fetchAll(db)
            }, apply: { labels = $0 })
        }
    }
}

// MARK: - Boards

/// Boards = the team's boards (a row opens the board's settings) + New board.
private struct TeamBoardsPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var boards: [BoardEntity] = []
        var isOwner = false
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    // FEED-79: a failed board delete is a toast, never a rose line.
    @Environment(\.toaster) private var toaster
    @State private var data = Data()
    @State private var deleteBoardTarget: BoardEntity?
    @State private var deletingBoard = false

    var body: some View {
        TeamSettingsPageChrome(section: .boards) {
            TeamBoardsSection(
                boards: data.boards,
                accountId: accountId,
                teamId: teamId,
                isOwner: data.isOwner,
                boardsApi: deps.boardsApi,
                repositoriesApi: deps.repositoriesApi,
                onDelete: { board in deleteBoardTarget = board }
            )
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            let userId = deps.auth.userId
            await observePage(deps, accountId: accountId, { db in
                Data(
                    boards: try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db),
                    isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId)
                )
            }, apply: { data = $0 })
        }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $deleteBoardTarget) { board in
                    GlassAlert(
                        prompt: Prompts.TrashBoard.copy(name: board.name),
                        enabled: ["trash": !deletingBoard],
                        handlers: ["trash": { Task { await deleteBoard(board) } }]
                    )
                }
        }
    }

    private func deleteBoard(_ board: BoardEntity) async {
        deletingBoard = true
        defer { deletingBoard = false; deleteBoardTarget = nil }
        do {
            try await deps.teamsApi.deleteBoard(accountId: accountId, boardId: board.id)
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "delete", thing: "the board"),
                description: error.trpcUserMessage
            )
        }
    }
}

// MARK: - Agents

/// Agents = the viewer's Assistant, the team agents (a row opens the
/// agent's settings; read-only for members) + New agent (settings-nav.json
/// `nav` Agents group), then the team's Events band (NEW-2).
private struct TeamAgentsPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var agents: [AgentDefinitionEntity] = []
        /// The viewer's own Assistant; nil = not created yet.
        var assistantId: String?
        var isOwner = false
        var events: [EventEntity] = []
        var sessionTitles: [String: String] = [:]
        var users: [UserEntity] = []
        /// #EXP-1735: the Lead Assistant row's options (the team's members
        /// and their display names).
        var members: [AgentSettings.LeadMember] = []
        /// The synced team row's `updated_at`: `agents.setLeadAssistant`
        /// bumps it, so the lead is re-read whenever it moves (iOS does not
        /// store `lead_assistant_id`).
        var teamUpdatedAt: String?
        /// #EXP-1788: false until the first local read lands (the bands
        /// draw skeleton rows, feedback-states.json `placeholder`).
        var loaded = false
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var data = Data()
    @State private var agentTarget: AgentPageTarget?
    @State private var showAssistant = false
    /// #EXP-1735: the lead's member as an option id (`leadNone` = none),
    /// from `agents.lead`; nil = not read yet (#EXP-1788: the row stays
    /// disabled, like the IDE's agents_index).
    @State private var leadUserId: String?
    /// #EXP-1735: the Lead Assistant pick while its write is in flight.
    @State private var leadPending: String?

    var body: some View {
        TeamSettingsPageChrome(section: .agents) {
            // #EXP-1735 (agent-settings.json `lead`): ONE settings Picker
            // row where the Coordinator row was, owners pick, members read.
            leadRow
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .glassSection()
                .accessibilityIdentifier(AgentSettings.leadRowTestId)
            TeamAgentsSection(
                agents: data.agents,
                isOwner: data.isOwner,
                loading: !data.loaded,
                onOpen: { agentTarget = AgentPageTarget(agentId: $0) },
                onOpenAssistant: { showAssistant = true }
            )
            TeamEventsSection(events: data.events, sessionTitles: data.sessionTitles, users: data.users, loading: !data.loaded)
        }
        .navigationDestination(item: $agentTarget) { target in
            AgentSettingsPage(teamId: teamId, agentId: target.agentId, isOwner: data.isOwner)
                .bareBackButton()
        }
        .navigationDestination(isPresented: $showAssistant) {
            AssistantSettingsPage(teamId: teamId, agentId: data.assistantId)
                .bareBackButton()
        }
        // S-3: the Agents group mounting creates the viewer's Assistant on
        // first use (once per team per app run).
        .task(id: "ensure/\(accountId)/\(teamId)") {
            await AssistantEnsure.run(deps, accountId: accountId, teamId: teamId)
        }
        // #EXP-1735: the lead's member comes from the server (its Assistant
        // is another member's, never synced here), read on appear and again
        // whenever the synced team row changes (a lead pick bumps it).
        .task(id: "lead/\(accountId)/\(teamId)/\(data.teamUpdatedAt ?? "")") {
            await loadLead()
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            let userId = deps.auth.userId
            await observePage(deps, accountId: accountId, { db in
                let events = try EventEntity.filter(Column("team_id") == teamId)
                    .order(Column("created_at").desc, Column("id").desc)
                    .limit(TeamEventsSection.cap)
                    .fetchAll(db)
                let sessionIds = Array(Set(events.compactMap(\.sessionId)))
                let sessions = sessionIds.isEmpty
                    ? []
                    : try SessionEntity.filter(sessionIds.contains(Column("id"))).fetchAll(db)
                // #EXP-1714: the viewer's global Assistant lists in every team.
                let agents = try SessionQueries.teamAgents(db: db, teamId: teamId, me: userId)
                return Data(
                    // S-3: the team agents only (role agent); the viewer's
                    // own Assistant is the `@assistant` row.
                    agents: agents.filter { !$0.isAssistant },
                    assistantId: agents.first { $0.isAssistant && $0.ownerId == userId }?.id,
                    isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId),
                    events: events,
                    sessionTitles: Dictionary(
                        sessions.map { ($0.id, $0.title ?? "") }, uniquingKeysWith: { a, _ in a }
                    ),
                    users: try UserEntity.fetchAll(db),
                    members: try Self.leadMembers(db, teamId: teamId),
                    teamUpdatedAt: try TeamEntity.fetchOne(db, key: teamId)?.updatedAt,
                    loaded: true
                )
            }, apply: { data = $0 })
        }
    }

    /// #EXP-1735: the team's members with their display names.
    private nonisolated static func leadMembers(_ db: Database, teamId: String) throws -> [AgentSettings.LeadMember] {
        let userIds = try TeamMemberEntity.filter(Column("team_id") == teamId).fetchAll(db).map(\.userId)
        let users = try UserEntity.filter(userIds.contains(Column("id"))).fetchAll(db)
        let byId = Dictionary(users.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        return userIds.map { AgentSettings.LeadMember(userId: $0, name: memberDisplayName(byId[$0], id: $0)) }
    }

    private func loadLead() async {
        do {
            let lead = try await deps.agentsApi.lead(accountId: accountId, teamId: teamId)
            leadUserId = lead?.userId ?? AgentSettings.leadNone
        } catch {
            // A failed read keeps the last value (nil = still disabled, never
            // a false None); the next team change retries.
        }
    }

    /// #EXP-1735: THE select row (the Coordinator row's look): Lead
    /// Assistant, None or a team member; an owner's pick saves at once.
    private var leadRow: some View {
        let names = Dictionary(data.members.map { ($0.userId, $0.name) }, uniquingKeysWith: { first, _ in first })
        return GlassPickerRow(
            AgentSettings.LeadCopy.label,
            selection: Binding(
                get: { leadPending ?? leadUserId ?? AgentSettings.leadNone },
                set: { saveLead($0) }
            ),
            options: AgentSettings.leadOptions(data.members),
            label: { id in
                id == AgentSettings.leadNone ? AgentSettings.LeadCopy.none : names[id] ?? memberPseudonym(userId: id)
            },
            enabled: data.isOwner && leadPending == nil && leadUserId != nil
        )
    }

    /// #EXP-1735: `agents.setLeadAssistant {teamId, userId | null}`,
    /// owner-only; a failure toasts and keeps the previous lead.
    private func saveLead(_ next: String) {
        guard data.isOwner, leadPending == nil, let current = leadUserId, next != current else { return }
        leadPending = next
        let userId = next == AgentSettings.leadNone ? nil : next
        Task {
            do {
                try await deps.agentsApi.setLeadAssistant(accountId: accountId, teamId: teamId, userId: userId)
                leadUserId = next
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: "save", thing: "the lead Assistant"),
                    description: error.trpcUserMessage
                )
            }
            leadPending = nil
        }
    }
}

/// The agent page's navigation item: an agent id, or nil for a new one.
private struct AgentPageTarget: Hashable {
    let agentId: String?
}

// MARK: - Repositories

/// The repositories registry (server-only, read over tRPC). Member-visible
/// since EXP-557: any member connects/adds; row removal is sharer-or-owner.
private struct TeamRepositoriesPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var team: TeamEntity?
        var boards: [BoardEntity] = []
        var isOwner = false
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var data = Data()

    var body: some View {
        TeamSettingsPageChrome(section: .repositories) {
            TeamRepositoriesSection(
                accountId: accountId,
                team: data.team,
                currentUserId: deps.auth.userId,
                isOwner: data.isOwner,
                boards: data.boards,
                repositoriesApi: deps.repositoriesApi,
                integrationsApi: deps.integrationsApi,
                instanceBaseURL: deps.auth.instanceBaseURL(forAccountId: accountId)
            )
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            let userId = deps.auth.userId
            await observePage(deps, accountId: accountId, { db in
                Data(
                    team: try TeamEntity.fetchOne(db, key: teamId),
                    boards: try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db),
                    isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId)
                )
            }, apply: { data = $0 })
        }
    }
}

// MARK: - Prompts

/// NEW-7: Prompts — a row opens the prompt's page (read-only for members);
/// "New prompt" is the owner's.
private struct TeamPromptsPage: View {
    let teamId: String

    private struct Data: Equatable, Sendable {
        var prompts: [PromptEntity] = []
        var isOwner = false
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var data = Data()

    var body: some View {
        TeamSettingsPageChrome(section: .prompts) {
            TeamPromptsSection(
                prompts: data.prompts,
                isOwner: data.isOwner,
                onOpen: { id in pushRoute(.prompt(accountId: accountId, teamId: teamId, id: id)) }
            )
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            let userId = deps.auth.userId
            await observePage(deps, accountId: accountId, { db in
                Data(
                    prompts: try PromptEntity.filter(Column("team_id") == teamId)
                        .order(Column("sort_order"), Column("name"))
                        .fetchAll(db),
                    isOwner: try TeamPageQueries.isOwner(db, teamId: teamId, userId: userId)
                )
            }, apply: { data = $0 })
        }
    }
}

// MARK: - Skills

/// #EXP-1661 (skills.json `list`): Settings › Skills on a phone = ONE glass
/// group of the team's skills sorted by name (`Skills.sortSkills`), each row
/// the name + the Default pill + the description; a row opens the skill's
/// read-only page. Phones never create or edit a skill (no New button).
private struct SkillsSettingsPage: View {
    let teamId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var skills: [SkillEntity]?

    var body: some View {
        TeamSettingsPageChrome(section: .skills) {
            Group {
                if let skills {
                    if skills.isEmpty {
                        EmptyState(.skills)
                    } else {
                        VStack(spacing: 0) {
                            ForEach(skills) { skill in
                                row(skill)
                            }
                        }
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier(Skills.TestId.list)
                    }
                } else {
                    SkeletonRows(rows: 2, rowHeight: 52)
                }
            }
            .glassSection()
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(Skills.TestId.page)
        }
        .task(id: "\(accountId)/\(teamId)") {
            let teamId = teamId
            await observePage(deps, accountId: accountId, { db in
                try Skills.sortSkills(SkillEntity.filter(Column("team_id") == teamId).fetchAll(db))
            }, apply: { skills = $0 })
        }
    }

    /// The MCP server row's look (Settings › MCP): glyph, name + the Default
    /// pill (Built-in on a built-in, #VAPP-213), the description as a one-line caption.
    private func row(_ skill: SkillEntity) -> some View {
        Button {
            pushRoute(.skill(accountId: accountId, teamId: teamId, id: skill.id))
        } label: {
            HStack(spacing: 12) {
                AppIcon(AppIcons.uiDocs, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(skill.name)
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        if let pill = Skills.skillPill(builtin: skill.builtin, enabledByDefault: skill.enabledByDefault) {
                            GlassPill(pill).fixedSize()
                        }
                    }
                    if !skill.description.isEmpty {
                        Text(skill.description)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .flatRow()
        .accessibilityIdentifier(Skills.TestId.row)
    }
}
