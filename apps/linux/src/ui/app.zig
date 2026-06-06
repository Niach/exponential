//! GTK4 + libadwaita GUI.
//!
//! Bindings are hand-declared in `gtk.zig` (translate-c can't parse GTK headers
//! on Zig 0.16). Single-instance with HANDLES_COMMAND_LINE so the browser's
//! `exp://oauth-return#token=…` redirect is forwarded to the running instance.
//! The login view is driven by /api/auth-config (password + Google + OIDC
//! providers). After sign-in we open a per-account SQLite DB, start the 10-shape
//! SyncManager, and show a live issue list that refreshes whenever a sync thread
//! reports changes (via g_idle_add — coalesced through `refresh_pending`).

const std = @import("std");
const gtk = @import("gtk.zig");
const oauth = @import("oauth.zig");
const storage = @import("../core/storage.zig");
const md = @import("markdown_editor.zig");
const format = @import("format.zig");
const widgets = @import("widgets.zig");
const settings = @import("settings.zig");
const integrations = @import("integrations.zig");
const trpc = @import("../core/api/trpc.zig");
const mutate = @import("../core/api/mutate.zig");
const http = @import("../core/api/http.zig");
const auth_api = @import("../core/auth/auth_api.zig");
const AccountStore = @import("../core/auth/account_store.zig").AccountStore;
const ServerAccount = @import("../core/auth/server_account.zig").ServerAccount;
const Database = @import("../core/db/database.zig").Database;
const sync = @import("../core/electric/sync_manager.zig");
const identity_store = @import("../core/agent/identity_store.zig");
const agent_manager = @import("../core/agent/agent_manager.zig");
const terminal_dock = @import("terminal_dock.zig");
const Heartbeat = @import("../core/agent/heartbeat.zig").Heartbeat;

extern fn readlink(path: [*:0]const u8, buf: [*]u8, bufsiz: usize) isize;

const default_instance = "https://next.exponential.at";

const AppState = struct {
    gpa: std.mem.Allocator,
    app: gtk.Object,
    window: gtk.Object = null,
    toolbar: gtk.Object = null,

    // login view
    instance_entry: gtk.Object = null,
    email_entry: gtk.Object = null,
    password_entry: gtk.Object = null,
    error_label: gtk.Object = null,
    login_instance: ?[]u8 = null, // last-entered instance URL
    pending_instance: ?[]u8 = null, // instance an OAuth round-trip is in flight for

    // tracker
    db: ?Database = null,
    sync_engine: ?sync.SyncManager = null,
    sync_started: bool = false,
    instance: ?[]u8 = null, // duped; SyncManager borrows it
    token: ?[]u8 = null,
    issue_list: gtk.Object = null,
    sidebar_pane: gtk.Object = null, // the sidebar toolbar (toggled with Ctrl+B)
    sidebar_list: gtk.Object = null,
    sidebar_account: gtk.Object = null,
    content_nav: gtk.Object = null, // AdwNavigationView (list → detail subpages)
    term_dock: ?*terminal_dock.TerminalDock = null, // IDE-style bottom terminal dock
    list_page: gtk.Object = null, // root AdwNavigationPage (the issue list)
    selected_project_id: ?[]u8 = null, // null = all issues
    selected_project_name: ?[]u8 = null, // for the create-issue default + title
    selected_project_repo: ?[]u8 = null, // GitHub repo of the selected project ("" → none)
    detail_issue_id: ?[]u8 = null, // issue currently shown in the detail pane
    shown_project_count: i64 = -1, // so the sidebar rebuilds only when projects change
    shown_workspace_count: i64 = -1,

    // workspace context (the tracker is scoped to one workspace, like the web)
    active_workspace_id: ?[]u8 = null, // gpa-owned
    workspace_ids: [][]u8 = &.{}, // gpa-owned; parallel to the switcher dropdown
    switcher_area: gtk.Object = null, // sidebar slot holding the workspace switcher
    switcher_popover: gtk.Object = null, // the switcher's dropdown popover (for popdown)
    repo_banner: gtk.Object = null, // GitHub repo banner above the list (hidden when none)
    heartbeat: ?*Heartbeat = null, // desktop-agent online ping for heartbeat_ws
    heartbeat_ws: ?[]u8 = null, // gpa-owned; which workspace the heartbeat serves
    agent_core: ?*agent_manager.Manager = null, // the Rust agent loop, when registered
    agent_core_ws: ?[]u8 = null, // gpa-owned; which workspace the agent serves

    // filter bar
    active_tab: format.Tab = .all,
    search_text: ?[]u8 = null, // gpa-owned lowercased query, null = no filter
    tab_buttons: [3]gtk.Object = .{ null, null, null }, // all / active / backlog
    search_entry: gtk.Object = null,
    collapsed: [format.status_display_order.len]bool = @splat(false),
    // multi-select filters (additive on top of the tab's status preset)
    filter_statuses: [format.statuses.len]bool = @splat(false),
    filter_priorities: [format.priorities.len]bool = @splat(false),
    filter_labels: std.ArrayListUnmanaged(FilterLabel) = .empty, // gpa-owned
    pills_box: gtk.Object = null,

    refresh_pending: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
    suppress_project_select: bool = false, // set while re-selecting the active row on rebuild
};

/// A label currently used as a filter — name/colour kept so pills can render it
/// without a re-query.
const FilterLabel = struct { id: []u8, name: []u8, color: []u8 };

fn statusDisplayIndex(value: []const u8) usize {
    for (format.status_display_order, 0..) |s, i| if (std.mem.eql(u8, s, value)) return i;
    return 0;
}

fn priorityIndex(value: []const u8) usize {
    for (format.priorities, 0..) |o, i| if (std.mem.eql(u8, o.value, value)) return i;
    return 0;
}

fn anyPriorityFilter(state: *AppState) bool {
    for (state.filter_priorities) |on| if (on) return true;
    return false;
}

fn anyStatusFilter(state: *AppState) bool {
    for (state.filter_statuses) |on| if (on) return true;
    return false;
}

/// Index of a status value into `format.statuses` (the filter array order).
fn statusOptionIndex(value: []const u8) usize {
    for (format.statuses, 0..) |o, i| if (std.mem.eql(u8, o.value, value)) return i;
    return 0;
}

fn hasAnyFilter(state: *AppState) bool {
    return anyStatusFilter(state) or anyPriorityFilter(state) or state.filter_labels.items.len > 0;
}

fn addFilterLabel(state: *AppState, id: []const u8, name: []const u8, color: []const u8) void {
    for (state.filter_labels.items) |fl| if (std.mem.eql(u8, fl.id, id)) return; // already present
    const gpa = state.gpa;
    // All three are gpa-owned and later individually freed, so never store a
    // non-heap fallback ("") — allocate all or bail cleanly.
    const id_d = gpa.dupe(u8, id) catch return;
    const name_d = gpa.dupe(u8, name) catch {
        gpa.free(id_d);
        return;
    };
    const color_d = gpa.dupe(u8, color) catch {
        gpa.free(id_d);
        gpa.free(name_d);
        return;
    };
    state.filter_labels.append(gpa, .{ .id = id_d, .name = name_d, .color = color_d }) catch {
        gpa.free(id_d);
        gpa.free(name_d);
        gpa.free(color_d);
    };
}

fn removeFilterLabel(state: *AppState, id: []const u8) void {
    var i: usize = 0;
    while (i < state.filter_labels.items.len) : (i += 1) {
        if (std.mem.eql(u8, state.filter_labels.items[i].id, id)) {
            const fl = state.filter_labels.items[i];
            state.gpa.free(fl.id);
            state.gpa.free(fl.name);
            state.gpa.free(fl.color);
            _ = state.filter_labels.orderedRemove(i);
            return;
        }
    }
}

fn clearFilters(state: *AppState) void {
    state.filter_statuses = @splat(false);
    state.filter_priorities = @splat(false);
    for (state.filter_labels.items) |fl| {
        state.gpa.free(fl.id);
        state.gpa.free(fl.name);
        state.gpa.free(fl.color);
    }
    state.filter_labels.clearAndFree(state.gpa);
}

pub fn run(gpa: std.mem.Allocator, args: []const [:0]const u8) u8 {
    registerSchemeHandler(gpa) catch {};

    var state = AppState{ .gpa = gpa, .app = undefined };

    const app = gtk.adw_application_new("at.exponential.desktop", gtk.APP_HANDLES_COMMAND_LINE);
    defer gtk.g_object_unref(app);
    state.app = app;

    _ = gtk.g_signal_connect_data(app, "command-line", @ptrCast(&onCommandLine), &state, null, 0);

    const argv = gpa.alloc([*:0]const u8, args.len) catch return 1;
    defer gpa.free(argv);
    for (args, 0..) |a, i| argv[i] = a.ptr;

    return @intCast(gtk.g_application_run(app, @intCast(args.len), argv.ptr));
}

fn onCommandLine(_: gtk.Object, cmdline: gtk.Object, data: gtk.gpointer) callconv(.c) c_int {
    const state: *AppState = @ptrCast(@alignCast(data));

    var argc: c_int = 0;
    const argv = gtk.g_application_command_line_get_arguments(cmdline, &argc);
    defer if (argv) |a| gtk.g_strfreev(a);

    var captured: ?[]u8 = null;
    if (argv) |a| {
        var i: usize = 0;
        while (i < @as(usize, @intCast(argc))) : (i += 1) {
            const cstr = a[i] orelse continue;
            if (oauth.parseDeepLinkToken(state.gpa, std.mem.span(cstr)) catch null) |tok| {
                if (captured) |old| state.gpa.free(old);
                captured = tok;
            }
        }
    }

    if (state.window == null) buildWindow(state);
    if (captured) |tok| {
        defer state.gpa.free(tok);
        completeLogin(state, tok);
    }
    gtk.gtk_window_present(state.window);
    return 0;
}

fn buildWindow(state: *AppState) void {
    widgets.applyCss(); // GTK is initialised by now (display exists)
    const window = gtk.adw_application_window_new(state.app);
    gtk.gtk_window_set_title(window, "Exponential");
    gtk.gtk_window_set_default_size(window, 1180, 780);
    // Floor the window so the sidebar + content never collapse into each other.
    gtk.gtk_widget_set_size_request(window, 880, 600);
    state.window = window;
    if (hasAccount(state.gpa)) enterTracker(state) else showInstanceEntry(state);
}

/// Wrap a login screen's box in a header-bar shell and make it the window
/// content (the auth screens have no sidebar/navigation).
fn setLoginContent(state: *AppState, box: gtk.Object) void {
    const toolbar = gtk.adw_toolbar_view_new();
    const header = gtk.adw_header_bar_new();
    gtk.adw_header_bar_set_title_widget(header, gtk.adw_window_title_new("Exponential", ""));
    gtk.adw_toolbar_view_add_top_bar(toolbar, header);
    gtk.adw_toolbar_view_set_content(toolbar, box);
    gtk.adw_application_window_set_content(state.window, toolbar);
    state.toolbar = toolbar;
}

// --- Login: instance entry, then methods gated by /api/auth-config ---

/// Screen 1: choose the instance (matches iOS InstanceView → LoginView flow).
fn showInstanceEntry(state: *AppState) void {
    const box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 14);
    gtk.gtk_widget_set_halign(box, gtk.ALIGN_CENTER);
    gtk.gtk_widget_set_valign(box, gtk.ALIGN_CENTER);
    gtk.gtk_widget_set_size_request(box, 380, -1);

    const title = gtk.gtk_label_new(null);
    gtk.gtk_label_set_markup(title, "<span size='xx-large' weight='bold'>Connect to Exponential</span>");
    gtk.gtk_box_append(box, title);

    const subtitle = gtk.gtk_label_new("Enter your Exponential instance URL.");
    gtk.gtk_widget_add_css_class(subtitle, "dim-label");
    gtk.gtk_box_append(box, subtitle);

    const inst = gtk.gtk_entry_new();
    gtk.gtk_entry_set_placeholder_text(inst, "https://your-instance");
    if (std.fmt.allocPrintSentinel(state.gpa, "{s}", .{state.login_instance orelse default_instance}, 0)) |z| {
        defer state.gpa.free(z);
        gtk.gtk_editable_set_text(inst, z.ptr);
    } else |_| {}
    state.instance_entry = inst;
    gtk.gtk_box_append(box, inst);

    const cont = gtk.gtk_button_new_with_label("Continue");
    gtk.gtk_widget_add_css_class(cont, "suggested-action");
    gtk.gtk_widget_add_css_class(cont, "pill");
    _ = gtk.g_signal_connect_data(cont, "clicked", @ptrCast(&onContinueClicked), state, null, 0);
    gtk.gtk_box_append(box, cont);

    setLoginContent(state, box);
}

fn onContinueClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    const instance = std.mem.span(gtk.gtk_editable_get_text(state.instance_entry));
    if (state.login_instance) |p| state.gpa.free(p);
    state.login_instance = state.gpa.dupe(u8, instance) catch null;
    showLogin(state);
}

fn onBackClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    showInstanceEntry(@ptrCast(@alignCast(data)));
}

/// Screen 2: render ONLY the sign-in methods the instance enables — Google,
/// then each OIDC provider, an "or" divider, then the password form — gated on
/// /api/auth-config exactly like iOS LoginView / web OAuthProviderButtons.
fn showLogin(state: *AppState) void {
    const instance = state.login_instance orelse default_instance;

    const box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 12);
    gtk.gtk_widget_set_halign(box, gtk.ALIGN_CENTER);
    gtk.gtk_widget_set_valign(box, gtk.ALIGN_CENTER);
    gtk.gtk_widget_set_size_request(box, 380, -1);

    const back = gtk.gtk_button_new_with_label("← Change instance");
    gtk.gtk_widget_add_css_class(back, "flat");
    gtk.gtk_widget_set_halign(back, gtk.ALIGN_START);
    _ = gtk.g_signal_connect_data(back, "clicked", @ptrCast(&onBackClicked), state, null, 0);
    gtk.gtk_box_append(box, back);

    const title = gtk.gtk_label_new(null);
    gtk.gtk_label_set_markup(title, "<span size='xx-large' weight='bold'>Sign in</span>");
    gtk.gtk_widget_set_halign(title, gtk.ALIGN_START);
    gtk.gtk_box_append(box, title);

    const caption = gtk.gtk_label_new(null);
    if (std.fmt.allocPrintSentinel(state.gpa, "{s}", .{instance}, 0)) |z| {
        defer state.gpa.free(z);
        gtk.gtk_label_set_text(caption, z.ptr);
    } else |_| {}
    gtk.gtk_widget_add_css_class(caption, "dim-label");
    gtk.gtk_widget_set_halign(caption, gtk.ALIGN_START);
    gtk.gtk_box_append(box, caption);

    const err = gtk.gtk_label_new("");
    gtk.gtk_widget_add_css_class(err, "error");
    state.error_label = err;
    gtk.gtk_box_append(box, err);

    var cfg_opt = auth_api.fetchAuthConfig(state.gpa, instance, 6) catch null;
    defer if (cfg_opt) |*cc| cc.deinit();
    if (cfg_opt == null) {
        gtk.gtk_label_set_text(err, "Couldn't reach that instance — go back and check the URL.");
        setLoginContent(state, box);
        return;
    }
    const cfg = &cfg_opt.?;

    var has_oauth = false;

    if (cfg.googleLoginEnabled()) {
        has_oauth = true;
        const g = gtk.gtk_button_new_with_label("Continue with Google");
        gtk.gtk_widget_add_css_class(g, "pill");
        _ = gtk.g_signal_connect_data(g, "clicked", @ptrCast(&onGoogleClicked), state, null, 0);
        gtk.gtk_box_append(box, g);
    }

    if (cfg.oidcProviders()) |arr| switch (arr) {
        .array => |items| for (items.items) |item| {
            const o = trpc.asObject(item) orelse continue;
            const pid = trpc.objString(o, "id") orelse continue;
            const pname = trpc.objString(o, "name") orelse pid;
            has_oauth = true;
            const lbl = std.fmt.allocPrintSentinel(state.gpa, "Continue with {s}", .{pname}, 0) catch continue;
            defer state.gpa.free(lbl);
            const btn = gtk.gtk_button_new_with_label(lbl.ptr);
            gtk.gtk_widget_add_css_class(btn, "pill");
            // stash the provider id on the button (GLib-owned; freed on destroy).
            if (state.gpa.dupeZ(u8, pid)) |tmp| {
                defer state.gpa.free(tmp);
                gtk.g_object_set_data_full(btn, "exp-provider-id", @ptrCast(gtk.g_strdup(tmp.ptr)), @ptrCast(&gtk.g_free));
            } else |_| {}
            _ = gtk.g_signal_connect_data(btn, "clicked", @ptrCast(&onOidcClicked), state, null, 0);
            gtk.gtk_box_append(box, btn);
        },
        else => {},
    };

    if (cfg.passwordEnabled()) {
        if (has_oauth) {
            const divider = gtk.gtk_label_new("or");
            gtk.gtk_widget_add_css_class(divider, "dim-label");
            gtk.gtk_box_append(box, divider);
        }
        const email = gtk.gtk_entry_new();
        gtk.gtk_entry_set_placeholder_text(email, "Email");
        state.email_entry = email;
        gtk.gtk_box_append(box, email);

        const pw = gtk.gtk_password_entry_new();
        state.password_entry = pw;
        gtk.gtk_box_append(box, pw);

        const signin = gtk.gtk_button_new_with_label("Sign in");
        gtk.gtk_widget_add_css_class(signin, "suggested-action");
        _ = gtk.g_signal_connect_data(signin, "clicked", @ptrCast(&onPasswordSignIn), state, null, 0);
        gtk.gtk_box_append(box, signin);
    } else if (!has_oauth) {
        gtk.gtk_label_set_text(err, "This instance has no sign-in methods enabled.");
    }

    setLoginContent(state, box);
}

fn launchOauth(state: *AppState, param: []const u8, value: []const u8) void {
    const instance = state.login_instance orelse default_instance;
    if (state.pending_instance) |p| state.gpa.free(p);
    state.pending_instance = state.gpa.dupe(u8, instance) catch null;

    const trimmed = std.mem.trimEnd(u8, instance, "/");
    const url = std.fmt.allocPrintSentinel(
        state.gpa,
        "{s}/api/mobile-oauth-start?{s}={s}",
        .{ trimmed, param, value },
        0,
    ) catch return;
    defer state.gpa.free(url);
    _ = gtk.g_app_info_launch_default_for_uri(url.ptr, null, null);
}

fn onGoogleClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    launchOauth(@ptrCast(@alignCast(data)), "provider", "google");
}

fn onOidcClicked(button: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    const raw = gtk.g_object_get_data(button, "exp-provider-id") orelse return;
    launchOauth(state, "providerId", std.mem.span(@as([*:0]const u8, @ptrCast(raw))));
}

fn onPasswordSignIn(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    const instance = state.login_instance orelse default_instance;
    const email = std.mem.span(gtk.gtk_editable_get_text(state.email_entry));
    const password = std.mem.span(gtk.gtk_editable_get_text(state.password_entry));

    var res = auth_api.signInWithPassword(state.gpa, instance, email, password, 20) catch {
        setError(state, "Sign-in request failed.");
        return;
    };
    defer res.deinit();

    if (!res.ok()) {
        setError(state, res.errorMessage() orelse "Invalid email or password.");
        return;
    }

    var store = AccountStore.open(state.gpa) catch return;
    defer store.deinit();
    const id = ServerAccount.makeId(state.gpa, instance) catch return;
    defer state.gpa.free(id);
    store.upsert(.{
        .id = id,
        .instance_url = instance,
        .token = res.token(),
        .user_id = res.userId(),
        .user_email = res.email(),
        .user_name = res.name(),
        .is_admin = res.isAdmin(),
    }) catch {};
    store.save() catch {};

    enterTracker(state);
}

fn setError(state: *AppState, msg: []const u8) void {
    const lbl = state.error_label orelse return;
    if (std.fmt.allocPrintSentinel(state.gpa, "{s}", .{msg}, 0)) |z| {
        defer state.gpa.free(z);
        gtk.gtk_label_set_text(lbl, z.ptr);
    } else |_| {}
}

fn completeLogin(state: *AppState, token: []const u8) void {
    const instance = state.pending_instance orelse default_instance;

    var store = AccountStore.open(state.gpa) catch return;
    defer store.deinit();
    const id = ServerAccount.makeId(state.gpa, instance) catch return;
    defer state.gpa.free(id);

    // OAuth/OIDC hands back only a token (no user object), so fetch the session
    // to fill in email/name/admin for the sidebar + member resolution.
    if (auth_api.fetchSession(state.gpa, instance, token, 20)) |result| {
        var sess = result;
        defer sess.deinit();
        store.upsert(.{
            .id = id,
            .instance_url = instance,
            .token = token,
            .user_id = sess.userId(),
            .user_email = sess.email(),
            .user_name = sess.name(),
            .is_admin = sess.isAdmin(),
        }) catch {};
    } else |_| {
        store.upsert(.{ .id = id, .instance_url = instance, .token = token }) catch {};
    }
    store.save() catch {};

    enterTracker(state);
}

// --- Tracker view ---

/// One-shot: fetch the session for a token-only account + persist its identity.
/// Dupes the id/instance/token into a scratch arena first so the store upsert
/// can't read freed slices if it reallocates.
fn backfillIdentity(state: *AppState, store: *AccountStore, active: ServerAccount) void {
    const token = active.token orelse return;
    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const id = a.dupe(u8, active.id) catch return;
    const inst = a.dupe(u8, active.instance_url) catch return;
    const tok = a.dupe(u8, token) catch return;
    if (auth_api.fetchSession(state.gpa, inst, tok, 20)) |result| {
        var sess = result;
        defer sess.deinit();
        if (sess.email() == null and sess.userId() == null) return;
        store.upsert(.{
            .id = id,
            .instance_url = inst,
            .token = tok,
            .user_id = sess.userId(),
            .user_email = sess.email(),
            .user_name = sess.name(),
            .is_admin = sess.isAdmin(),
        }) catch {};
        store.save() catch {};
    } else |_| {}
}

fn enterTracker(state: *AppState) void {
    var store = AccountStore.open(state.gpa) catch {
        showLogin(state);
        return;
    };
    defer store.deinit();
    const active = activeAccount(&store) orelse {
        showLogin(state);
        return;
    };

    // Accounts saved before we fetched the session (OAuth logins persisted only a
    // token) have no identity — backfill it so the sidebar shows the email.
    if (active.user_email == null and active.token != null) {
        backfillIdentity(state, &store, active);
    }

    if (state.instance == null) state.instance = state.gpa.dupe(u8, active.instance_url) catch null;
    if (state.token == null) {
        if (active.token) |t| state.token = state.gpa.dupe(u8, t) catch null;
    }
    const account_id = state.gpa.dupe(u8, active.id) catch return;
    defer state.gpa.free(account_id);

    if (state.db == null) state.db = openAccountDb(state, account_id) catch null;

    buildTrackerUI(state);

    // First launch for this account → a one-time welcome (deferred to idle so the
    // main window is presented first).
    if (!hasOnboarded(state.gpa, account_id)) {
        markOnboarded(state.gpa, account_id);
        _ = gtk.g_idle_add(@ptrCast(&showOnboardingIdle), state);
    }

    if (state.db != null and !state.sync_started) {
        if (state.instance) |inst| {
            state.sync_engine = sync.SyncManager{
                .gpa = state.gpa,
                .db = &state.db.?,
                .base_url = inst,
                .token = state.token,
                .notify = &onSyncChanged,
                .notify_ctx = state,
            };
            state.sync_engine.?.start() catch {};
            state.sync_started = true;
        }
    }
}

