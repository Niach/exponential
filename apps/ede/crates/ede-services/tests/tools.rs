//! The MCP face's tool list (`exponential-services tools`): every domain's
//! actions, each with the capability its description names. A new action
//! is added here on purpose, so the list the agents see never drifts.

use ede_services::{mcp::tools, Registry};

#[test]
fn every_domain_lists_its_tools_with_their_capability() {
    let registry = Registry::default();
    let tools = tools(&registry);
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    let expected = [
        "network_get", "network_wifi_enable", "network_connect", "network_disconnect", "network_wired_enable", "network_vpn_connect", "network_vpn_disconnect",
        "sound_get", "sound_set_volume", "sound_mute", "sound_set_mic_volume", "sound_mute_mic", "sound_set_default", "sound_set_default_source", "sound_set_stream_volume", "sound_mute_stream",
        "media_get", "media_play_pause", "media_next", "media_previous", "media_stop", "media_seek",
        "bluetooth_get", "bluetooth_power", "bluetooth_connect", "bluetooth_disconnect", "bluetooth_discover",
        "power_get", "power_set_profile", "power_suspend", "power_hibernate", "power_reboot", "power_poweroff",
        "disks_get", "disks_mount", "disks_unmount",
        "updates_get", "updates_refresh", "updates_upgrade",
        "firmware_get", "firmware_refresh",
        "printers_get",
        "about_get", "about_check",
    ];
    assert_eq!(names, expected);
    let cap = |name: &str| tools.iter().find(|t| t["name"] == name).unwrap()["description"].as_str().unwrap().rsplit("Needs the ").next().unwrap().to_string();
    for ui in ["sound_set_stream_volume", "sound_mute_stream", "sound_set_default_source", "media_play_pause", "media_seek"] {
        assert!(cap(ui).starts_with("ui capability"), "{ui}: {}", cap(ui));
    }
    for system in ["network_wired_enable", "network_vpn_connect", "network_vpn_disconnect", "network_wifi_enable"] {
        assert!(cap(system).starts_with("system capability"), "{system}: {}", cap(system));
    }
    let seek = tools.iter().find(|t| t["name"] == "media_seek").unwrap();
    assert_eq!(seek["inputSchema"]["required"], serde_json::json!(["position"]));
}
