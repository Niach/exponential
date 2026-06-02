//! Hand-declared C-ABI bindings for the Rust `agent-core` cdylib (mirrors
//! `crates/agent-core/include/agent_core.h`). Like the GTK bindings, we declare
//! the extern fns by hand (the header is trivial C, but hand-declaring keeps the
//! Zig side self-contained). Linked in build.zig:linkAgentCore.

/// Opaque `AgentCore*`.
pub const AgentCore = ?*anyopaque;

/// `void (*)(void* ctx, const char* event_json, size_t len)`. The JSON is
/// borrowed for the call only — copy anything you keep. May fire on a background
/// thread; the host marshals to its UI thread (we g_idle_add).
pub const EventCallback = ?*const fn (ctx: ?*anyopaque, json: [*c]const u8, len: usize) callconv(.c) void;

pub extern fn agent_core_create(config_json: [*:0]const u8) AgentCore;
pub extern fn agent_core_set_event_callback(core: AgentCore, ctx: ?*anyopaque, cb: EventCallback) void;
pub extern fn agent_core_start(core: AgentCore) c_int;
pub extern fn agent_core_stop(core: AgentCore) c_int;
pub extern fn agent_core_free(core: AgentCore) void;
pub extern fn agent_core_submit_run_result(core: AgentCore, run_id: [*:0]const u8, exit_code: c_int, final_text: [*:0]const u8) c_int;
pub extern fn agent_core_cancel_run(core: AgentCore, run_id: [*:0]const u8) c_int;