fn onSignOut(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));

    // Stop sync — cancellable long-polls abort within ~1s, then threads join.
    if (state.sync_engine) |*se| se.stop();
    state.sync_started = false;
    if (state.db) |*d| d.close();
    state.db = null;

    // Threads are stopped, so the borrowed creds are safe to free now.
    if (state.instance) |p| state.gpa.free(p);
    state.instance = null;
    if (state.token) |p| state.gpa.free(p);
    state.token = null;
    if (state.selected_project_id) |p| state.gpa.free(p);
    state.selected_project_id = null;
    if (state.selected_project_name) |p| state.gpa.free(p);
    state.selected_project_name = null;
    if (state.selected_project_repo) |p| state.gpa.free(p);
    state.selected_project_repo = null;
    if (state.detail_issue_id) |p| state.gpa.free(p);
    state.detail_issue_id = null;
    if (state.search_text) |p| state.gpa.free(p);
    state.search_text = null;
    if (state.heartbeat) |hb| hb.stop();
    state.heartbeat = null;
    if (state.heartbeat_ws) |p| state.gpa.free(p);
    state.heartbeat_ws = null;
    if (state.agent_core) |m| agent_manager.stop(m);
    state.agent_core = null;
    if (state.agent_core_ws) |p| state.gpa.free(p);
    state.agent_core_ws = null;
    // The dock's GTK widgets die with the window content; free our struct (the
    // agent core was stopped just above, so it won't touch the dock anymore).
    if (state.term_dock) |d| state.gpa.destroy(d);
    state.term_dock = null;
    if (state.active_workspace_id) |p| state.gpa.free(p);
    state.active_workspace_id = null;
    freeWorkspaceIds(state);
    state.shown_project_count = -1;
    state.shown_workspace_count = -1;
    state.active_tab = .all;
    state.collapsed = @splat(false);
    clearFilters(state);
    state.issue_list = null;
    state.sidebar_pane = null;
    state.sidebar_list = null;
    state.sidebar_account = null;
    state.switcher_area = null;
    state.switcher_popover = null;
    state.repo_banner = null;
    state.content_nav = null;
    state.list_page = null;
    state.search_entry = null;
    state.tab_buttons = .{ null, null, null };
    state.pills_box = null;

    var store = AccountStore.open(state.gpa) catch null;
    if (store) |*s| {
        s.removeActive() catch {};
        s.deinit();
    }

    showInstanceEntry(state);
}

fn activeAccount(store: *AccountStore) ?ServerAccount {
    const accts = store.list();
    if (accts.len == 0) return null;
    if (store.active_id) |aid| {
        for (accts) |acc| if (std.mem.eql(u8, acc.id, aid)) return acc;
    }
    return accts[0];
}

fn openAccountDb(state: *AppState, account_id: []const u8) !Database {
    const dir = try storage.configDir(state.gpa);
    defer state.gpa.free(dir);
    const db_dir = try std.fmt.allocPrint(state.gpa, "{s}/db", .{dir});
    defer state.gpa.free(db_dir);
    storage.ensureDir(db_dir) catch {};
    const path = try std.fmt.allocPrintSentinel(state.gpa, "{s}/{s}.sqlite", .{ db_dir, account_id }, 0);
    defer state.gpa.free(path);
    return Database.open(state.gpa, path);
}

fn buildTrackerUI(state: *AppState) void {
    // Web-like layout: a persistent navigation sidebar on the left and a content
    // area that NAVIGATES (list → issue subpage with a back button) via
    // AdwNavigationView — no always-on third detail pane.
    const panes = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 0);
    gtk.gtk_widget_set_vexpand(panes, 1);

    // --- Sidebar pane (own header bar with Sign out) ---
    const sidebar_toolbar = gtk.adw_toolbar_view_new();
    gtk.gtk_widget_set_size_request(sidebar_toolbar, 260, -1);
    // Fixed width: without this an inner hexpanding child (the "Projects" header)
    // propagates up and the sidebar grows with the window. Pin it so only the
    // content pane absorbs extra width.
    gtk.gtk_widget_set_hexpand(sidebar_toolbar, 0);
    gtk.gtk_widget_add_css_class(sidebar_toolbar, "exp-sidebar"); // one cohesive surface + divider
    state.sidebar_pane = sidebar_toolbar;
    const sidebar_header = gtk.adw_header_bar_new();
    gtk.gtk_widget_add_css_class(sidebar_header, "exp-sidebar-header"); // blend into the sidebar surface
    gtk.adw_header_bar_set_title_widget(sidebar_header, gtk.adw_window_title_new("Exponential", ""));
    gtk.adw_header_bar_set_show_end_title_buttons(sidebar_header, 0); // controls live on the content side
    // Sign out moved into the footer user-identity menu (mirrors the web sidebar).
    gtk.adw_toolbar_view_add_top_bar(sidebar_toolbar, sidebar_header);

    const sidebar_box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);

    // Workspace switcher slot (a dropdown when >1 workspace; rebuilt on change).
    const switcher_area = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
    gtk.gtk_widget_set_margin_start(switcher_area, 8);
    gtk.gtk_widget_set_margin_end(switcher_area, 8);
    gtk.gtk_widget_set_margin_top(switcher_area, 8);
    gtk.gtk_widget_set_margin_bottom(switcher_area, 4);
    gtk.gtk_box_append(sidebar_box, switcher_area);
    state.switcher_area = switcher_area;

    // "Projects" group header with a trailing "+" (mirrors the web sidebar group).
    const projects_header = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 0);
    gtk.gtk_widget_set_margin_start(projects_header, 14);
    gtk.gtk_widget_set_margin_end(projects_header, 6);
    gtk.gtk_widget_set_margin_top(projects_header, 4);
    const projects_label = gtk.gtk_label_new("Projects");
    gtk.gtk_widget_add_css_class(projects_label, "dim-label");
    gtk.gtk_widget_add_css_class(projects_label, "caption-heading");
    gtk.gtk_widget_set_halign(projects_label, gtk.ALIGN_START);
    gtk.gtk_widget_set_hexpand(projects_label, 1);
    gtk.gtk_box_append(projects_header, projects_label);
    const add_project = gtk.gtk_button_new_with_label("+");
    gtk.gtk_widget_add_css_class(add_project, "flat");
    gtk.gtk_widget_set_tooltip_text(add_project, "New project");
    _ = gtk.g_signal_connect_data(add_project, "clicked", @ptrCast(&onNewProjectClicked), state, null, 0);
    gtk.gtk_box_append(projects_header, add_project);
    gtk.gtk_box_append(sidebar_box, projects_header);

    const sidebar = gtk.gtk_list_box_new();
    gtk.gtk_widget_add_css_class(sidebar, "navigation-sidebar");
    _ = gtk.g_signal_connect_data(sidebar, "row-selected", @ptrCast(&onProjectSelected), state, null, 0);
    const sidebar_scrolled = gtk.gtk_scrolled_window_new();
    gtk.gtk_widget_set_vexpand(sidebar_scrolled, 1);
    gtk.gtk_scrolled_window_set_child(sidebar_scrolled, sidebar);
    gtk.gtk_box_append(sidebar_box, sidebar_scrolled);
    state.sidebar_list = sidebar;

    // (Workspace settings now lives in the switcher popover, mirroring the web.)

    // Send feedback (opens the instance's /feedback page in the browser).
    const feedback_btn = gtk.gtk_button_new_with_label("\u{1F4E3} Send feedback");
    gtk.gtk_widget_add_css_class(feedback_btn, "flat");
    gtk.gtk_widget_add_css_class(feedback_btn, "dim-label");
    gtk.gtk_widget_set_halign(feedback_btn, gtk.ALIGN_START);
    gtk.gtk_widget_set_margin_start(feedback_btn, 8);
    _ = gtk.g_signal_connect_data(feedback_btn, "clicked", @ptrCast(&onFeedbackClicked), state, null, 0);
    gtk.gtk_box_append(sidebar_box, feedback_btn);

    // User-identity menu: avatar + email → Integrations / Sign out.
    const user_box = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    {
        var arena = std.heap.ArenaAllocator.init(state.gpa);
        defer arena.deinit();
        var name_buf: [128]u8 = undefined;
        var display: []const u8 = "Account";
        var store = AccountStore.open(state.gpa) catch null;
        if (store) |*s| {
            if (activeAccount(s)) |acc| {
                display = std.fmt.bufPrint(&name_buf, "{s}", .{acc.user_name orelse acc.user_email orelse "Account"}) catch "Account";
            }
            s.deinit();
        }
        gtk.gtk_box_append(user_box, widgets.avatar(arena.allocator(), if (display.len > 0) display else "?"));
    }
    const account = gtk.gtk_label_new("");
    gtk.gtk_widget_add_css_class(account, "caption");
    gtk.gtk_widget_set_halign(account, gtk.ALIGN_START);
    gtk.gtk_widget_set_hexpand(account, 1);
    gtk.gtk_label_set_ellipsize(account, gtk.ELLIPSIZE_END);
    gtk.gtk_box_append(user_box, account);
    state.sidebar_account = account;

    const user_menu = gtk.gtk_menu_button_new();
    gtk.gtk_widget_add_css_class(user_menu, "flat");
    gtk.gtk_menu_button_set_child(user_menu, user_box);
    gtk.gtk_widget_set_margin_start(user_menu, 8);
    gtk.gtk_widget_set_margin_end(user_menu, 8);
    gtk.gtk_widget_set_margin_bottom(user_menu, 8);
    gtk.gtk_widget_set_margin_top(user_menu, 2);

    const user_pop = gtk.gtk_popover_new();
    const user_pop_box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 2);

    const integrations_item = gtk.gtk_button_new_with_label("Integrations");
    gtk.gtk_widget_add_css_class(integrations_item, "flat");
    gtk.gtk_widget_set_halign(integrations_item, gtk.ALIGN_FILL);
    _ = gtk.g_signal_connect_data(integrations_item, "clicked", @ptrCast(&onIntegrationsClicked), state, null, 0);
    gtk.gtk_box_append(user_pop_box, integrations_item);

    const signout_item = gtk.gtk_button_new_with_label("Sign out");
    gtk.gtk_widget_add_css_class(signout_item, "flat");
    gtk.gtk_widget_set_halign(signout_item, gtk.ALIGN_FILL);
    _ = gtk.g_signal_connect_data(signout_item, "clicked", @ptrCast(&onSignOut), state, null, 0);
    gtk.gtk_box_append(user_pop_box, signout_item);
    gtk.gtk_popover_set_child(user_pop, user_pop_box);
    gtk.gtk_menu_button_set_popover(user_menu, user_pop);
    gtk.gtk_box_append(sidebar_box, user_menu);

    updateAccountLabel(state);

    gtk.adw_toolbar_view_set_content(sidebar_toolbar, sidebar_box);
    gtk.gtk_box_append(panes, sidebar_toolbar);

    // --- Content navigation view (root = issue list page) ---
    const nav = gtk.adw_navigation_view_new();
    gtk.gtk_widget_set_hexpand(nav, 1);
    state.content_nav = nav;

    const list_toolbar = gtk.adw_toolbar_view_new();
    const list_header = gtk.adw_header_bar_new();
    gtk.adw_header_bar_set_show_start_title_buttons(list_header, 0); // only the inner edge
    const new_issue = gtk.gtk_button_new_with_label("New issue");
    gtk.gtk_widget_add_css_class(new_issue, "suggested-action");
    _ = gtk.g_signal_connect_data(new_issue, "clicked", @ptrCast(&onNewIssueClicked), state, null, 0);
    gtk.adw_header_bar_pack_end(list_header, new_issue);
    gtk.adw_toolbar_view_add_top_bar(list_toolbar, list_header);

    const center = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);

    // GitHub repo banner (shown when the selected project has a linked repo).
    const repo_banner = gtk.gtk_button_new_with_label("");
    gtk.gtk_widget_add_css_class(repo_banner, "flat");
    gtk.gtk_widget_set_halign(repo_banner, gtk.ALIGN_START);
    gtk.gtk_widget_set_margin_start(repo_banner, 12);
    gtk.gtk_widget_set_margin_top(repo_banner, 4);
    gtk.gtk_widget_set_visible(repo_banner, 0);
    _ = gtk.g_signal_connect_data(repo_banner, "clicked", @ptrCast(&onRepoBannerClicked), state, null, 0);
    gtk.gtk_box_append(center, repo_banner);
    state.repo_banner = repo_banner;

    gtk.gtk_box_append(center, buildFilterBar(state));
    const pills = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    gtk.gtk_widget_set_margin_start(pills, 16);
    gtk.gtk_widget_set_margin_end(pills, 16);
    gtk.gtk_box_append(center, pills);
    state.pills_box = pills;

    const list = gtk.gtk_list_box_new();
    gtk.gtk_widget_add_css_class(list, "navigation-sidebar");
    _ = gtk.g_signal_connect_data(list, "row-activated", @ptrCast(&onIssueActivated), state, null, 0);
    const issue_scrolled = gtk.gtk_scrolled_window_new();
    gtk.gtk_widget_set_vexpand(issue_scrolled, 1);
    gtk.gtk_scrolled_window_set_child(issue_scrolled, list);
    gtk.gtk_box_append(center, issue_scrolled);
    state.issue_list = list;

    gtk.adw_toolbar_view_set_content(list_toolbar, center);
    const list_page = gtk.adw_navigation_page_new(list_toolbar, "All Issues");
    state.list_page = list_page;
    gtk.adw_navigation_view_push(nav, list_page);
    // Wrap the content nav in an IDE-style vertical split: agent runs land in a
    // collapsible bottom terminal dock instead of a throwaway window.
    if (terminal_dock.TerminalDock.create(state.gpa, nav)) |dock| {
        state.term_dock = dock;
        gtk.gtk_widget_set_hexpand(dock.paned, 1);
        gtk.gtk_box_append(panes, dock.paned);
    } else {
        gtk.gtk_box_append(panes, nav);
    }

    gtk.adw_application_window_set_content(state.window, panes);

    // Ctrl/⌘+B toggles the sidebar (mirrors the web sidebar shortcut).
    const key = gtk.gtk_event_controller_key_new();
    _ = gtk.g_signal_connect_data(key, "key-pressed", @ptrCast(&onWindowKey), state, null, 0);
    gtk.gtk_widget_add_controller(state.window, key);

    styleTabs(state);
    doRefresh(state);
}

fn onWindowKey(_: gtk.Object, keyval: c_uint, _: c_uint, mods: c_uint, data: gtk.gpointer) callconv(.c) c_int {
    const state: *AppState = @ptrCast(@alignCast(data));
    const ctrl_or_super = (mods & (1 << 2)) != 0 or (mods & (1 << 26)) != 0; // Control / Super
    if (ctrl_or_super and (keyval == 'b' or keyval == 'B')) {
        if (state.sidebar_pane) |pane| {
            gtk.gtk_widget_set_visible(pane, if (gtk.gtk_widget_get_visible(pane) != 0) 0 else 1);
            return 1; // handled
        }
    }
    return 0;
}

/// Filter tabs (All / Active / Backlog) + a search entry — mirrors the iOS
/// filter bar / web issue-filter-bar.
fn buildFilterBar(state: *AppState) gtk.Object {
    const bar = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_set_margin_start(bar, 16);
    gtk.gtk_widget_set_margin_end(bar, 16);
    gtk.gtk_widget_set_margin_top(bar, 4);
    gtk.gtk_widget_set_margin_bottom(bar, 8);

    const tabs = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 0);
    gtk.gtk_widget_add_css_class(tabs, "linked");
    const all = gtk.gtk_button_new_with_label("All");
    const active = gtk.gtk_button_new_with_label("Active");
    const backlog = gtk.gtk_button_new_with_label("Backlog");
    _ = gtk.g_signal_connect_data(all, "clicked", @ptrCast(&onTabAll), state, null, 0);
    _ = gtk.g_signal_connect_data(active, "clicked", @ptrCast(&onTabActive), state, null, 0);
    _ = gtk.g_signal_connect_data(backlog, "clicked", @ptrCast(&onTabBacklog), state, null, 0);
    gtk.gtk_box_append(tabs, all);
    gtk.gtk_box_append(tabs, active);
    gtk.gtk_box_append(tabs, backlog);
    state.tab_buttons = .{ all, active, backlog };
    gtk.gtk_box_append(bar, tabs);

    // Multi-select filter popover (priority + labels), built lazily so it always
    // reflects current labels + selection.
    const filter_btn = gtk.gtk_menu_button_new();
    gtk.gtk_menu_button_set_child(filter_btn, gtk.gtk_label_new("Filter"));
    gtk.gtk_menu_button_set_create_popup_func(filter_btn, @ptrCast(&buildFilterPopup), state, null);
    gtk.gtk_box_append(bar, filter_btn);

    const search = gtk.gtk_search_entry_new();
    gtk.gtk_widget_set_hexpand(search, 1);
    _ = gtk.g_signal_connect_data(search, "search-changed", @ptrCast(&onSearchChanged), state, null, 0);
    gtk.gtk_box_append(bar, search);
    state.search_entry = search;

    return bar;
}

fn styleTabs(state: *AppState) void {
    const active_idx: usize = switch (state.active_tab) {
        .all => 0,
        .active => 1,
        .backlog => 2,
    };
    for (state.tab_buttons, 0..) |btn, i| {
        if (btn == null) continue;
        if (i == active_idx) gtk.gtk_widget_add_css_class(btn, "suggested-action") else gtk.gtk_widget_remove_css_class(btn, "suggested-action");
    }
}

fn onTabAll(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    setTab(@ptrCast(@alignCast(data)), .all);
}
fn onTabActive(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    setTab(@ptrCast(@alignCast(data)), .active);
}
fn onTabBacklog(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    setTab(@ptrCast(@alignCast(data)), .backlog);
}

fn setTab(state: *AppState, tab: format.Tab) void {
    state.active_tab = tab;
    styleTabs(state);
    refreshIssues(state);
}

fn onSearchChanged(entry: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    const text = std.mem.span(gtk.gtk_editable_get_text(entry));
    if (state.search_text) |p| state.gpa.free(p);
    state.search_text = if (text.len == 0) null else (std.ascii.allocLowerString(state.gpa, text) catch null);
    refreshIssues(state);
}

// --- Filter popover (priority + labels) + removable active-filter pills ---

const FilterToggleCtx = struct {
    state: *AppState,
    is_label: bool,
    is_status: bool = false, // status index into format.statuses when set
    idx: usize, // priority index (!is_label && !is_status) or status index
    id: ?[:0]u8, // label id/name/colour (gpa) when is_label
    name: ?[:0]u8,
    color: ?[:0]u8,
};

fn freeFilterToggle(p: gtk.gpointer) callconv(.c) void {
    const c: *FilterToggleCtx = @ptrCast(@alignCast(p));
    const gpa = c.state.gpa;
    if (c.id) |x| gpa.free(x);
    if (c.name) |x| gpa.free(x);
    if (c.color) |x| gpa.free(x);
    gpa.destroy(c);
}

/// Lazily build the filter popover (so it reflects current labels + selection).
/// set_active runs before connecting "toggled", so priming doesn't fire it.
fn buildFilterPopup(button: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    const pop = gtk.gtk_popover_new();
    const col = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 4);
    gtk.gtk_widget_set_margin_top(col, 8);
    gtk.gtk_widget_set_margin_bottom(col, 8);
    gtk.gtk_widget_set_margin_start(col, 8);
    gtk.gtk_widget_set_margin_end(col, 8);

    gtk.gtk_box_append(col, widgets.sectionTitle("Status"));
    for (format.statuses, 0..) |opt, i| {
        var buf: [64]u8 = undefined;
        const lbl = std.fmt.bufPrintZ(&buf, "{s}", .{opt.label}) catch continue;
        const chk = gtk.gtk_check_button_new_with_label(lbl.ptr);
        if (state.filter_statuses[i]) gtk.gtk_check_button_set_active(chk, 1);
        const ctx = state.gpa.create(FilterToggleCtx) catch continue;
        ctx.* = .{ .state = state, .is_label = false, .is_status = true, .idx = i, .id = null, .name = null, .color = null };
        gtk.g_object_set_data_full(chk, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeFilterToggle));
        _ = gtk.g_signal_connect_data(chk, "toggled", @ptrCast(&onFilterToggled), ctx, null, 0);
        gtk.gtk_box_append(col, chk);
    }

    gtk.gtk_box_append(col, widgets.sectionTitle("Priority"));
    for (format.priorities, 0..) |opt, i| {
        var buf: [64]u8 = undefined;
        const lbl = std.fmt.bufPrintZ(&buf, "{s}", .{opt.label}) catch continue;
        const chk = gtk.gtk_check_button_new_with_label(lbl.ptr);
        if (state.filter_priorities[i]) gtk.gtk_check_button_set_active(chk, 1);
        const ctx = state.gpa.create(FilterToggleCtx) catch continue;
        ctx.* = .{ .state = state, .is_label = false, .idx = i, .id = null, .name = null, .color = null };
        gtk.g_object_set_data_full(chk, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeFilterToggle));
        _ = gtk.g_signal_connect_data(chk, "toggled", @ptrCast(&onFilterToggled), ctx, null, 0);
        gtk.gtk_box_append(col, chk);
    }

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    if (state.db) |*db| {
        const ws_opt: ?[]const u8 = state.active_workspace_id orelse (db.firstWorkspaceId(a) catch null);
        if (ws_opt) |ws| {
            const labels = db.listLabels(a, ws) catch &[_]Database.LabelRow{};
            if (labels.len > 0) gtk.gtk_box_append(col, widgets.sectionTitle("Labels"));
            for (labels) |l| {
                const name_z = a.dupeZ(u8, l.name) catch continue;
                const chk = gtk.gtk_check_button_new_with_label(name_z.ptr);
                for (state.filter_labels.items) |fl| {
                    if (std.mem.eql(u8, fl.id, l.id)) {
                        gtk.gtk_check_button_set_active(chk, 1);
                        break;
                    }
                }
                const ctx = state.gpa.create(FilterToggleCtx) catch continue;
                ctx.* = .{
                    .state = state,
                    .is_label = true,
                    .idx = 0,
                    .id = state.gpa.dupeZ(u8, l.id) catch null,
                    .name = state.gpa.dupeZ(u8, l.name) catch null,
                    .color = state.gpa.dupeZ(u8, l.color) catch null,
                };
                gtk.g_object_set_data_full(chk, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeFilterToggle));
                _ = gtk.g_signal_connect_data(chk, "toggled", @ptrCast(&onFilterToggled), ctx, null, 0);
                gtk.gtk_box_append(col, chk);
            }
        }
    }

    gtk.gtk_popover_set_child(pop, col);
    gtk.gtk_menu_button_set_popover(button, pop);
}

fn onFilterToggled(check: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *FilterToggleCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    const active = gtk.gtk_check_button_get_active(check) != 0;
    if (ctx.is_status) {
        if (ctx.idx < state.filter_statuses.len) state.filter_statuses[ctx.idx] = active;
    } else if (ctx.is_label) {
        if (active)
            addFilterLabel(state, ctx.id orelse return, ctx.name orelse "", ctx.color orelse "")
        else
            removeFilterLabel(state, ctx.id orelse return);
    } else if (ctx.idx < state.filter_priorities.len) {
        state.filter_priorities[ctx.idx] = active;
    }
    refreshIssues(state); // also refreshes the pills
}

const PillCtx = struct {
    state: *AppState,
    kind: enum { status, priority, label, clear },
    idx: usize,
    id: ?[:0]u8,
};

fn freePill(p: gtk.gpointer) callconv(.c) void {
    const c: *PillCtx = @ptrCast(@alignCast(p));
    if (c.id) |x| c.state.gpa.free(x);
    c.state.gpa.destroy(c);
}

