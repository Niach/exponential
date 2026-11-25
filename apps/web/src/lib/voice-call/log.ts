// EXP-2111 (voice-call.json `log`): where a web call's lines and its one
// failure event go. The console line is also the diagnostics breadcrumb (the
// SDK's console breadcrumbs) while "Send diagnostics" is on.
import { VOICE_CALL_LOG, type CallLogEnd } from "@exp/etalk/voice-call"

import { captureClientNotice } from "@/lib/observability/client"

export function logCallLine(line: string): void {
  console.info(`[voice] ${line}`)
}

export function reportCallEnd(end: CallLogEnd): void {
  const { tag, by, reason, stage, phase, ...extra } = end
  captureClientNotice({
    message: VOICE_CALL_LOG.message.replace(`{reason}`, reason),
    fingerprint: [`voice-call-end`, reason],
    tags: { call_tag: tag, call_by: by, call_reason: reason, call_stage: stage, call_phase: phase },
    extra: { ...extra },
  })
}
