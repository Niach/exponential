import Foundation

// MARK: - Models

public struct AuthConfig: Codable, Sendable {
    public let passwordEnabled: Bool
    public let oidcProviders: [OidcProvider]
    public let googleLoginEnabled: Bool
    public let appleLoginEnabled: Bool

    public init(
        passwordEnabled: Bool = true,
        oidcProviders: [OidcProvider] = [],
        googleLoginEnabled: Bool = false,
        appleLoginEnabled: Bool = false
    ) {
        self.passwordEnabled = passwordEnabled
        self.oidcProviders = oidcProviders
        self.googleLoginEnabled = googleLoginEnabled
        self.appleLoginEnabled = appleLoginEnabled
    }

    // appleLoginEnabled is absent from pre-SIWA servers (self-hosted lag) —
    // decode it as optional so the login screen keeps working against them.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        passwordEnabled = try c.decode(Bool.self, forKey: .passwordEnabled)
        oidcProviders = try c.decode([OidcProvider].self, forKey: .oidcProviders)
        googleLoginEnabled = try c.decode(Bool.self, forKey: .googleLoginEnabled)
        appleLoginEnabled = try c.decodeIfPresent(Bool.self, forKey: .appleLoginEnabled) ?? false
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
                // Rendered verbatim on the login screen — show Better Auth's
                // `message` field, never the raw response body (EXP-219).
                return .failure(message: Self.authErrorMessage(from: data)
                    ?? "Sign-in failed (HTTP \(response.statusCode))")
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