fn clearBox(box: gtk.Object) void {
    var child = gtk.gtk_widget_get_first_child(box);
    while (child != null) {
        const next = gtk.gtk_widget_get_next_sibling(child);
        gtk.gtk_box_remove(box, child);
        child = next;
    }
}

fn refreshPills(state: *AppState) void {
    const box = state.pills_box orelse return;
    clearBox(box);
    if (!hasAnyFilter(state)) {
        gtk.gtk_widget_set_visible(box, 0);
        return;
    }
    gtk.gtk_widget_set_visible(box, 1);

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    for (state.filter_statuses, 0..) |on, i| {
        if (!on) continue;
        const text = std.fmt.allocPrint(a, "Status: {s}  ✕", .{format.statuses[i].label}) catch continue;
        addPill(state, box, a, .status, i, null, text, null);
    }
    for (state.filter_priorities, 0..) |on, i| {
        if (!on) continue;
        const text = std.fmt.allocPrint(a, "Priority: {s}  ✕", .{format.priorities[i].label}) catch continue;
        addPill(state, box, a, .priority, i, null, text, null);
    }
    for (state.filter_labels.items) |fl| {
        const text = std.fmt.allocPrint(a, "{s}  ✕", .{fl.name}) catch continue;
        addPill(state, box, a, .label, 0, fl.id, text, fl.color);
    }
    addPill(state, box, a, .clear, 0, null, "Clear all", null);
}

fn addPill(
    state: *AppState,
    box: gtk.Object,
    arena: std.mem.Allocator,
    kind: @TypeOf(@as(PillCtx, undefined).kind),
    idx: usize,
    id: ?[]const u8,
    text: []const u8,
    color: ?[]const u8,
) void {
    const btn = gtk.gtk_button_new_with_label("");
    gtk.gtk_widget_add_css_class(btn, "exp-chip");
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 4);
    if (color) |c| gtk.gtk_box_append(row, widgets.dot(c));
    const lbl = gtk.gtk_label_new(null);
    if (arena.dupeZ(u8, text)) |z| gtk.gtk_label_set_text(lbl, z.ptr) else |_| {}
    gtk.gtk_box_append(row, lbl);
    gtk.gtk_button_set_child(btn, row);

    const ctx = state.gpa.create(PillCtx) catch return;
    ctx.* = .{
        .state = state,
        .kind = kind,
        .idx = idx,
        .id = if (id) |x| (state.gpa.dupeZ(u8, x) catch null) else null,
    };
    gtk.g_object_set_data_full(btn, "exp-ctx", @ptrCast(ctx), @ptrCast(&freePill));
    _ = gtk.g_signal_connect_data(btn, "clicked", @ptrCast(&onPillClicked), ctx, null, 0);
    gtk.gtk_box_append(box, btn);
}

fn onPillClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *PillCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    switch (ctx.kind) {
        .status => if (ctx.idx < state.filter_statuses.len) {
            state.filter_statuses[ctx.idx] = false;
        },
        .priority => if (ctx.idx < state.filter_priorities.len) {
            state.filter_priorities[ctx.idx] = false;
        },
        .label => if (ctx.id) |id| removeFilterLabel(state, id),
        .clear => clearFilters(state),
    }
    refreshIssues(state); // rebuilds the pills (frees this ctx) — don't touch ctx after
}

fn matchesStatusFilter(state: *AppState, value: []const u8) bool {
    if (!anyStatusFilter(state)) return true;
    return state.filter_statuses[statusOptionIndex(value)];
}

fn matchesPriorityFilter(state: *AppState, value: []const u8) bool {
    if (!anyPriorityFilter(state)) return true;
    return state.filter_priorities[priorityIndex(value)];
}

fn matchesLabelFilter(state: *AppState, chips: []const Database.LabelChip) bool {
    if (state.filter_labels.items.len == 0) return true;
    for (chips) |c| {
        for (state.filter_labels.items) |fl| {
            if (std.mem.eql(u8, fl.id, c.label_id)) return true;
        }
    }
    return false;
}

/// Refresh issues every time; rebuild the sidebar only when the project or
/// workspace set changes (so selecting a project isn't disrupted by updates).
fn doRefresh(state: *AppState) void {
    ensureActiveWorkspace(state);
    reconcileHeartbeat(state);
    reconcileAgent(state);
    refreshIssues(state);
    updateAccountLabel(state);
    const db = if (state.db) |*d| d else return;
    const pc = db.count("projects") catch 0;
    const wc = db.count("workspaces") catch 0;
    if (pc != state.shown_project_count or wc != state.shown_workspace_count) {
        state.shown_project_count = pc;
        state.shown_workspace_count = wc;
        refreshSidebar(state);
    }
}

/// Start/stop the desktop-agent heartbeat so it matches whether the ACTIVE
/// workspace has a stored agent identity on this machine. Called each refresh.
fn reconcileHeartbeat(state: *AppState) void {
    const want: ?[]const u8 = if (state.active_workspace_id) |ws|
        (if (identity_store.existsFor(state.gpa, ws)) ws else null)
    else
        null;

    const have = state.heartbeat_ws;
    const same = (want == null and have == null) or
        (want != null and have != null and std.mem.eql(u8, want.?, have.?));
    if (same) return;

    // Tear down any running heartbeat (worker frees itself).
    if (state.heartbeat) |hb| {
        hb.stop();
        state.heartbeat = null;
    }
    if (state.heartbeat_ws) |p| state.gpa.free(p);
    state.heartbeat_ws = null;

    // Start one for the desired workspace, if registered.
    if (want) |ws| {
        const instance = state.instance orelse return;
        const key = identity_store.readField(state.gpa, ws, "apiKey") orelse return;
        defer state.gpa.free(key);
        state.heartbeat = Heartbeat.spawn(state.gpa, instance, key, ws);
        if (state.heartbeat != null) state.heartbeat_ws = state.gpa.dupe(u8, ws) catch null;
    }
}

/// Start/stop the Rust agent-core loop to match whether the active workspace has
/// a stored agent identity. Built from the identity (api key + agent user id),
/// the GitHub token, and per-machine repo/worktree/db paths. Called each refresh.
fn reconcileAgent(state: *AppState) void {
    const want: ?[]const u8 = if (state.active_workspace_id) |ws|
        (if (identity_store.existsFor(state.gpa, ws)) ws else null)
    else
        null;

    const have = state.agent_core_ws;
    const same = (want == null and have == null) or
        (want != null and have != null and std.mem.eql(u8, want.?, have.?));
    if (same) return;

    if (state.agent_core) |m| {
        agent_manager.stop(m);
        state.agent_core = null;
    }
    if (state.agent_core_ws) |p| state.gpa.free(p);
    state.agent_core_ws = null;

    const ws = want orelse return;
    const instance = state.instance orelse return;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    const api_key = identity_store.readField(a, ws, "apiKey") orelse return;
    const agent_uid = identity_store.readField(a, ws, "agentUserId") orelse return;
    // agent-core fetches a fresh per-repo GitHub App installation token from the
    // server (companion.repoToken) just before clone/push, so the host no longer
    // feeds a token. Left empty (the agent self-fetches).
    const github = "";
    const dir = storage.configDir(a) catch return;
    const repos_root = std.fmt.allocPrint(a, "{s}/repos", .{dir}) catch return;
    const worktrees_root = std.fmt.allocPrint(a, "{s}/worktrees", .{dir}) catch return;
    const db_path = std.fmt.allocPrint(a, "{s}/agent-state-{s}.sqlite", .{ dir, ws }) catch return;

    const json = std.json.Stringify.valueAlloc(a, .{
        .baseUrl = instance,
        .apiKey = api_key,
        .botUserId = agent_uid,
        .githubToken = github,
        .reposRoot = repos_root,
        .worktreesRoot = worktrees_root,
        .branchPrefix = "agent",
        .driver = "claude",
        .dbPath = db_path,
        .maxConcurrent = 2,
        .timeoutS = 30,
    }, .{}) catch return;

    state.agent_core = agent_manager.start(state.gpa, json, ws);
    if (state.agent_core) |m| {
        state.agent_core_ws = state.gpa.dupe(u8, ws) catch null;
        // Route agent runs into the IDE-style terminal dock.
        if (state.term_dock) |d| agent_manager.setDock(m, @ptrCast(d), terminal_dock.mountForManager);
    }
}

/// Default the active workspace once sync delivers it. Prefer the user's own
/// (non-public, owned) default workspace so agent registration/run pins to the
/// right one — firstWorkspaceId can resolve to the synced public/shared workspace.
fn ensureActiveWorkspace(state: *AppState) void {
    if (state.active_workspace_id != null) return;
    const db = if (state.db) |*d| d else return;
    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    if (currentUserId(state, arena.allocator())) |uid| {
        if (db.defaultOwnedWorkspaceId(arena.allocator(), uid) catch null) |ws| {
            state.active_workspace_id = state.gpa.dupe(u8, ws) catch null;
            return;
        }
    }
    if (db.firstWorkspaceId(arena.allocator()) catch null) |ws|
        state.active_workspace_id = state.gpa.dupe(u8, ws) catch null;
}

fn currentUserId(state: *AppState, arena: std.mem.Allocator) ?[]const u8 {
    var store = AccountStore.open(state.gpa) catch return null;
    defer store.deinit();
    const acc = activeAccount(&store) orelse return null;
    const uid = acc.user_id orelse return null;
    return arena.dupe(u8, uid) catch null;
}

/// Whether the current user may set moderation fields (status/priority/assignee/
/// due/recurrence). Mirrors the server: on a public workspace, only members can;
/// a private workspace only syncs to members, so they always can.
fn canModerate(state: *AppState) bool {
    const db = if (state.db) |*d| d else return true;
    const ws = state.active_workspace_id orelse return true;
    if (!db.isWorkspacePublic(ws)) return true;
    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    // OAuth accounts don't persist a user_id; when we can't determine membership
    // don't restrict (the server is the real gate) — avoids locking out a
    // legitimate owner who signed in via OAuth.
    const uid = currentUserId(state, arena.allocator()) orelse return true;
    return db.isWorkspaceMember(ws, uid);
}

fn freeWorkspaceIds(state: *AppState) void {
    // Workspace ids are uuids (non-empty) on success; an OOM fallback leaves a
    // zero-length non-heap slice — only free the real ones.
    for (state.workspace_ids) |id| if (id.len > 0) state.gpa.free(id);
    if (state.workspace_ids.len > 0) state.gpa.free(state.workspace_ids);
    state.workspace_ids = &.{};
}

fn updateAccountLabel(state: *AppState) void {
    const lbl = state.sidebar_account orelse return;
    var store = AccountStore.open(state.gpa) catch return;
    defer store.deinit();
    const acc = activeAccount(&store) orelse return;
    const who = acc.user_email orelse acc.user_name orelse "Signed in";
    if (std.fmt.allocPrintSentinel(state.gpa, "{s}", .{who}, 0)) |z| {
        defer state.gpa.free(z);
        gtk.gtk_label_set_text(lbl, z.ptr);
    } else |_| {}
}

fn refreshSidebar(state: *AppState) void {
    const sidebar = state.sidebar_list orelse return;
    const db = if (state.db) |*d| d else return;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    // Workspace switcher: a full-width menu button (initial square + name +
    // chevron) opening a popover listing workspaces (✓ on active), plus
    // "New workspace" and "Workspace settings" — mirrors the web sidebar header.
    if (state.switcher_area) |area| {
        clearBox(area);
        state.switcher_popover = null;
        const workspaces = db.listWorkspaces(a) catch &[_]Database.WorkspaceRow{};
        if (workspaces.len > 0) {
            var active_name: []const u8 = workspaces[0].name;
            for (workspaces) |w| {
                if (state.active_workspace_id) |aw| {
                    if (std.mem.eql(u8, aw, w.id)) active_name = w.name;
                }
            }

            const btn = gtk.gtk_menu_button_new();
            gtk.gtk_widget_add_css_class(btn, "flat");
            const child = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
            gtk.gtk_box_append(child, widgets.avatar(a, active_name));
            const name_lbl = gtk.gtk_label_new(null);
            var nbuf: [200]u8 = undefined;
            if (std.fmt.bufPrintZ(&nbuf, "<b>{s}</b>", .{active_name[0..@min(active_name.len, 160)]})) |z| {
                gtk.gtk_label_set_markup(name_lbl, z.ptr);
            } else |_| {}
            gtk.gtk_widget_set_halign(name_lbl, gtk.ALIGN_START);
            gtk.gtk_widget_set_hexpand(name_lbl, 1);
            gtk.gtk_label_set_ellipsize(name_lbl, gtk.ELLIPSIZE_END);
            gtk.gtk_box_append(child, name_lbl);
            gtk.gtk_menu_button_set_child(btn, child);
            gtk.gtk_menu_button_set_always_show_arrow(btn, 1);

            const pop = gtk.gtk_popover_new();
            state.switcher_popover = pop;
            const pbox = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 2);
            gtk.gtk_widget_set_size_request(pbox, 220, -1);

            for (workspaces) |w| {
                const wctx = state.gpa.create(WsSwitchCtx) catch continue;
                wctx.state = state;
                wctx.ws_id = state.gpa.dupeZ(u8, w.id) catch {
                    state.gpa.destroy(wctx);
                    continue;
                };
                const rb = gtk.gtk_button_new();
                gtk.gtk_widget_add_css_class(rb, "flat");
                const rbox = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
                gtk.gtk_box_append(rbox, widgets.avatar(a, w.name));
                const wl = gtk.gtk_label_new(null);
                if (a.dupeZ(u8, w.name)) |z| gtk.gtk_label_set_text(wl, z.ptr) else |_| {}
                gtk.gtk_widget_set_halign(wl, gtk.ALIGN_START);
                gtk.gtk_widget_set_hexpand(wl, 1);
                gtk.gtk_box_append(rbox, wl);
                const is_active = if (state.active_workspace_id) |aw| std.mem.eql(u8, aw, w.id) else false;
                if (is_active) gtk.gtk_box_append(rbox, gtk.gtk_label_new("✓"));
                gtk.gtk_button_set_child(rb, rbox);
                // The ctx is owned by the row widget; freed when it's destroyed.
                gtk.g_object_set_data_full(rb, "exp-ws-ctx", @ptrCast(wctx), @ptrCast(&freeWsSwitch));
                _ = gtk.g_signal_connect_data(rb, "clicked", @ptrCast(&onSwitchWorkspace), wctx, null, 0);
                gtk.gtk_box_append(pbox, rb);
            }

            gtk.gtk_box_append(pbox, gtk.gtk_separator_new(gtk.ORIENTATION_HORIZONTAL));

            const new_ws = gtk.gtk_button_new_with_label("\u{FF0B} New workspace");
            gtk.gtk_widget_add_css_class(new_ws, "flat");
            _ = gtk.g_signal_connect_data(new_ws, "clicked", @ptrCast(&onNewWorkspaceClicked), state, null, 0);
            gtk.gtk_box_append(pbox, new_ws);

            const ws_settings = gtk.gtk_button_new_with_label("\u{2699} Workspace settings");
            gtk.gtk_widget_add_css_class(ws_settings, "flat");
            _ = gtk.g_signal_connect_data(ws_settings, "clicked", @ptrCast(&onSwitcherSettings), state, null, 0);
            gtk.gtk_box_append(pbox, ws_settings);

            gtk.gtk_popover_set_child(pop, pbox);
            gtk.gtk_menu_button_set_popover(btn, pop);
            gtk.gtk_box_append(area, btn);
        }
    }

    gtk.gtk_list_box_remove_all(sidebar);
    gtk.gtk_list_box_append(sidebar, sidebarRow(null, "All issues", "", "", state));

    const projects = db.listProjects(a, state.active_workspace_id) catch return;
    for (projects) |p| gtk.gtk_list_box_append(sidebar, sidebarRow(p.id, p.name, p.github_repo, p.color, state));

    // Re-assert the active row's selection (lost by remove_all) so the current
    // project stays highlighted. index 0 = "All issues" (null selection).
    var sel_index: c_int = 0;
    if (state.selected_project_id) |sel| {
        for (projects, 0..) |p, i| {
            if (std.mem.eql(u8, p.id, sel)) {
                sel_index = @intCast(i + 1);
                break;
            }
        }
    }
    const sel_row = gtk.gtk_list_box_get_row_at_index(sidebar, sel_index);
    if (sel_row != null) {
        state.suppress_project_select = true;
        gtk.gtk_list_box_select_row(sidebar, sel_row);
        state.suppress_project_select = false;
    }
}

/// Per-workspace-row context for the switcher popover (owns a duped id; freed
/// when the row button is destroyed via the connect destroy-notify).
const WsSwitchCtx = struct { state: *AppState, ws_id: [:0]u8 };

fn freeWsSwitch(p: gtk.gpointer) callconv(.c) void {
    const c: *WsSwitchCtx = @ptrCast(@alignCast(p));
    c.state.gpa.free(c.ws_id);
    c.state.gpa.destroy(c);
}

fn onSwitchWorkspace(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const c: *WsSwitchCtx = @ptrCast(@alignCast(data));
    switchWorkspace(c.state, c.ws_id);
}

/// Switch the active workspace and reset the project scope to "all issues".
/// `ws_id` is duped before any refresh (which destroys the calling row + its ctx).
fn switchWorkspace(state: *AppState, ws_id: []const u8) void {
    if (state.active_workspace_id) |aw| {
        if (std.mem.eql(u8, aw, ws_id)) {
            if (state.switcher_popover) |pop| gtk.gtk_popover_popdown(pop);
            return;
        }
    }
    // Dupe first; on OOM keep the current workspace rather than acting on the wrong one.
    const new_ws = state.gpa.dupe(u8, ws_id) catch return;
    if (state.active_workspace_id) |p| state.gpa.free(p);
    state.active_workspace_id = new_ws;
    if (state.selected_project_id) |p| state.gpa.free(p);
    state.selected_project_id = null;
    if (state.selected_project_name) |p| state.gpa.free(p);
    state.selected_project_name = null;
    if (state.selected_project_repo) |p| state.gpa.free(p);
    state.selected_project_repo = null;
    if (state.switcher_popover) |pop| gtk.gtk_popover_popdown(pop);
    updateRepoBanner(state);
    refreshSidebar(state); // rebuilds the switcher → frees the calling row's ctx
    refreshIssues(state);
}

fn onSwitcherSettings(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    if (state.switcher_popover) |pop| gtk.gtk_popover_popdown(pop);
    onSettingsClicked(null, data);
}

fn onNewWorkspaceClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    if (state.switcher_popover) |pop| gtk.gtk_popover_popdown(pop);
    openWorkspaceDialog(state);
}

/// A sidebar row carrying its project id + name + repo (GLib-owned, freed on
/// destroy); `id == null` means the "All issues" entry. Project rows lead with a
/// small colour dot (matching the web sidebar); the "All issues" row has none.
fn sidebarRow(id: ?[:0]const u8, name: [:0]const u8, repo: []const u8, color: []const u8, state: *AppState) gtk.Object {
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_set_margin_top(row, 6);
    gtk.gtk_widget_set_margin_bottom(row, 6);
    gtk.gtk_widget_set_margin_start(row, 8);
    gtk.gtk_widget_set_margin_end(row, 8);
    if (id != null and color.len > 0) gtk.gtk_box_append(row, widgets.dot(color));

    const lbl = gtk.gtk_label_new(name.ptr);
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(row, lbl);

    if (id) |pid| {
        if (state.gpa.dupeZ(u8, pid)) |tmp| {
            defer state.gpa.free(tmp);
            gtk.g_object_set_data_full(row, "exp-project-id", @ptrCast(gtk.g_strdup(tmp.ptr)), @ptrCast(&gtk.g_free));
        } else |_| {}
    }
    gtk.g_object_set_data_full(row, "exp-project-name", @ptrCast(gtk.g_strdup(name.ptr)), @ptrCast(&gtk.g_free));
    if (repo.len > 0) {
        if (state.gpa.dupeZ(u8, repo)) |tmp| {
            defer state.gpa.free(tmp);
            gtk.g_object_set_data_full(row, "exp-project-repo", @ptrCast(gtk.g_strdup(tmp.ptr)), @ptrCast(&gtk.g_free));
        } else |_| {}
    }
    return row;
}

fn onProjectSelected(_: gtk.Object, row: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    if (state.suppress_project_select) return; // re-selecting the active row on rebuild
    if (row == null) return;
    const child = gtk.gtk_list_box_row_get_child(row);
    const raw = gtk.g_object_get_data(child, "exp-project-id");
    const raw_name = gtk.g_object_get_data(child, "exp-project-name");
    const raw_repo = gtk.g_object_get_data(child, "exp-project-repo");

    if (state.selected_project_id) |p| state.gpa.free(p);
    state.selected_project_id = if (raw == null)
        null
    else
        state.gpa.dupe(u8, std.mem.span(@as([*:0]const u8, @ptrCast(raw)))) catch null;

    if (state.selected_project_name) |p| state.gpa.free(p);
    state.selected_project_name = if (raw == null or raw_name == null)
        null
    else
        state.gpa.dupe(u8, std.mem.span(@as([*:0]const u8, @ptrCast(raw_name)))) catch null;

    if (state.selected_project_repo) |p| state.gpa.free(p);
    state.selected_project_repo = if (raw_repo == null)
        null
    else
        state.gpa.dupe(u8, std.mem.span(@as([*:0]const u8, @ptrCast(raw_repo)))) catch null;

    updateRepoBanner(state);
    refreshIssues(state);
}

/// Show the GitHub repo banner for the selected project (hidden otherwise).
fn updateRepoBanner(state: *AppState) void {
    const banner = state.repo_banner orelse return;
    const repo = state.selected_project_repo orelse {
        gtk.gtk_widget_set_visible(banner, 0);
        return;
    };
    if (repo.len == 0) {
        gtk.gtk_widget_set_visible(banner, 0);
        return;
    }
    var buf: [256]u8 = undefined;
    if (std.fmt.bufPrintZ(&buf, "‹/›  {s}", .{repo[0..@min(repo.len, buf.len - 8)]})) |z| {
        gtk.gtk_button_set_label(banner, z.ptr);
    } else |_| {}
    gtk.gtk_widget_set_visible(banner, 1);
}

fn onRepoBannerClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    const repo = state.selected_project_repo orelse return;
    if (repo.len == 0) return;
    var buf: [320]u8 = undefined;
    const url = std.fmt.bufPrintZ(&buf, "https://github.com/{s}", .{repo}) catch return;
    _ = gtk.g_app_info_launch_default_for_uri(url.ptr, null, null);
}

/// Open the instance's /feedback page in the browser (the web route forwards to
/// an external feedback URL when one is configured, else shows the in-app form).
fn onFeedbackClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    const inst = state.instance orelse return;
    const base = std.mem.trimEnd(u8, inst, "/");
    var buf: [512]u8 = undefined;
    const url = std.fmt.bufPrintZ(&buf, "{s}/feedback", .{base}) catch return;
    _ = gtk.g_app_info_launch_default_for_uri(url.ptr, null, null);
}

fn onIssueActivated(_: gtk.Object, row: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    if (row == null) return;
    const child = gtk.gtk_list_box_row_get_child(row);

    // Group headers toggle collapse; issue rows open the detail pane.
    if (gtk.g_object_get_data(child, "exp-toggle-status")) |raw| {
        const status_value = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));
        const idx = statusDisplayIndex(status_value);
        state.collapsed[idx] = !state.collapsed[idx];
        refreshIssues(state);
        return;
    }
    const raw = gtk.g_object_get_data(child, "exp-issue-id") orelse return;
    showIssueDetail(state, std.mem.span(@as([*:0]const u8, @ptrCast(raw))));
}

