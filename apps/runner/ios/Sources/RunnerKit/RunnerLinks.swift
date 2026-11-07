// #VAPP-268: what reaches the runner from outside, as lifecycle.json events,
// and the functions it routes to the instance.
import Foundation

public enum RunnerLinks {
    /// `<config id>://<screen>?<params>` → `{type: deepLink, screen, params}`;
    /// nil = another scheme or no screen. The Engine decides what may open
    /// (genUI's `crossAppTarget`: exposed screens only).
    public static func event(_ url: URL, id: String) -> String? {
        guard url.scheme?.lowercased() == id.lowercased(),
              let parts = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let screen = parts.host, !screen.isEmpty else { return nil }
        var params: [String: String] = [:]
        for item in parts.queryItems ?? [] { params[item.name] = item.value ?? "" }
        return RunnerJSON.text(["type": "deepLink", "screen": screen, "params": params])
    }
}

public enum RunnerFunctions {
    /// The record writes (the Engine's store).
    public static let records = ["records_upsert", "records_delete", "records_batch"]

    /// Every app-callable capability tool of the Engine's catalog
    /// (capabilities.json `capabilities[].scopes[].tools`, `hostOnly` ones
    /// excluded): the broker refuses what the package did not declare.
    public static let capabilities = [
        "camera_capture",
        "clipboard_read", "clipboard_write",
        "files_open", "files_save",
        // #VAPP-273: Exponential Desktop's own (unavailable here: the broker says so).
        "focus_window",
        "http_fetch",
        "launch_app",
        "location_current",
        "notifications_cancel", "notifications_post",
        "photos_get", "photos_list", "photos_pick",
        "print_pdf", "print_print",
        "share_send",
        "windows_list",
    ]

    /// What the painter registers as host functions, each answered by `RunnerSession.call`.
    public static var all: [String] { records + capabilities }
}
