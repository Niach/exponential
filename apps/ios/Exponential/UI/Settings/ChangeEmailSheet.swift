import ExpCore
import ExpUI
import SwiftUI

/// EXP-1126: change the account's primary email — web `ChangeEmailDialog`
/// parity. Two steps against Better Auth's email-otp plugin: the NEW address
/// gets a 6-digit code (`request-email-change`), the code swaps the address
/// (`change-email`). The signed-in session is the ownership proof of the old
/// mailbox. Every call carries the SCREEN's account credentials, never the
/// active account's (Settings can show a non-active server).
@MainActor
@Observable
final class ChangeEmailModel {
    enum Step { case address, code }

    let accountId: String
    let currentEmail: String
    var step: Step = .address
    var newEmail = ""
    var code = ""
    var busy = false
    var error: String?

    init(accountId: String, currentEmail: String) {
        self.accountId = accountId
        self.currentEmail = currentEmail
    }

    /// Step 1 (and "Resend code"). Returns nothing — the step flips on success.
    func sendCode(deps: AppDependencies) async {
        let target = newEmail.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !target.isEmpty, !busy else { return }
        if target == currentEmail.lowercased() {
            error = "That is already your email."
            return
        }
        guard let account = deps.auth.accounts.first(where: { $0.id == accountId }),
              let token = account.token else { return }
        busy = true
        error = nil
        defer { busy = false }
        switch await deps.authApi.requestEmailChange(
            instanceUrl: account.instanceUrl, token: token, newEmail: target
        ) {
        case .success:
            newEmail = target
            code = ""
            step = .code
        case let .failure(message):
            error = message
        }
    }

    /// Step 2. On success re-reads the session and persists the new identity
    /// into the account store; returns whether the change landed.
    func confirm(deps: AppDependencies) async -> Bool {
        let otp = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard otp.count >= 6, !busy else { return false }
        guard let account = deps.auth.accounts.first(where: { $0.id == accountId }),
              let token = account.token else { return false }
        busy = true
        error = nil
        defer { busy = false }
        switch await deps.authApi.changeEmail(
            instanceUrl: account.instanceUrl, token: token, newEmail: newEmail, otp: otp
        ) {
        case .success:
            // The server's view wins; the address we sent is the fallback when
            // the re-read is indeterminate (offline blip right after the swap).
            let user = await deps.authApi.fetchSession(accountId: accountId)
            deps.auth.updateIdentity(
                accountId: accountId,
                email: user?.email ?? newEmail,
                name: user?.name
            )
            return true
        case let .failure(message):
            error = message
            return false
        }
    }

    func useDifferentEmail() {
        error = nil
        code = ""
        step = .address
    }
}

struct ChangeEmailSheet: View {
    @State private var model: ChangeEmailModel
    let onChanged: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @FocusState private var focused: Bool

    init(accountId: String, currentEmail: String, onChanged: @escaping (String) -> Void) {
        _model = State(initialValue: ChangeEmailModel(accountId: accountId, currentEmail: currentEmail))
        self.onChanged = onChanged
    }

    var body: some View {
        GlassSheetChrome(title: "Change your email") {
            VStack(alignment: .leading, spacing: 12) {
                Text(description)
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .fixedSize(horizontal: false, vertical: true)

                if model.step == .address {
                    GlassTextField("New email", text: $model.newEmail, accessibilityIdentifier: "change-email-field")
                        .keyboardType(.emailAddress)
                        .textContentType(.emailAddress)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .focused($focused)
                        .onSubmit { Task { await model.sendCode(deps: deps) } }
                } else {
                    GlassTextField("Code", text: $model.code, accessibilityIdentifier: "change-email-code-field")
                        .textContentType(.oneTimeCode)
                        .keyboardType(.numberPad)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .focused($focused)
                        .onSubmit { Task { await confirm() } }
                }

                if let error = model.error {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red.opacity(0.8))
                        .fixedSize(horizontal: false, vertical: true)
                }

                if model.step == .code {
                    HStack(spacing: 16) {
                        actionLink("Resend code") {
                            Task { await model.sendCode(deps: deps) }
                        }
                        actionLink("Use a different email") {
                            model.useDifferentEmail()
                            focused = true
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .center)
                    .disabled(model.busy)
                }
            }
            .padding(16)
        } primaryAction: {
            if model.step == .address {
                GlassSubmitButton(
                    model.busy ? "Sending…" : "Send code",
                    enabled: !model.newEmail.trimmingCharacters(in: .whitespaces).isEmpty,
                    loading: model.busy
                ) {
                    Task { await model.sendCode(deps: deps) }
                }
                .accessibilityIdentifier("change-email-send-button")
            } else {
                GlassSubmitButton(
                    model.busy ? "Checking…" : "Confirm",
                    enabled: model.code.trimmingCharacters(in: .whitespaces).count >= 6,
                    loading: model.busy
                ) {
                    Task { await confirm() }
                }
                .accessibilityIdentifier("change-email-confirm-button")
            }
        }
        .onAppear { focused = true }
    }

    private var description: String {
        switch model.step {
        case .address:
            return "We'll send a code to the new address. Sign-in codes, notifications and @mentions use it once confirmed; your old address stops working for sign-in."
        case .code:
            return "Enter the 6-digit code we sent to \(model.newEmail). It expires in 10 minutes."
        }
    }

    private func confirm() async {
        let target = model.newEmail
        if await model.confirm(deps: deps) {
            onChanged(target)
            dismiss()
        }
    }

    private func actionLink(_ label: String, action: @escaping () -> Void) -> some View {
        Button(label, action: action)
            .font(.footnote)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
    }
}