fn showIssueDetail(state: *AppState, id: []const u8) void {
    const nav = state.content_nav orelse return;
    const db = if (state.db) |*d| d else return;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    const issue = (db.getIssue(a, id) catch null) orelse return;

    if (state.detail_issue_id) |p| state.gpa.free(p);
    state.detail_issue_id = state.gpa.dupe(u8, id) catch null;

    // Workspace members offered by @mention autocomplete in the description +
    // comment editors (agents excluded — you mention people). Lives in arena `a`,
    // valid for the rest of this builder; each editor dups what it needs.
    const mention_members: []const md.MentionMember = blk: {
        const rows = db.listMembers(a, issue.workspace_id) catch break :blk &.{};
        var ms: std.ArrayList(md.MentionMember) = .empty;
        for (rows) |m| {
            if (std.mem.eql(u8, m.role, "agent")) continue;
            ms.append(a, .{ .name = m.name, .email = m.email }) catch {};
        }
        break :blk ms.toOwnedSlice(a) catch &.{};
    };

    // The detail subpage: its own header bar with Edit (back is auto-injected by
    // AdwNavigationView).
    const detail_toolbar = gtk.adw_toolbar_view_new();
    const detail_header = gtk.adw_header_bar_new();
    gtk.adw_header_bar_set_show_start_title_buttons(detail_header, 0); // back button + content controls
    const edit_btn = gtk.gtk_button_new_with_label("Edit");
    gtk.gtk_widget_add_css_class(edit_btn, "flat");
    _ = gtk.g_signal_connect_data(edit_btn, "clicked", @ptrCast(&onEditClicked), state, null, 0);
    gtk.adw_header_bar_pack_end(detail_header, edit_btn);

    // "AI" — start an interactive agent session on this issue. Only when a
    // desktop agent is registered for this workspace (the core is running).
    if (state.agent_core != null) {
        const ai_btn = gtk.gtk_button_new_with_label("AI");
        gtk.gtk_widget_add_css_class(ai_btn, "flat");
        gtk.gtk_widget_set_tooltip_text(ai_btn, "Start an interactive agent session for this issue");
        if (makeIssueActionCtx(state, id)) |ctx| {
            gtk.g_object_set_data_full(ai_btn, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeIssueActionCtx));
            _ = gtk.g_signal_connect_data(ai_btn, "clicked", @ptrCast(&onAiClicked), ctx, null, 0);
        }
        gtk.adw_header_bar_pack_end(detail_header, ai_btn);

        // "Cancel" — stop the run in flight for this issue. Shown only while the
        // agent is actively working (not parked awaiting approval/answer).
        if (isAgentBusy(issue.agent_plan_state)) {
            const cancel_btn = gtk.gtk_button_new_with_label("Cancel");
            gtk.gtk_widget_add_css_class(cancel_btn, "flat");
            gtk.gtk_widget_set_tooltip_text(cancel_btn, "Cancel the agent run for this issue");
            if (makeIssueActionCtx(state, id)) |ctx| {
                gtk.g_object_set_data_full(cancel_btn, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeIssueActionCtx));
                _ = gtk.g_signal_connect_data(cancel_btn, "clicked", @ptrCast(&onCancelClicked), ctx, null, 0);
            }
            gtk.adw_header_bar_pack_end(detail_header, cancel_btn);
        }
    }

    // "Changes" — open the agent's PR (the diff) in the browser. Shown only when
    // the issue has a synced PR.
    if (issue.pr_url.len > 0) {
        const changes_btn = gtk.gtk_button_new_with_label("Changes");
        gtk.gtk_widget_add_css_class(changes_btn, "flat");
        gtk.gtk_widget_set_tooltip_text(changes_btn, "View the agent's pull request");
        if (makeOpenUrlCtx(state, issue.pr_url)) |ctx| {
            gtk.g_object_set_data_full(changes_btn, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeOpenUrlCtx));
            _ = gtk.g_signal_connect_data(changes_btn, "clicked", @ptrCast(&onOpenUrl), ctx, null, 0);
        }
        gtk.adw_header_bar_pack_end(detail_header, changes_btn);
    }
    gtk.adw_toolbar_view_add_top_bar(detail_toolbar, detail_header);

    const scrolled = gtk.gtk_scrolled_window_new();
    gtk.gtk_widget_set_vexpand(scrolled, 1);

    const box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 12);
    gtk.gtk_widget_set_margin_top(box, 18);
    gtk.gtk_widget_set_margin_bottom(box, 18);
    gtk.gtk_widget_set_margin_start(box, 18);
    gtk.gtk_widget_set_margin_end(box, 18);

    // Editable title (saves on blur).
    const title_entry = gtk.gtk_entry_new();
    gtk.gtk_editable_set_text(title_entry, issue.title.ptr);
    gtk.gtk_widget_add_css_class(title_entry, "exp-title-entry");
    gtk.gtk_box_append(box, title_entry);

    // Properties: inline-editable status / priority / due / assignee. On a public
    // workspace, non-members can't moderate, so these are disabled (server-gated).
    const can_mod = canModerate(state);
    const props = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_set_margin_top(props, 2);
    const status_m = statusMenu(state, id, issue.status);
    const priority_m = priorityMenu(state, id, issue.priority);
    const due_m = dueMenu(state, id, issue.due_date);
    const assignee_m = assigneeMenu(state, a, id, issue.assignee);
    if (!can_mod) {
        gtk.gtk_widget_set_sensitive(status_m, 0);
        gtk.gtk_widget_set_sensitive(priority_m, 0);
        gtk.gtk_widget_set_sensitive(due_m, 0);
        gtk.gtk_widget_set_sensitive(assignee_m, 0);
    }
    gtk.gtk_box_append(props, status_m);
    gtk.gtk_box_append(props, priority_m);
    gtk.gtk_box_append(props, due_m);
    gtk.gtk_box_append(props, assignee_m);
    gtk.gtk_box_append(box, props);

    // Labels: chips + a picker. Chips live in their own box so toggling a label
    // updates it in place (optimistic) without rebuilding the description.
    const labels_row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    const chips_box = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    if (db.listAllIssueLabels(a)) |chips| {
        for (chips) |c| {
            if (!std.mem.eql(u8, c.issue_id, id)) continue;
            gtk.gtk_box_append(chips_box, labeledChip(state, a, c.label_id, c.name, c.color));
        }
    } else |_| {}
    gtk.gtk_box_append(labels_row, chips_box);
    gtk.gtk_box_append(labels_row, labelMenu(state, a, id, issue.workspace_id, chips_box));
    gtk.gtk_box_append(box, labels_row);

    // Description — a fully editable inline markdown editor (styling + async
    // images + upload), saving on blur. The DetailEditCtx owns the issue id so
    // image upload + the blur handlers have a stable pointer.
    gtk.gtk_box_append(box, widgets.sectionTitle("Description"));
    if (md.MarkdownEditor.create(state.gpa)) |ed| {
        const ctx = buildDetailEditCtx(state, id, title_entry, issue.title, ed, issue.description);
        const ctx_issue_id: ?[]const u8 = if (ctx) |c| c.issue_id else null;
        ed.setIssueContext(state.instance, state.token, ctx_issue_id);
        ed.setMentionMembers(mention_members);
        ed.setText(issue.description);
        // Inline sizing: grow with content (min 180, cap 600) rather than eat all space.
        gtk.gtk_widget_set_vexpand(ed.scrolled, 0);
        gtk.gtk_scrolled_window_set_propagate_natural_height(ed.scrolled, 1);
        gtk.gtk_scrolled_window_set_max_content_height(ed.scrolled, 600);
        gtk.gtk_widget_set_size_request(ed.container, -1, 180);
        gtk.g_object_set_data_full(box, "exp-detail-editor", @ptrCast(ed), @ptrCast(&destroyDetailEditor));
        if (ctx) |c| {
            gtk.g_object_set_data_full(box, "exp-detail-edit", @ptrCast(c), @ptrCast(&freeDetailEdit));
            attachBlur(title_entry, &onTitleBlur, c);
            attachBlur(ed.view, &onDescBlur, c);
        }
        gtk.gtk_box_append(box, ed.container);
    }

    // Comments + activity. Plan/question text lives in the structured store
    // (issue_agent_state), fetched on demand via agentPlan.getState — NOT in
    // comments — so we render a first-class plan panel + a human-only thread.
    const comments = db.listComments(a, id) catch &[_]Database.CommentRow{};
    const events = db.listIssueEvents(a, id) catch &[_]Database.IssueEventRow{};

    // The latest agent lifecycle event being an error drives the Retry CTA.
    var latest_is_error = false;
    {
        var i: usize = events.len;
        while (i > 0) {
            i -= 1;
            if (isAgentEvent(events[i].type)) {
                latest_is_error = std.mem.eql(u8, events[i].type, "agent_error");
                break;
            }
        }
    }

    // Plan/question TEXT now comes from the synced `agent_runs` shape (read in
    // getIssue) — no blocking agentPlan.getState round-trip on detail open.
    const plan_text: []const u8 = issue.plan_text;
    const question_text: []const u8 = issue.question;

    // First-class agent plan panel (above the human thread).
    if (agentPlanPanel(state, a, id, issue.agent_plan_state, issue.agent_plan_approver, plan_text, question_text, latest_is_error)) |panel| {
        gtk.gtk_box_append(box, panel);
    }

    // Human conversation: regular comments only (plan/question are in the panel).
    var regular_count: usize = 0;
    for (comments) |c| {
        if (std.mem.eql(u8, c.kind, "regular")) regular_count += 1;
    }
    const ch = gtk.gtk_label_new(null);
    if (std.fmt.allocPrintSentinel(a, "Comments ({d})", .{regular_count}, 0)) |c| {
        gtk.gtk_label_set_text(ch, c.ptr);
    } else |_| {}
    gtk.gtk_widget_add_css_class(ch, "title-4");
    gtk.gtk_widget_set_halign(ch, gtk.ALIGN_START);
    gtk.gtk_widget_set_margin_top(ch, 8);
    gtk.gtk_box_append(box, ch);

    // Linear-style activity timeline: merge regular comments + synced issue_events
    // by created_at (both come back ordered, so a linear two-pointer merge works).
    const TimelineItem = union(enum) { comment: Database.CommentRow, event: Database.IssueEventRow };
    var timeline = std.ArrayList(TimelineItem).empty;
    {
        var ci: usize = 0;
        var ei: usize = 0;
        while (ci < comments.len or ei < events.len) {
            const take_comment = if (ci >= comments.len)
                false
            else if (ei >= events.len)
                true
            else
                std.mem.order(u8, comments[ci].created_at, events[ei].created_at) != .gt;
            if (take_comment) {
                timeline.append(a, .{ .comment = comments[ci] }) catch {};
                ci += 1;
            } else {
                timeline.append(a, .{ .event = events[ei] }) catch {};
                ei += 1;
            }
        }
    }

    const comments_box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 6);
    for (timeline.items) |item| {
        switch (item) {
            .comment => |c| {
                // Skip plan/question comments — the panel renders those now.
                if (!std.mem.eql(u8, c.kind, "regular")) continue;
                gtk.gtk_box_append(comments_box, commentBubble(a, c.author, c.kind, c.body, false, false));
            },
            .event => |e| gtk.gtk_box_append(comments_box, eventLine(a, e)),
        }
    }
    gtk.gtk_box_append(box, comments_box);

    gtk.gtk_box_append(box, commentComposer(state, id, comments_box, mention_members));

    gtk.gtk_scrolled_window_set_child(scrolled, box);
    gtk.adw_toolbar_view_set_content(detail_toolbar, scrolled);
    const page = gtk.adw_navigation_page_new(detail_toolbar, issue.identifier.ptr);

    // Always normalise the stack to [list, detail]: opening from the list shows
    // the subpage, and refreshing an already-open issue (after an edit / inline
    // label create) swaps it in place instead of stacking. Back returns to the
    // list either way; the old detail page is destroyed (freeing its editor).
    var pages = [_]gtk.Object{ state.list_page, page };
    gtk.adw_navigation_view_replace(nav, &pages, 2);
}

fn destroyDetailEditor(p: gtk.gpointer) callconv(.c) void {
    const ed: *md.MarkdownEditor = @ptrCast(@alignCast(p));
    ed.destroy();
}

// --- Inline detail editing: title + description save on blur ---

const DetailEditCtx = struct {
    state: *AppState,
    issue_id: [:0]u8,
    title_entry: gtk.Object,
    title_original: []u8,
    editor: *md.MarkdownEditor,
    desc_original: []u8,
};

fn buildDetailEditCtx(state: *AppState, id: []const u8, title_entry: gtk.Object, title: []const u8, editor: *md.MarkdownEditor, desc: []const u8) ?*DetailEditCtx {
    const gpa = state.gpa;
    const ctx = gpa.create(DetailEditCtx) catch return null;
    ctx.state = state;
    ctx.title_entry = title_entry;
    ctx.editor = editor;
    ctx.issue_id = gpa.dupeZ(u8, id) catch {
        gpa.destroy(ctx);
        return null;
    };
    ctx.title_original = gpa.dupe(u8, title) catch {
        gpa.free(ctx.issue_id);
        gpa.destroy(ctx);
        return null;
    };
    ctx.desc_original = gpa.dupe(u8, desc) catch {
        gpa.free(ctx.issue_id);
        gpa.free(ctx.title_original);
        gpa.destroy(ctx);
        return null;
    };
    return ctx;
}

fn freeDetailEdit(p: gtk.gpointer) callconv(.c) void {
    const c: *DetailEditCtx = @ptrCast(@alignCast(p));
    const gpa = c.state.gpa;
    gpa.free(c.issue_id);
    gpa.free(c.title_original);
    gpa.free(c.desc_original);
    gpa.destroy(c);
}

/// Attach a GtkEventControllerFocus whose "leave" fires `handler(controller, ctx)`.
fn attachBlur(widget: gtk.Object, handler: *const fn (gtk.Object, gtk.gpointer) callconv(.c) void, ctx: *DetailEditCtx) void {
    const fc = gtk.gtk_event_controller_focus_new();
    _ = gtk.g_signal_connect_data(fc, "leave", @ptrCast(handler), ctx, null, 0);
    gtk.gtk_widget_add_controller(widget, fc);
}

fn onTitleBlur(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *DetailEditCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    // Skip if the widget is no longer rooted — a "leave" emitted during page
    // teardown must not touch a dying widget tree.
    if (gtk.gtk_widget_get_root(ctx.title_entry) == null) return;
    const title = std.mem.trim(u8, std.mem.span(gtk.gtk_editable_get_text(ctx.title_entry)), " \t\r\n");
    if (title.len == 0 or std.mem.eql(u8, title, ctx.title_original)) return;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const json = std.json.Stringify.valueAlloc(arena.allocator(), .{ .id = ctx.issue_id, .title = title }, .{}) catch return;
    mutate.fire(state.gpa, state.instance.?, state.token, "issues.update", json);
    if (state.gpa.dupe(u8, title)) |d| {
        state.gpa.free(ctx.title_original);
        ctx.title_original = d;
    } else |_| {} // keep old original → re-saves next blur (harmless)
}

fn onDescBlur(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *DetailEditCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    // Skip if the editor is no longer rooted (teardown) — don't read a dying buffer.
    if (gtk.gtk_widget_get_root(ctx.editor.view) == null) return;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const text = ctx.editor.getText(a) catch return;
    if (std.mem.eql(u8, text, ctx.desc_original)) return;

    const json = std.json.Stringify.valueAlloc(a, .{ .id = ctx.issue_id, .description = text }, .{}) catch return;
    mutate.fire(state.gpa, state.instance.?, state.token, "issues.update", json);
    if (state.gpa.dupe(u8, text)) |d| {
        state.gpa.free(ctx.desc_original);
        ctx.desc_original = d;
    } else |_| {}
}

fn commentBubble(arena: std.mem.Allocator, author: []const u8, kind: []const u8, body: []const u8, awaiting_answer: bool, latest_question: bool) gtk.Object {
    const cbox = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 2);

    // Agent plan/question comments get distinct cards + headers (web parity).
    const header = if (std.mem.eql(u8, kind, "plan"))
        std.fmt.allocPrintSentinel(arena, "✦ Plan · {s}", .{author}, 0) catch null
    else if (std.mem.eql(u8, kind, "question"))
        std.fmt.allocPrintSentinel(arena, "{s} · {s}", .{ if (awaiting_answer and latest_question) "⚠ Waiting for your answer" else "Question", author }, 0) catch null
    else
        std.fmt.allocPrintSentinel(arena, "{s}", .{author}, 0) catch null;
    gtk.gtk_widget_add_css_class(cbox, if (std.mem.eql(u8, kind, "plan"))
        "exp-plan"
    else if (std.mem.eql(u8, kind, "question"))
        "exp-question"
    else
        "exp-comment");

    const author_lbl = gtk.gtk_label_new(null);
    if (header) |t| gtk.gtk_label_set_text(author_lbl, t.ptr);
    gtk.gtk_widget_add_css_class(author_lbl, "dim-label");
    gtk.gtk_widget_add_css_class(author_lbl, "caption-heading");
    gtk.gtk_widget_set_halign(author_lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(cbox, author_lbl);

    const body_lbl = gtk.gtk_label_new(null);
    if (arena.dupeZ(u8, body)) |z| gtk.gtk_label_set_text(body_lbl, z.ptr) else |_| {}
    gtk.gtk_label_set_wrap(body_lbl, 1);
    gtk.gtk_label_set_xalign(body_lbl, 0.0);
    gtk.gtk_label_set_selectable(body_lbl, 1);
    gtk.gtk_widget_set_halign(body_lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(cbox, body_lbl);
    return cbox;
}

// --- Agent plan approval (human-side: approve / request changes) ---

const PlanActionCtx = struct {
    state: *AppState,
    issue_id: [:0]u8,
    approve: bool,
    action_row: gtk.Object,
};

fn freePlanAction(p: gtk.gpointer) callconv(.c) void {
    const c: *PlanActionCtx = @ptrCast(@alignCast(p));
    c.state.gpa.free(c.issue_id);
    c.state.gpa.destroy(c);
}

/// Under the latest plan comment: Approve / Request-changes when the issue is
/// awaiting approval, or a green "Approved" badge once approved.
fn appendPlanActions(state: *AppState, box: gtk.Object, issue_id: []const u8, plan_state: []const u8, approver: []const u8) void {
    if (std.mem.eql(u8, plan_state, "approved")) {
        const lbl = gtk.gtk_label_new(null);
        var buf: [256]u8 = undefined;
        const txt = if (approver.len > 0)
            std.fmt.bufPrintZ(&buf, "<span foreground='#22c55e'>✓ Approved by {s}</span>", .{approver[0..@min(approver.len, 200)]}) catch null
        else
            std.fmt.bufPrintZ(&buf, "<span foreground='#22c55e'>✓ Approved</span>", .{}) catch null;
        if (txt) |t| gtk.gtk_label_set_markup(lbl, t.ptr);
        gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
        gtk.gtk_box_append(box, lbl);
        return;
    }
    if (!std.mem.eql(u8, plan_state, "awaiting_approval")) return;

    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    const approve = gtk.gtk_button_new_with_label("Approve");
    gtk.gtk_widget_add_css_class(approve, "suggested-action");
    const reject = gtk.gtk_button_new_with_label("Request changes");
    gtk.gtk_widget_add_css_class(reject, "flat");
    const hint = gtk.gtk_label_new("or reply below to refine");
    gtk.gtk_widget_add_css_class(hint, "dim-label");
    gtk.gtk_widget_add_css_class(hint, "caption");
    gtk.gtk_box_append(row, approve);
    gtk.gtk_box_append(row, reject);
    // "Approve & continue here" — approve with the human session, then resume
    // the agent interactively in a terminal (desktop-registered workspaces only).
    if (state.agent_core != null) {
        const cont = gtk.gtk_button_new_with_label("Approve & continue here");
        gtk.gtk_widget_add_css_class(cont, "flat");
        gtk.gtk_widget_set_tooltip_text(cont, "Approve and continue the agent interactively in a terminal");
        if (makeIssueActionCtx(state, issue_id)) |ctx| {
            gtk.g_object_set_data_full(cont, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeIssueActionCtx));
            _ = gtk.g_signal_connect_data(cont, "clicked", @ptrCast(&onApproveInteractive), ctx, null, 0);
        }
        gtk.gtk_box_append(row, cont);
    }
    gtk.gtk_box_append(row, hint);
    wirePlanButton(state, approve, issue_id, true, row);
    wirePlanButton(state, reject, issue_id, false, row);
    gtk.gtk_box_append(box, row);
}

// --- Structured agent plan panel (web / iOS / Android parity) ---

fn isAgentEvent(t: []const u8) bool {
    const kinds = [_][]const u8{ "agent_started", "plan_ready", "agent_question", "agent_answer", "pr_opened", "pr_merged", "agent_error" };
    for (kinds) |k| {
        if (std.mem.eql(u8, t, k)) return true;
    }
    return false;
}

fn planLabel(a: std.mem.Allocator, text: []const u8, dim: bool) gtk.Object {
    const lbl = gtk.gtk_label_new(null);
    if (a.dupeZ(u8, text)) |z| gtk.gtk_label_set_text(lbl, z.ptr) else |_| {}
    gtk.gtk_label_set_wrap(lbl, 1);
    gtk.gtk_label_set_xalign(lbl, 0.0);
    gtk.gtk_label_set_selectable(lbl, 1);
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    if (dim) gtk.gtk_widget_add_css_class(lbl, "dim-label");
    return lbl;
}

/// The structured agent plan/question panel. Lifecycle is driven by `plan_state`;
/// the plan/question TEXT is fetched by the caller via agentPlan.getState
/// (server-only `issue_agent_state`, not synced) and passed in. Returns null when
/// the issue has no agent activity.
fn agentPlanPanel(
    state: *AppState,
    a: std.mem.Allocator,
    issue_id: []const u8,
    plan_state: []const u8,
    approver: []const u8,
    plan_text: []const u8,
    question_text: []const u8,
    latest_is_error: bool,
) ?gtk.Object {
    if (plan_state.len == 0 and !latest_is_error) return null;

    const card = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 8);
    gtk.gtk_widget_add_css_class(card, "exp-plan");
    gtk.gtk_widget_set_margin_top(card, 8);

    const title = gtk.gtk_label_new("✦ Agent plan");
    gtk.gtk_widget_add_css_class(title, "caption-heading");
    gtk.gtk_widget_set_halign(title, gtk.ALIGN_START);
    gtk.gtk_box_append(card, title);

    if (std.mem.eql(u8, plan_state, "drafting") or std.mem.eql(u8, plan_state, "planning")) {
        gtk.gtk_box_append(card, planLabel(a, "Agent is working on a plan…", true));
    } else if (std.mem.eql(u8, plan_state, "awaiting_answer")) {
        gtk.gtk_box_append(card, planLabel(a, "⚠ The agent has a question", false));
        gtk.gtk_box_append(card, planLabel(a, if (question_text.len > 0) question_text else "Loading…", false));
        if (canModerate(state)) gtk.gtk_box_append(card, answerComposer(state, issue_id));
    } else if (std.mem.eql(u8, plan_state, "awaiting_approval") or std.mem.eql(u8, plan_state, "approved")) {
        gtk.gtk_box_append(card, planLabel(a, if (plan_text.len > 0) plan_text else "Loading plan…", false));
        // Approve / Request changes (awaiting_approval) or the "Approved by X"
        // badge (approved) — reuse the existing affordance.
        appendPlanActions(state, card, issue_id, plan_state, approver);
    }

    if (latest_is_error) {
        const err_row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
        gtk.gtk_widget_set_margin_top(err_row, 4);
        const el = gtk.gtk_label_new(null);
        gtk.gtk_label_set_markup(el, "<span foreground='#f87171'>The agent hit an error.</span>");
        gtk.gtk_widget_set_halign(el, gtk.ALIGN_START);
        gtk.gtk_box_append(err_row, el);
        const retry = gtk.gtk_button_new_with_label("Retry");
        gtk.gtk_widget_add_css_class(retry, "flat");
        if (makeIssueActionCtx(state, issue_id)) |ctx| {
            gtk.g_object_set_data_full(retry, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeIssueActionCtx));
            _ = gtk.g_signal_connect_data(retry, "clicked", @ptrCast(&onRetryClicked), ctx, null, 0);
        }
        gtk.gtk_box_append(err_row, retry);
        gtk.gtk_box_append(card, err_row);
    }

    return card;
}

