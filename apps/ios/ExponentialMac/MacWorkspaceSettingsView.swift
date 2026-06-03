import ExpCore
import ExpUI
import GRDB
import SwiftUI

struct WorkspaceSettingsTarget: Identifiable, Hashable {
    let accountId: String
    let workspaceId: String
    var id: String { accountId + ":" + workspaceId }
}

@MainActor
@Observable
final class MacWorkspaceSettingsModel {
    var workspace: WorkspaceEntity?
    var members: [WorkspaceMemberEntity] = []
    var invites: [WorkspaceInviteEntity] = []
    var labels: [LabelEntity] = []
    var projects: [ProjectEntity] = []
    var users: [UserEntity] = []
    var error: String?

    let accountId: String
    let workspaceId: String
    private let deps: MacAppDependencies
    private var tasks: [Task<Void, Never>] = []

    init(deps: MacAppDependencies, accountId: String, workspaceId: String) {
        self.deps = deps
        self.accountId = accountId
        self.workspaceId = workspaceId
    }

    var isOwnerOrAdmin: Bool {
        if deps.auth.isAdmin { return true }
        guard let uid = deps.auth.userId else { return false }
        return members.contains { $0.userId == uid && $0.role == DomainContract.workspaceRoleOwner }
    }

    func user(_ id: String) -> UserEntity? { users.first { $0.id == id } }

    func start() {
        guard tasks.isEmpty, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let wid = workspaceId
        let wsObs = ValueObservation.tracking { db in try WorkspaceEntity.fetchOne(db, key: wid) }
        let memberObs = ValueObservation.tracking { db in
            try WorkspaceMemberEntity.filter(Column("workspace_id") == wid).fetchAll(db)
        }
        let inviteObs = ValueObservation.tracking { db in
            try WorkspaceInviteEntity.filter(Column("workspace_id") == wid).fetchAll(db)
        }
        let labelObs = ValueObservation.tracking { db in
            try LabelEntity.filter(Column("workspace_id") == wid).fetchAll(db)
        }
        let projectObs = ValueObservation.tracking { db in
            try ProjectEntity.filter(Column("workspace_id") == wid).fetchAll(db)
        }
        let userObs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }

        tasks.append(Task { @MainActor [weak self] in
            do { for try await row in wsObs.values(in: pool) { self?.workspace = row } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in memberObs.values(in: pool) { self?.members = rows } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in inviteObs.values(in: pool) { self?.invites = rows.filter { $0.acceptedAt == nil } } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in labelObs.values(in: pool) { self?.labels = rows.sorted { $0.name < $1.name } } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in projectObs.values(in: pool) { self?.projects = rows.filter { $0.archivedAt == nil } } } catch {}
        })
        tasks.append(Task { @MainActor [weak self] in
            do { for try await rows in userObs.values(in: pool) { self?.users = rows } } catch {}
        })
    }

    func stop() {
        tasks.forEach { $0.cancel() }
        tasks = []
    }

    // MARK: - Mutations

    private func run(_ op: @escaping () async throws -> Void) {
        Task { do { try await op() } catch { self.error = error.localizedDescription } }
    }

    func setName(_ name: String) {
        run { [self] in
            try await deps.workspacesApi.update(accountId: accountId, UpdateWorkspaceInput(id: workspaceId, name: name))
        }
    }

    func deleteWorkspace(onSuccess: @escaping () -> Void) {
        run { [self] in
            try await deps.workspacesApi.delete(accountId: accountId, workspaceId: workspaceId)
            onSuccess()
        }
    }

    func setPublic(_ isPublic: Bool) {
        run { [self] in
            try await deps.workspacesApi.update(accountId: accountId, UpdateWorkspaceInput(
                id: workspaceId,
                isPublic: isPublic,
                publicWritePolicy: isPublic ? DomainContract.publicWritePolicyMembers : nil
            ))
        }
    }

    func setWritePolicy(_ policy: String) {
        run { [self] in
            try await deps.workspacesApi.update(accountId: accountId, UpdateWorkspaceInput(id: workspaceId, publicWritePolicy: policy))
        }
    }

    func setRole(_ memberId: String, _ role: String) {
        run { [self] in try await deps.workspaceMembersApi.updateRole(accountId: accountId, memberId: memberId, role: role) }
    }

    func removeMember(_ memberId: String) {
        run { [self] in try await deps.workspaceMembersApi.remove(accountId: accountId, memberId: memberId) }
    }

    func createInvite() {
        run { [self] in
            let result = try await deps.workspaceInvitesApi.create(
                accountId: accountId, workspaceId: workspaceId, role: DomainContract.workspaceRoleMember
            )
            let base = deps.auth.instanceUrl ?? ""
            Platform.copyToPasteboard("\(base)/invite/\(result.token)")
        }
    }

    func revokeInvite(_ id: String) {
        run { [self] in try await deps.workspaceInvitesApi.revoke(accountId: accountId, inviteId: id) }
    }

    func deleteProject(_ id: String) {
        run { [self] in try await deps.workspacesApi.deleteProject(accountId: accountId, projectId: id) }
    }

    func createLabel(name: String, color: String) {
        run { [self] in
            _ = try await deps.labelsApi.create(accountId: accountId, CreateLabelInput(name: name, color: color, workspaceId: workspaceId))
        }
    }

    func updateLabel(_ id: String, name: String? = nil, color: String? = nil) {
        run { [self] in
            try await deps.labelsApi.update(accountId: accountId, UpdateLabelInput(id: id, name: name, color: color))
        }
    }

    func deleteLabel(_ id: String) {
        run { [self] in try await deps.labelsApi.delete(accountId: accountId, id: id) }
    }
}

