import Foundation

/// EXP-836: the ▶ seed's machine REQUEST — why the machine a launch surface
/// was ASKED to run on is not the one the run would land on.
///
/// A launch surface resolves its machine by precedence (request → the person's
/// own pick → their default machine → the first candidate, web
/// `lib/launch-device.ts`), so a request that is not a candidate falls back
/// SILENTLY: the person taps ▶ on one machine and the run starts on another.
/// The three reasons — and their wording — are shared with web's
/// `deviceRequestNote` (`hooks/use-launch-composer.ts`); the composer keeps
/// the two slots (a request is one-shot, a pick retires it) and this resolves
/// the note over the machine pool.
public enum LaunchDeviceRequest {
    /// The note for the options line, or nil when there is nothing to explain:
    /// no request, a request that SETTLED (it is the resolved machine), or a
    /// devices pool that has not synced yet — a machine that has not arrived
    /// is not a missing one, and the request keeps outranking the default
    /// until it does.
    public static func note(
        requested: String?, resolved: String?, devices: [SteerDevice]?
    ) -> String? {
        guard let requested, !requested.isEmpty, requested != resolved else { return nil }
        // Still syncing: no verdict yet.
        guard let devices else { return nil }
        guard let row = devices.first(where: { $0.deviceId == requested }) else {
            return "That machine is no longer in your registry."
        }
        let label = row.deviceLabel.isEmpty ? row.deviceId : row.deviceLabel
        if !row.isOnline { return "\(label) is offline." }
        // EXP-409: online, yet every installed agent is signed out there.
        if !row.hasRunnableAgent { return "No agent is signed in on \(label)." }
        // A candidate that simply lost to nothing — the caller's own fallback
        // rules (an agent-less subject, say) speak for themselves.
        return nil
    }
}
