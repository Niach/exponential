import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// Per-server management screen. Reached from Settings → Servers → tap a row.
/// Centralizes the actions that used to be split between the top-of-Settings
/// "Sign out" button (which only signed out the active account) and the
/// long-press context menu on the server list (which only offered "Remove").
struct ServerDetailView: View {
    let accountId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @State private var showRemoveConfirm = false
    @State private var showDeleteAccountConfirm = false
    @State private var deletingAccount = false
    @Environment(\.toaster) private var toaster
    // EXP-311: the signed-in user's synced row — carries the profile image
    // the account store doesn't. Nil until the users shape has landed.
    @State private var user: UserEntity?
    @State private var userObservationTask: Task<Void, Never>?
    /// The stored `users.timezone` (nil = unset → "UTC", web parity) and
    /// whether the read landed — the row renders only after it did, so it
    /// never flashes a wrong zone.
    @State private var timezone: String?
    @State private var timezoneLoaded = false

    private var account: ServerAccount? {
        deps.auth.accounts.first { $0.id == accountId }
    }

    /// The identity the header renders: the observed users row, else a
    /// display-only stand-in from the account store so name/initials are
    /// right even before the first sync.
    private var identityUser: UserEntity? {
        if let user { return user }
        guard let account, let email = account.userEmail, !email.isEmpty else { return nil }
        return UserEntity(
            id: account.userId ?? "",
            name: account.userName,
            email: email,
            image: nil,
            createdAt: "",
            updatedAt: ""
        )
    }