struct MacWorkspaceSettingsView: View {
    @Environment(MacAppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    let target: WorkspaceSettingsTarget

    @State private var model: MacWorkspaceSettingsModel?
    @State private var newLabelName = ""
    @State private var newLabelColor = DEFAULT_LABEL_COLOR
    @State private var agentName = MacAgentService.defaultAgentName
    @State private var nameDraft: String?
    @State private var editingLabelId: String?
    @State private var editingName = ""
    @State private var showDeleteConfirm = false

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(model?.workspace?.name ?? "Workspace").font(.headline)
                Spacer()
                Button("Done") { dismiss() }
            }
            .padding()
            Divider()
            if let model {
                Form {
                    generalSection(model)
                    membersSection(model)
                    invitesSection(model)
                    projectsSection(model)
                    labelsSection(model)
                    agentSection()
                    dangerSection(model)
                    if let error = model.error {
                        Text(error).foregroundStyle(.red).font(.callout)
                    }
                }
                .formStyle(.grouped)
                .confirmationDialog(
                    "Delete this workspace?",
                    isPresented: $showDeleteConfirm,
                    titleVisibility: .visible
                ) {
                    Button("Delete Workspace", role: .destructive) {
                        model.deleteWorkspace { dismiss() }
                    }
                    Button("Cancel", role: .cancel) {}
                } message: {
                    Text("This permanently deletes the workspace and all its data.")
                }
            } else {
                ProgressView().frame(maxHeight: .infinity)
            }
        }
        .frame(width: 520, height: 620)
        .onAppear {
            if model == nil {
                let m = MacWorkspaceSettingsModel(deps: deps, accountId: target.accountId, workspaceId: target.workspaceId)
                model = m
                m.start()
            }
        }
        .onDisappear { model?.stop() }
    }

    @ViewBuilder
    private func generalSection(_ model: MacWorkspaceSettingsModel) -> some View {
        if let ws = model.workspace {
            Section("General") {
                TextField("Name", text: Binding(
                    get: { nameDraft ?? ws.name },
                    set: { nameDraft = $0 }
                ))
                .textFieldStyle(.roundedBorder)
                .disabled(!model.isOwnerOrAdmin)
                .onSubmit {
                    let trimmed = (nameDraft ?? "").trimmingCharacters(in: .whitespaces)
                    if !trimmed.isEmpty, trimmed != ws.name { model.setName(trimmed) }
                }
                Toggle("Public workspace", isOn: Binding(
                    get: { ws.isPublic },
                    set: { model.setPublic($0) }
                ))
                .disabled(!model.isOwnerOrAdmin)
                if ws.isPublic {
                    Picker("Who can write", selection: Binding(
                        get: { ws.publicWritePolicy ?? DomainContract.publicWritePolicyMembers },
                        set: { model.setWritePolicy($0) }
                    )) {
                        Text("Members only").tag(DomainContract.publicWritePolicyMembers)
                        Text("Everyone").tag(DomainContract.publicWritePolicyEveryone)
                    }
                    .disabled(!model.isOwnerOrAdmin)
                }
            }
        }
    }

    private func membersSection(_ model: MacWorkspaceSettingsModel) -> some View {
        Section("Members") {
            ForEach(model.members) { member in
                let u = model.user(member.userId)
                HStack {
                    VStack(alignment: .leading) {
                        Text(u?.name ?? u?.email ?? member.userId)
                        Text(member.role).font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    if model.isOwnerOrAdmin {
                        Menu {
                            if member.role != DomainContract.workspaceRoleOwner {
                                Button("Make owner") { model.setRole(member.id, DomainContract.workspaceRoleOwner) }
                            }
                            if member.role != DomainContract.workspaceRoleMember {
                                Button("Make member") { model.setRole(member.id, DomainContract.workspaceRoleMember) }
                            }
                            Button("Remove", role: .destructive) { model.removeMember(member.id) }
                        } label: { Image(systemName: "ellipsis.circle") }
                        .menuStyle(.borderlessButton).fixedSize()
                    }
                }
            }
        }
    }

    private func invitesSection(_ model: MacWorkspaceSettingsModel) -> some View {
        Section("Invites") {
            Button("Create invite link (copied)") { model.createInvite() }
                .disabled(!model.isOwnerOrAdmin)
            ForEach(model.invites) { invite in
                HStack {
                    Text(invite.role).foregroundStyle(.secondary)
                    Spacer()
                    Button("Revoke", role: .destructive) { model.revokeInvite(invite.id) }
                        .disabled(!model.isOwnerOrAdmin)
                }
            }
        }
    }

    private func projectsSection(_ model: MacWorkspaceSettingsModel) -> some View {
        Section("Projects") {
            ForEach(model.projects) { project in
                HStack {
                    Text(project.name)
                    Spacer()
                    Text(project.prefix).font(.caption).foregroundStyle(.tertiary)
                    if model.isOwnerOrAdmin {
                        Button(role: .destructive) { model.deleteProject(project.id) } label: {
                            Image(systemName: "trash")
                        }
                        .buttonStyle(.borderless)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func agentSection() -> some View {
        let agent = deps.agentService
        let wid = target.workspaceId
        Section("Desktop Agent") {
            if agent.isRegistered(wid) {
                let id = agent.identity(wid)
                HStack(spacing: 8) {
                    Circle().fill(agent.isOnline(wid) ? Color.green : Color.secondary).frame(width: 8, height: 8)
                    Text(id?.agentName ?? "This Mac")
                    Spacer()
                    Text(agent.isOnline(wid) ? "Online" : "Connecting…").font(.caption).foregroundStyle(.secondary)
                }
                if let login = id?.githubLogin, !login.isEmpty {
                    Label("GitHub: \(login)", systemImage: "checkmark.seal.fill").foregroundStyle(.green)
                } else if let p = agent.githubPrompt {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Enter code **\(p.userCode)** at \(p.uri)").font(.caption)
                        ProgressView().controlSize(.small)
                    }
                } else {
                    Button("Connect GitHub") { Task { await agent.connectGitHub(workspaceId: wid) } }.disabled(agent.busy)
                }
                Button("Unregister this Mac", role: .destructive) {
                    Task { await agent.unregister(workspaceId: wid) }
                }
                .disabled(agent.busy)
            } else {
                TextField("Agent name", text: $agentName).textFieldStyle(.roundedBorder)
                Button("Register this Mac as an agent") {
                    Task { await agent.register(accountId: target.accountId, workspaceId: wid, name: agentName) }
                }
                .disabled(agent.busy || agentName.trimmingCharacters(in: .whitespaces).isEmpty)
                Text("Registers this Mac so it can run coding agents on assigned issues (while the app is open).")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let err = agent.lastError {
                Text(err).font(.caption).foregroundStyle(.red)
            }
        }
    }

    @ViewBuilder
    private func dangerSection(_ model: MacWorkspaceSettingsModel) -> some View {
        if let ws = model.workspace, model.isOwnerOrAdmin, !ws.isPublic {
            Section("Danger Zone") {
                Button("Delete Workspace", role: .destructive) { showDeleteConfirm = true }
            }
        }
    }

    private func labelsSection(_ model: MacWorkspaceSettingsModel) -> some View {
        Section("Labels") {
            ForEach(model.labels) { label in
                HStack(spacing: 10) {
                    Menu {
                        ColorSwatchGrid(selection: Binding(
                            get: { label.color },
                            set: { model.updateLabel(label.id, color: $0) }
                        ))
                        .padding(8)
                        .frame(width: 200)
                    } label: {
                        Circle().fill(Color(hex: label.color) ?? .gray).frame(width: 14, height: 14)
                    }
                    .menuStyle(.borderlessButton).fixedSize()

                    if editingLabelId == label.id {
                        TextField("Name", text: $editingName)
                            .textFieldStyle(.roundedBorder)
                            .onSubmit {
                                let n = editingName.trimmingCharacters(in: .whitespaces)
                                if !n.isEmpty { model.updateLabel(label.id, name: n) }
                                editingLabelId = nil
                            }
                    } else {
                        Text(label.name)
                            .onTapGesture { editingLabelId = label.id; editingName = label.name }
                    }
                    Spacer()
                    Button(role: .destructive) { model.deleteLabel(label.id) } label: { Image(systemName: "trash") }
                        .buttonStyle(.borderless)
                }
            }
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    TextField("New label", text: $newLabelName).textFieldStyle(.roundedBorder)
                    Button("Add") {
                        let name = newLabelName.trimmingCharacters(in: .whitespaces)
                        guard !name.isEmpty else { return }
                        model.createLabel(name: name, color: newLabelColor)
                        newLabelName = ""
                    }
                    .disabled(newLabelName.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                ColorSwatchGrid(selection: $newLabelColor)
            }
        }
    }
}
