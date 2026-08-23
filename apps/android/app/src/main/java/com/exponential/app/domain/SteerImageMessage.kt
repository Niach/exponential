package com.exponential.app.domain

// The steer message composed from typed text plus attached images (EXP-511):
// one string, byte-identical across web (lib/steer-image-message.ts), iOS
// (SteerImageMessage.swift) and here. The host rewrites each embed token to a
// local file path before the agent sees it and reverse-rewrites the echoed
// transcript back, so drift in this format breaks echo dedupe.

const val MAX_STEER_IMAGES = 4

fun buildSteerImageMessage(text: String, attachmentIds: List<String>): String {
    val trimmed = text.trim()
    if (attachmentIds.isEmpty()) return trimmed
    val embeds = attachmentIds.joinToString("\n") { "![image](/api/attachments/$it)" }
    if (trimmed.isEmpty()) return embeds
    return "$trimmed\n\n$embeds"
}