    /// The bundled cloud (prod or staging) — "Remove server" is nonsensical for
    /// it (it's the app's built-in instance, not a user-added server), so that
    /// action is hidden. Sign out / delete account stay available.
    private var isBuiltInCloud: Bool {
        guard let base = WebLinks.normalizedBase(account?.instanceUrl) else { return false }
        return base == WebLinks.normalizedBase(AppConstants.publicCloudUrl)
            || base == WebLinks.normalizedBase(AppConstants.stagingCloudUrl)
    }

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    identitySection
                    // ×4: Timezone right under the identity card, a picker
                    // row (the daily digest's clock, EXP-369).
                    if account?.token != nil, timezoneLoaded {
                        timezoneSection
                    }
                    // EXP-1126: how this account signs in — only while it
                    // holds a session (every call is bearer-authenticated).
                    if account?.token != nil {
                        SignInMethodsSection(accountId: accountId)
                    }
                    actionsSection
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 96)
            }
        }
        .navigationTitle(account?.displayName ?? "Server")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .onAppear { startObservingUser() }
        .task(id: accountId) { await loadTimezone() }
        .onDisappear { userObservationTask?.cancel() }
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4. Each on a
        // zero-size node of its own (EXP-240): two presentations stacked on
        // one node and SwiftUI drops the second.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showRemoveConfirm) {
                    GlassAlert(
                        prompt: Prompts.RemoveServer.copy(server: account?.displayName ?? "this server"),
                        handlers: ["remove": { removeServer() }]
                    )
                }
        }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(isPresented: $showDeleteAccountConfirm) {
                    GlassAlert(
                        prompt: Prompts.DeleteAccount.copy(server: account?.displayName),
                        handlers: ["delete": { Task { await deleteAccount() } }]
                    )
                }
        }
    }

    /// Unregister push, revoke the session, drop the account and its cache.
    private func removeServer() {
        guard let account else { return }
        Task {
            // Unregister while the credentials still exist — the
            // request needs the bearer token removeAccount drops.
            await deps.pushTokenManager.unregister(accountId: account.id)
            // Revoke the server session AFTER the unregister (which
            // needs it live) and BEFORE removeAccount drops the token.
            if let token = account.token {
                await deps.authApi.signOut(instanceUrl: account.instanceUrl, token: token)
            }
            await deps.syncManager.signOut(accountId: account.id)
            deps.auth.removeAccount(id: account.id)
            deps.db.closePool(forAccountId: account.id)
            DatabaseManager.deleteFiles(forAccountId: account.id)
            // The share-extension board mirror lives in app-group
            // defaults, not the DB — scrub it here too, since the
            // board loader may never have been instantiated.
            SharedBoardMirror.remove(accountId: account.id)
            dismiss()
        }
    }

    /// Server-side deletion first; only on success tear down the local
    /// account + cache (mirrors the "Remove server" path).
    private func deleteAccount() async {
        guard let account, !deletingAccount else { return }
        deletingAccount = true
        defer { deletingAccount = false }
        do {
            try await deps.usersApi.deleteAccount(accountId: account.id)
        } catch {
            toaster.error("Couldn't delete account", description: error.trpcUserMessage)
            return
        }
        // No push-token unregister here: deleting the user server-side
        // cascades their fcm_tokens rows away.
        await deps.syncManager.signOut(accountId: account.id)
        deps.auth.removeAccount(id: account.id)
        deps.db.closePool(forAccountId: account.id)
        DatabaseManager.deleteFiles(forAccountId: account.id)
        SharedBoardMirror.remove(accountId: account.id)
        dismiss()
    }

    /// Watches the account's own users row for the profile image (EXP-311).
    private func startObservingUser() {
        guard userObservationTask == nil,
              let userId = account?.userId,
              let pool = try? deps.db.pool(forAccountId: accountId)
        else { return }
        userObservationTask = Task {
            let obs = ValueObservation.tracking { db in
                try UserEntity.fetchOne(db, key: userId)
            }
            do {
                for try await item in obs.values(in: pool) {
                    await MainActor.run { user = item }
                }
            } catch {}
        }
    }

    /// EXP-311: signed in, the header is the ACCOUNT identity — avatar
    /// (profile image / initials) + "Firstname Lastname" with the email
    /// below, unified with the web account page. Signed out, the server
    /// block stays so the row remains identifiable.
    private var identitySection: some View {
        sectionStack(title: nil) {
            HStack(spacing: 12) {
                if account?.token != nil, let identityUser {
                    UserAvatar(user: identityUser, id: account?.userId, size: 40)
                    VStack(alignment: .leading, spacing: 2) {
                        let name = memberDisplayName(identityUser, id: account?.userId)
                        Text(name)
                            .font(.body)
                            .foregroundStyle(.white)
                        if let email = account?.userEmail, !email.isEmpty, email != name {
                            Text(email)
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }
                } else {
                    AppIcon(AppIcons.settingsServers, size: AppIcon.Size.large)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(account?.displayName ?? "")
                            .font(.body)
                            .foregroundStyle(.white)
                        if let email = account?.userEmail, !email.isEmpty {
                            Text(email)
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        Text(account?.token == nil ? "Signed out" : "Signed in")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                    }
                }
                Spacer()
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .glassRow()
        }
    }

    /// Every IANA zone the phone knows, plus the stored one should the phone
    /// lack it (web `timezoneOptions`).
    private static let knownZones: [String] = TimeZone.knownTimeZoneIdentifiers.sorted()

    private var timezoneSection: some View {
        let current = timezone ?? "UTC"
        let zones = Self.knownZones.contains(current) ? Self.knownZones : [current] + Self.knownZones
        return GlassPicker(
            items: zones.map { PickerItem(value: $0, label: $0) },
            mode: .single,
            value: [current],
            onChange: { picked in
                guard let next = picked.first, next != current else { return }
                Task { await setTimezone(next) }
            },
            search: true,
            emptyText: "No matching timezone.",
            title: "Timezone"
        ) {
            GlassPickerRowLabel("Timezone", value: current)
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
        }
        .glassSection()
    }

    private func loadTimezone() async {
        guard account?.token != nil else { return }
        do {
            timezone = try await deps.usersApi.timezone(accountId: accountId)
            timezoneLoaded = true
        } catch {
            // No row rather than a guessed zone; the next visit retries.
        }
    }

    /// Optimistic: the row reads the pick at once and reverts on a refusal.
    private func setTimezone(_ next: String) async {
        let previous = timezone
        timezone = next
        do {
            try await deps.usersApi.setTimezone(accountId: accountId, timezone: next, onlyIfUnset: false)
        } catch {
            timezone = previous
            toaster.error("Couldn't update timezone", description: error.trpcUserMessage)
        }
    }

    /// Whether anything renders ABOVE "Remove server" — what decides its
    /// leading hairline (a divider over the first row would fence off nothing).
    private var hasAccountActions: Bool {
        account?.token != nil || account?.instanceUrl != nil
    }

    /// EXP-994: ONE grouped card, rows separated by hairlines — the Settings
    /// idiom. These used to be individually bordered rows in a 6pt-gapped
    /// stack, the shape every other grouped list on this client stopped using.
    private var actionsSection: some View {
        sectionStack(title: nil) {
            VStack(spacing: 0) {
                if account?.token != nil {
                    Button {
                        Task {
                            // Capture the URL + token BEFORE removeAccount —
                            // `account` is a computed lookup that returns nil
                            // once removed.
                            let url = account?.instanceUrl
                            let token = account?.token
                            // Unregister while the credentials still exist — the
                            // request needs the bearer token removeAccount drops.
                            await deps.pushTokenManager.unregister(accountId: accountId)
                            // Revoke the server session AFTER the unregister
                            // (which needs it live) and BEFORE the token drops.
                            if let url, let token {
                                await deps.authApi.signOut(instanceUrl: url, token: token)
                            }
                            await deps.syncManager.signOut(accountId: accountId)
                            deps.auth.removeAccount(id: accountId)
                            // Delete the local DB so a signed-out account leaves
                            // no user data at rest (mirrors the Remove-server path).
                            deps.db.closePool(forAccountId: accountId)
                            DatabaseManager.deleteFiles(forAccountId: accountId)
                            SharedBoardMirror.remove(accountId: accountId)
                            // Re-add the URL so the user can re-auth via the login
                            // flow without losing the entry.
                            if let url {
                                deps.auth.setInstanceUrl(url)
                            }
                            dismiss()
                        }
                    } label: {
                        // White, not red — signing out is routine, only the
                        // destructive rows below are red (Android parity,
                        // EXP-577).
                        actionRow(
                            icon: AppIcons.navSignOut,
                            title: "Sign out",
                            tint: .white
                        )
                    }
                    .buttonStyle(.plain)

                    GlassDivider()

                    // App Store guideline 5.1.1(v): account deletion must be
                    // initiable in-app, not via email.
                    Button {
                        showDeleteAccountConfirm = true
                    } label: {
                        actionRow(
                            icon: AppIcons.uiDeleteAccount,
                            title: deletingAccount ? "Deleting account…" : "Delete account",
                            tint: .red
                        )
                    }
                    .buttonStyle(.plain)
                    .disabled(deletingAccount)
                } else if let url = account?.instanceUrl {
                    Button {
                        deps.auth.setInstanceUrl(url)
                        dismiss()
                    } label: {
                        actionRow(
                            icon: AppIcons.uiRefresh,
                            title: "Reauthenticate",
                            tint: .white
                        )
                    }
                    .buttonStyle(.plain)
                }

                if !isBuiltInCloud {
                    if hasAccountActions { GlassDivider() }
                    Button {
                        showRemoveConfirm = true
                    } label: {
                        actionRow(
                            icon: AppIcons.uiDelete,
                            title: "Remove server",
                            tint: .red
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
            .glassSection()
        }
    }

    /// EXP-698: the header is the ONE shared `GlassSectionHeader` (which
    /// carries its own bottom inset), so the stack only owns the content gap.
    @ViewBuilder
    private func sectionStack<Content: View>(title: String?, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            if let title {
                GlassSectionHeader(title)
            }
            content()
        }
    }

    /// One row of the actions card: no fill and no border of its own — the
    /// card carries both (EXP-994).
    private func actionRow(icon: String, title: String, tint: Color) -> some View {
        HStack(spacing: 12) {
            AppIcon(icon, size: AppIcon.Size.medium)
                .foregroundStyle(tint.opacity(tint == .red ? 0.85 : TextOpacity.secondary))
                .frame(width: 22)
            Text(title)
                .font(.body)
                .foregroundStyle(tint == .red ? .red.opacity(0.9) : .white)
            Spacer()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
    }
}
