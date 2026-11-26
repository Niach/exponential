import Foundation

/// #EXP-2143 follow-up: "Confirm it's you" — the step-up sheet's reducer,
/// locked ×4 by `packages/domain-contract/fixtures/step-up.json` (web
/// `@exp/domain-contract/step-up`, desktop `domain::step_up`, Android
/// `domain/StepUp`). A SESSION_NOT_FRESH refusal opens the sheet in place; a
/// proof marks the SAME session fresh and the refused action runs again.
public enum StepUp {
    public static let notFresh = "SESSION_NOT_FRESH"
    /// The person dismissed the system passkey prompt: no error.
    public static let cancelled = "CANCELLED"

    public static let title = "Confirm it's you"
    public static let passkeyButton = "Use passkey"
    public static let confirm = "Continue"
    public static let resend = "Send a new code"
    public static let notFreshCopy = "Sign in again to do this (your current session is older than a day)."

    public enum Method: String, CaseIterable, Sendable {
        case passkey, email, password

        public var switchLabel: String {
            switch self {
            case .passkey: "Use a passkey"
            case .email: "Email me a code"
            case .password: "Use my password"
            }
        }

        /// The method's field label (none for the passkey).
        public var field: String? {
            switch self {
            case .passkey: nil
            case .email: "Code"
            case .password: "Password"
            }
        }

        public func description(email: String) -> String {
            switch self {
            case .passkey: "Use your passkey to continue."
            case .email: "Enter the code we sent to \(email)."
            case .password: "Enter your password to continue."
            }
        }
    }

    public enum Sheet: String, Sendable { case closed, open, busy }

    public enum Action: String, Sendable { case idle, waiting, resume, dropped, failed }

    public struct State: Equatable, Sendable {
        public var sheet: Sheet = .closed
        public var method: Method?
        public var methods: [Method] = []
        public var error: String?
        public var action: Action = .idle
        public var retried = false

        public init(
            sheet: Sheet = .closed, method: Method? = nil, methods: [Method] = [],
            error: String? = nil, action: Action = .idle, retried: Bool = false
        ) {
            self.sheet = sheet
            self.method = method
            self.methods = methods
            self.error = error
            self.action = action
            self.retried = retried
        }
    }

    public enum Event: Equatable, Sendable {
        case refused(code: String, methods: [Method])
        case choose(Method)
        case submit
        case proved
        case rejected(code: String)
        case cancel
        case done
    }

    /// The sentence for a refused proof (`CANCELLED` = none).
    public static func errorCopy(_ code: String) -> String? {
        switch code {
        case cancelled: nil
        case "INVALID_OTP": "That code is not right. Check the email and try again."
        case "OTP_EXPIRED": "That code expired. Request a new one."
        case "TOO_MANY_ATTEMPTS": "Too many attempts. Request a new code."
        case "INVALID_PASSWORD": "That password is not right."
        case "PASSKEY_FAILED": "Couldn't confirm it with that passkey."
        default: "Couldn't confirm it's you. Try again."
        }
    }

    public static func reduce(_ state: State, _ event: Event) -> State {
        var next = state
        switch event {
        case let .refused(code, methods):
            if code != notFresh {
                next.sheet = .closed
                next.action = .failed
                next.error = nil
            } else if state.retried || methods.isEmpty {
                // A second refusal after a proof never loops.
                next.sheet = .closed
                next.action = .failed
                next.error = notFreshCopy
                if !state.retried { next.methods = [] }
            } else {
                next.sheet = .open
                next.method = methods.first
                next.methods = methods
                next.error = nil
                next.action = .waiting
            }
        case let .choose(method):
            guard state.sheet == .open, state.methods.contains(method) else { return state }
            next.method = method
            next.error = nil
        case .submit:
            guard state.sheet == .open else { return state }
            next.sheet = .busy
            next.error = nil
        case .proved:
            guard state.sheet == .busy else { return state }
            next.sheet = .closed
            next.error = nil
            next.action = .resume
            next.retried = true
        case let .rejected(code):
            guard state.sheet == .busy else { return state }
            next.sheet = .open
            next.error = errorCopy(code)
        case .cancel:
            guard state.sheet != .closed else { return state }
            next.sheet = .closed
            next.error = nil
            next.action = .dropped
        case .done:
            next.action = .idle
            next.retried = false
        }
        return next
    }
}
