// The fixture is the contract (automations.json `webhook`, server-only).
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/automations.json" with { type: "json" }
import { newWebhookSecret, verifyWebhook, webhookPath, webhookSignature, webhookStatus, type WebhookError } from "./webhooks"

const hook = fixture.webhook

test(`the secret`, () => {
  for (const entry of hook.secretCases) expect(newWebhookSecret(Buffer.alloc(hook.secretBytes, 7))).toBe(entry.expected)
  expect(newWebhookSecret().startsWith(hook.secretPrefix)).toBe(true)
})

test(`the signature`, () => {
  for (const entry of hook.signatureCases) expect(webhookSignature(entry.secret, entry.timestamp, entry.body)).toBe(entry.expected)
})

describe(`verifyWebhook`, () => {
  for (const entry of hook.verifyCases) {
    test(entry.name, () => expect(verifyWebhook(entry)).toEqual(entry.expected as never))
  }
})

test(`statuses and the path`, () => {
  for (const code of [...hook.errors, `unknownHook`, `rateLimited`]) expect(webhookStatus(code as WebhookError)).toBe((hook.httpStatus as Record<string, number>)[code]!)
  for (const entry of hook.pathCases) expect(webhookPath(entry.appId, entry.name)).toBe(entry.expected)
})
