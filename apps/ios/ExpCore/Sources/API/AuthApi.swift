import Foundation

// MARK: - Models

public struct AuthConfig: Codable, Sendable {
    public let passwordEnabled: Bool
    public let oidcProviders: [OidcProvider]
    public let googleLoginEnabled: Bool
    public let googleCalendarEnabled: Bool

    public init(
        passwordEnabled: Bool = true,
        oidcProviders: [OidcProvider] = [],
        googleLoginEnabled: Bool = false,
        googleCalendarEnabled: Bool = false
    ) {
        self.passwordEnabled = passwordEnabled
        self.oidcProviders = oidcProviders
        self.googleLoginEnabled = googleLoginEnabled
        self.googleCalendarEnabled = googleCalendarEnabled
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

    public init(id: String, email: String, name: String? = nil, isAdmin: Bool? = nil) {
        self.id = id
        self.email = email
        self.name = name
        self.isAdmin = isAdmin
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
        guard let baseUrl = auth.accounts.first(where: { $0.id == accountId })?.instanceUrl,
              let url = URL(string: "\(baseUrl)/api/auth/get-session") else {
            return nil
        }
        do {
            let (data, response) = try await httpClient.get(url, accountId: accountId)
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
