import AuthenticationServices
import ExpCore
import UIKit

/// EXP-1596: the on-device half of Add passkey (web `authClient.passkey.addPasskey`,
/// Android `ServerDetailViewModel.addPasskey`). Runs the platform
/// authenticator's registration against the served creation options and hands
/// back the raw attestation, or nil when the user dismissed the system sheet
/// (a cancel is never an error). The login screen's assertion ceremony
/// (`LoginViewModel.startPasskeyLogin`) is the sign-in twin.
@MainActor
final class PasskeyRegistrationCeremony: NSObject,
    ASAuthorizationControllerDelegate, ASAuthorizationControllerPresentationContextProviding {
    enum Outcome {
        case created(PasskeyAttestation)
        case cancelled
        case failed(String)
    }

    // ASAuthorizationController does not retain itself while its sheet is up.
    private var controller: ASAuthorizationController?
    private var continuation: CheckedContinuation<Outcome, Never>?

    func register(_ options: PasskeyRegistrationOptions) async -> Outcome {
        continuation?.resume(returning: .cancelled)
        let provider = ASAuthorizationPlatformPublicKeyCredentialProvider(
            relyingPartyIdentifier: options.rpId
        )
        let request = provider.createCredentialRegistrationRequest(
            challenge: options.challenge,
            name: options.userName,
            userID: options.userId
        )
        if !options.excludedCredentialIds.isEmpty {
            request.excludedCredentials = options.excludedCredentialIds.map {
                ASAuthorizationPlatformPublicKeyCredentialDescriptor(credentialID: $0)
            }
        }
        let controller = ASAuthorizationController(authorizationRequests: [request])
        controller.delegate = self
        controller.presentationContextProvider = self
        self.controller = controller
        return await withCheckedContinuation { continuation in
            self.continuation = continuation
            controller.performRequests()
        }
    }

    private func finish(_ outcome: Outcome) {
        controller = nil
        let continuation = continuation
        self.continuation = nil
        continuation?.resume(returning: outcome)
    }

    // MARK: - ASAuthorizationControllerPresentationContextProviding

    nonisolated func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap(\.windows)
                .first { $0.isKeyWindow } ?? ASPresentationAnchor()
        }
    }

    // MARK: - ASAuthorizationControllerDelegate

    nonisolated func authorizationController(
        controller: ASAuthorizationController,
        didCompleteWithAuthorization authorization: ASAuthorization
    ) {
        // Reduce the non-Sendable credential to raw bytes before the hop.
        guard let registration = authorization.credential
            as? ASAuthorizationPlatformPublicKeyCredentialRegistration,
            let attestationObject = registration.rawAttestationObject else {
            Task { @MainActor in self.finish(.failed("")) }
            return
        }
        let attestation = PasskeyAttestation(
            credentialId: registration.credentialID,
            clientDataJSON: registration.rawClientDataJSON,
            attestationObject: attestationObject,
            attachment: "platform"
        )
        Task { @MainActor in self.finish(.created(attestation)) }
    }

    nonisolated func authorizationController(
        controller: ASAuthorizationController,
        didCompleteWithError error: Error
    ) {
        let cancelled = (error as? ASAuthorizationError)?.code == .canceled
        let description = error.localizedDescription
        Task { @MainActor in
            self.finish(cancelled ? .cancelled : .failed(description))
        }
    }
}
