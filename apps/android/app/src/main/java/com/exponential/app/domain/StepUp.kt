package com.exponential.app.domain

/**
 * #EXP-2143 follow-up: "Confirm it's you" — the step-up sheet's reducer,
 * locked ×4 by `packages/domain-contract/fixtures/step-up.json` (web
 * `@exp/domain-contract/step-up`, desktop `domain::step_up`, iOS ExpCore
 * `StepUp`). A SESSION_NOT_FRESH refusal opens the sheet in place; a proof
 * marks the SAME session fresh and the refused action runs again.
 */
object StepUp {
    const val NOT_FRESH = "SESSION_NOT_FRESH"

    /** The person dismissed the system passkey prompt: no error. */
    const val CANCELLED = "CANCELLED"

    const val TITLE = "Confirm it's you"
    const val PASSKEY_BUTTON = "Use passkey"
    const val CONFIRM = "Continue"
    const val RESEND = "Send a new code"
    const val NOT_FRESH_COPY = "Sign in again to do this (your current session is older than a day)."

    enum class Method(val wire: String, val switchLabel: String, val field: String?) {
        Passkey("passkey", "Use a passkey", null),
        Email("email", "Email me a code", "Code"),
        Password("password", "Use my password", "Password"),
        ;

        fun description(email: String): String = when (this) {
            Passkey -> "Use your passkey to continue."
            Email -> "Enter the code we sent to $email."
            Password -> "Enter your password to continue."
        }

        companion object {
            fun fromWire(value: String): Method? = entries.firstOrNull { it.wire == value }
        }
    }

    enum class Sheet { Closed, Open, Busy }

    enum class Action { Idle, Waiting, Resume, Dropped, Failed }

    data class State(
        val sheet: Sheet = Sheet.Closed,
        val method: Method? = null,
        val methods: List<Method> = emptyList(),
        val error: String? = null,
        val action: Action = Action.Idle,
        val retried: Boolean = false,
    )

    sealed interface Event {
        data class Refused(val code: String, val methods: List<Method>) : Event
        data class Choose(val method: Method) : Event
        data object Submit : Event
        data object Proved : Event
        data class Rejected(val code: String) : Event
        data object Cancel : Event
        data object Done : Event
    }

    /** The sentence for a refused proof (`CANCELLED` = none). */
    fun errorCopy(code: String): String? = when (code) {
        CANCELLED -> null
        "INVALID_OTP" -> "That code is not right. Check the email and try again."
        "OTP_EXPIRED" -> "That code expired. Request a new one."
        "TOO_MANY_ATTEMPTS" -> "Too many attempts. Request a new code."
        "INVALID_PASSWORD" -> "That password is not right."
        "PASSKEY_FAILED" -> "Couldn't confirm it with that passkey."
        else -> "Couldn't confirm it's you. Try again."
    }

    fun reduce(state: State, event: Event): State = when (event) {
        is Event.Refused -> when {
            event.code != NOT_FRESH -> state.copy(sheet = Sheet.Closed, action = Action.Failed, error = null)
            state.retried || event.methods.isEmpty() -> state.copy(
                sheet = Sheet.Closed,
                action = Action.Failed,
                error = NOT_FRESH_COPY,
                methods = if (state.retried) state.methods else emptyList(),
            )
            else -> state.copy(
                sheet = Sheet.Open,
                method = event.methods.first(),
                methods = event.methods,
                error = null,
                action = Action.Waiting,
            )
        }
        is Event.Choose ->
            if (state.sheet != Sheet.Open || event.method !in state.methods) state
            else state.copy(method = event.method, error = null)
        Event.Submit ->
            if (state.sheet != Sheet.Open) state else state.copy(sheet = Sheet.Busy, error = null)
        Event.Proved ->
            if (state.sheet != Sheet.Busy) state
            else state.copy(sheet = Sheet.Closed, error = null, action = Action.Resume, retried = true)
        is Event.Rejected ->
            if (state.sheet != Sheet.Busy) state else state.copy(sheet = Sheet.Open, error = errorCopy(event.code))
        Event.Cancel ->
            if (state.sheet == Sheet.Closed) state
            else state.copy(sheet = Sheet.Closed, error = null, action = Action.Dropped)
        Event.Done -> state.copy(action = Action.Idle, retried = false)
    }
}
