import ExpCore
import ExpUI
import SwiftUI

struct MacLoginView: View {
    @Environment(MacAppDependencies.self) private var deps
    @State private var vm: MacLoginViewModel?

    var body: some View {
        VStack(spacing: 20) {
            Text("Exponential")
                .font(.largeTitle.weight(.bold))

            if let vm {
                if deps.auth.hasInstance {
                    loginContent(vm)
                } else {
                    instancePicker(vm)
                }
                if let error = vm.error {
                    Text(error)
                        .font(.callout)
                        .foregroundStyle(.red)
                        .multilineTextAlignment(.center)
                }
            } else {
                ProgressView()
            }
        }
        .padding(40)
        .frame(maxWidth: 420)
        .onAppear {
            if vm == nil {
                let model = MacLoginViewModel(authApi: deps.authApi, auth: deps.auth)
                vm = model
                if deps.auth.hasInstance {
                    Task { await model.loadConfig() }
                }
            }
        }
    }

    // MARK: - Instance picker

    @ViewBuilder
    private func instancePicker(_ vm: MacLoginViewModel) -> some View {
        VStack(spacing: 14) {
            Text("Connect to a server")
                .foregroundStyle(.secondary)

            Button {
                vm.chooseInstance(AppConstants.defaultCloudUrl)
            } label: {
                Text(AppConstants.isStaging ? "Connect to Staging Cloud" : "Connect to Cloud")
                    .frame(maxWidth: .infinity)
            }
            .controlSize(.large)
            .buttonStyle(.borderedProminent)
            .tint(Accent.indigo)

            orSeparator

            TextField("https://your-instance.example.com", text: Binding(
                get: { vm.customInstance },
                set: { vm.customInstance = $0 }
            ))
            .textFieldStyle(.roundedBorder)
            .onSubmit { vm.chooseInstance(vm.customInstance) }

            Button("Continue") { vm.chooseInstance(vm.customInstance) }
                .disabled(vm.customInstance.trimmingCharacters(in: .whitespaces).isEmpty)
        }
    }

    // MARK: - Login form

    @ViewBuilder
    private func loginContent(_ vm: MacLoginViewModel) -> some View {
        VStack(spacing: 14) {
            if let host = deps.auth.instanceUrl {
                Text(host)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            if vm.configLoading {
                ProgressView()
            } else if let configError = vm.configError {
                Text(configError).foregroundStyle(.red)
                Button("Retry") { Task { await vm.loadConfig() } }
            } else if let config = vm.config {
                if config.googleLoginEnabled {
                    oauthButton("Continue with Google") { vm.startGoogle() }
                }
                ForEach(config.oidcProviders) { provider in
                    oauthButton("Continue with \(provider.name)") { vm.startOAuth(providerId: provider.id) }
                }

                if config.passwordEnabled {
                    if config.googleLoginEnabled || !config.oidcProviders.isEmpty {
                        orSeparator
                    }
                    TextField("Email", text: Binding(get: { vm.email }, set: { vm.email = $0 }))
                        .textFieldStyle(.roundedBorder)
                        .textContentType(.username)
                    SecureField("Password", text: Binding(get: { vm.password }, set: { vm.password = $0 }))
                        .textFieldStyle(.roundedBorder)
                        .textContentType(.password)
                        .onSubmit { Task { await vm.signIn() } }
                    Button {
                        Task { await vm.signIn() }
                    } label: {
                        Text("Sign In").frame(maxWidth: .infinity)
                    }
                    .controlSize(.large)
                    .buttonStyle(.borderedProminent)
                    .tint(Accent.indigo)
                    .disabled(vm.loading || vm.email.isEmpty || vm.password.isEmpty)
                }
            }

            Button("Use a different server") { vm.goBackToInstance() }
                .buttonStyle(.link)
                .padding(.top, 4)
        }
    }

    @ViewBuilder
    private func oauthButton(_ label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label).frame(maxWidth: .infinity)
        }
        .controlSize(.large)
    }

    /// Horizontal "──── or ────" separator. Uses fixed-height rules rather than a
    /// `Divider()` in an `HStack` — that renders a *vertical* rule with unbounded
    /// height that stretches the whole form apart.
    private var orSeparator: some View {
        HStack(spacing: 10) {
            Rectangle().fill(Color.secondary.opacity(0.25)).frame(height: 1)
            Text("or").font(.caption).foregroundStyle(.secondary).fixedSize()
            Rectangle().fill(Color.secondary.opacity(0.25)).frame(height: 1)
        }
        .frame(maxWidth: 280)
        .padding(.vertical, 2)
    }
}
