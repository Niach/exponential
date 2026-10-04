import ExpCore
import ExpUI
import SwiftUI

// EXP-1121 — "Ready to code?": the checklist a not-ready Start coding opens.
// Three ordered steps (GitHub, a repository on the board, a device online)
// off the pure `CodingReadiness.derive`, re-derived LIVE while the sheet is
// open — the rows read the observed `CodingReadinessModel` + the issue's view
// model, so picking a repository or a machine coming online ticks the row
// and advances the bar. Android's `CodingReadinessSheet` draws the same
// geometry; every string comes from `CodingReadiness.Copy`.

/// The not-ready circle's corner mark: an 8pt amber disc ringed in the bar's
/// background so it reads as cut out of the circle's edge.
struct CodingReadinessBadgeDot: View {
    var body: some View {
        Circle()
            .fill(DesignTokens.Semantic.yellow)
            .frame(width: FloatingBarTokens.badgeSize, height: FloatingBarTokens.badgeSize)
            .padding(2)
            .background(Circle().fill(GlassTokens.backgroundBottom))
    }
}

/// A fix the HOST performs after the sheet closes (a route push).
enum CodingReadinessRoute {
    case connectGithub
    case boardSettings
    case openDevices
}

extension CodingReadinessModel {
    /// The issue's readiness — the ONE derivation the bar circle, the
    /// switcher row and the sheet share.
    func readiness(vm: IssueDetailViewModel, remoteStartEnabled: Bool?) -> CodingReadiness.Readiness {
        readiness(
            isMember: vm.permissions.isMember,
            remoteStartEnabled: remoteStartEnabled,
            teamName: vm.team?.name ?? "",
            board: vm.board
        )
    }
}

private enum ReadinessSheetTokens {
    static let rowHPadding: CGFloat = 16
    static let rowVPadding: CGFloat = 14
    static let iconSize: CGFloat = 24
    static let iconGap: CGFloat = 12
    static let stepGlyph: CGFloat = 13
    static let checkGlyph: CGFloat = 12
    static let segmentHeight: CGFloat = 3
    static let segmentGap: CGFloat = 4
    static let fixHeight: CGFloat = 36
    static let fixRadius: CGFloat = 10
    static let fixHPadding: CGFloat = 14
    static let startHeight: CGFloat = 50
    static let amber = DesignTokens.Semantic.yellow
    static let green = DesignTokens.Semantic.green
}

