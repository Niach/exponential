import ExpCore
import ExpUI
import SwiftUI

/// #EXP-2143 follow-up: "Confirm it's you" (fixtures/step-up.json; web
/// `useStepUp`, Android `ServerDetailViewModel.runGated`). Wraps an action
/// behind Better Auth's fresh-session gate: when it answers `.notFresh`, the
/// sheet opens with the account's ways in; a proof marks THIS session fresh
/// and the action runs again by itself. The state machine is ExpCore's
/// `StepUp` reducer, replayed ×4. Every call carries the SCREEN's account.
@MainActor
@Observable
final class StepUpModel {
    /// A gated action's answer: the gate refused it, or it handled its own
    /// outcome (success or its own failure toast).
    enum Gated { case handled, notFresh }

    private(set) var state = StepUp.State()
    private(set) var email = ""
    var secret = ""

    @ObservationIgnored private var accountId = ""
    @ObservationIgnored private var pending: (() async -> Void)?
    private let ceremony = PasskeyRegistrationCeremony()

    var isPresented: Bool { state.sheet != .closed }

    @discardableResult
    private func dispatch(_ event: StepUp.Event) -> StepUp.State {
        state = StepUp.reduce(state, event)
        return state
    }

    /// Run a gated action; a refusal opens the sheet, a proof resumes it.
    /// Nothing to prove with = `onFailed` with the old sentence.
    func run(
        deps: AppDependencies,
        accountId: String,
        onFailed: @escaping (String) -> Void,
        action: @escaping () async -> Gated
    ) async {
        guard await action() == .notFresh else {
            dispatch(.done)
            return
        }
        self.accountId = accountId
        var methods: [StepUp.Method] = []
        if !state.retried, let account = credentials(deps) {
            if case let .success(answer) = await deps.authApi.stepUpMethods(
                instanceUrl: account.instanceUrl, token: account.token
            ) {
                methods = answer.methods
                email = answer.email
            }
        }
        let next = dispatch(.refused(code: StepUp.notFresh, methods: methods))
        if next.action == .failed {
            pending = nil
            onFailed(next.error ?? StepUp.notFreshCopy)
            dispatch(.done)
            return
        }
        secret = ""
        pending = { [weak self] in
            await self?.run(deps: deps, accountId: accountId, onFailed: onFailed, action: action)
        }
        if next.method == .email { await sendCode(deps: deps) }
    }

    /// Choosing email mails the code at once (and again on "Send a new code").
    func sendCode(deps: AppDependencies) async {
        guard let account = credentials(deps) else { return }
        if case let .failure(refused) = await deps.authApi.stepUpSendCode(
            instanceUrl: account.instanceUrl, token: account.token
        ) {
            // The sheet stays; the error reads under the field.
            dispatch(.submit)
            dispatch(.rejected(code: refused.code))
        }
    }

    func choose(_ method: StepUp.Method, deps: AppDependencies) async {
        let before = state.method
        let next = dispatch(.choose(method))
        guard next.method != before else { return }
        secret = ""
        if next.method == .email { await sendCode(deps: deps) }
    }

    /// The sheet was dismissed: the action is dropped quietly, no toast, the
    /// screen under it keeps its state.
    func cancel() {
        guard state.sheet != .closed else { return }
        pending = nil
        dispatch(.cancel)
        dispatch(.done)
    }

    func submit(deps: AppDependencies) async {
        guard state.sheet == .open, let method = state.method, let account = credentials(deps) else { return }
        let (instanceUrl, token) = (account.instanceUrl, account.token)
        let value = secret
        if method != .passkey, value.trimmingCharacters(in: .whitespaces).isEmpty { return }
        dispatch(.submit)
        let result: Result<Void, StepUpRefused>
        switch method {
        case .email:
            result = await deps.authApi.stepUpVerifyCode(
                instanceUrl: instanceUrl, token: token, code: value.trimmingCharacters(in: .whitespaces)
            )
        case .password:
            result = await deps.authApi.stepUpPassword(instanceUrl: instanceUrl, token: token, password: value)
        case .passkey:
            result = await passkeyProof(deps: deps, instanceUrl: instanceUrl, token: token)
        }
        switch result {
        case .success:
            // The sheet closes; the action resumes once it is gone
            // (`dismissed`), so the system passkey sheet has a window to sit on.
            dispatch(.proved)
            secret = ""
        case let .failure(refused):
            dispatch(.rejected(code: refused.code))
        }
    }

    /// The sheet finished closing: after a proof the refused action runs
    /// again by itself; after a cancel there is nothing left to do.
    func dismissed() async {
        guard state.action == .resume, let resume = pending else { return }
        pending = nil
        await resume()
    }

