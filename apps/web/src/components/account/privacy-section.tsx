import { GlassGroup, GlassSectionHeader, GlassToggleRow } from "@exp/ui"
import { useSendDiagnostics } from "@/lib/observability/preference"

// #LOGS-2: Settings › Account › Privacy — "Send diagnostics" (crash and error
// reports, performance timings), per DEVICE, default on. Off = the
// diagnostics SDK is never loaded on this device (closed at once if it was).
// The natives mirror the placement (after Passkeys, before Danger zone).
export function PrivacySection() {
  const [sendDiagnostics, setSendDiagnostics] = useSendDiagnostics()
  return (
    <div>
      <GlassSectionHeader label="Privacy" />
      <GlassGroup>
        <GlassToggleRow
          id="send-diagnostics"
          label="Send diagnostics"
          checked={sendDiagnostics}
          onCheckedChange={setSendDiagnostics}
        />
      </GlassGroup>
    </div>
  )
}
