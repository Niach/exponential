import Foundation

// MARK: - Models

public struct AuthConfig: Codable, Sendable {
    public let passwordEnabled: Bool
    // Password sign-up is open on this instance — gates the "Create account"
    // hand-off (server: buildAuthConfig).
    public let signupEnabled: Bool
    // The instance can send mail — gates the "Forgot password?" hand-off.
    public let passwordResetEnabled: Bool
    public let oidcProviders: [OidcProvider]
    public let googleLoginEnabled: Bool
    public let appleLoginEnabled: Bool
    // One-time code login is offered (the instance can send mail) — EXP-857.
    public let emailOtpEnabled: Bool
    // Passkey login is offered.
    public let passkeyEnabled: Bool

    public init(
        passwordEnabled: Bool = true,
        signupEnabled: Bool = false,
        passwordResetEnabled: Bool = false,
        oidcProviders: [OidcProvider] = [],
        googleLoginEnabled: Bool = false,
        appleLoginEnabled: Bool = false,
        emailOtpEnabled: Bool = false,
        passkeyEnabled: Bool = false
    ) {
        self.passwordEnabled = passwordEnabled
        self.signupEnabled = signupEnabled
        self.passwordResetEnabled = passwordResetEnabled
        self.oidcProviders = oidcProviders
        self.googleLoginEnabled = googleLoginEnabled
        self.appleLoginEnabled = appleLoginEnabled
        self.emailOtpEnabled = emailOtpEnabled
        self.passkeyEnabled = passkeyEnabled
    }

    // The login screen is the FIRST thing to touch an arbitrary instance URL,
    // so the optional flags decode permissively and default to false: a
    // missing flag hides the affordance rather than offering a link the
    // server would dead-end, and never bricks the screen outright.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        passwordEnabled = try c.decode(Bool.self, forKey: .passwordEnabled)
        signupEnabled = try c.decodeIfPresent(Bool.self, forKey: .signupEnabled) ?? false
        passwordResetEnabled = try c.decodeIfPresent(Bool.self, forKey: .passwordResetEnabled) ?? false
        oidcProviders = try c.decode([OidcProvider].self, forKey: .oidcProviders)
        googleLoginEnabled = try c.decode(Bool.self, forKey: .googleLoginEnabled)
        appleLoginEnabled = try c.decodeIfPresent(Bool.self, forKey: .appleLoginEnabled) ?? false
        emailOtpEnabled = try c.decodeIfPresent(Bool.self, forKey: .emailOtpEnabled) ?? false
        passkeyEnabled = try c.decodeIfPresent(Bool.self, forKey: .passkeyEnabled) ?? false
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

/// The served WebAuthn assertion options, or why there are none. A failure is
/// never fatal on the login screen: it falls back to the browser handoff.
public enum PasskeyOptionsResult: Sendable {
    case success(PasskeyAuthenticationOptions)
    case failure(message: String)
}

/// What a `get-session` read established. `invalidated` is the only DEFINITIVE
/// dead-session verdict (see `AuthApi.classifySessionRead`); `indeterminate`
/// covers every "we don't know" case — offline, timeout, 5xx, garbled body — and
/// must never sign an account out.
enum SessionReadOutcome: Sendable {
    case user(AuthUser)
    case invalidated
    case indeterminate
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
               let token = Self.sessionTokenFromCookies(response) {
                return .success(token: token, user: user)
            }