fn onRetryClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *IssueActionCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const json = std.json.Stringify.valueAlloc(arena.allocator(), .{ .issueId = ctx.issue_id }, .{}) catch return;
    if (state.instance) |inst| mutate.fire(state.gpa, inst, state.token, "agentPlan.retry", json);
}

// --- Answer composer (human answers the agent's open question) ---

const AnswerCtx = struct {
    state: *AppState,
    issue_id: [:0]u8,
    entry: gtk.Object,
};

fn freeAnswer(p: gtk.gpointer) callconv(.c) void {
    const c: *AnswerCtx = @ptrCast(@alignCast(p));
    c.state.gpa.free(c.issue_id);
    c.state.gpa.destroy(c);
}

fn answerComposer(state: *AppState, issue_id: []const u8) gtk.Object {
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    gtk.gtk_widget_set_margin_top(row, 4);

    const view = gtk.gtk_text_view_new();
    gtk.gtk_text_view_set_wrap_mode(view, gtk.WRAP_WORD_CHAR);
    gtk.gtk_text_view_set_top_margin(view, 6);
    gtk.gtk_text_view_set_left_margin(view, 8);
    const scrolled = gtk.gtk_scrolled_window_new();
    gtk.gtk_widget_add_css_class(scrolled, "card");
    gtk.gtk_scrolled_window_set_child(scrolled, view);
    gtk.gtk_scrolled_window_set_max_content_height(scrolled, 120);
    gtk.gtk_widget_set_size_request(scrolled, -1, 56);
    gtk.gtk_widget_set_hexpand(scrolled, 1);
    gtk.gtk_box_append(row, scrolled);

    const btn = gtk.gtk_button_new_with_label("Send answer");
    gtk.gtk_widget_add_css_class(btn, "suggested-action");
    gtk.gtk_widget_set_valign(btn, gtk.ALIGN_END);
    gtk.gtk_box_append(row, btn);

    const ctx = state.gpa.create(AnswerCtx) catch return row;
    ctx.* = .{
        .state = state,
        .issue_id = state.gpa.dupeZ(u8, issue_id) catch {
            state.gpa.destroy(ctx);
            return row;
        },
        .entry = view,
    };
    gtk.g_object_set_data_full(btn, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeAnswer));
    _ = gtk.g_signal_connect_data(btn, "clicked", @ptrCast(&onAnswerSubmit), ctx, null, 0);
    return row;
}

fn onAnswerSubmit(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *AnswerCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    const buffer = gtk.gtk_text_view_get_buffer(ctx.entry);
    var start: [128]u8 align(8) = undefined;
    var end: [128]u8 align(8) = undefined;
    gtk.gtk_text_buffer_get_bounds(buffer, @ptrCast(&start), @ptrCast(&end));
    const raw = gtk.gtk_text_buffer_get_text(buffer, @ptrCast(&start), @ptrCast(&end), 0) orelse return;
    defer gtk.g_free(@ptrCast(raw));
    const text = std.mem.trim(u8, std.mem.span(raw), " \t\r\n");
    if (text.len == 0) return;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const json = std.json.Stringify.valueAlloc(arena.allocator(), .{
        .issueId = ctx.issue_id,
        .answer = text,
    }, .{}) catch return;
    if (state.instance) |inst| mutate.fire(state.gpa, inst, state.token, "agentPlan.answerQuestion", json);
    gtk.gtk_text_buffer_set_text(buffer, "", 0);
}

// --- Interactive agent actions (AI button + approve-and-continue) ---

const IssueActionCtx = struct { state: *AppState, issue_id: [:0]u8 };

fn makeIssueActionCtx(state: *AppState, id: []const u8) ?*IssueActionCtx {
    const ctx = state.gpa.create(IssueActionCtx) catch return null;
    ctx.state = state;
    ctx.issue_id = state.gpa.dupeZ(u8, id) catch {
        state.gpa.destroy(ctx);
        return null;
    };
    return ctx;
}

fn freeIssueActionCtx(p: gtk.gpointer) callconv(.c) void {
    const ctx: *IssueActionCtx = @ptrCast(@alignCast(p));
    ctx.state.gpa.free(ctx.issue_id);
    ctx.state.gpa.destroy(ctx);
}

fn onAiClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *IssueActionCtx = @ptrCast(@alignCast(data));
    if (ctx.state.agent_core) |m| agent_manager.requestInteractive(m, ctx.issue_id);
}

fn onCancelClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *IssueActionCtx = @ptrCast(@alignCast(data));
    if (ctx.state.agent_core) |m| agent_manager.cancelIssue(m, ctx.issue_id);
}

/// Whether the agent is actively working an issue (so a "Cancel" button makes
/// sense) — i.e. not parked awaiting human input and not idle.
fn isAgentBusy(agent_plan_state: []const u8) bool {
    const busy = [_][]const u8{ "drafting", "planning", "approved", "coding" };
    for (busy) |s| {
        if (std.mem.eql(u8, agent_plan_state, s)) return true;
    }
    return false;
}

const OpenUrlCtx = struct { gpa: std.mem.Allocator, url: [:0]u8 };

fn makeOpenUrlCtx(state: *AppState, url: []const u8) ?*OpenUrlCtx {
    const ctx = state.gpa.create(OpenUrlCtx) catch return null;
    ctx.gpa = state.gpa;
    ctx.url = state.gpa.dupeZ(u8, url) catch {
        state.gpa.destroy(ctx);
        return null;
    };
    return ctx;
}

fn freeOpenUrlCtx(p: gtk.gpointer) callconv(.c) void {
    const ctx: *OpenUrlCtx = @ptrCast(@alignCast(p));
    ctx.gpa.free(ctx.url);
    ctx.gpa.destroy(ctx);
}

fn onOpenUrl(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *OpenUrlCtx = @ptrCast(@alignCast(data));
    _ = gtk.g_app_info_launch_default_for_uri(ctx.url.ptr, null, null);
}

fn onApproveInteractive(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *IssueActionCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    // 1. Approve with the human session (the agent credential can't self-approve).
    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    if (std.json.Stringify.valueAlloc(arena.allocator(), .{ .issueId = ctx.issue_id }, .{})) |json| {
        if (state.instance) |inst| {
            mutate.fire(state.gpa, inst, state.token, "agentPlan.approvePlan", json);
        }
    } else |_| {}
    // 2. Resume the interactive session to implement the plan.
    if (state.agent_core) |m| agent_manager.approveInteractive(m, ctx.issue_id);
}

/// One compact activity line for the merged timeline (status/assignee/label/PR/
/// plan/error events synced via the issue_events shape).
fn eventLine(a: std.mem.Allocator, e: Database.IssueEventRow) gtk.Object {
    const verb = if (std.mem.eql(u8, e.type, "status_changed"))
        "changed the status"
    else if (std.mem.eql(u8, e.type, "assignee_changed"))
        "changed the assignee"
    else if (std.mem.eql(u8, e.type, "label_added"))
        "added a label"
    else if (std.mem.eql(u8, e.type, "label_removed"))
        "removed a label"
    else if (std.mem.eql(u8, e.type, "pr_opened"))
        "opened a pull request"
    else if (std.mem.eql(u8, e.type, "pr_merged"))
        "merged the pull request"
    else if (std.mem.eql(u8, e.type, "plan_ready"))
        "shared a plan"
    else if (std.mem.eql(u8, e.type, "agent_error"))
        "hit an error"
    else
        e.type;
    const who = if (e.actor.len > 0) e.actor else "Someone";
    const lbl = gtk.gtk_label_new(null);
    if (std.fmt.allocPrintSentinel(a, "• {s} {s}", .{ who, verb }, 0)) |t| {
        gtk.gtk_label_set_text(lbl, t.ptr);
    } else |_| {}
    gtk.gtk_widget_add_css_class(lbl, "dim-label");
    gtk.gtk_widget_add_css_class(lbl, "caption");
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    gtk.gtk_widget_set_margin_start(lbl, 4);
    return lbl;
}

fn wirePlanButton(state: *AppState, btn: gtk.Object, issue_id: []const u8, approve: bool, row: gtk.Object) void {
    const ctx = state.gpa.create(PlanActionCtx) catch return;
    ctx.state = state;
    ctx.approve = approve;
    ctx.action_row = row;
    ctx.issue_id = state.gpa.dupeZ(u8, issue_id) catch {
        state.gpa.destroy(ctx);
        return;
    };
    gtk.g_object_set_data_full(btn, "exp-ctx", @ptrCast(ctx), @ptrCast(&freePlanAction));
    _ = gtk.g_signal_connect_data(btn, "clicked", @ptrCast(&onPlanAction), ctx, null, 0);
}

fn onPlanAction(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *PlanActionCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    const row = ctx.action_row; // local: row outlives ctx (cleared below frees ctx)
    const approve = ctx.approve;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const json = std.json.Stringify.valueAlloc(arena.allocator(), .{ .issueId = ctx.issue_id }, .{}) catch return;
    mutate.fire(state.gpa, state.instance.?, state.token, if (approve) "agentPlan.approvePlan" else "agentPlan.requestChanges", json);

    // Optimistic: replace the buttons with a status (sync delivers the real state).
    clearBox(row); // destroys the buttons → frees their ctxs; don't touch ctx after
    const lbl = gtk.gtk_label_new(null);
    gtk.gtk_label_set_markup(lbl, if (approve)
        "<span foreground='#22c55e'>✓ Plan approved</span>"
    else
        "<span foreground='#eab308'>Changes requested</span>");
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    gtk.gtk_box_append(row, lbl);
}

// ---------------------------------------------------------------------------
// Inline property editing (status / priority / assignee popovers)
// ---------------------------------------------------------------------------

const PropKind = enum { status, priority, assignee };

const PropCtx = struct {
    state: *AppState,
    issue_id: [:0]u8,
    field: []const u8, // "status" | "priority" | "assigneeId"
    kind: PropKind,
    value: ?[:0]u8, // null → JSON null (Unassigned)
    display: [:0]u8,
    button: gtk.Object,
    popover: gtk.Object,
};

fn freeProp(p: gtk.gpointer) callconv(.c) void {
    const c: *PropCtx = @ptrCast(@alignCast(p));
    const gpa = c.state.gpa;
    gpa.free(c.issue_id);
    if (c.value) |v| gpa.free(v);
    gpa.free(c.display);
    gpa.destroy(c);
}

fn personGlyph() gtk.Object {
    const lbl = gtk.gtk_label_new("👤");
    gtk.gtk_widget_add_css_class(lbl, "dim-label");
    return lbl;
}

/// The contents shown on a property menu button: icon (for status/priority) +
/// a text label. Display/value are short; a stack buffer suffices.
fn propChild(kind: PropKind, value: []const u8, display: []const u8) gtk.Object {
    const box = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    switch (kind) {
        .status => gtk.gtk_box_append(box, widgets.statusIcon(value)),
        .priority => gtk.gtk_box_append(box, widgets.priorityIcon(value)),
        .assignee => gtk.gtk_box_append(box, personGlyph()),
    }
    const lbl = gtk.gtk_label_new(null);
    var buf: [256]u8 = undefined;
    if (std.fmt.bufPrintZ(&buf, "{s}", .{display[0..@min(display.len, buf.len - 1)]})) |z|
        gtk.gtk_label_set_text(lbl, z.ptr)
    else |_| {}
    gtk.gtk_box_append(box, lbl);
    return box;
}

fn addPropOption(
    state: *AppState,
    list: gtk.Object,
    issue_id: []const u8,
    field: []const u8,
    kind: PropKind,
    value: ?[]const u8,
    display: []const u8,
    icon: gtk.Object,
    button: gtk.Object,
    popover: gtk.Object,
) void {
    const r = gtk.gtk_button_new_with_label("");
    gtk.gtk_widget_add_css_class(r, "flat");
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    gtk.gtk_box_append(row, icon);
    const lbl = gtk.gtk_label_new(null);
    if (state.gpa.dupeZ(u8, display)) |z| {
        defer state.gpa.free(z);
        gtk.gtk_label_set_text(lbl, z.ptr);
    } else |_| {}
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    gtk.gtk_widget_set_hexpand(lbl, 1);
    gtk.gtk_box_append(row, lbl);
    setButtonChild(r, row);

    const gpa = state.gpa;
    const issue_z = gpa.dupeZ(u8, issue_id) catch return;
    const display_z = gpa.dupeZ(u8, display) catch {
        gpa.free(issue_z);
        return;
    };
    const value_z: ?[:0]u8 = if (value) |v| (gpa.dupeZ(u8, v) catch {
        gpa.free(issue_z);
        gpa.free(display_z);
        return;
    }) else null;
    const ctx = gpa.create(PropCtx) catch {
        gpa.free(issue_z);
        gpa.free(display_z);
        if (value_z) |v| gpa.free(v);
        return;
    };
    ctx.* = .{
        .state = state,
        .issue_id = issue_z,
        .field = field,
        .kind = kind,
        .value = value_z,
        .display = display_z,
        .button = button,
        .popover = popover,
    };
    gtk.g_object_set_data_full(r, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeProp));
    _ = gtk.g_signal_connect_data(r, "clicked", @ptrCast(&onPropSelected), ctx, null, 0);
    gtk.gtk_box_append(list, r);
}

fn onPropSelected(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *PropCtx = @ptrCast(@alignCast(data));
    const value: ?[]const u8 = if (ctx.value) |v| v else null;
    // Optimistic: update the button immediately, mutate in the background.
    setButtonChild(ctx.button, propChild(ctx.kind, if (ctx.value) |v| v else "", ctx.display));
    fireIssueUpdate(ctx.state, ctx.issue_id, ctx.field, value);
    gtk.gtk_popover_popdown(ctx.popover);
}

fn statusMenu(state: *AppState, issue_id: []const u8, current: []const u8) gtk.Object {
    return optionMenu(state, issue_id, "status", .status, &format.statuses, current, format.status(current).label);
}

fn priorityMenu(state: *AppState, issue_id: []const u8, current: []const u8) gtk.Object {
    return optionMenu(state, issue_id, "priority", .priority, &format.priorities, current, format.priority(current).label);
}

fn optionMenu(
    state: *AppState,
    issue_id: []const u8,
    field: []const u8,
    kind: PropKind,
    options: []const format.Option,
    current: []const u8,
    current_label: []const u8,
) gtk.Object {
    const button = gtk.gtk_menu_button_new();
    setMenuButtonChild(button, propChild(kind, current, current_label));
    const popover = gtk.gtk_popover_new();
    const list = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 2);
    for (options) |opt| {
        addPropOption(state, list, issue_id, field, kind, opt.value, opt.label, widgets.iconLabel(opt), button, popover);
    }
    gtk.gtk_popover_set_child(popover, list);
    gtk.gtk_menu_button_set_popover(button, popover);
    return button;
}

fn assigneeMenu(state: *AppState, arena: std.mem.Allocator, issue_id: []const u8, current_name: []const u8) gtk.Object {
    const button = gtk.gtk_menu_button_new();
    const display = if (current_name.len > 0) current_name else "Unassigned";
    setMenuButtonChild(button, propChild(.assignee, "", display));
    const popover = gtk.gtk_popover_new();
    const list = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 2);
    addPropOption(state, list, issue_id, "assigneeId", .assignee, null, "Unassigned", personGlyph(), button, popover);
    if (state.db) |*db| {
        if (db.listUsers(arena)) |users| {
            for (users) |u| addPropOption(state, list, issue_id, "assigneeId", .assignee, u.id, u.name, personGlyph(), button, popover);
        } else |_| {}
    }
    gtk.gtk_popover_set_child(popover, list);
    gtk.gtk_menu_button_set_popover(button, popover);
    return button;
}

// ---------------------------------------------------------------------------
// Due-date popover (GtkCalendar + Set/Clear)
// ---------------------------------------------------------------------------

const DueCtx = struct {
    state: *AppState,
    issue_id: [:0]u8,
    button: gtk.Object,
    popover: gtk.Object,
    calendar: gtk.Object,
};

fn freeDue(p: gtk.gpointer) callconv(.c) void {
    const c: *DueCtx = @ptrCast(@alignCast(p));
    c.state.gpa.free(c.issue_id);
    c.state.gpa.destroy(c);
}

fn dueChild(ymd: []const u8) gtk.Object {
    const lbl = gtk.gtk_label_new(null);
    var buf: [32]u8 = undefined;
    if (ymd.len >= 10) {
        if (std.fmt.bufPrintZ(&buf, "🗓 {s}", .{ymd[0..10]})) |z| gtk.gtk_label_set_text(lbl, z.ptr) else |_| {}
    } else {
        gtk.gtk_label_set_text(lbl, "🗓 No due date");
    }
    return lbl;
}

fn dueMenu(state: *AppState, issue_id: []const u8, current: []const u8) gtk.Object {
    const button = gtk.gtk_menu_button_new();
    setMenuButtonChild(button, dueChild(current));
    const popover = gtk.gtk_popover_new();
    const col = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 8);
    const cal = gtk.gtk_calendar_new();
    if (current.len >= 10) {
        const y = std.fmt.parseInt(c_int, current[0..4], 10) catch 0;
        const m = std.fmt.parseInt(c_int, current[5..7], 10) catch 0;
        const d = std.fmt.parseInt(c_int, current[8..10], 10) catch 0;
        if (y > 0 and m > 0 and d > 0) {
            const dt = gtk.g_date_time_new_local(y, m, d, 0, 0, 0);
            if (dt != null) {
                gtk.gtk_calendar_select_day(cal, dt);
                gtk.g_date_time_unref(dt);
            }
        }
    }
    gtk.gtk_box_append(col, cal);

    const buttons = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    const clear = gtk.gtk_button_new_with_label("Clear");
    gtk.gtk_widget_add_css_class(clear, "flat");
    const set = gtk.gtk_button_new_with_label("Set");
    gtk.gtk_widget_add_css_class(set, "suggested-action");
    gtk.gtk_widget_set_hexpand(clear, 1);
    gtk.gtk_box_append(buttons, clear);
    gtk.gtk_box_append(buttons, set);
    gtk.gtk_box_append(col, buttons);

    const ctx = state.gpa.create(DueCtx) catch return button;
    ctx.* = .{
        .state = state,
        .issue_id = state.gpa.dupeZ(u8, issue_id) catch {
            state.gpa.destroy(ctx);
            return button;
        },
        .button = button,
        .popover = popover,
        .calendar = cal,
    };
    gtk.g_object_set_data_full(button, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeDue));
    _ = gtk.g_signal_connect_data(clear, "clicked", @ptrCast(&onDueClear), ctx, null, 0);
    _ = gtk.g_signal_connect_data(set, "clicked", @ptrCast(&onDueSet), ctx, null, 0);

    gtk.gtk_popover_set_child(popover, col);
    gtk.gtk_menu_button_set_popover(button, popover);
    return button;
}

fn onDueSet(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *DueCtx = @ptrCast(@alignCast(data));
    const dt = gtk.gtk_calendar_get_date(ctx.calendar);
    if (dt == null) return;
    const y = gtk.g_date_time_get_year(dt);
    const m = gtk.g_date_time_get_month(dt);
    const d = gtk.g_date_time_get_day_of_month(dt);
    gtk.g_date_time_unref(dt);
    var buf: [16]u8 = undefined;
    const ymd = std.fmt.bufPrint(&buf, "{d:0>4}-{d:0>2}-{d:0>2}", .{ y, m, d }) catch return;
    setMenuButtonChild(ctx.button, dueChild(ymd));
    fireIssueUpdate(ctx.state, ctx.issue_id, "dueDate", ymd);
    gtk.gtk_popover_popdown(ctx.popover);
}

fn onDueClear(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *DueCtx = @ptrCast(@alignCast(data));
    setMenuButtonChild(ctx.button, dueChild(""));
    fireIssueUpdate(ctx.state, ctx.issue_id, "dueDate", null);
    gtk.gtk_popover_popdown(ctx.popover);
}

// ---------------------------------------------------------------------------
// Label picker (toggle add/remove → rebuild detail)
// ---------------------------------------------------------------------------

const LabelCtx = struct {
    state: *AppState,
    issue_id: [:0]u8,
    label_id: [:0]u8,
    name: [:0]u8,
    color: [:0]u8,
    on: bool,
    check_label: gtk.Object, // "✓"/"  " in the popover row
    chips_box: gtk.Object, // detail chips container, updated in place
};

fn freeLabel(p: gtk.gpointer) callconv(.c) void {
    const c: *LabelCtx = @ptrCast(@alignCast(p));
    const gpa = c.state.gpa;
    gpa.free(c.issue_id);
    gpa.free(c.label_id);
    gpa.free(c.name);
    gpa.free(c.color);
    gpa.destroy(c);
}

/// A label chip tagged with its label id so the picker can find + remove it.
fn labeledChip(state: *AppState, arena: std.mem.Allocator, label_id: []const u8, name: []const u8, color: []const u8) gtk.Object {
    const c = widgets.chip(arena, name, color);
    if (state.gpa.dupeZ(u8, label_id)) |z| {
        defer state.gpa.free(z);
        gtk.g_object_set_data_full(c, "exp-label-id", @ptrCast(gtk.g_strdup(z.ptr)), @ptrCast(&gtk.g_free));
    } else |_| {}
    return c;
}

