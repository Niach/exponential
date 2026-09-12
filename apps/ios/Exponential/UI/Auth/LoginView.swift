import ExpCore
import ExpUI
import SwiftUI

struct LoginView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.openURL) private var openURL
    @State private var viewModel: LoginViewModel?
    @FocusState private var focusedField: Field?

    private enum Field: Hashable {
        case email, password, code
    }

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    HStack {
                        GhostIconButton(AppIcons.uiChevronLeft, accessibilityLabel: "Back") {
                            viewModel?.goBack()
                        }
                        .frame(width: 44, height: 44)
                        Spacer()
                    }

                    Spacer().frame(height: 32)

                    // EXP-857: one title across the four clients, and no button
                    // on this screen says "Sign in" any more — signing in and
                    // signing up are the same act here.
                    Text("Continue to Exponential")
                        .font(.system(size: 28, weight: .bold))
                        .foregroundStyle(.white)

                    if let instanceUrl = deps.auth.instanceUrl {
                        Text(instanceUrl)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }

                    Spacer().frame(height: 24)

                    if let vm = viewModel {
                        loginContent(vm)
                    }
                }
                .padding(.horizontal, 32)
                .padding(.top, 16)
            }
        }
        .onAppear {
            if viewModel == nil {
                viewModel = LoginViewModel(authApi: deps.authApi, auth: deps.auth)
            }
            Task {
                await viewModel?.loadConfig()
            }
        }
    }

    @ViewBuilder
    private func loginContent(_ vm: LoginViewModel) -> some View {
        VStack(spacing: 16) {
            if vm.configLoading {
                ProgressView()
                    .tint(.white)
                    .frame(maxWidth: .infinity, alignment: .center)
            } else if let configError = vm.configError {
                Text(configError)
                    .font(.callout)
                    .foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .center)
            } else {
                VStack(spacing: 16) {
                    // OAuth providers. Sign in with Apple leads — App Store
                    // guideline 4.8 requires it alongside Google, and the HIG
                    // wants it placed no less prominently than other options.
                    if let config = vm.config {
                        if config.appleLoginEnabled {
                            GlassOAuthButton("Continue with Apple", action: {
                                vm.startAppleOAuthFlow()
                            }) {
                                // Apple's brand mark, not a registry glyph:
                                // Lucide has no Apple logo and SIWA must use
                                // Apple's own art (App Store guideline 4.8).
                                Image(systemName: "apple.logo")
                                    .font(.body.weight(.medium))
                            }
                        }

                        if config.googleLoginEnabled {
                            GlassOAuthButton("Continue with Google", action: {
                                vm.startGoogleOAuthFlow()
                            }) {
                                // SF Symbols has no Google mark — the official
                                // multi-color G is drawn in GoogleLogoMark.
                                GoogleLogoMark()
                                    .frame(width: 17, height: 17)
                            }
                        }

                        ForEach(config.oidcProviders) { provider in
                            GlassOAuthButton("Continue with \(provider.name)", action: {
                                vm.startOAuthFlow(providerId: provider.id)
                            }) {
                                EmptyView()
                            }
                        }

                        // EXP-857: the email branch is one more button in the
                        // same stack; it opens the code (or password) step in
                        // place instead of leading with a form.
                        if (config.emailOtpEnabled || config.passwordEnabled), vm.emailStep == .hidden {
                            GlassOAuthButton("Continue with email", action: {
                                vm.showEmailStep()
                            }) {
                                AppIcon(AppIcons.uiMail, size: AppIcon.Size.medium)
                            }
                            .accessibilityIdentifier("login-continue-with-email-button")
                        }

                        if config.passkeyEnabled {
                            GlassOAuthButton("Login with passkey", action: {
                                vm.startPasskeyLogin()
                            }) {
                                AppIcon(AppIcons.authPasskey, size: AppIcon.Size.medium)
                            }
                            .accessibilityIdentifier("login-passkey-button")
                        }

                        if vm.emailStep != .hidden {
                            divider
                            if vm.usesCodeFlow {
                                emailCodeForm(vm, config: config)
                            } else if config.passwordEnabled {
                                passwordForm(vm)
                            }
                        }
                    }
                }
                .padding(24)
                .glassCard()

                if let error = vm.error {
                    Text(error)
                        .font(.callout)
                        .foregroundStyle(.red)
                        .padding(.horizontal, 4)
                }
            }
        }
    }

    private var divider: some View {
        HStack {
            Rectangle()
                .fill(Color.white.opacity(0.1))
                .frame(height: 0.5)
            Text("or")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Rectangle()
                .fill(Color.white.opacity(0.1))
                .frame(height: 0.5)
        }
    }

    /// The one-time code branch (EXP-857): address first, then the mailed code.
    /// Both steps live in the same card the buttons are in.
    @ViewBuilder
    private func emailCodeForm(_ vm: LoginViewModel, config: AuthConfig) -> some View {
        VStack(spacing: 12) {
            if vm.emailStep == .code {
                Text("We sent a 6-digit code to \(vm.codeSentTo ?? vm.email).")
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .frame(maxWidth: .infinity, alignment: .leading)

                GlassTextField("Code", text: Binding(
                    get: { vm.code },
                    set: { vm.code = $0 }
                ), accessibilityIdentifier: "login-code-field")
                    .textContentType(.oneTimeCode)
                    .keyboardType(.numberPad)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .focused($focusedField, equals: .code)
                    .onSubmit {
                        Task { await vm.verifyCode() }
                    }

                GlassSubmitButton(
                    vm.verifyingCode ? "Checking…" : "Continue",
                    enabled: !vm.code.isEmpty,
                    loading: vm.verifyingCode
                ) {
                    Task { await vm.verifyCode() }
                }
                .accessibilityIdentifier("login-verify-code-button")

                HStack(spacing: 16) {
                    actionLink("Resend code", identifier: "login-resend-code-link") {
                        Task { await vm.resendCode() }
                    }
                    actionLink("Use a different email", identifier: "login-change-email-link") {
                        focusedField = .email
                        vm.changeEmail()
                    }
                }
                .frame(maxWidth: .infinity, alignment: .center)
            } else {
                GlassTextField("Email", text: Binding(
                    get: { vm.email },
                    set: { vm.email = $0 }
                ), accessibilityIdentifier: "login-email-field")
                    .keyboardType(.emailAddress)
                    .textContentType(.emailAddress)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .focused($focusedField, equals: .email)
                    .onSubmit {
                        Task { await vm.sendCode() }
                    }

                GlassSubmitButton(
                    vm.sendingCode ? "Sending code…" : "Send code",
                    enabled: !vm.email.isEmpty,
                    loading: vm.sendingCode
                ) {
                    Task { await vm.sendCode() }
                }
                .accessibilityIdentifier("login-send-code-button")

                if config.passwordEnabled {
                    actionLink("Use a password instead", identifier: "login-use-password-link") {
                        vm.usePassword()
                    }
                    .frame(maxWidth: .infinity, alignment: .center)
                }
            }
        }
    }

    @ViewBuilder
    private func passwordForm(_ vm: LoginViewModel) -> some View {
        VStack(spacing: 12) {
            GlassTextField("Email", text: Binding(
                get: { vm.email },
                set: { vm.email = $0 }
            ), accessibilityIdentifier: "login-email-field")
                .keyboardType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .focused($focusedField, equals: .email)
                .onSubmit { focusedField = .password }

            GlassTextField("Password", text: Binding(
                get: { vm.password },
                set: { vm.password = $0 }
            ), isSecure: true, accessibilityIdentifier: "login-password-field")
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .focused($focusedField, equals: .password)
                .onSubmit {
                    Task { await vm.signIn() }
                }

            GlassSubmitButton(
                "Continue",
                enabled: !vm.email.isEmpty && !vm.password.isEmpty,
                loading: vm.loading
            ) {
                Task { await vm.signIn() }
            }
            .accessibilityIdentifier("login-submit-button")

            // Sign-up and password reset are web flows on every native client
            // (desktop parity) — hand off to the browser, and only for what the
            // server publishes as available.
            if let config = vm.config, config.passwordResetEnabled || config.signupEnabled {
                let instanceUrl = deps.auth.instanceUrl
                HStack(spacing: 16) {
                    if config.passwordResetEnabled,
                       let url = AuthApi.forgotPasswordUrl(instanceUrl: instanceUrl) {
                        webLink("Forgot password?", url: url, identifier: "login-forgot-password-link")
                    }
                    if config.signupEnabled,
                       let url = AuthApi.registerUrl(instanceUrl: instanceUrl) {
                        webLink("Create account", url: url, identifier: "login-create-account-link")
                    }
                }
                .frame(maxWidth: .infinity, alignment: .center)
            }
        }
    }

    /// An in-app text link (the web ones below leave for the browser).
    @ViewBuilder
    private func actionLink(_ label: String, identifier: String, action: @escaping () -> Void) -> some View {
        Button(label, action: action)
            .font(.footnote)
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .accessibilityIdentifier(identifier)
    }

    @ViewBuilder
    private func webLink(_ label: String, url: URL, identifier: String) -> some View {
        Button(label) {
            openURL(url)
        }
        .font(.footnote)
        .foregroundStyle(.white.opacity(TextOpacity.secondary))
        .accessibilityIdentifier(identifier)
    }

}