struct CodingReadinessSheet: View {
    let model: CodingReadinessModel
    let vm: IssueDetailViewModel
    let remoteStartEnabled: Bool?
    let accountId: String
    /// Ready + tapped: the host dismisses and starts.
    let onStart: () -> Void
    /// A route fix: the host dismisses, then pushes.
    let onRoute: (CodingReadinessRoute) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.openURL) private var openURL
    @Environment(\.scenePhase) private var scenePhase
    @State private var showPicker = false
    /// SLOP-26: "Connect GitHub" opens the Add-repository picker, the phone's
    /// guided flow — it names the missing prerequisite (not linked / expired
    /// / not installed) with its one fix, then lists the live repositories,
    /// and a pick adds the repo AND points this board at it.
    @State private var showGithub = false
    /// EXP-1169: "Set up a server" opens the ONE device setup block.
    @State private var showAddDevice = false

    private var readiness: CodingReadiness.Readiness {
        model.readiness(vm: vm, remoteStartEnabled: remoteStartEnabled)
    }

    var body: some View {
        let readiness = readiness
        GlassSheetChrome(
            title: CodingReadiness.Copy.title,
            pinnedHeader: { header(readiness) },
            content: {
                VStack(spacing: 0) {
                    ForEach(Array(readiness.steps.enumerated()), id: \.element.key) { index, step in
                        if index > 0 { GlassDivider() }
                        row(step)
                    }
                    GlassDivider()
                }
            },
            primaryAction: { startButton(ready: readiness.ready) }
        )
        .task { await model.refresh(board: vm.board) }
        // Back from Safari (the desktop download, a GitHub connect): re-probe
        // the server half; the devices tick on their own.
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { Task { await model.refresh(board: vm.board) } }
        }
        .sheet(isPresented: $showPicker) {
            if let board = vm.board {
                CodingReadinessRepoPickerSheet(model: model, board: board, accountId: accountId)
            }
        }
        // One presentation per node: the Add device and GitHub sheets hang
        // off zero-size nodes of their own.
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .sheet(isPresented: $showAddDevice) {
                    AddDeviceSheet(accountId: accountId)
                }
        )
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .sheet(isPresented: $showGithub) {
                    if let board = vm.board {
                        GithubRepoPicker(
                            accountId: accountId,
                            teamId: board.teamId,
                            integrationsApi: deps.integrationsApi
                        ) { picked in
                            _ = try await readinessAddFromGithub(
                                deps: deps, model: model, board: board, accountId: accountId, picked: picked
                            )
                        }
                    }
                }
        )
    }

    // MARK: - Header

    private func header(_ readiness: CodingReadiness.Readiness) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(readiness.summary)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: ReadinessSheetTokens.segmentGap) {
                ForEach(readiness.steps, id: \.key) { step in
                    Capsule()
                        .fill(segmentColor(step.state))
                        .frame(height: ReadinessSheetTokens.segmentHeight)
                        .frame(maxWidth: .infinity)
                }
            }
            .padding(.top, 12)
            .accessibilityHidden(true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, GlassSheetTokens.headerHPadding)
        .padding(.bottom, 14)
    }

    private func segmentColor(_ state: CodingReadiness.StepState) -> Color {
        switch state {
        case .met: ReadinessSheetTokens.green
        case .current: ReadinessSheetTokens.amber
        case .pending: .white.opacity(0.10)
        }
    }

    // MARK: - Rows

    @ViewBuilder
    private func row(_ step: CodingReadiness.Step) -> some View {
        Group {
            switch step.state {
            case .met: metRow(step)
            case .current, .pending: openRow(step)
            }
        }
        .padding(.horizontal, ReadinessSheetTokens.rowHPadding)
        .padding(.vertical, ReadinessSheetTokens.rowVPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(step.state == .current ? ReadinessSheetTokens.amber.opacity(0.08) : .clear)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("readiness-step-\(step.key.rawValue)")
    }

    private func metRow(_ step: CodingReadiness.Step) -> some View {
        HStack(spacing: ReadinessSheetTokens.iconGap) {
            Circle()
                .fill(ReadinessSheetTokens.green.opacity(0.18))
                .frame(width: ReadinessSheetTokens.iconSize, height: ReadinessSheetTokens.iconSize)
                .overlay {
                    AppIcon(AppIcons.uiCheck, size: ReadinessSheetTokens.checkGlyph, weight: .semibold)
                        .foregroundStyle(ReadinessSheetTokens.green)
                }
            Text(step.title)
                .font(.callout)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
            Spacer(minLength: 8)
            if let detail = step.detail {
                Text(detail)
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .multilineTextAlignment(.trailing)
            }
        }
    }

    private func openRow(_ step: CodingReadiness.Step) -> some View {
        let current = step.state == .current
        return HStack(alignment: .top, spacing: ReadinessSheetTokens.iconGap) {
            stepIcon(step)
            VStack(alignment: .leading, spacing: 4) {
                Text(step.title)
                    .font(.callout.weight(current ? .medium : .regular))
                    .foregroundStyle(.white)
                    .fixedSize(horizontal: false, vertical: true)
                if let body = step.body {
                    Text(body)
                        .font(.system(size: 14))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .fixedSize(horizontal: false, vertical: true)
                }
                let fixes = visibleFixes(step)
                if current, !fixes.isEmpty {
                    FlowLayout(spacing: 8) {
                        ForEach(Array(fixes.enumerated()), id: \.element) { index, fix in
                            fixButton(fix, primary: index == 0)
                        }
                    }
                    .padding(.top, 6)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func stepIcon(_ step: CodingReadiness.Step) -> some View {
        let current = step.state == .current
        return ZStack {
            if current {
                Circle().stroke(ReadinessSheetTokens.amber, lineWidth: 1.5)
            } else {
                Circle().stroke(
                    Color.white.opacity(0.30),
                    style: StrokeStyle(lineWidth: 1, dash: FloatingBarTokens.dashPattern)
                )
            }
            AppIcon(Self.icon(step.key), size: ReadinessSheetTokens.stepGlyph)
                .foregroundStyle(current ? ReadinessSheetTokens.amber : .white.opacity(TextOpacity.tertiary))
        }
        .frame(width: ReadinessSheetTokens.iconSize, height: ReadinessSheetTokens.iconSize)
        // Top-aligned with the title's first line.
        .padding(.top, -2)
    }

    static func icon(_ key: CodingReadiness.StepKey) -> String {
        switch key {
        case .github: AppIcons.settingsRepositories
        case .repository: AppIcons.actionRepository
        case .device: AppIcons.uiDevice
        }
    }

    /// Only fixes the viewer can perform: every fix here is member-level on
    /// iOS (`boards.setRepository` = `mutate_resources`, which any member
    /// holds; board repositories are edited in Team settings › Boards, open
    /// to members), and the sheet only ever opens for a member — so the one
    /// filter left is a board to act on.
    private func visibleFixes(_ step: CodingReadiness.Step) -> [CodingReadiness.Fix] {
        step.fixes.filter { fix in
            switch fix {
            case .chooseRepository, .boardSettings: vm.board != nil && vm.permissions.isMember
            default: true
            }
        }
    }

    // MARK: - Fixes

    private func fixButton(_ fix: CodingReadiness.Fix, primary: Bool) -> some View {
        Button {
            perform(fix)
        } label: {
            Text(fix.label)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(primary ? Color.black : Color.white)
                .lineLimit(1)
                .padding(.horizontal, ReadinessSheetTokens.fixHPadding)
                .frame(height: ReadinessSheetTokens.fixHeight)
                .background(
                    RoundedRectangle(cornerRadius: ReadinessSheetTokens.fixRadius)
                        .fill(primary ? Color.white : GlassTokens.fillCard)
                )
                .overlay {
                    if !primary {
                        RoundedRectangle(cornerRadius: ReadinessSheetTokens.fixRadius)
                            .stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                    }
                }
                .contentShape(RoundedRectangle(cornerRadius: ReadinessSheetTokens.fixRadius))
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("readiness-fix-\(fix.rawValue)")
    }

    private func perform(_ fix: CodingReadiness.Fix) {
        switch fix {
        case .chooseRepository:
            showPicker = true
        case .connectGithub:
            // The picker IS the guided flow on the phone; without a board to
            // point at the repo, Team settings' connection block is the fix.
            if vm.board != nil {
                showGithub = true
            } else {
                onRoute(.connectGithub)
            }
        case .boardSettings:
            onRoute(.boardSettings)
        case .openDevices:
            onRoute(.openDevices)
        case .getDesktopApp:
            // The sheet stays: back from Safari, the device row ticks.
            openURL(AppConstants.desktopReleasesUrl)
        case .setUpServer:
            // The sheet stays under it: a machine that signs in from the
            // block ticks the device row.
            showAddDevice = true
        }
    }

    // MARK: - Footer

    private func startButton(ready: Bool) -> some View {
        Button {
            if ready { onStart() }
        } label: {
            HStack(spacing: 8) {
                AppIcon(AppIcons.actionRun, size: AppIcon.Size.medium, weight: .medium)
                Text(CodingReadiness.Copy.start)
                    .font(.body.weight(.medium))
            }
            .foregroundStyle(ready ? Color.black : Color.white.opacity(TextOpacity.quaternary))
            .frame(maxWidth: .infinity)
            .frame(height: ReadinessSheetTokens.startHeight)
            .background(Capsule().fill(ready ? Color.white : GlassTokens.fillCard))
            .overlay {
                if !ready {
                    Capsule().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .disabled(!ready)
        .accessibilityIdentifier("readiness-start")
    }
}

// MARK: - Repository picker

/// "Choose repository": the team's repositories for THIS board — the ones
/// named like it first ("matches board"), the rest tagged with the board
/// already using them — plus the add-from-GitHub escape hatch. A pick calls
/// `boards.setRepository` (board settings' own call) and closes; the
/// checklist underneath ticks off the synced board row.
struct CodingReadinessRepoPickerSheet: View {
    let model: CodingReadinessModel
    let board: BoardEntity
    let accountId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var busyId: String?
    @State private var errorText: String?
    @State private var showAddRepo = false

    var body: some View {
        let rows = model.pickerRows(board: board, query: query)
        GlassSheetChrome(title: CodingReadiness.Copy.fixChooseRepository, pinnedHeader: {
            GlassSheetSearchField(
                placeholder: CodingReadiness.Copy.pickerSearch,
                text: $query,
                accessibilityIdentifier: "readiness-repo-search"
            )
            .padding(.horizontal, 16)
            .padding(.bottom, 8)
        }, content: {
            VStack(alignment: .leading, spacing: 0) {
                if let errorText {
                    Text(errorText)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                }
                if model.repos == nil {
                    ProgressView()
                        .tint(.white)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 20)
                } else if rows.isEmpty && query.isEmpty {
                    Text(CodingReadiness.Copy.pickerEmpty)
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .padding(.horizontal, 16)
                        .padding(.vertical, 14)
                } else {
                    ForEach(rows, id: \.id) { row in
                        repoRow(row)
                        GlassDivider()
                    }
                }
                addFromGithubRow
            }
            .padding(.bottom, 8)
        })
        .task { await model.refresh(board: board) }
        .sheet(isPresented: $showAddRepo) {
            GithubRepoPicker(
                accountId: accountId,
                teamId: board.teamId,
                integrationsApi: deps.integrationsApi
            ) { picked in
                let pointed = try await readinessAddFromGithub(
                    deps: deps, model: model, board: board, accountId: accountId, picked: picked
                )
                if pointed { dismiss() }
            }
        }
    }

    private func repoRow(_ row: CodingReadinessRepoPicker.Row) -> some View {
        Button {
            Task { await pick(row.id) }
        } label: {
            HStack(spacing: 10) {
                AppIcon(AppIcons.uiRepository, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(row.fullName)
                    .font(.subheadline.monospaced())
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                if busyId == row.id {
                    ProgressView().controlSize(.small).tint(.white)
                } else if let tag = row.tag {
                    Text(tag)
                        .font(.footnote)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(busyId != nil)
        .accessibilityIdentifier("readiness-repo-\(row.fullName)")
    }

    private var addFromGithubRow: some View {
        Button {
            showAddRepo = true
        } label: {
            HStack(spacing: 10) {
                AppIcon(AppIcons.uiGithub, size: AppIcon.Size.small)
                Text(CodingReadiness.Copy.pickerAddFromGithub)
                    .font(.subheadline)
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .padding(.horizontal, 16)
            .padding(.vertical, 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(busyId != nil)
        .accessibilityIdentifier("readiness-repo-add")
    }

    private func pick(_ repositoryId: String) async {
        guard busyId == nil else { return }
        busyId = repositoryId
        errorText = nil
        defer { busyId = nil }
        do {
            try await deps.boardsApi.setRepository(
                accountId: accountId, boardId: board.id, repositoryId: repositoryId
            )
            RepositoryDirectory.invalidate(accountId: accountId, teamId: board.teamId)
            dismiss()
        } catch {
            errorText = error.trpcUserMessage
        }
    }
}

/// The readiness flow's add-from-GitHub: register the picked repo with the
/// team (`repositories.add`), then point the board at it (`boards.
/// setRepository`, board settings' own call) so the repository row ticks,
/// then re-list. Throws on failure (the GitHub sheet renders it inline);
/// returns whether the board now points at the repo.
@MainActor
func readinessAddFromGithub(
    deps: AppDependencies,
    model: CodingReadinessModel,
    board: BoardEntity,
    accountId: String,
    picked: GithubPickerRepo
) async throws -> Bool {
    let id = try await deps.repositoriesApi.add(
        accountId: accountId,
        teamId: board.teamId,
        fullName: picked.fullName,
        defaultBranch: picked.defaultBranch,
        isPrivate: picked.`private`
    )
    if let id {
        try await deps.boardsApi.setRepository(
            accountId: accountId, boardId: board.id, repositoryId: id
        )
    }
    RepositoryDirectory.invalidate(accountId: accountId, teamId: board.teamId)
    await model.refresh(board: board)
    return id != nil
}