    /// Native Sign in with Apple: exchange the on-device ASAuthorization
    /// identityToken for a Better Auth session via the social sign-in endpoint.
    /// The `nonce` (when present) must be the SAME raw string the SIWA request
    /// was created with — Apple embeds the request nonce verbatim in the idToken
    /// claim and Better Auth's apple provider compares raw equality. Fails
    /// gracefully so the caller can fall back to the web OAuth hop (self-hosted
    /// / pre-SIWA servers reject the native exchange).
    public func signInWithApple(instanceUrl: String, identityToken: String, nonce: String?) async -> SignInResult {
        guard let url = URL(string: "\(instanceUrl)/api/auth/sign-in/social") else {
            return .failure(message: "Invalid instance URL")
        }

        do {
            let body = try JSONEncoder().encode(
                AppleSignInInput(provider: "apple", idToken: AppleIdToken(token: identityToken, nonce: nonce))
            )
            let (data, response) = try await httpClient.postUnauthenticated(url, body: body)

            guard (200...299).contains(response.statusCode) else {
                return .failure(message: Self.authErrorMessage(from: data)
                    ?? "Sign-in failed (HTTP \(response.statusCode))")
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

    /// Best-effort push of the user's name to Better Auth's core update-user
    /// endpoint (bearer-authenticated). Used right after a native SIWA sign-in
    /// to persist the `fullName` Apple delivers ONLY in the on-device credential
    /// (never in the idToken). Returns whether the write succeeded; callers
    /// never surface a failure.
    public func updateUserName(instanceUrl: String, token: String, name: String) async -> Bool {
        guard let url = URL(string: "\(instanceUrl)/api/auth/update-user") else { return false }
        do {
            let body = try JSONEncoder().encode(["name": name])
            let (_, response) = try await httpClient.post(url, body: body, bearerToken: token)
            return (200...299).contains(response.statusCode)
        } catch {
            return false
        }
    }

    /// `POST /api/auth/sign-out` — best-effort server-side session revocation
    /// (desktop `login.rs` parity). Without it a leaked bearer token would
    /// survive local sign-out for the full 60-day sliding session expiry.
    /// Local sign-out must proceed even when this fails (offline sign-out is
    /// legal), so the call is timeout-capped and never throws; callers await it
    /// AFTER the push-token unregister (which still needs a live session) and
    /// BEFORE the token is dropped locally.
    @discardableResult
    public func signOut(instanceUrl: String, token: String) async -> Bool {
        guard let url = URL(string: "\(instanceUrl)/api/auth/sign-out") else { return false }
        do {
            let (_, response) = try await httpClient.post(
                url, body: Data("{}".utf8), bearerToken: token, timeout: 3
            )
            return (200...299).contains(response.statusCode)
        } catch {
            return false
        }
    }

    /// Extract the user-presentable `message` from a Better Auth error body
    /// (`{"code": "...", "message": "Invalid email or password"}`).
    private static func authErrorMessage(from data: Data) -> String? {
        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let message = root["message"] as? String, !message.isEmpty else { return nil }
        return message
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

    /// Fetch the session, retrying briefly. A login must resolve a stable
    /// userId before its token is persisted — per-user account identity (and the
    /// local DB file) is keyed on it, so a nil userId would strand the account.
    public func fetchSessionRetrying(
        instanceUrl: String, token: String, attempts: Int = 3, delayMs: UInt64 = 500
    ) async -> AuthUser? {
        for attempt in 0..<max(1, attempts) {
            if let user = await fetchSession(instanceUrl: instanceUrl, token: token), !user.id.isEmpty {
                return user
            }
            if attempt < attempts - 1 {
                try? await Task.sleep(nanoseconds: delayMs * 1_000_000)
            }
        }
        return nil
    }

    // OAuth start URLs carry the attempt's PKCE S256 code_challenge (REV-13,
    // base64url — URL-safe as-is): the server's return page then deep-links a
    // single-use code instead of the raw session token; the view model redeems
    // it via exchangeOauthCode with the in-memory verifier.

    public func oauthStartUrl(instanceUrl: String, providerId: String, codeChallenge: String) -> URL? {
        let encoded = providerId.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? providerId
        return URL(string: "\(instanceUrl)/api/mobile-oauth-start?providerId=\(encoded)&code_challenge=\(codeChallenge)")
    }

    public func googleStartUrl(instanceUrl: String, codeChallenge: String) -> URL? {
        URL(string: "\(instanceUrl)/api/mobile-oauth-start?provider=google&code_challenge=\(codeChallenge)")
    }

    public func appleStartUrl(instanceUrl: String, codeChallenge: String) -> URL? {
        URL(string: "\(instanceUrl)/api/mobile-oauth-start?provider=apple&code_challenge=\(codeChallenge)")
    }

    /// Redeem an oauth-return PKCE code for the session token (REV-13):
    /// POST /api/mobile-oauth-exchange with the code from the callback URL and
    /// the in-memory verifier the attempt started with. Nil on any failure
    /// (unknown/expired/replayed code, wrong verifier, network) — the caller
    /// surfaces a login error.
    public func exchangeOauthCode(instanceUrl: String, code: String, codeVerifier: String) async -> String? {
        guard let url = URL(string: "\(instanceUrl)/api/mobile-oauth-exchange") else { return nil }
        do {
            let body = try JSONEncoder().encode(["code": code, "code_verifier": codeVerifier])
            let (data, response) = try await httpClient.postUnauthenticated(url, body: body)
            guard (200...299).contains(response.statusCode) else { return nil }
            let parsed = try JSONDecoder().decode(OauthExchangeResponseBody.self, from: data)
            return parsed.token
        } catch {
            return nil
        }
    }
}

// MARK: - Request Types

// Native SIWA exchange body: { provider: "apple", idToken: { token, nonce? } }.
// The synthesized encoder omits `nonce` when nil (optional → encodeIfPresent),
// matching Better Auth's optional nonce field.
private struct AppleIdToken: Encodable {
    let token: String
    let nonce: String?
}

private struct AppleSignInInput: Encodable {
    let provider: String
    let idToken: AppleIdToken
}

// MARK: - Response Types

private struct SignInResponseBody: Codable {
    let token: String?
    let user: AuthUser?
}

private struct SessionResponse: Codable {
    let user: AuthUser?
}

private struct OauthExchangeResponseBody: Codable {
    let token: String?
}