    /// The login's assertion against this account's passkeys; options for
    /// any rp but this instance are refused (`relyingPartyMatchesInstance`).
    private func passkeyProof(deps: AppDependencies, instanceUrl: String, token: String) async -> Result<Void, StepUpRefused> {
        let options: PasskeyAuthenticationOptions
        switch await deps.authApi.stepUpPasskeyOptions(instanceUrl: instanceUrl, token: token) {
        case let .success(served): options = served
        case let .failure(refused): return .failure(refused)
        }
        guard PasskeyWire.relyingPartyMatchesInstance(rpId: options.rpId, instanceUrl: instanceUrl) else {
            return .failure(StepUpRefused(code: "PASSKEY_FAILED"))
        }
        switch await ceremony.authenticate(options) {
        case let .asserted(assertion):
            return await deps.authApi.stepUpPasskey(
                instanceUrl: instanceUrl, token: token, cookieHeader: options.cookieHeader, assertion: assertion
            )
        case .cancelled: return .failure(StepUpRefused(code: StepUp.cancelled))
        case .failed: return .failure(StepUpRefused(code: "PASSKEY_FAILED"))
        }
    }

    private struct Credentials {
        let instanceUrl: String
        let token: String
    }

    private func credentials(_ deps: AppDependencies) -> Credentials? {
        guard let account = deps.auth.accounts.first(where: { $0.id == accountId }),
              let token = account.token else { return nil }
        return Credentials(instanceUrl: account.instanceUrl, token: token)
    }
}

/// THE "Confirm it's you" sheet (fixtures/step-up.json layout, web
/// `StepUpDialog`, Android `StepUpSheet`): the method's one line, its field,
/// the inline error, the other methods as text buttons, the one primary
/// button. Swipe-down is every sheet's cancel.
struct StepUpSheet: View {
    @Bindable var model: StepUpModel

    @Environment(AppDependencies.self) private var deps
    @FocusState private var focused: Bool

    var body: some View {
        let method = model.state.method ?? .passkey
        let busy = model.state.sheet == .busy
        GlassSheetChrome(title: StepUp.title) {
            VStack(alignment: .leading, spacing: 12) {
                Text(method.description(email: model.email))
                    .font(.expFootnote)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .fixedSize(horizontal: false, vertical: true)

                if let field = method.field {
                    Group {
                        if method == .email {
                            GlassTextField(field, text: $model.secret, accessibilityIdentifier: "step-up-field")
                                .textContentType(.oneTimeCode)
                                .keyboardType(.numberPad)
                        } else {
                            GlassTextField(
                                field, text: $model.secret, isSecure: true, accessibilityIdentifier: "step-up-field"
                            )
                            .textContentType(.password)
                        }
                    }
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .focused($focused)
                    .disabled(busy)
                    .onSubmit { Task { await model.submit(deps: deps) } }
                    .overlay { invalidBorder }
                }

                // FEED-79: the refused proof is INPUT, inline under its field.
                if let error = model.state.error {
                    Text(error)
                        .font(.exp(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("step-up-error")
                }

                let others = model.state.methods.filter { $0 != method }
                if method == .email || !others.isEmpty {
                    HStack(spacing: 16) {
                        if method == .email {
                            actionLink(StepUp.resend) { Task { await model.sendCode(deps: deps) } }
                        }
                        ForEach(others, id: \.self) { other in
                            actionLink(other.switchLabel) {
                                Task {
                                    await model.choose(other, deps: deps)
                                    focused = other != .passkey
                                }
                            }
                            .accessibilityIdentifier("step-up-switch-\(other.rawValue)")
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .center)
                    .disabled(busy)
                }
            }
            .padding(16)
        } primaryAction: {
            GlassSubmitButton(
                method == .passkey ? StepUp.passkeyButton : StepUp.confirm,
                enabled: method == .passkey || !model.secret.trimmingCharacters(in: .whitespaces).isEmpty,
                loading: busy
            ) {
                Task { await model.submit(deps: deps) }
            }
            .accessibilityIdentifier("step-up-confirm")
        }
        .interactiveDismissDisabled(busy)
        .onAppear { focused = method != .passkey }
    }

    @ViewBuilder private var invalidBorder: some View {
        if model.state.error != nil {
            RoundedRectangle(cornerRadius: DesignTokens.Radius.md)
                .strokeBorder(DesignTokens.Semantic.red, lineWidth: 1)
        }
    }

    private func actionLink(_ label: String, action: @escaping () -> Void) -> some View {
        Button(label, action: action)
            .font(.expFootnote)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
    }
}