            return .failure(message: "Sign-in succeeded but no session token returned")
        } catch {
            return .failure(message: error.userFacingMessage)
        }
    }

    /// Ask the instance to mail a 6-digit sign-in code (EXP-857). The server
    /// answers 200 whether or not the address is known — never surface an
    /// existence hint here.
    public func sendSignInCode(instanceUrl: String, email: String) async -> SendCodeResult {
        guard let url = URL(string: "\(instanceUrl)/api/auth/email-otp/send-verification-otp") else {
            return .failure(message: "Invalid instance URL")
        }
        do {
            let body = try JSONEncoder().encode(["email": email, "type": "sign-in"])
            let (data, response) = try await httpClient.postUnauthenticated(url, body: body)
            guard (200...299).contains(response.statusCode) else {
                if let message = Self.authErrorMessage(from: data) {
                    return .failure(message: message)
                }
                if response.statusCode == 429 {
                    return .failure(message: "Too many requests. Wait a minute and try again.")
                }
                return .failure(message: "Couldn't send the code (HTTP \(response.statusCode))")
            }
            return .success
        } catch {
            return .failure(message: error.userFacingMessage)
        }
    }

    /// Redeem a mailed one-time code for a session. Same `{token, user}` body
    /// (and Set-Cookie fallback) as the password sign-in.
    public func signInWithEmailCode(instanceUrl: String, email: String, code: String) async -> SignInResult {
        guard let url = URL(string: "\(instanceUrl)/api/auth/sign-in/email-otp") else {
            return .failure(message: "Invalid instance URL")
        }
        do {
            let body = try JSONEncoder().encode(["email": email, "otp": code])
            let (data, response) = try await httpClient.postUnauthenticated(url, body: body)

            guard (200...299).contains(response.statusCode) else {
                return .failure(message: EmailCodeCopy.message(
                    code: Self.authErrorCode(from: data),
                    serverMessage: Self.authErrorMessage(from: data)
                ))
            }

            let parsed = try JSONDecoder().decode(SignInResponseBody.self, from: data)
            if let token = parsed.token, let user = parsed.user {
                return .success(token: token, user: user)
            }
            if let user = parsed.user,
               let token = Self.authTokenHeader(response) ?? Self.sessionTokenFromCookies(response) {
                return .success(token: token, user: user)
            }
            return .failure(message: "Sign-in succeeded but no session token returned")
        } catch {
            return .failure(message: error.userFacingMessage)
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
               let token = Self.sessionTokenFromCookies(response) {
                return .success(token: token, user: user)
            }

            return .failure(message: "Sign-in succeeded but no session token returned")
        } catch {
            return .failure(message: error.userFacingMessage)
        }
    }

    // MARK: - Passkey (EXP-857)

    /// Start a WebAuthn assertion: the served options PLUS every Set-Cookie
    /// pair of that response. One of those cookies is the signed challenge —
    /// this client runs without a cookie jar on purpose, so the verify call
    /// below replays them by hand or the server has nothing to compare against.
    public func passkeyAuthenticationOptions(instanceUrl: String) async -> PasskeyOptionsResult {
        guard let url = URL(string: "\(instanceUrl)/api/auth/passkey/generate-authenticate-options") else {
            return .failure(message: "Invalid instance URL")
        }
        do {
            let (data, response) = try await httpClient.getUnauthenticated(url)
            guard (200...299).contains(response.statusCode) else {
                return .failure(message: Self.authErrorMessage(from: data)
                    ?? "Passkey sign-in is unavailable (HTTP \(response.statusCode))")
            }
            let cookieHeader = PasskeyWire.cookieHeaderValue(
                fromSetCookieHeader: response.value(forHTTPHeaderField: "Set-Cookie")
            )
            guard let options = PasskeyWire.parseAuthenticationOptions(data, cookieHeader: cookieHeader) else {
                return .failure(message: "The server sent no usable passkey challenge")
            }
            return .success(options)
        } catch {
            return .failure(message: error.userFacingMessage)
        }
    }

    /// Hand the platform authenticator's assertion to the server and take the
    /// session it answers with.
    public func verifyPasskeyAuthentication(
        instanceUrl: String, cookieHeader: String, assertion: PasskeyAssertion
    ) async -> SignInResult {
        guard let url = URL(string: "\(instanceUrl)/api/auth/passkey/verify-authentication") else {
            return .failure(message: "Invalid instance URL")
        }
        do {
            let body = try PasskeyWire.verifyRequestBody(assertion)
            let headers = cookieHeader.isEmpty ? [:] : ["Cookie": cookieHeader]
            let (data, response) = try await httpClient.postUnauthenticated(url, body: body, headers: headers)
            guard (200...299).contains(response.statusCode) else {
                return .failure(message: Self.authErrorMessage(from: data)
                    ?? "Passkey sign-in failed (HTTP \(response.statusCode))")
            }
            let parsed = try JSONDecoder().decode(PasskeyVerifyResponseBody.self, from: data)
            guard let user = parsed.user else {
                return .failure(message: "Sign-in succeeded but no account was returned")
            }
            if let token = parsed.session?.token
                ?? Self.authTokenHeader(response)
                ?? Self.sessionTokenFromCookies(response) {
                return .success(token: token, user: user)
            }
            return .failure(message: "Sign-in succeeded but no session token returned")
        } catch {
            return .failure(message: error.userFacingMessage)
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

    /// The machine-readable half of the same body (`code`), which decides the
    /// one-time-code copy.
    private static func authErrorCode(from data: Data) -> String? {
        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let code = root["code"] as? String, !code.isEmpty else { return nil }
        return code
    }

    /// Better Auth's bearer plugin also mirrors the freshly minted session
    /// token into a response header.
    private static func authTokenHeader(_ response: HTTPURLResponse) -> String? {
        guard let token = response.value(forHTTPHeaderField: "set-auth-token"), !token.isEmpty else {
            return nil
        }
        return token
    }

    /// Last resort: the session token out of the Set-Cookie header (this client
    /// keeps no cookie jar, so nothing else would pick it up).
    private static func sessionTokenFromCookies(_ response: HTTPURLResponse) -> String? {
        guard let cookies = response.value(forHTTPHeaderField: "Set-Cookie"),
              let range = cookies.range(of: #"session_token=([^;]+)"#, options: .regularExpression),
              let tokenRange = cookies[range].range(of: "=") else { return nil }
        let token = String(cookies[tokenRange.upperBound...].prefix(while: { $0 != ";" }))
        return token.isEmpty ? nil : token
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

    /// Session read for a stored account. Unlike the explicit-credential form
    /// below, this one can attribute a DEAD session to an account, so it trips
    /// the dead-session gate — the path that unsticks a surface whose only
    /// affordance was retrying forever (the onboarding team step re-reads the
    /// session on appear).
    public func fetchSession(accountId: String) async -> AuthUser? {
        guard let account = auth.accounts.first(where: { $0.id == accountId }) else { return nil }
        switch await readSession(instanceUrl: account.instanceUrl, token: account.token) {
        case let .user(user):
            return user
        case .invalidated:
            SessionGate.shared.invalidate(accountId: accountId)
            return nil
        case .indeterminate:
            return nil
        }
    }

    // Core session read. Takes instanceUrl + token explicitly so a login flow
    // can capture session fields (incl. onboardingCompletedAt) BEFORE persisting
    // the token, avoiding any window where the account looks "not onboarded".
    // Never trips the gate: mid-login there is no account to invalidate yet.
    public func fetchSession(instanceUrl: String, token: String?) async -> AuthUser? {
        if case let .user(user) = await readSession(instanceUrl: instanceUrl, token: token) {
            return user
        }
        return nil
    }

    private func readSession(instanceUrl: String, token: String?) async -> SessionReadOutcome {
        guard let url = URL(string: "\(instanceUrl)/api/auth/get-session") else { return .indeterminate }
        do {
            let (data, response) = try await httpClient.get(url, bearerToken: token)
            return Self.classifySessionRead(
                statusCode: response.statusCode, body: data, tokenPresented: token != nil
            )
        } catch {
            // Transport failure: offline, DNS, TLS, timeout. Indistinguishable
            // from a healthy server we can't reach, so it must stay harmless.
            return .indeterminate
        }
    }

    /// The one rule that decides whether a session read PROVES the credential is
    /// dead. Better Auth answers a dead bearer with 200 + no session rather than
    /// a 401, so 2xx-with-no-user is the definitive signal — but only when a
    /// token was actually presented (tokenless reads are legitimately userless).
    /// Everything else (non-2xx, unparseable bodies) is indeterminate on
    /// purpose: the conservatism that keeps an offline app signed in.
    static func classifySessionRead(
        statusCode: Int, body: Data, tokenPresented: Bool
    ) -> SessionReadOutcome {
        guard (200...299).contains(statusCode) else { return .indeterminate }
        if let session = try? JSONDecoder().decode(SessionResponse.self, from: body),
           let user = session.user {
            return .user(user)
        }
        guard Self.isUserlessSessionBody(body) else { return .indeterminate }
        return tokenPresented ? .invalidated : .indeterminate
    }

    /// Does a 2xx `get-session` body actually say "no session"? Better Auth
    /// sends a bare `null` for a dead credential (not decodable as an object at
    /// all), so the shapes that count are JSON null and an object without a
    /// user. A body we can't parse says nothing and must not sign anyone out.
    private static func isUserlessSessionBody(_ body: Data) -> Bool {
        guard let root = try? JSONSerialization.jsonObject(
            with: body, options: [.fragmentsAllowed]
        ) else { return false }
        if root is NSNull { return true }
        guard let dict = root as? [String: Any] else { return false }
        let user = dict["user"]
        return user == nil || user is NSNull
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

    // Registration and password reset stay WEB flows on every native client
    // (desktop `open_register` parity) — the app hands off to the browser
    // instead of reimplementing sign-up/reset. Static so a login screen can
    // build them from the instance URL alone; nil for a blank instance URL.
    // EXP-759: `ref=ios-app` rides the register URL so the web signup claims
    // it as the signup source (users.signup_ref) — the only way to know a
    // signup STARTED in this app, since the account itself is created in the
    // browser. Android and desktop send their own tokens.

    public static func registerUrl(instanceUrl: String?) -> URL? {
        guard let base = WebLinks.normalizedBase(instanceUrl) else { return nil }
        return URL(string: "\(base)/auth/register?ref=ios-app")
    }

    public static func forgotPasswordUrl(instanceUrl: String?) -> URL? {
        guard let base = WebLinks.normalizedBase(instanceUrl) else { return nil }
        return URL(string: "\(base)/auth/forgot-password")
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

    /// The browser handoff (EXP-857): the web login page completes with ANY
    /// method and returns through the same PKCE deep link as the OAuth hops.
    /// It is the automatic fallback whenever the on-device passkey ceremony
    /// cannot run (a self-hosted instance the app has no association with).
    public func browserLoginStartUrl(instanceUrl: String, codeChallenge: String) -> URL? {
        URL(string: "\(instanceUrl)/api/mobile-oauth-start?provider=browser&code_challenge=\(codeChallenge)")
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

// verify-authentication answers with the whole session object, not the flat
// `{token, user}` the sign-in endpoints use.
private struct PasskeyVerifyResponseBody: Codable {
    struct Session: Codable {
        let token: String?
    }
    let session: Session?
    let user: AuthUser?
}

private struct OauthExchangeResponseBody: Codable {
    let token: String?
}