fn labelMenu(state: *AppState, arena: std.mem.Allocator, issue_id: []const u8, workspace_id: []const u8, chips_box: gtk.Object) gtk.Object {
    const button = gtk.gtk_menu_button_new();
    setMenuButtonChild(button, gtk.gtk_label_new("🏷 Labels"));
    const popover = gtk.gtk_popover_new();
    const list = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 2);

    const attached = if (state.db) |*db| (db.listIssueLabelIds(arena, issue_id) catch &[_][:0]const u8{}) else &[_][:0]const u8{};
    const labels = if (state.db) |*db| (db.listLabels(arena, workspace_id) catch &[_]Database.LabelRow{}) else &[_]Database.LabelRow{};
    if (labels.len == 0) {
        const empty = gtk.gtk_label_new("No labels in this workspace");
        gtk.gtk_widget_add_css_class(empty, "dim-label");
        gtk.gtk_box_append(list, empty);
    }
    for (labels) |l| {
        var on = false;
        for (attached) |aid| {
            if (std.mem.eql(u8, aid, l.id)) {
                on = true;
                break;
            }
        }
        const r = gtk.gtk_button_new_with_label("");
        gtk.gtk_widget_add_css_class(r, "flat");
        const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
        const check = gtk.gtk_label_new(if (on) "✓" else "  ");
        gtk.gtk_box_append(row, check);
        gtk.gtk_box_append(row, widgets.dot(l.color));
        const name = gtk.gtk_label_new(null);
        if (arena.dupeZ(u8, l.name)) |z| gtk.gtk_label_set_text(name, z.ptr) else |_| {}
        gtk.gtk_widget_set_halign(name, gtk.ALIGN_START);
        gtk.gtk_widget_set_hexpand(name, 1);
        gtk.gtk_box_append(row, name);
        setButtonChild(r, row);

        const gpa = state.gpa;
        const ctx = gpa.create(LabelCtx) catch continue;
        ctx.state = state;
        ctx.on = on;
        ctx.check_label = check;
        ctx.chips_box = chips_box;
        ctx.issue_id = gpa.dupeZ(u8, issue_id) catch {
            gpa.destroy(ctx);
            continue;
        };
        ctx.label_id = gpa.dupeZ(u8, l.id) catch {
            gpa.free(ctx.issue_id);
            gpa.destroy(ctx);
            continue;
        };
        ctx.name = gpa.dupeZ(u8, l.name) catch {
            gpa.free(ctx.issue_id);
            gpa.free(ctx.label_id);
            gpa.destroy(ctx);
            continue;
        };
        ctx.color = gpa.dupeZ(u8, l.color) catch {
            gpa.free(ctx.issue_id);
            gpa.free(ctx.label_id);
            gpa.free(ctx.name);
            gpa.destroy(ctx);
            continue;
        };
        gtk.g_object_set_data_full(r, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeLabel));
        _ = gtk.g_signal_connect_data(r, "clicked", @ptrCast(&onLabelToggle), ctx, null, 0);
        gtk.gtk_box_append(list, r);
    }

    // Inline create (web parity: mint a label from the picker).
    const create_row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 4);
    gtk.gtk_widget_set_margin_top(create_row, 6);
    const entry = gtk.gtk_entry_new();
    gtk.gtk_entry_set_placeholder_text(entry, "New label…");
    gtk.gtk_widget_set_hexpand(entry, 1);
    gtk.gtk_box_append(create_row, entry);
    const add_btn = gtk.gtk_button_new_with_label("Create");
    gtk.gtk_widget_add_css_class(add_btn, "suggested-action");
    gtk.gtk_box_append(create_row, add_btn);
    gtk.gtk_box_append(list, create_row);
    addLabelCreate(state, issue_id, workspace_id, entry, add_btn);

    gtk.gtk_popover_set_child(popover, list);
    gtk.gtk_menu_button_set_popover(button, popover);
    return button;
}

fn addLabelCreate(state: *AppState, issue_id: []const u8, workspace_id: []const u8, entry: gtk.Object, add_btn: gtk.Object) void {
    const gpa = state.gpa;
    const ctx = gpa.create(LabelCreateCtx) catch return;
    ctx.state = state;
    ctx.entry = entry;
    ctx.issue_id = gpa.dupeZ(u8, issue_id) catch {
        gpa.destroy(ctx);
        return;
    };
    ctx.workspace_id = gpa.dupeZ(u8, workspace_id) catch {
        gpa.free(ctx.issue_id);
        gpa.destroy(ctx);
        return;
    };
    gtk.g_object_set_data_full(add_btn, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeLabelCreate));
    _ = gtk.g_signal_connect_data(add_btn, "clicked", @ptrCast(&onLabelCreate), ctx, null, 0);
    _ = gtk.g_signal_connect_data(entry, "activate", @ptrCast(&onLabelCreate), ctx, null, 0);
}

fn onLabelToggle(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *LabelCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    const new_on = !ctx.on;

    // Optimistic: flip the check + add/remove the chip in place; mutate async.
    gtk.gtk_label_set_text(ctx.check_label, if (new_on) "✓" else "  ");
    if (new_on) {
        var arena = std.heap.ArenaAllocator.init(state.gpa);
        defer arena.deinit();
        gtk.gtk_box_append(ctx.chips_box, labeledChip(state, arena.allocator(), ctx.label_id, ctx.name, ctx.color));
    } else {
        removeChipById(ctx.chips_box, ctx.label_id);
    }

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const json = std.json.Stringify.valueAlloc(arena.allocator(), .{ .issueId = ctx.issue_id, .labelId = ctx.label_id }, .{}) catch return;
    mutate.fire(state.gpa, state.instance.?, state.token, if (new_on) "issueLabels.add" else "issueLabels.remove", json);
    ctx.on = new_on;
}

const LabelCreateCtx = struct {
    state: *AppState,
    issue_id: [:0]u8,
    workspace_id: [:0]u8,
    entry: gtk.Object,
};

fn freeLabelCreate(p: gtk.gpointer) callconv(.c) void {
    const c: *LabelCreateCtx = @ptrCast(@alignCast(p));
    c.state.gpa.free(c.issue_id);
    c.state.gpa.free(c.workspace_id);
    c.state.gpa.destroy(c);
}

/// Create a new workspace label, attach it to the issue, and rebuild the detail.
/// Deliberate action → blocking is acceptable (we need the new id to attach).
fn onLabelCreate(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *LabelCreateCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    const name = std.mem.trim(u8, std.mem.span(gtk.gtk_editable_get_text(ctx.entry)), " \t");
    if (name.len == 0) return;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    const cj = std.json.Stringify.valueAlloc(a, .{ .workspaceId = ctx.workspace_id, .name = name, .color = "#6366f1" }, .{}) catch return;
    var resp = trpc.call(state.gpa, state.instance.?, "labels.create", cj, state.token, 20) catch return;
    defer resp.deinit();
    if (!resp.ok()) return;

    // Pull the new label id and attach it to the issue.
    const label_obj = trpc.asObject(resp.data() orelse return) orelse return;
    const lbl = trpc.asObject(label_obj.get("label") orelse return) orelse return;
    const new_id = trpc.objString(lbl, "id") orelse return;

    const aj = std.json.Stringify.valueAlloc(a, .{ .issueId = ctx.issue_id, .labelId = new_id }, .{}) catch return;
    var ar = trpc.call(state.gpa, state.instance.?, "issueLabels.add", aj, state.token, 20) catch return;
    ar.deinit();

    const iid = state.gpa.dupe(u8, ctx.issue_id) catch return;
    defer state.gpa.free(iid);
    showIssueDetail(state, iid); // reflects the new chip + picker (frees ctx)
}

/// Remove the chip in `chips_box` whose stashed label id matches.
fn removeChipById(chips_box: gtk.Object, label_id: []const u8) void {
    var child = gtk.gtk_widget_get_first_child(chips_box);
    while (child != null) {
        const next = gtk.gtk_widget_get_next_sibling(child);
        if (gtk.g_object_get_data(child, "exp-label-id")) |raw| {
            if (std.mem.eql(u8, std.mem.span(@as([*:0]const u8, @ptrCast(raw))), label_id)) {
                gtk.gtk_box_remove(chips_box, child);
                return;
            }
        }
        child = next;
    }
}

// ---------------------------------------------------------------------------
// Comment composer
// ---------------------------------------------------------------------------

const CommentCtx = struct {
    state: *AppState,
    issue_id: [:0]u8,
    editor: *md.MarkdownEditor,
    comments_box: gtk.Object,
};

fn freeComment(p: gtk.gpointer) callconv(.c) void {
    const c: *CommentCtx = @ptrCast(@alignCast(p));
    c.editor.destroy();
    c.state.gpa.free(c.issue_id);
    c.state.gpa.destroy(c);
}

fn commentComposer(state: *AppState, issue_id: []const u8, comments_box: gtk.Object, mention_members: []const md.MentionMember) gtk.Object {
    // Rich markdown composer — reuses the description MarkdownEditor (toolbar:
    // bold/italic/strike/code/headings/lists/task-lists/quote/links + image
    // upload) so comments reach parity with the web composer. Return inserts a
    // newline; Ctrl/⌘+Return submits.
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    gtk.gtk_widget_set_margin_top(row, 6);

    const editor = md.MarkdownEditor.create(state.gpa) orelse return row;
    gtk.gtk_widget_set_hexpand(editor.container, 1);
    if (editor.scrolled) |sc| gtk.gtk_widget_set_size_request(sc, -1, 64);
    gtk.gtk_box_append(row, editor.container);

    const btn = gtk.gtk_button_new_with_label("Comment");
    gtk.gtk_widget_add_css_class(btn, "suggested-action");
    gtk.gtk_widget_set_valign(btn, gtk.ALIGN_END);
    gtk.gtk_box_append(row, btn);

    const ctx = state.gpa.create(CommentCtx) catch {
        state.gpa.destroy(editor);
        return row;
    };
    ctx.* = .{
        .state = state,
        .issue_id = state.gpa.dupeZ(u8, issue_id) catch {
            state.gpa.destroy(editor);
            state.gpa.destroy(ctx);
            return row;
        },
        .editor = editor,
        .comments_box = comments_box,
    };
    // Borrow the stable duped issue_id (+ app-lifetime instance/token) for image upload.
    editor.setIssueContext(state.instance, state.token, ctx.issue_id);
    editor.setMentionMembers(mention_members);
    gtk.g_object_set_data_full(btn, "exp-ctx", @ptrCast(ctx), @ptrCast(&freeComment));
    _ = gtk.g_signal_connect_data(btn, "clicked", @ptrCast(&onCommentSubmit), ctx, null, 0);

    const key = gtk.gtk_event_controller_key_new();
    _ = gtk.g_signal_connect_data(key, "key-pressed", @ptrCast(&onCommentKey), ctx, null, 0);
    gtk.gtk_widget_add_controller(editor.view, key);
    return row;
}

/// Ctrl/⌘+Return submits the comment; plain Return inserts a newline.
fn onCommentKey(_: gtk.Object, keyval: c_uint, _: c_uint, mods: c_uint, data: gtk.gpointer) callconv(.c) c_int {
    const is_enter = keyval == 0xFF0D or keyval == 0xFF8D; // GDK_KEY_Return / KP_Enter
    const ctrl_or_super = (mods & (1 << 2)) != 0 or (mods & (1 << 26)) != 0; // Control / Super
    if (is_enter and ctrl_or_super) {
        onCommentSubmit(null, data);
        return 1; // handled
    }
    return 0;
}

fn onCommentSubmit(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *CommentCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    const raw = ctx.editor.getText(a) catch return;
    const text = std.mem.trim(u8, raw, " \t\r\n");
    if (text.len == 0) return;

    const json = std.json.Stringify.valueAlloc(a, .{
        .issueId = ctx.issue_id,
        .body = text,
    }, .{}) catch return;
    // Post async; optimistically show it only while this issue's detail is still
    // on screen (the composer and comments_box share that subtree's lifetime, so
    // this is just defensive — sync delivers the canonical row regardless).
    mutate.fire(state.gpa, state.instance.?, state.token, "comments.create", json);
    const showing_same = if (state.detail_issue_id) |d| std.mem.eql(u8, d, ctx.issue_id) else false;
    if (showing_same) gtk.gtk_box_append(ctx.comments_box, commentBubble(a, "You", "regular", text, false, false));
    ctx.editor.setText("");
}

/// Update a single scalar field of an issue (status/priority/dueDate/assigneeId)
/// off the main thread. `value` is interpolated raw — only ever an enum, uuid,
/// or YYYY-MM-DD, so no JSON escaping is required; null clears the field.
fn fireIssueUpdate(state: *AppState, issue_id: []const u8, field: []const u8, value: ?[]const u8) void {
    const instance = state.instance orelse return;
    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const json = if (value) |v|
        std.fmt.allocPrint(a, "{{\"id\":\"{s}\",\"{s}\":\"{s}\"}}", .{ issue_id, field, v }) catch return
    else
        std.fmt.allocPrint(a, "{{\"id\":\"{s}\",\"{s}\":null}}", .{ issue_id, field }) catch return;
    mutate.fire(state.gpa, instance, state.token, "issues.update", json);
}

// GtkButton.set_child / GtkMenuButton.set_child share the same need; wrap the
// raw externs for readability.
fn setButtonChild(button: gtk.Object, child: gtk.Object) void {
    gtk.gtk_button_set_child(button, child);
}
fn setMenuButtonChild(button: gtk.Object, child: gtk.Object) void {
    gtk.gtk_menu_button_set_child(button, child);
}

// --- Create-issue dialog (the web create-issue dialog, ported) ---

const status_options = [_:null]?[*:0]const u8{ "backlog", "todo", "in_progress", "done", "cancelled" };
const priority_options = [_:null]?[*:0]const u8{ "none", "urgent", "high", "medium", "low" };

// Recurrence: the fixed interval set (contract.generated.zig recurrence_intervals)
// + units. interval+unit are always set together (server's assertRecurrencePair).
const recurrence_interval_options = [_:null]?[*:0]const u8{ "1", "2", "3", "4", "5", "6", "7", "8", "10", "12", "14", "21", "30" };
const recurrence_interval_values = [_]i64{ 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 21, 30 };
const recurrence_unit_options = [_:null]?[*:0]const u8{ "day", "week", "month" };

fn recurrenceIntervalIndex(value: i64) c_uint {
    for (recurrence_interval_values, 0..) |v, i| if (v == value) return @intCast(i);
    return 0;
}

const CreateCtx = struct {
    state: *AppState,
    arena: std.heap.ArenaAllocator, // all dialog-lifetime allocations
    issue_id: ?[:0]const u8, // null = create; set = edit (issues.update)
    editing: bool,
    dialog: gtk.Object,
    title_entry: gtk.Object,
    editor: *md.MarkdownEditor,
    status_dd: gtk.Object,
    priority_dd: gtk.Object,
    project_dd: gtk.Object,
    assignee_dd: gtk.Object,
    submit_btn: gtk.Object = null,
    due_button: gtk.Object = null,
    due_calendar: gtk.Object = null,
    due_popover: gtk.Object = null,
    due_value: ?[]const u8 = null, // arena-owned "YYYY-MM-DD"
    repeat_check: gtk.Object = null,
    rec_interval_dd: gtk.Object = null,
    rec_unit_dd: gtk.Object = null,
    create_more: gtk.Object = null, // check button (create mode only)
    error_label: gtk.Object,
    project_ids: [][:0]const u8, // arena; index matches the project dropdown
    assignee_ids: []?[]const u8, // arena; [0] = null (Unassigned)
    label_ids: [][:0]const u8, // arena; index matches label_checks
    label_checks: []gtk.Object, // arena
    label_initial: []bool, // arena; whether attached when the dialog opened
};

fn optionAt(opts: [*:null]const ?[*:0]const u8, idx: c_uint) []const u8 {
    var i: usize = 0;
    while (opts[i]) |s| : (i += 1) {
        if (i == idx) return std.mem.span(s);
    }
    return std.mem.span(opts[0].?);
}

fn onNewIssueClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    openIssueDialog(@ptrCast(@alignCast(data)), null);
}

fn onEditClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    if (state.detail_issue_id) |iid| openIssueDialog(state, iid);
}

fn indexOfOption(opts: [*:null]const ?[*:0]const u8, value: []const u8) c_uint {
    var i: usize = 0;
    while (opts[i]) |s| : (i += 1) {
        if (std.mem.eql(u8, std.mem.span(s), value)) return @intCast(i);
    }
    return 0;
}

// --- Async dialog submit: run the request on a worker, deliver the result to a
//     main-thread callback via g_idle, so saving never blocks the UI. The dialog
//     is made non-closable while in flight so the callback's ctx stays alive. ---

const SubmitDoneFn = *const fn (ctx: ?*anyopaque, ok: bool, err: ?[*:0]const u8) void;

const SubmitJob = struct {
    gpa: std.mem.Allocator,
    instance: []u8,
    token: ?[]u8,
    proc: []const u8, // static literal
    json: []u8,
    cb: SubmitDoneFn,
    cb_ctx: ?*anyopaque,
    ok: bool = false,
    err: ?[:0]u8 = null,
};

fn submitAsync(state: *AppState, proc: []const u8, json: []const u8, cb: SubmitDoneFn, cb_ctx: ?*anyopaque) void {
    const gpa = state.gpa;
    const instance = state.instance orelse return;
    const job = gpa.create(SubmitJob) catch return;
    job.gpa = gpa;
    job.proc = proc;
    job.cb = cb;
    job.cb_ctx = cb_ctx;
    job.ok = false;
    job.err = null;
    job.instance = gpa.dupe(u8, instance) catch {
        gpa.destroy(job);
        return;
    };
    job.json = gpa.dupe(u8, json) catch {
        gpa.free(job.instance);
        gpa.destroy(job);
        return;
    };
    job.token = if (state.token) |t| (gpa.dupe(u8, t) catch null) else null;
    const th = std.Thread.spawn(.{}, submitWorker, .{job}) catch {
        submitWorker(job);
        return;
    };
    th.detach();
}

fn submitWorker(job: *SubmitJob) void {
    defer _ = gtk.g_idle_add(@ptrCast(&submitDone), job);
    var resp = trpc.call(job.gpa, job.instance, job.proc, job.json, job.token, 30) catch return;
    defer resp.deinit();
    job.ok = resp.ok();
    if (!job.ok) {
        if (resp.errorMessage()) |m| job.err = job.gpa.dupeZ(u8, m) catch null;
    }
}

fn submitDone(data: gtk.gpointer) callconv(.c) c_int {
    const job: *SubmitJob = @ptrCast(@alignCast(data));
    job.cb(job.cb_ctx, job.ok, if (job.err) |e| e.ptr else null);
    job.gpa.free(job.instance);
    job.gpa.free(job.json);
    if (job.token) |t| job.gpa.free(t);
    if (job.err) |e| job.gpa.free(e);
    job.gpa.destroy(job);
    return 0; // G_SOURCE_REMOVE
}

/// Shared create/edit dialog: `issue_id == null` creates (issues.create); a set
/// id edits (prefilled, issues.update). All dialog-lifetime allocations live in
/// `ctx.arena`, freed wholesale when the dialog closes.
fn openIssueDialog(state: *AppState, issue_id: ?[]const u8) void {
    const db = if (state.db) |*d| d else return;
    if (state.instance == null) return;
    const editing = issue_id != null;

    const ctx = state.gpa.create(CreateCtx) catch return;
    ctx.state = state;
    ctx.arena = std.heap.ArenaAllocator.init(state.gpa);
    ctx.editing = editing;
    ctx.issue_id = null;
    ctx.due_button = null;
    ctx.due_calendar = null;
    ctx.due_popover = null;
    ctx.due_value = null;
    ctx.create_more = null;
    const a = ctx.arena.allocator();

    ctx.editor = md.MarkdownEditor.create(state.gpa) orelse {
        ctx.arena.deinit();
        state.gpa.destroy(ctx);
        return;
    };

    const projects = db.listProjects(a, state.active_workspace_id) catch {
        freeCreateCtx(ctx);
        return;
    };
    if (projects.len == 0) {
        freeCreateCtx(ctx);
        return;
    }

    const detail = if (issue_id) |iid| (db.getIssue(a, iid) catch null) else null;
    ctx.issue_id = if (issue_id) |iid| (a.dupeZ(u8, iid) catch null) else null;
    ctx.editor.setIssueContext(state.instance, state.token, ctx.issue_id);

    // Determine the active project (default for create / the issue's for edit)
    // and the workspace whose labels we offer.
    var default_proj: usize = 0;
    for (projects, 0..) |p, i| {
        if (detail) |d| {
            if (std.mem.eql(u8, p.id, d.project_id)) default_proj = i;
        } else if (state.selected_project_id) |sel| {
            if (std.mem.eql(u8, p.id, sel)) default_proj = i;
        }
    }
    const workspace_id = if (detail) |d| d.workspace_id else projects[default_proj].workspace_id;

    ctx.project_ids = a.alloc([:0]const u8, projects.len) catch {
        freeCreateCtx(ctx);
        return;
    };
    const proj_names = a.alloc(?[*:0]const u8, projects.len + 1) catch {
        freeCreateCtx(ctx);
        return;
    };
    for (projects, 0..) |p, i| {
        ctx.project_ids[i] = a.dupeZ(u8, p.id) catch "";
        proj_names[i] = p.name.ptr;
    }
    proj_names[projects.len] = null;

    const dialog = gtk.adw_dialog_new();
    gtk.adw_dialog_set_title(dialog, if (editing) "Edit issue" else "New issue");
    gtk.adw_dialog_set_content_width(dialog, 600);
    gtk.adw_dialog_set_content_height(dialog, 640);
    ctx.dialog = dialog;

    const tv = gtk.adw_toolbar_view_new();
    const header = gtk.adw_header_bar_new();
    const cancel = gtk.gtk_button_new_with_label("Cancel");
    _ = gtk.g_signal_connect_data(cancel, "clicked", @ptrCast(&onCreateCancel), ctx, null, 0);
    gtk.adw_header_bar_pack_start(header, cancel);
    const create_btn = gtk.gtk_button_new_with_label(if (editing) "Save" else "Create");
    gtk.gtk_widget_add_css_class(create_btn, "suggested-action");
    _ = gtk.g_signal_connect_data(create_btn, "clicked", @ptrCast(&onCreateSubmit), ctx, null, 0);
    gtk.adw_header_bar_pack_end(header, create_btn);
    ctx.submit_btn = create_btn;
    gtk.adw_toolbar_view_add_top_bar(tv, header);

    const form = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 10);
    gtk.gtk_widget_set_margin_top(form, 16);
    gtk.gtk_widget_set_margin_bottom(form, 16);
    gtk.gtk_widget_set_margin_start(form, 16);
    gtk.gtk_widget_set_margin_end(form, 16);

    const title_entry = gtk.gtk_entry_new();
    gtk.gtk_entry_set_placeholder_text(title_entry, "Issue title");
    gtk.gtk_box_append(form, title_entry);
    ctx.title_entry = title_entry;

    gtk.gtk_widget_set_vexpand(ctx.editor.container, 1);
    gtk.gtk_box_append(form, ctx.editor.container);

    // Row 1: status · priority · project
    const row1 = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    const status_dd = gtk.gtk_drop_down_new_from_strings(&status_options);
    const priority_dd = gtk.gtk_drop_down_new_from_strings(&priority_options);
    const project_dd = gtk.gtk_drop_down_new_from_strings(@ptrCast(proj_names.ptr));
    gtk.gtk_drop_down_set_selected(project_dd, @intCast(default_proj));
    gtk.gtk_widget_set_hexpand(project_dd, 1);
    gtk.gtk_box_append(row1, status_dd);
    gtk.gtk_box_append(row1, priority_dd);
    gtk.gtk_box_append(row1, project_dd);
    gtk.gtk_box_append(form, row1);
    ctx.status_dd = status_dd;
    ctx.priority_dd = priority_dd;
    ctx.project_dd = project_dd;

    // Row 2: due date · assignee
    const row2 = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_box_append(row2, buildDialogDueButton(ctx, if (detail) |d| d.due_date else ""));
    gtk.gtk_box_append(row2, buildAssigneeDropdown(ctx, a, if (detail) |d| d.assignee_id else ""));
    gtk.gtk_box_append(form, row2);

    // Row 3: recurrence (Repeat + every N + unit)
    const row3 = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    const repeat = gtk.gtk_check_button_new_with_label("Repeat every");
    const rec_interval = gtk.gtk_drop_down_new_from_strings(&recurrence_interval_options);
    const rec_unit = gtk.gtk_drop_down_new_from_strings(&recurrence_unit_options);
    const has_rec = if (detail) |d| (d.recurrence_interval > 0 and d.recurrence_unit.len > 0) else false;
    if (has_rec) {
        gtk.gtk_check_button_set_active(repeat, 1);
        gtk.gtk_drop_down_set_selected(rec_interval, recurrenceIntervalIndex(detail.?.recurrence_interval));
        gtk.gtk_drop_down_set_selected(rec_unit, indexOfOption(&recurrence_unit_options, detail.?.recurrence_unit));
    } else {
        gtk.gtk_widget_set_sensitive(rec_interval, 0);
        gtk.gtk_widget_set_sensitive(rec_unit, 0);
    }
    _ = gtk.g_signal_connect_data(repeat, "toggled", @ptrCast(&onRepeatToggled), ctx, null, 0);
    gtk.gtk_box_append(row3, repeat);
    gtk.gtk_box_append(row3, rec_interval);
    gtk.gtk_box_append(row3, rec_unit);
    gtk.gtk_box_append(form, row3);
    ctx.repeat_check = repeat;
    ctx.rec_interval_dd = rec_interval;
    ctx.rec_unit_dd = rec_unit;

    // Public-workspace non-members can only set title/description/labels.
    if (!canModerate(state)) {
        gtk.gtk_widget_set_sensitive(status_dd, 0);
        gtk.gtk_widget_set_sensitive(priority_dd, 0);
        gtk.gtk_widget_set_sensitive(ctx.assignee_dd, 0);
        gtk.gtk_widget_set_sensitive(ctx.due_button, 0);
        gtk.gtk_widget_set_sensitive(repeat, 0);
        gtk.gtk_widget_set_sensitive(rec_interval, 0);
        gtk.gtk_widget_set_sensitive(rec_unit, 0);
    }

    // Labels
    const labels = db.listLabels(a, workspace_id) catch &[_]Database.LabelRow{};
    if (labels.len > 0) {
        gtk.gtk_box_append(form, widgets.sectionTitle("Labels"));
        const label_box = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
        ctx.label_ids = a.alloc([:0]const u8, labels.len) catch &[_][:0]const u8{};
        ctx.label_checks = a.alloc(gtk.Object, labels.len) catch &[_]gtk.Object{};
        ctx.label_initial = a.alloc(bool, labels.len) catch &[_]bool{};
        const attached = if (issue_id) |iid| (db.listIssueLabelIds(a, iid) catch &[_][:0]const u8{}) else &[_][:0]const u8{};
        for (labels, 0..) |l, i| {
            if (i < ctx.label_ids.len) ctx.label_ids[i] = a.dupeZ(u8, l.id) catch "";
            const name_z = a.dupeZ(u8, l.name) catch "";
            const chk = gtk.gtk_check_button_new_with_label(name_z.ptr);
            var on = false;
            for (attached) |aid| {
                if (std.mem.eql(u8, aid, l.id)) {
                    on = true;
                    break;
                }
            }
            if (on) gtk.gtk_check_button_set_active(chk, 1);
            if (i < ctx.label_checks.len) ctx.label_checks[i] = chk;
            if (i < ctx.label_initial.len) ctx.label_initial[i] = on;
            gtk.gtk_box_append(label_box, chk);
        }
        gtk.gtk_box_append(form, label_box);
    } else {
        ctx.label_ids = &[_][:0]const u8{};
        ctx.label_checks = &[_]gtk.Object{};
        ctx.label_initial = &[_]bool{};
    }

    const err = gtk.gtk_label_new("");
    gtk.gtk_widget_add_css_class(err, "error");
    gtk.gtk_box_append(form, err);
    ctx.error_label = err;

    if (!editing) {
        const more = gtk.gtk_check_button_new_with_label("Create more");
        gtk.gtk_box_append(form, more);
        ctx.create_more = more;
    }

    // Prefill for edit.
    if (detail) |d| {
        gtk.gtk_editable_set_text(title_entry, d.title.ptr);
        ctx.editor.setText(d.description);
        gtk.gtk_drop_down_set_selected(status_dd, indexOfOption(&status_options, d.status));
        gtk.gtk_drop_down_set_selected(priority_dd, indexOfOption(&priority_options, d.priority));
        gtk.gtk_widget_set_sensitive(project_dd, 0); // can't move between projects here
    }

    gtk.adw_toolbar_view_set_content(tv, form);
    gtk.adw_dialog_set_child(dialog, tv);
    _ = gtk.g_signal_connect_data(dialog, "closed", @ptrCast(&onCreateClosed), ctx, null, 0);
    gtk.adw_dialog_present(dialog, state.window);
}

