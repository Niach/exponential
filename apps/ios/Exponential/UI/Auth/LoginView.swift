import ExpUI
import SwiftUI

struct LoginView: View {
    @Environment(AppDependencies.self) private var deps
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
                        Button {
                            viewModel?.goBack()
                        } label: {
                            Image(systemName: "chevron.left")
                                .font(.body.weight(.medium))
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .padding(8)
                                .glassButton()
                        }
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
                            oauthButton(label: "Continue with Apple", action: {
                                vm.startAppleOAuthFlow()
                            }) {
                                Image(systemName: "apple.logo")
                                    .font(.body.weight(.medium))
                            }
                        }

                        if config.googleLoginEnabled {
                            oauthButton(label: "Continue with Google", action: {
                                vm.startGoogleOAuthFlow()
                            }) {
                                // SF Symbols has no Google mark — the official
                                // multi-color G is drawn in GoogleLogoMark.
                                GoogleLogoMark()
                                    .frame(width: 17, height: 17)
                            }
                        }

                        ForEach(config.oidcProviders) { provider in
                            oauthButton(label: "Continue with \(provider.name)", action: {
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

    @ViewBuilder
    private func oauthButton(label: String, action: @escaping () -> Void, @ViewBuilder icon: () -> some View) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                icon()
                Text(label)
            }
            .font(.body.weight(.medium))
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 14)
        }
        .background(Color.white.opacity(0.08))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.white.opacity(0.15), lineWidth: 0.5)
        )
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
            glassTextField("Email", text: Binding(
                get: { vm.email },
                set: { vm.email = $0 }
            ), keyboardType: .emailAddress, accessibilityIdentifier: "login-email-field")
                .focused($focusedField, equals: .email)
                .onSubmit { focusedField = .password }

            glassTextField("Password", text: Binding(
                get: { vm.password },
                set: { vm.password = $0 }
            ), isSecure: true, accessibilityIdentifier: "login-password-field")
                .focused($focusedField, equals: .password)
                .onSubmit {
                    Task { await vm.signIn() }
                }

            Button {
                Task { await vm.signIn() }
            } label: {
                Group {
                    if vm.loading {
                        ProgressView()
                            .tint(.white)
                    } else {
                        Text("Sign in")
                    }
                }
                .font(.body.weight(.medium))
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
            }
            .disabled(vm.loading || vm.email.isEmpty || vm.password.isEmpty)
            .accessibilityIdentifier("login-submit-button")
            .background(
                (vm.email.isEmpty || vm.password.isEmpty || vm.loading)
                    ? Color.white.opacity(0.06)
                    : Color.white.opacity(0.15)
            )
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
            )
        }
    }

    @ViewBuilder
    private func glassTextField(_ placeholder: String, text: Binding<String>, keyboardType: UIKeyboardType = .default, isSecure: Bool = false, accessibilityIdentifier: String = "") -> some View {
        Group {
            if isSecure {
                // Under -uiTesting (fastlane snapshot), use a PLAIN field: the
                // system "Save Password?" sheet only triggers on secure text
                // entry, appears seconds later at an unbeatable moment, and is
                // hosted outside the app so XCUITest cannot reliably dismiss
                // it (it photobombed the store shots repeatedly). No secure
                // field ⇒ no sheet. Real users always get the SecureField.
                if ProcessInfo.processInfo.arguments.contains("-uiTesting") {
                    TextField(placeholder, text: text)
                } else {
                    SecureField(placeholder, text: text)
                }
            } else {
                TextField(placeholder, text: text)
                    .keyboardType(keyboardType)
            }
        }
        .accessibilityIdentifier(accessibilityIdentifier)
        .textFieldStyle(.plain)
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(Color.white.opacity(0.06))
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.white.opacity(0.1), lineWidth: 0.5)
        )
        .foregroundStyle(.white)
    }
}
