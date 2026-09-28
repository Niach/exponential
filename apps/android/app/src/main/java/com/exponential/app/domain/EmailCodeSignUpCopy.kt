package com.exponential.app.domain

// EXP-1026: the email-code sign-up name step's copy, byte-identical with iOS
// `EmailCodeSignUpCopy` (ExpCore `EmailOtp.swift`); `EmailCodeSignUpCopyTest`
// locks the literals.
object EmailCodeSignUpCopy {
    const val TITLE = "What should we call you?"
    const val BODY = "This is how your teammates see you."
    const val FIELD_LABEL = "Name"
    const val PLACEHOLDER = "Your name"
    const val BUTTON = "Create account"
    const val EMPTY_NAME_ERROR = "Enter your name to continue."
}