/// A due-date menu button for the dialog (calendar + Set/Clear), writing into
/// `ctx.due_value`.
fn buildDialogDueButton(ctx: *CreateCtx, current: []const u8) gtk.Object {
    const button = gtk.gtk_menu_button_new();
    if (current.len >= 10) ctx.due_value = ctx.arena.allocator().dupe(u8, current[0..10]) catch null;
    gtk.gtk_menu_button_set_child(button, dueChild(ctx.due_value orelse ""));
    const popover = gtk.gtk_popover_new();
    const col = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 8);
    const cal = gtk.gtk_calendar_new();
    if (current.len >= 10) {
        const y = std.fmt.parseInt(c_int, current[0..4], 10) catch 0;
        const m = std.fmt.parseInt(c_int, current[5..7], 10) catch 0;
        const d = std.fmt.parseInt(c_int, current[8..10], 10) catch 0;
        if (y > 0 and m > 0 and d > 0) {
            const dt = gtk.g_date_time_new_local(y, m, d, 0, 0, 0);
            if (dt != null) {
                gtk.gtk_calendar_select_day(cal, dt);
                gtk.g_date_time_unref(dt);
            }
        }
    }
    gtk.gtk_box_append(col, cal);
    const buttons = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    const clear = gtk.gtk_button_new_with_label("Clear");
    gtk.gtk_widget_add_css_class(clear, "flat");
    gtk.gtk_widget_set_hexpand(clear, 1);
    const set = gtk.gtk_button_new_with_label("Set");
    gtk.gtk_widget_add_css_class(set, "suggested-action");
    gtk.gtk_box_append(buttons, clear);
    gtk.gtk_box_append(buttons, set);
    gtk.gtk_box_append(col, buttons);
    gtk.gtk_popover_set_child(popover, col);
    gtk.gtk_menu_button_set_popover(button, popover);

    ctx.due_button = button;
    ctx.due_calendar = cal;
    ctx.due_popover = popover;
    _ = gtk.g_signal_connect_data(clear, "clicked", @ptrCast(&onDialogDueClear), ctx, null, 0);
    _ = gtk.g_signal_connect_data(set, "clicked", @ptrCast(&onDialogDueSet), ctx, null, 0);
    return button;
}

fn onDialogDueSet(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *CreateCtx = @ptrCast(@alignCast(data));
    const dt = gtk.gtk_calendar_get_date(ctx.due_calendar);
    if (dt == null) return;
    const y = gtk.g_date_time_get_year(dt);
    const m = gtk.g_date_time_get_month(dt);
    const d = gtk.g_date_time_get_day_of_month(dt);
    gtk.g_date_time_unref(dt);
    const ymd = std.fmt.allocPrint(ctx.arena.allocator(), "{d:0>4}-{d:0>2}-{d:0>2}", .{ y, m, d }) catch return;
    ctx.due_value = ymd;
    gtk.gtk_menu_button_set_child(ctx.due_button, dueChild(ymd));
    gtk.gtk_popover_popdown(ctx.due_popover);
}

fn onDialogDueClear(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *CreateCtx = @ptrCast(@alignCast(data));
    ctx.due_value = null;
    gtk.gtk_menu_button_set_child(ctx.due_button, dueChild(""));
    gtk.gtk_popover_popdown(ctx.due_popover);
}

fn buildAssigneeDropdown(ctx: *CreateCtx, a: std.mem.Allocator, current_id: []const u8) gtk.Object {
    const db = if (ctx.state.db) |*d| d else {
        ctx.assignee_ids = &[_]?[]const u8{};
        return gtk.gtk_drop_down_new_from_strings(@ptrCast(&[_:null]?[*:0]const u8{"Unassigned"}));
    };
    const users = db.listUsers(a) catch &[_]Database.UserRow{};
    const names = a.alloc(?[*:0]const u8, users.len + 2) catch {
        ctx.assignee_ids = &[_]?[]const u8{};
        return gtk.gtk_drop_down_new_from_strings(@ptrCast(&[_:null]?[*:0]const u8{"Unassigned"}));
    };
    ctx.assignee_ids = a.alloc(?[]const u8, users.len + 1) catch &[_]?[]const u8{};
    names[0] = "Unassigned";
    if (ctx.assignee_ids.len > 0) ctx.assignee_ids[0] = null;
    var selected: c_uint = 0;
    for (users, 0..) |u, i| {
        names[i + 1] = u.name.ptr;
        if (i + 1 < ctx.assignee_ids.len) ctx.assignee_ids[i + 1] = a.dupe(u8, u.id) catch null;
        if (std.mem.eql(u8, u.id, current_id)) selected = @intCast(i + 1);
    }
    names[users.len + 1] = null;
    const dd = gtk.gtk_drop_down_new_from_strings(@ptrCast(names.ptr));
    gtk.gtk_drop_down_set_selected(dd, selected);
    gtk.gtk_widget_set_hexpand(dd, 1);
    ctx.assignee_dd = dd;
    return dd;
}

fn onCreateCancel(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *CreateCtx = @ptrCast(@alignCast(data));
    _ = gtk.adw_dialog_close(ctx.dialog);
}

fn onCreateClosed(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    freeCreateCtx(@ptrCast(@alignCast(data)));
}

fn freeCreateCtx(ctx: *CreateCtx) void {
    ctx.editor.destroy();
    ctx.arena.deinit();
    ctx.state.gpa.destroy(ctx);
}

/// Collect the currently-checked label ids into `a`.
fn selectedLabelIds(ctx: *CreateCtx, a: std.mem.Allocator) [][]const u8 {
    var out: std.ArrayListUnmanaged([]const u8) = .empty;
    for (ctx.label_checks, 0..) |chk, i| {
        if (chk != null and gtk.gtk_check_button_get_active(chk) != 0 and i < ctx.label_ids.len)
            out.append(a, ctx.label_ids[i]) catch {};
    }
    return out.toOwnedSlice(a) catch &[_][]const u8{};
}

/// Enable the interval/unit dropdowns only while "Repeat" is checked.
fn onRepeatToggled(check: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *CreateCtx = @ptrCast(@alignCast(data));
    const on: c_int = if (gtk.gtk_check_button_get_active(check) != 0) 1 else 0;
    gtk.gtk_widget_set_sensitive(ctx.rec_interval_dd, on);
    gtk.gtk_widget_set_sensitive(ctx.rec_unit_dd, on);
}

fn onCreateSubmit(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *CreateCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    const title = std.mem.span(gtk.gtk_editable_get_text(ctx.title_entry));
    if (title.len == 0) {
        gtk.gtk_label_set_text(ctx.error_label, "Title is required.");
        return;
    }

    const desc = ctx.editor.getText(a) catch "";
    const proj_i = gtk.gtk_drop_down_get_selected(ctx.project_dd);
    if (proj_i >= ctx.project_ids.len) return;
    var status = optionAt(&status_options, gtk.gtk_drop_down_get_selected(ctx.status_dd));
    const priority = optionAt(&priority_options, gtk.gtk_drop_down_get_selected(ctx.priority_dd));
    const assignee: ?[]const u8 = blk: {
        const ai = gtk.gtk_drop_down_get_selected(ctx.assignee_dd);
        if (ai < ctx.assignee_ids.len) break :blk ctx.assignee_ids[ai];
        break :blk null;
    };

    // Recurrence: interval+unit set together, else both null.
    const recurring = ctx.repeat_check != null and gtk.gtk_check_button_get_active(ctx.repeat_check) != 0;
    const rec_interval: ?i64 = if (recurring) recurrence_interval_values[gtk.gtk_drop_down_get_selected(ctx.rec_interval_dd)] else null;
    const rec_unit: ?[]const u8 = if (recurring) optionAt(&recurrence_unit_options, gtk.gtk_drop_down_get_selected(ctx.rec_unit_dd)) else null;
    if (recurring and ctx.issue_id == null) status = "todo"; // match the web client: new recurring issues start as todo

    const encoded = if (ctx.issue_id) |iid|
        std.json.Stringify.valueAlloc(a, .{
            .id = iid,
            .title = title,
            .status = status,
            .priority = priority,
            .assigneeId = assignee,
            .dueDate = ctx.due_value,
            .recurrenceInterval = rec_interval,
            .recurrenceUnit = rec_unit,
            .description = desc,
        }, .{})
    else
        std.json.Stringify.valueAlloc(a, .{
            .projectId = ctx.project_ids[proj_i],
            .title = title,
            .status = status,
            .priority = priority,
            .assigneeId = assignee,
            .dueDate = ctx.due_value,
            .recurrenceInterval = rec_interval,
            .recurrenceUnit = rec_unit,
            .description = desc,
            .labelIds = selectedLabelIds(ctx, a),
        }, .{});
    const json = encoded catch {
        gtk.gtk_label_set_text(ctx.error_label, "Couldn't encode the request.");
        return;
    };

    // Edit: issues.update has no labelIds, so reconcile labels async right away.
    if (ctx.issue_id) |iid| {
        for (ctx.label_checks, 0..) |chk, i| {
            if (chk == null or i >= ctx.label_ids.len) continue;
            const want = gtk.gtk_check_button_get_active(chk) != 0;
            const had = if (i < ctx.label_initial.len) ctx.label_initial[i] else false;
            if (want == had) continue;
            const lj = std.json.Stringify.valueAlloc(a, .{ .issueId = iid, .labelId = ctx.label_ids[i] }, .{}) catch continue;
            mutate.fire(state.gpa, state.instance.?, state.token, if (want) "issueLabels.add" else "issueLabels.remove", lj);
        }
    }

    setIssueDialogSaving(ctx, true);
    const proc = if (ctx.issue_id != null) "issues.update" else "issues.create";
    submitAsync(state, proc, json, &onIssueSubmitDone, ctx);
}

fn onIssueSubmitDone(cb_ctx: ?*anyopaque, ok: bool, err: ?[*:0]const u8) void {
    const ctx: *CreateCtx = @ptrCast(@alignCast(cb_ctx orelse return));
    if (!ok) {
        setIssueDialogSaving(ctx, false);
        gtk.gtk_label_set_text(ctx.error_label, err orelse "Couldn't save the issue.");
        return;
    }
    if (ctx.issue_id) |iid| showIssueDetail(ctx.state, iid); // reflect the edit
    if (ctx.create_more != null and gtk.gtk_check_button_get_active(ctx.create_more) != 0) {
        resetIssueDialog(ctx);
        setIssueDialogSaving(ctx, false);
        _ = gtk.gtk_widget_grab_focus(ctx.title_entry);
        return;
    }
    gtk.adw_dialog_set_can_close(ctx.dialog, 1);
    _ = gtk.adw_dialog_close(ctx.dialog); // sync pulls the new issue → list refreshes
}

fn setIssueDialogSaving(ctx: *CreateCtx, saving: bool) void {
    gtk.adw_dialog_set_can_close(ctx.dialog, if (saving) 0 else 1);
    if (ctx.submit_btn != null) {
        gtk.gtk_widget_set_sensitive(ctx.submit_btn, if (saving) 0 else 1);
        gtk.gtk_button_set_label(ctx.submit_btn, if (saving) "Saving…" else if (ctx.editing) "Save" else "Create");
    }
    if (saving) gtk.gtk_label_set_text(ctx.error_label, "");
}

/// Reset the create form for the "create more" flow.
fn resetIssueDialog(ctx: *CreateCtx) void {
    gtk.gtk_editable_set_text(ctx.title_entry, "");
    ctx.editor.setText("");
    gtk.gtk_drop_down_set_selected(ctx.status_dd, indexOfOption(&status_options, "backlog"));
    gtk.gtk_drop_down_set_selected(ctx.priority_dd, indexOfOption(&priority_options, "none"));
    gtk.gtk_drop_down_set_selected(ctx.assignee_dd, 0);
    ctx.due_value = null;
    if (ctx.due_button != null) gtk.gtk_menu_button_set_child(ctx.due_button, dueChild(""));
    if (ctx.repeat_check != null) {
        gtk.gtk_check_button_set_active(ctx.repeat_check, 0);
        gtk.gtk_widget_set_sensitive(ctx.rec_interval_dd, 0);
        gtk.gtk_widget_set_sensitive(ctx.rec_unit_dd, 0);
    }
    for (ctx.label_checks) |chk| if (chk != null) gtk.gtk_check_button_set_active(chk, 0);
    gtk.gtk_label_set_text(ctx.error_label, "");
}

// ---------------------------------------------------------------------------
// Create-project dialog (the web create-project dialog, ported)
// ---------------------------------------------------------------------------

const ProjectCtx = struct {
    state: *AppState,
    dialog: gtk.Object,
    name_entry: gtk.Object,
    prefix_entry: gtk.Object,
    error_label: gtk.Object,
    submit_btn: gtk.Object = null,
    color: [7]u8 = "#6366f1".*, // selected swatch
    selected_swatch: gtk.Object = null, // swatch button currently ringed
    auto_prefix_buf: [16]u8 = undefined, // last auto-derived prefix
    auto_prefix_len: usize = 0,
};

fn onNewProjectClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    openProjectDialog(@ptrCast(@alignCast(data)));
}

fn onSettingsClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    const db = if (state.db) |*d| d else return;
    const instance = state.instance orelse return;

    // Current user id (for the "(you)" marker) from the active account.
    var uid_buf: ?[]u8 = null;
    defer if (uid_buf) |b| state.gpa.free(b);
    if (AccountStore.open(state.gpa)) |store_val| {
        var store = store_val;
        defer store.deinit();
        if (activeAccount(&store)) |acc| {
            if (acc.user_id) |u| uid_buf = state.gpa.dupe(u8, u) catch null;
        }
    } else |_| {}

    settings.open(state.gpa, instance, state.token, db, state.window, uid_buf, state.active_workspace_id);
}

fn onIntegrationsClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(data));
    const instance = state.instance orelse return;
    integrations.open(state.gpa, instance, state.token, state.window);
}

fn openProjectDialog(state: *AppState) void {
    if (state.instance == null or state.db == null) return;
    const ctx = state.gpa.create(ProjectCtx) catch return;
    ctx.state = state;
    ctx.color = "#6366f1".*;
    ctx.selected_swatch = null;
    ctx.auto_prefix_len = 0;

    const dialog = gtk.adw_dialog_new();
    gtk.adw_dialog_set_title(dialog, "New project");
    gtk.adw_dialog_set_content_width(dialog, 420);
    ctx.dialog = dialog;

    const tv = gtk.adw_toolbar_view_new();
    const header = gtk.adw_header_bar_new();
    const cancel = gtk.gtk_button_new_with_label("Cancel");
    _ = gtk.g_signal_connect_data(cancel, "clicked", @ptrCast(&onProjectCancel), ctx, null, 0);
    gtk.adw_header_bar_pack_start(header, cancel);
    const create = gtk.gtk_button_new_with_label("Create");
    gtk.gtk_widget_add_css_class(create, "suggested-action");
    _ = gtk.g_signal_connect_data(create, "clicked", @ptrCast(&onProjectSubmit), ctx, null, 0);
    gtk.adw_header_bar_pack_end(header, create);
    ctx.submit_btn = create;
    gtk.adw_toolbar_view_add_top_bar(tv, header);

    const form = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 10);
    gtk.gtk_widget_set_margin_top(form, 16);
    gtk.gtk_widget_set_margin_bottom(form, 16);
    gtk.gtk_widget_set_margin_start(form, 16);
    gtk.gtk_widget_set_margin_end(form, 16);

    const name = gtk.gtk_entry_new();
    gtk.gtk_entry_set_placeholder_text(name, "Project name");
    gtk.gtk_box_append(form, name);
    ctx.name_entry = name;

    const prefix = gtk.gtk_entry_new();
    gtk.gtk_entry_set_placeholder_text(prefix, "Prefix (e.g. EXP)");
    gtk.gtk_box_append(form, prefix);
    ctx.prefix_entry = prefix;
    // Auto-derive the prefix from the name (until the user customises it).
    _ = gtk.g_signal_connect_data(name, "changed", @ptrCast(&onProjectNameChanged), ctx, null, 0);

    // Colour swatch grid (matches the web ColorSwatchGrid; default indigo).
    gtk.gtk_box_append(form, widgets.swatchGrid(@ptrCast(ctx), @ptrCast(&onProjectSwatch), ctx.color[0..], &ctx.selected_swatch));

    const err = gtk.gtk_label_new("");
    gtk.gtk_widget_add_css_class(err, "error");
    gtk.gtk_box_append(form, err);
    ctx.error_label = err;

    gtk.adw_toolbar_view_set_content(tv, form);
    gtk.adw_dialog_set_child(dialog, tv);
    _ = gtk.g_signal_connect_data(dialog, "closed", @ptrCast(&onProjectClosed), ctx, null, 0);
    gtk.adw_dialog_present(dialog, state.window);
}

fn onProjectCancel(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *ProjectCtx = @ptrCast(@alignCast(data));
    _ = gtk.adw_dialog_close(ctx.dialog);
}

fn onProjectClosed(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *ProjectCtx = @ptrCast(@alignCast(data));
    ctx.state.gpa.destroy(ctx);
}

fn onProjectSwatch(btn: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *ProjectCtx = @ptrCast(@alignCast(data));
    const raw = gtk.g_object_get_data(btn, "exp-color") orelse return;
    const color = std.mem.span(@as([*:0]const u8, @ptrCast(raw)));
    if (color.len >= 7) @memcpy(ctx.color[0..7], color[0..7]);
    if (ctx.selected_swatch) |prev| gtk.gtk_widget_remove_css_class(prev, "exp-swatch-on");
    gtk.gtk_widget_add_css_class(btn, "exp-swatch-on");
    ctx.selected_swatch = btn;
}

/// Auto-derive the project prefix (uppercased alphanumerics, ≤6) from the name,
/// until the user types a custom prefix of their own.
fn onProjectNameChanged(entry: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *ProjectCtx = @ptrCast(@alignCast(data));
    const cur = std.mem.span(gtk.gtk_editable_get_text(ctx.prefix_entry));
    const last_auto = ctx.auto_prefix_buf[0..ctx.auto_prefix_len];
    if (cur.len != 0 and !std.mem.eql(u8, cur, last_auto)) return; // user customised it
    const name = std.mem.span(gtk.gtk_editable_get_text(entry));
    var buf: [16]u8 = undefined;
    var n: usize = 0;
    for (name) |ch| {
        if (n >= 6) break;
        if (std.ascii.isAlphanumeric(ch)) {
            buf[n] = std.ascii.toUpper(ch);
            n += 1;
        }
    }
    buf[n] = 0;
    @memcpy(ctx.auto_prefix_buf[0..n], buf[0..n]);
    ctx.auto_prefix_len = n;
    gtk.gtk_editable_set_text(ctx.prefix_entry, @ptrCast(&buf));
}

fn onProjectSubmit(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *ProjectCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    const db = if (state.db) |*d| d else return;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    const name = std.mem.trim(u8, std.mem.span(gtk.gtk_editable_get_text(ctx.name_entry)), " \t");
    const prefix = std.mem.trim(u8, std.mem.span(gtk.gtk_editable_get_text(ctx.prefix_entry)), " \t");
    if (name.len == 0 or prefix.len == 0) {
        gtk.gtk_label_set_text(ctx.error_label, "Name and prefix are required.");
        return;
    }
    const ws = state.active_workspace_id orelse (db.firstWorkspaceId(a) catch null) orelse {
        gtk.gtk_label_set_text(ctx.error_label, "No workspace found yet — wait for sync.");
        return;
    };

    const json = std.json.Stringify.valueAlloc(a, .{
        .workspaceId = ws,
        .name = name,
        .prefix = prefix,
        .color = @as([]const u8, ctx.color[0..]),
    }, .{}) catch return;

    gtk.adw_dialog_set_can_close(ctx.dialog, 0);
    if (ctx.submit_btn != null) {
        gtk.gtk_widget_set_sensitive(ctx.submit_btn, 0);
        gtk.gtk_button_set_label(ctx.submit_btn, "Saving…");
    }
    gtk.gtk_label_set_text(ctx.error_label, "");
    submitAsync(state, "projects.create", json, &onProjectSubmitDone, ctx);
}

