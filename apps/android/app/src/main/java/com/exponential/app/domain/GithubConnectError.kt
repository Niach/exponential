package com.exponential.app.domain

// EXP-390: human copy for the `exponential://github-connected?error=<code>`
// deep link's failure slugs (the server's OAuth-callback error codes).
// Dropping the slug (as the app did before) made every failed connect look
// like a silent no-op — the Custom Tab closed and nothing happened. Copy
// mirrors the desktop's `connect_error_message` (github_connect.rs) — keep
// the three natives' strings identical (not a byte-parity contract, just
// courtesy).
fun githubConnectErrorMessage(code: String): String = when (code) {
    "session" -> "The connect link expired or was already used. Try connecting again."
    "exchange" -> "GitHub sign-in didn't complete. Try connecting again."
    "none" ->
        "The Exponential GitHub App isn't installed for any account you can access yet. Use Connect GitHub to install it."
    "notowner" ->
        "The authorized GitHub account only has collaborator access to existing installations. Install the App on your own account or organization."
    "orgperm" ->
        "Your organization hasn't approved the App's members-read permission yet. An org admin must accept it on GitHub, then reconnect."
    "forbidden" -> "Only team owners can connect GitHub accounts to a team."
    else -> "Something went wrong while connecting GitHub. Please try again."
}
