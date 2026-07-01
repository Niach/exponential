import Foundation

// MARK: - Models

public struct AuthConfig: Codable, Sendable {
    public let passwordEnabled: Bool
    public let oidcProviders: [OidcProvider]
    public let googleLoginEnabled: Bool

    public init(
        passwordEnabled: Bool = true,
        oidcProviders: [OidcProvider] = [],
        googleLoginEnabled: Bool = false
    ) {
        self.passwordEnabled = passwordEnabled
        self.oidcProviders = oidcProviders
        self.googleLoginEnabled = googleLoginEnabled
    }
}

public struct OidcProvider: Codable, Sendable, Identifiable {
    public let id: String
    public let name: String

    public init(id: String, name: String) {
        self.id = id
        self.name = name
    }
}

public struct AuthUser: Codable, Sendable {
    public let id: String
    public let email: String
    public let name: String?
    public let isAdmin: Bool?
    // better-auth additionalField (type date, input:false) — returned on
    // session reads as an ISO string or null, exactly like the web gate.
    public let onboardingCompletedAt: String?

    public init(id: String, email: String, name: String? = nil, isAdmin: Bool? = nil, onboardingCompletedAt: String? = nil) {
        self.id = id
        self.email = email
        self.name = name
        self.isAdmin = isAdmin
        self.onboardingCompletedAt = onboardingCompletedAt
    }
}

public enum SignInResult: Sendable {
    case success(token: String, user: AuthUser)
    case failure(message: String)
}

// MARK: - API

public final class AuthApi: Sendable {
    private let httpClient: HTTPClient
    private let auth: AuthRepository

    public init(httpClient: HTTPClient, auth: AuthRepository) {
        self.httpClient = httpClient
        self.auth = auth
    }

    public func signInWithPassword(instanceUrl: String, email: String, password: String) async -> SignInResult {
        guard let url = URL(string: "\(instanceUrl)/api/auth/sign-in/email") else {
            return .failure(message: "Invalid instance URL")
        }

        do {
            let body = try JSONEncoder().encode(["email": email, "password": password])
            let (data, response) = try await httpClient.postUnauthenticated(url, body: body)

            guard (200...299).contains(response.statusCode) else {
                let text = String(data: data, encoding: .utf8) ?? ""
                return .failure(message: "HTTP \(response.statusCode): \(text)")
            }

            let parsed = try JSONDecoder().decode(SignInResponseBody.self, from: data)

            // Better Auth bearer plugin returns { token, user }
            if let token = parsed.token, let user = parsed.user {
                return .success(token: token, user: user)
            }

            // Fallback: extract session token from Set-Cookie header
            if let user = parsed.user,
               let cookies = response.value(forHTTPHeaderField: "Set-Cookie"),
               let range = cookies.range(of: #"session_token=([^;]+)"#, options: .regularExpression),
               let tokenRange = cookies[range].range(of: "=") {
                let token = String(cookies[tokenRange.upperBound...].prefix(while: { $0 != ";" }))
                return .success(token: token, user: user)
            }

            return .failure(message: "Sign-in succeeded but no session token returned")
        } catch {
            return .failure(message: error.localizedDescription)
        }
    }

    public func fetchAuthConfig(instanceUrl: String) async throws -> AuthConfig {
        guard let url = URL(string: "\(instanceUrl)/api/auth-config") else {
            throw HTTPError.invalidResponse
        }
        let (data, response) = try await httpClient.getUnauthenticated(url)
        guard (200...299).contains(response.statusCode) else {
            throw HTTPError.httpError(response.statusCode, String(data: data, encoding: .utf8) ?? "")
        }
        return try JSONDecoder().decode(AuthConfig.self, from: data)
    }

    public func fetchSession(accountId: String) async -> AuthUser? {
        guard let account = auth.accounts.first(where: { $0.id == accountId }) else { return nil }
        return await fetchSession(instanceUrl: account.instanceUrl, token: account.token)
    }

    // Core session read. Takes instanceUrl + token explicitly so a login flow
    // can capture session fields (incl. onboardingCompletedAt) BEFORE persisting
    // the token, avoiding any window where the account looks "not onboarded".
    public func fetchSession(instanceUrl: String, token: String?) async -> AuthUser? {
        guard let url = URL(string: "\(instanceUrl)/api/auth/get-session") else { return nil }
        do {
            let (data, response) = try await httpClient.get(url, bearerToken: token)
            guard (200...299).contains(response.statusCode) else { return nil }
            let session = try JSONDecoder().decode(SessionResponse.self, from: data)
            return session.user
        } catch {
            return nil
        }
    }

    public func oauthStartUrl(instanceUrl: String, providerId: String) -> URL? {
        let encoded = providerId.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? providerId
        return URL(string: "\(instanceUrl)/api/mobile-oauth-start?providerId=\(encoded)")
    }

    public func googleStartUrl(instanceUrl: String) -> URL? {
        URL(string: "\(instanceUrl)/api/mobile-oauth-start?provider=google")
    }
}

// MARK: - Response Types

private struct SignInResponseBody: Codable {
    let token: String?
    let user: AuthUser?
}

private struct SessionResponse: Codable {
    let user: AuthUser?
}