fn onProjectSubmitDone(cb_ctx: ?*anyopaque, ok: bool, err: ?[*:0]const u8) void {
    const ctx: *ProjectCtx = @ptrCast(@alignCast(cb_ctx orelse return));
    if (ok) {
        gtk.adw_dialog_set_can_close(ctx.dialog, 1);
        _ = gtk.adw_dialog_close(ctx.dialog); // sync delivers the project → sidebar refreshes
        return;
    }
    gtk.adw_dialog_set_can_close(ctx.dialog, 1);
    if (ctx.submit_btn != null) {
        gtk.gtk_widget_set_sensitive(ctx.submit_btn, 1);
        gtk.gtk_button_set_label(ctx.submit_btn, "Create");
    }
    gtk.gtk_label_set_text(ctx.error_label, err orelse "Couldn't create the project.");
}

// --- Create workspace dialog (from the switcher's "New workspace") ----------

const WorkspaceCtx = struct {
    state: *AppState,
    dialog: gtk.Object,
    name_entry: gtk.Object,
    error_label: gtk.Object,
    submit_btn: gtk.Object = null,
};

fn openWorkspaceDialog(state: *AppState) void {
    if (state.instance == null) return;
    const ctx = state.gpa.create(WorkspaceCtx) catch return;
    ctx.state = state;

    const dialog = gtk.adw_dialog_new();
    gtk.adw_dialog_set_title(dialog, "New workspace");
    gtk.adw_dialog_set_content_width(dialog, 420);
    ctx.dialog = dialog;

    const tv = gtk.adw_toolbar_view_new();
    const header = gtk.adw_header_bar_new();
    const cancel = gtk.gtk_button_new_with_label("Cancel");
    _ = gtk.g_signal_connect_data(cancel, "clicked", @ptrCast(&onWorkspaceCancel), ctx, null, 0);
    gtk.adw_header_bar_pack_start(header, cancel);
    const create = gtk.gtk_button_new_with_label("Create");
    gtk.gtk_widget_add_css_class(create, "suggested-action");
    _ = gtk.g_signal_connect_data(create, "clicked", @ptrCast(&onWorkspaceSubmit), ctx, null, 0);
    gtk.adw_header_bar_pack_end(header, create);
    ctx.submit_btn = create;
    gtk.adw_toolbar_view_add_top_bar(tv, header);

    const form = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 10);
    gtk.gtk_widget_set_margin_top(form, 16);
    gtk.gtk_widget_set_margin_bottom(form, 16);
    gtk.gtk_widget_set_margin_start(form, 16);
    gtk.gtk_widget_set_margin_end(form, 16);

    const name = gtk.gtk_entry_new();
    gtk.gtk_entry_set_placeholder_text(name, "Workspace name");
    _ = gtk.g_signal_connect_data(name, "activate", @ptrCast(&onWorkspaceSubmit), ctx, null, 0);
    gtk.gtk_box_append(form, name);
    ctx.name_entry = name;

    const err = gtk.gtk_label_new("");
    gtk.gtk_widget_add_css_class(err, "error");
    gtk.gtk_box_append(form, err);
    ctx.error_label = err;

    gtk.adw_toolbar_view_set_content(tv, form);
    gtk.adw_dialog_set_child(dialog, tv);
    _ = gtk.g_signal_connect_data(dialog, "closed", @ptrCast(&onWorkspaceClosed), ctx, null, 0);
    gtk.adw_dialog_present(dialog, state.window);
}

fn onWorkspaceCancel(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *WorkspaceCtx = @ptrCast(@alignCast(data));
    _ = gtk.adw_dialog_close(ctx.dialog);
}

fn onWorkspaceClosed(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *WorkspaceCtx = @ptrCast(@alignCast(data));
    ctx.state.gpa.destroy(ctx);
}

fn onWorkspaceSubmit(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *WorkspaceCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    const name = std.mem.trim(u8, std.mem.span(gtk.gtk_editable_get_text(ctx.name_entry)), " \t");
    if (name.len == 0) {
        gtk.gtk_label_set_text(ctx.error_label, "Name is required.");
        return;
    }
    const json = std.json.Stringify.valueAlloc(a, .{ .name = name }, .{}) catch return;

    gtk.adw_dialog_set_can_close(ctx.dialog, 0);
    if (ctx.submit_btn != null) {
        gtk.gtk_widget_set_sensitive(ctx.submit_btn, 0);
        gtk.gtk_button_set_label(ctx.submit_btn, "Saving…");
    }
    gtk.gtk_label_set_text(ctx.error_label, "");
    submitAsync(state, "workspaces.create", json, &onWorkspaceSubmitDone, ctx);
}

fn onWorkspaceSubmitDone(cb_ctx: ?*anyopaque, ok: bool, err: ?[*:0]const u8) void {
    const ctx: *WorkspaceCtx = @ptrCast(@alignCast(cb_ctx orelse return));
    if (ok) {
        gtk.adw_dialog_set_can_close(ctx.dialog, 1);
        _ = gtk.adw_dialog_close(ctx.dialog); // sync delivers the workspace → switcher refreshes
        return;
    }
    gtk.adw_dialog_set_can_close(ctx.dialog, 1);
    if (ctx.submit_btn != null) {
        gtk.gtk_widget_set_sensitive(ctx.submit_btn, 1);
        gtk.gtk_button_set_label(ctx.submit_btn, "Create");
    }
    gtk.gtk_label_set_text(ctx.error_label, err orelse "Couldn't create the workspace.");
}

// --- First-run onboarding (a once-per-account welcome, gated by a local marker
//     file; the users shape doesn't sync onboardingCompletedAt). Reuses the
//     existing create-workspace / create-project dialogs. ---

fn onboardingMarkerPath(gpa: std.mem.Allocator, account_id: []const u8) ?[]u8 {
    const dir = storage.configDir(gpa) catch return null;
    defer gpa.free(dir);
    return std.fmt.allocPrint(gpa, "{s}/onboarded-{s}", .{ dir, account_id }) catch null;
}

fn hasOnboarded(gpa: std.mem.Allocator, account_id: []const u8) bool {
    const path = onboardingMarkerPath(gpa, account_id) orelse return true; // fail-safe: don't nag
    defer gpa.free(path);
    return storage.fileExists(path);
}

fn markOnboarded(gpa: std.mem.Allocator, account_id: []const u8) void {
    const dir = storage.configDir(gpa) catch return;
    defer gpa.free(dir);
    storage.ensureDir(dir) catch return;
    const path = onboardingMarkerPath(gpa, account_id) orelse return;
    defer gpa.free(path);
    storage.writeSecret(path, "") catch {};
}

const OnboardCtx = struct { state: *AppState, dialog: gtk.Object };

/// Present the welcome on the next idle tick, so the main window is shown first.
fn showOnboardingIdle(data: gtk.gpointer) callconv(.c) c_int {
    showOnboarding(@ptrCast(@alignCast(data)));
    return 0; // G_SOURCE_REMOVE
}

fn showOnboarding(state: *AppState) void {
    const ctx = state.gpa.create(OnboardCtx) catch return;
    ctx.state = state;

    const dialog = gtk.adw_dialog_new();
    gtk.adw_dialog_set_title(dialog, "Welcome");
    gtk.adw_dialog_set_content_width(dialog, 460);
    ctx.dialog = dialog;

    const tv = gtk.adw_toolbar_view_new();
    const header = gtk.adw_header_bar_new();
    gtk.adw_toolbar_view_add_top_bar(tv, header);

    const box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 12);
    gtk.gtk_widget_set_margin_top(box, 24);
    gtk.gtk_widget_set_margin_bottom(box, 24);
    gtk.gtk_widget_set_margin_start(box, 24);
    gtk.gtk_widget_set_margin_end(box, 24);
    gtk.gtk_widget_set_halign(box, gtk.ALIGN_CENTER);

    const icon = gtk.gtk_label_new(null);
    gtk.gtk_label_set_markup(icon, "<span size='xx-large' foreground='#818cf8'>\u{1F4CB}</span>");
    gtk.gtk_box_append(box, icon);

    const title = gtk.gtk_label_new(null);
    gtk.gtk_label_set_markup(title, "<span size='x-large' weight='bold'>Welcome to Exponential</span>");
    gtk.gtk_box_append(box, title);

    const desc = gtk.gtk_label_new("Create a project and start tracking work — then let a coding agent open pull requests for you. Already set up on the web? Just skip.");
    gtk.gtk_widget_add_css_class(desc, "dim-label");
    gtk.gtk_label_set_wrap(desc, 1);
    gtk.gtk_widget_set_halign(desc, gtk.ALIGN_CENTER);
    gtk.gtk_widget_set_size_request(desc, 360, -1);
    gtk.gtk_box_append(box, desc);

    const actions = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_set_halign(actions, gtk.ALIGN_CENTER);
    gtk.gtk_widget_set_margin_top(actions, 8);
    const proj_btn = gtk.gtk_button_new_with_label("New project");
    gtk.gtk_widget_add_css_class(proj_btn, "suggested-action");
    _ = gtk.g_signal_connect_data(proj_btn, "clicked", @ptrCast(&onOnboardProject), ctx, null, 0);
    gtk.gtk_box_append(actions, proj_btn);
    const agent_btn = gtk.gtk_button_new_with_label("Set up coding agent");
    _ = gtk.g_signal_connect_data(agent_btn, "clicked", @ptrCast(&onOnboardSetupAgent), ctx, null, 0);
    gtk.gtk_box_append(actions, agent_btn);
    const skip = gtk.gtk_button_new_with_label("Skip");
    gtk.gtk_widget_add_css_class(skip, "flat");
    _ = gtk.g_signal_connect_data(skip, "clicked", @ptrCast(&onOnboardSkip), ctx, null, 0);
    gtk.gtk_box_append(actions, skip);
    gtk.gtk_box_append(box, actions);

    gtk.adw_toolbar_view_set_content(tv, box);
    gtk.adw_dialog_set_child(dialog, tv);
    _ = gtk.g_signal_connect_data(dialog, "closed", @ptrCast(&onOnboardClosed), ctx, null, 0);
    gtk.adw_dialog_present(dialog, state.window);
}

fn onOnboardClosed(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *OnboardCtx = @ptrCast(@alignCast(data));
    ctx.state.gpa.destroy(ctx);
}

fn onOnboardWorkspace(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *OnboardCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    _ = gtk.adw_dialog_close(ctx.dialog); // frees ctx via onOnboardClosed
    openWorkspaceDialog(state);
}

fn onOnboardProject(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *OnboardCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    _ = gtk.adw_dialog_close(ctx.dialog);
    openProjectDialog(state);
}

fn onOnboardSetupAgent(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *OnboardCtx = @ptrCast(@alignCast(data));
    const state = ctx.state;
    _ = gtk.adw_dialog_close(ctx.dialog);
    // Open Settings, where the "Register this machine as a desktop agent"
    // section + the Connect-GitHub link live.
    onSettingsClicked(null, @ptrCast(state));
}

fn onOnboardSkip(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const ctx: *OnboardCtx = @ptrCast(@alignCast(data));
    _ = gtk.adw_dialog_close(ctx.dialog);
}

/// Called on a sync thread; coalesce bursts into a single main-loop refresh.
fn onSyncChanged(ctx: ?*anyopaque) callconv(.c) void {
    const state: *AppState = @ptrCast(@alignCast(ctx));
    if (!state.refresh_pending.swap(true, .acq_rel)) {
        _ = gtk.g_idle_add(@ptrCast(&refreshIdle), state);
    }
}

fn refreshIdle(data: gtk.gpointer) callconv(.c) c_int {
    const state: *AppState = @ptrCast(@alignCast(data));
    state.refresh_pending.store(false, .release);
    doRefresh(state);
    return 0; // G_SOURCE_REMOVE
}

fn refreshIssues(state: *AppState) void {
    if (state.issue_list == null) return;
    const db = if (state.db) |*d| d else return;

    var arena = std.heap.ArenaAllocator.init(state.gpa);
    defer arena.deinit();
    const a = arena.allocator();

    // Center title: project name (or "All Issues") + the filtered count.
    const issues = db.listIssues(a, state.selected_project_id, state.active_workspace_id, 2000) catch return;

    // issue_id → its label chips (one query, bucketed — no N+1). Used for both
    // the row dots and the label filter.
    var issue_labels = std.StringHashMap(std.ArrayListUnmanaged(Database.LabelChip)).init(a);
    if (db.listAllIssueLabels(a)) |chips| {
        for (chips) |c| {
            const gop = issue_labels.getOrPut(c.issue_id) catch continue;
            if (!gop.found_existing) gop.value_ptr.* = .empty;
            gop.value_ptr.append(a, c) catch {};
        }
    } else |_| {}

    gtk.gtk_list_box_remove_all(state.issue_list);

    var shown: usize = 0;
    for (format.status_display_order) |status_value| {
        // Explicit status filter (from the popover) hides whole groups.
        if (!matchesStatusFilter(state, status_value)) continue;
        // Collect this status's matching issues first, so an empty group is hidden.
        var group: std.ArrayListUnmanaged(Database.IssueRow) = .empty;
        for (issues) |iss| {
            if (!std.mem.eql(u8, iss.status, status_value)) continue;
            if (!format.tabIncludesStatus(state.active_tab, iss.status)) continue;
            if (!matchesSearch(state, iss.title)) continue;
            if (!matchesPriorityFilter(state, iss.priority)) continue;
            const chips_for = if (issue_labels.get(iss.id)) |list| list.items else &[_]Database.LabelChip{};
            if (!matchesLabelFilter(state, chips_for)) continue;
            group.append(a, iss) catch {};
        }
        if (group.items.len == 0) continue;
        shown += group.items.len;

        gtk.gtk_list_box_append(state.issue_list, statusGroupHeader(state, status_value, group.items.len));
        if (state.collapsed[statusDisplayIndex(status_value)]) continue;

        for (group.items) |iss| {
            const chips_for = if (issue_labels.get(iss.id)) |list| list.items else &[_]Database.LabelChip{};
            gtk.gtk_list_box_append(state.issue_list, issueRow(state, a, iss, chips_for));
        }
    }

    if (shown == 0) gtk.gtk_list_box_append(state.issue_list, emptyState(state));

    const title = state.selected_project_name orelse "All Issues";
    if (state.list_page) |lp| {
        if (std.fmt.allocPrintSentinel(a, "{s}  ·  {d}", .{ title, shown }, 0)) |hdr| {
            gtk.adw_navigation_page_set_title(lp, hdr.ptr);
        } else |_| {}
    }

    refreshPills(state);
}

/// Centered empty state shown when no issues match (distinguishes "filtered out"
/// from a genuinely empty tracker, guiding new users toward creating one).
fn emptyState(state: *AppState) gtk.Object {
    const has_filters = hasAnyFilter(state) or state.search_text != null;
    const box = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 6);
    gtk.gtk_widget_set_margin_top(box, 56);
    gtk.gtk_widget_set_halign(box, gtk.ALIGN_CENTER);
    const icon = gtk.gtk_label_new(null);
    gtk.gtk_label_set_markup(icon, "<span size='xx-large' foreground='#71717a'>\u{1F4CB}</span>");
    gtk.gtk_box_append(box, icon);
    const title = gtk.gtk_label_new(if (has_filters) "No matching issues" else "No issues yet");
    gtk.gtk_widget_add_css_class(title, "title-4");
    gtk.gtk_box_append(box, title);
    const hint = gtk.gtk_label_new(if (has_filters)
        "Try clearing filters or your search."
    else
        "Create your first issue with “New issue”.");
    gtk.gtk_widget_add_css_class(hint, "dim-label");
    gtk.gtk_box_append(box, hint);
    return box;
}

fn matchesSearch(state: *AppState, title: []const u8) bool {
    const q = state.search_text orelse return true;
    var buf: [512]u8 = undefined;
    const lower = if (title.len < buf.len) std.ascii.lowerString(buf[0..title.len], title) else title;
    return std.mem.indexOf(u8, lower, q) != null;
}

/// A collapsible status group header (chevron + status icon + label + count),
/// tagged so a row-activation toggles its collapsed state.
fn statusGroupHeader(state: *AppState, status_value: []const u8, count: usize) gtk.Object {
    const opt = format.status(status_value);
    const collapsed = state.collapsed[statusDisplayIndex(status_value)];

    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 6);
    gtk.gtk_widget_add_css_class(row, "exp-group-header");
    gtk.gtk_widget_set_margin_top(row, 4);
    gtk.gtk_widget_set_margin_start(row, 8);
    gtk.gtk_widget_set_margin_end(row, 8);
    gtk.gtk_widget_set_margin_top(row, 6);
    gtk.gtk_widget_set_margin_bottom(row, 2);

    const chevron = gtk.gtk_label_new(if (collapsed) "▸" else "▾");
    gtk.gtk_widget_add_css_class(chevron, "dim-label");
    gtk.gtk_box_append(row, chevron);
    gtk.gtk_box_append(row, widgets.iconLabel(opt));

    const lbl = gtk.gtk_label_new(null);
    var nbuf: [80]u8 = undefined;
    if (std.fmt.bufPrintZ(&nbuf, "<b>{s}</b>", .{opt.label})) |z| gtk.gtk_label_set_markup(lbl, z.ptr) else |_| {}
    gtk.gtk_widget_set_halign(lbl, gtk.ALIGN_START);
    gtk.gtk_widget_set_hexpand(lbl, 1);
    gtk.gtk_box_append(row, lbl);

    const cnt = gtk.gtk_label_new(null);
    var cbuf: [16]u8 = undefined;
    if (std.fmt.bufPrintZ(&cbuf, "{d}", .{count})) |z| gtk.gtk_label_set_text(cnt, z.ptr) else |_| {}
    gtk.gtk_widget_add_css_class(cnt, "dim-label");
    gtk.gtk_box_append(row, cnt);

    if (state.gpa.dupeZ(u8, status_value)) |tmp| {
        defer state.gpa.free(tmp);
        gtk.g_object_set_data_full(row, "exp-toggle-status", @ptrCast(gtk.g_strdup(tmp.ptr)), @ptrCast(&gtk.g_free));
    } else |_| {}
    return row;
}

/// A dense issue row: priority · identifier · status · title · label dots · due.
fn issueRow(state: *AppState, arena: std.mem.Allocator, iss: Database.IssueRow, chips: []const Database.LabelChip) gtk.Object {
    const row = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 10);
    gtk.gtk_widget_set_margin_top(row, 5);
    gtk.gtk_widget_set_margin_bottom(row, 5);
    gtk.gtk_widget_set_margin_start(row, 14);
    gtk.gtk_widget_set_margin_end(row, 10);
    if (state.gpa.dupeZ(u8, iss.id)) |tmp| {
        defer state.gpa.free(tmp);
        gtk.g_object_set_data_full(row, "exp-issue-id", @ptrCast(gtk.g_strdup(tmp.ptr)), @ptrCast(&gtk.g_free));
    } else |_| {}

    gtk.gtk_box_append(row, widgets.priorityIcon(iss.priority));

    const ident = gtk.gtk_label_new(iss.identifier.ptr);
    gtk.gtk_widget_add_css_class(ident, "monospace");
    gtk.gtk_widget_add_css_class(ident, "dim-label");
    gtk.gtk_widget_set_halign(ident, gtk.ALIGN_START);
    gtk.gtk_box_append(row, ident);

    gtk.gtk_box_append(row, widgets.statusIcon(iss.status));

    // Repeat glyph before the title for recurring issues (mirrors the web list).
    if (iss.recurrence_interval > 0) {
        const rec = gtk.gtk_label_new(null);
        gtk.gtk_label_set_markup(rec, "<span foreground='#818cf8'>\u{1F501}</span>");
        gtk.gtk_widget_set_tooltip_text(rec, "Recurring");
        gtk.gtk_box_append(row, rec);
    }

    const title = gtk.gtk_label_new(iss.title.ptr);
    gtk.gtk_widget_set_halign(title, gtk.ALIGN_START);
    gtk.gtk_widget_set_hexpand(title, 1);
    gtk.gtk_label_set_ellipsize(title, gtk.ELLIPSIZE_END);
    gtk.gtk_label_set_xalign(title, 0.0);
    gtk.gtk_box_append(row, title);

    // Label pills (dot + name), up to 3, with a "+N" overflow — matches the web row.
    if (chips.len > 0) {
        const pills = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 4);
        for (chips, 0..) |c, i| {
            if (i >= 3) break;
            gtk.gtk_box_append(pills, widgets.chip(arena, c.name, c.color));
        }
        if (chips.len > 3) {
            const more = gtk.gtk_label_new(null);
            if (std.fmt.allocPrintSentinel(arena, "+{d}", .{chips.len - 3}, 0)) |z| {
                gtk.gtk_label_set_text(more, z.ptr);
            } else |_| {}
            gtk.gtk_widget_add_css_class(more, "dim-label");
            gtk.gtk_box_append(pills, more);
        }
        gtk.gtk_box_append(row, pills);
    }

    if (iss.due_date.len > 0) {
        if (format.formatDue(arena, iss.due_date) catch null) |due| {
            const lbl = gtk.gtk_label_new(null);
            if (std.fmt.allocPrintSentinel(arena, "<span foreground='{s}'>🗓 {s}</span>", .{ due.color, due.text }, 0)) |m| {
                gtk.gtk_label_set_markup(lbl, m.ptr);
            } else |_| {}
            gtk.gtk_box_append(row, lbl);
        }
    }

    if (iss.assignee.len > 0) gtk.gtk_box_append(row, widgets.avatar(arena, iss.assignee));

    return row;
}

fn hasAccount(gpa: std.mem.Allocator) bool {
    var store = AccountStore.open(gpa) catch return false;
    defer store.deinit();
    return store.list().len > 0;
}

/// Write a .desktop file registering `x-scheme-handler/exp` → this binary so the
/// browser hands the OAuth redirect back to us. Best-effort.
fn registerSchemeHandler(gpa: std.mem.Allocator) !void {
    var exe_buf: [4096]u8 = undefined;
    const n = readlink("/proc/self/exe", &exe_buf, exe_buf.len);
    if (n <= 0) return;
    const exe = exe_buf[0..@intCast(n)];

    const home = std.c.getenv("HOME") orelse return;
    const apps_dir = try std.fmt.allocPrint(gpa, "{s}/.local/share/applications", .{std.mem.span(home)});
    defer gpa.free(apps_dir);
    storage.ensureDir(apps_dir) catch {};

    const path = try std.fmt.allocPrint(gpa, "{s}/at.exponential.desktop", .{apps_dir});
    defer gpa.free(path);
    const content = try std.fmt.allocPrint(
        gpa,
        "[Desktop Entry]\nType=Application\nName=Exponential\nExec={s} %u\nTerminal=false\nNoDisplay=true\nCategories=Development;\nMimeType=x-scheme-handler/exp;\n",
        .{exe},
    );
    defer gpa.free(content);
    std.Io.Dir.cwd().writeFile(io(), .{ .sub_path = path, .data = content }) catch {};

    const update_cmd = try std.fmt.allocPrintSentinel(gpa, "update-desktop-database {s}", .{apps_dir}, 0);
    defer gpa.free(update_cmd);
    _ = gtk.g_spawn_command_line_async(update_cmd.ptr, null);
    _ = gtk.g_spawn_command_line_async("xdg-mime default at.exponential.desktop x-scheme-handler/exp", null);
}

fn io() std.Io {
    return std.Io.Threaded.global_single_threaded.io();
}
