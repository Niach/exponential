import ExpCore
import ExpUI
import SwiftUI

struct LoginView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.openURL) private var openURL
    @State private var viewModel: LoginViewModel?
    @FocusState private var focusedField: Field?

    private enum Field: Hashable {
        case email, password
    }

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    HStack {
                        // Free-standing (no toolbar row to grow the target),
                        // so the 32pt circle rides inside a 44pt hit area.
                        CircleIconButton(AppIcons.uiChevronLeft, accessibilityLabel: "Back") {
                            viewModel?.goBack()
                        }
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                        Spacer()
                    }

                    Spacer().frame(height: 32)

                    Text("Sign in")
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

                        if (config.appleLoginEnabled || config.googleLoginEnabled || !config.oidcProviders.isEmpty) && config.passwordEnabled {
                            divider
                        }

                        if config.passwordEnabled {
                            passwordForm(vm)
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
                "Sign in",
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
