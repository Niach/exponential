//! Settings → MCP servers → "Add server" / "Edit" (EXP-810).
//!
//! Web parity: the `McpServerDialog` half of
//! `components/team/mcp-servers-section.tsx`. Authoring a server is
//! non-secret config only — a name, the transport, a URL or a command line,
//! the header/env NAMES a member's key fills, OAuth scopes and the
//! `enabledByDefault` switch. No credential is typed here (that is each
//! member's own Connect / Set key on the pane, held by the server); this
//! dialog only ever talks to `mcpServers.create` / `mcpServers.update`.
//!
//! Its own WINDOW rather than an [`crate::native_dialog::AlertSpec`]: the form
//! re-renders on its own state (the transport switches URL for Command, the
//! auth kind adds Scopes, the switch has to visibly move), and alert content
//! is built once. That was the whole reason the desktop half of EXP-792
//! shipped read-only.
//!
//! The list-shaped fields (arguments, header/env names, scopes) are ONE
//! whitespace-or-comma separated text field each rather than the web's chip
//! rows — the same call [`super::agents`] makes for an external agent's
//! `args`/`env`: a field whose values are single tokens does not need a chip
//! editor, and the validator refuses a bad name at submit with the server's
//! own sentence.
//!
//! #EXP-1629: the window stays native; its content is a document on the
//! native binding ([`document`] over [`McpServerDialogView::project`]);
//! #EXP-1784: the Runs on, Sign-in and Transport rows are genUI row Selects.

use std::collections::HashSet;
use std::rc::Rc;

use eui_gpui::{binding::Binding, view::SurfaceView};
use gpui::{div, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, WeakEntity, Window};
use gpui_component::{ActiveTheme as _, ElementExt as _};
use serde_json::{json, Value};

use api::mcp_servers::{McpServerConfig, McpServerDevice, McpServerFields};

use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::picker::device_picker::DevicePickerDevice;
use crate::queries;

use super::agent_doc::select_row;
use super::doc::{hex, not, path, IdeToasts, Ink, LINE, SM, XS};
use super::mcp_servers::McpServersPane;

/// (wire value, label) — the labels are the pane's own chip vocabulary
/// (web `MCP_TRANSPORT_LABELS` / `MCP_AUTH_LABELS`).
const TRANSPORT_OPTIONS: &[(&str, &str)] = &[("http", "HTTP"), ("stdio", "Command")];
const AUTH_NONE: (&str, &str) = ("none", "No sign-in");
const AUTH_OAUTH: (&str, &str) = ("oauth", "OAuth");
const AUTH_SECRET: (&str, &str) = ("secret", "API key");

/// Open the dialog over the MCP servers pane. `initial` = the row being
/// edited (`None` adds one); `pane` is refetched after a successful write —
/// `mcpServers.list` is a server read with no Electric echo, so the new row
/// only appears on a refetch.
pub(super) fn open(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    initial: Option<McpServerConfig>,
    device: Option<McpServerDevice>,
    pane: WeakEntity<McpServersPane>,
    // #VAPP-243: an add pre-filled with an app's upstream name.
    name: Option<String>,
) {
    let title = if initial.is_some() {
        "Edit MCP server"
    } else {
        "Add MCP server"
    };
    // The form owns the height (the rows come and go with the transport and
    // the auth kind), so cap against the opener and scroll inside.
    let height = (window.viewport_size().height * 0.85).min(px(540.));
    let spec = DialogSpec::new(title, size(px(460.), height))
        .resizable(size(px(400.), px(360.)));
    // The content box: the window less the panel's bottom padding, known
    // before the first paint so the footer never jumps.
    let content_height = f32::from(height) - 16.;
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| McpServerDialogView::new(team_id, initial, device, pane, name, content_height, window, cx));
        let busy = view.clone();
        DialogContent::new(view)
            // The view pins its own action bar and scrolls only the form.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).busy)
    });
}

/// The field set the dialog submits — the web `McpServerDraft`, read out of
/// the inputs so [`validate`] stays a pure function over it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Draft {
    name: String,
    /// `http` | `stdio`.
    transport: String,
    url: String,
    header_names: Vec<String>,
    command: String,
    args: Vec<String>,
    env_names: Vec<String>,
    scopes: Vec<String>,
    /// `none` | `oauth` | `secret`.
    auth: String,
    enabled_by_default: bool,
}

impl Draft {
    fn is_http(&self) -> bool {
        self.transport != "stdio"
    }

    /// The declared secret positions of this draft's transport — what an
    /// `auth: secret` row must hold exactly one of.
    fn names(&self) -> &[String] {
        if self.is_http() {
            &self.header_names
        } else {
            &self.env_names
        }
    }
}

/// `X-Api-Key, ACME_TOKEN` / `-y @acme/mcp` → the list the server takes.
/// Whitespace OR comma separated, order kept, duplicates dropped (the router
/// dedupes too, and a repeated chip is never what somebody meant).
fn parse_list(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in raw.split([',', ' ', '\t', '\n']) {
        let value = value.trim();
        if value.is_empty() || out.iter().any(|kept| kept == value) {
            continue;
        }
        out.push(value.to_string());
    }
    out
}

fn format_list(values: &[String]) -> String {
    values.join(" ")
}

/// The header/env NAME shape the server accepts (`mcpVariableNameSchema`,
/// web `MCP_VARIABLE_NAME_RE`: `^[A-Za-z_][A-Za-z0-9_-]*$`).
fn is_variable_name(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The URL rule, mirrored from the router's `urlSchema`: https anywhere,
/// http on loopback only (a local dev MCP never leaves the machine). `Err`
/// carries the sentence the web shows for the same input.
fn check_url(raw: &str) -> Result<(), &'static str> {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return Err("That URL does not parse.");
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // `user:pass@host:port` → `host`; a bracketed IPv6 literal keeps its
    // brackets, which is exactly what a hostname comparison wants.
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let host = match authority.strip_prefix('[') {
        Some(rest) => rest.split_once(']').map_or(authority, |(host, _)| host),
        None => authority.split_once(':').map_or(authority, |(host, _)| host),
    };
    if scheme.is_empty() || host.is_empty() {
        return Err("That URL does not parse.");
    }
    let scheme = scheme.to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    if scheme == "https" {
        return Ok(());
    }
    // VAPP-239: http on loopback or a LAN host (`device.urlCases`).
    if scheme == "http" && emcp::contract::upstream::mcp_url_allowed(raw) {
        return Ok(());
    }
    Err("The URL must be https:// (http:// only for localhost or a LAN host).")
}

/// The cross-field rules `mcpServers.create/update` enforce, client-side, so
/// the dialog says why before the round trip. `None` = submittable. Copy is
/// the web `validateMcpServerDraft`, string for string.
fn validate(draft: &Draft) -> Option<String> {
    if draft.name.trim().is_empty() {
        return Some("Give the server a name.".to_string());
    }
    let http = draft.is_http();
    if http {
        if draft.url.trim().is_empty() {
            return Some("Paste the server's URL.".to_string());
        }
        if let Err(message) = check_url(draft.url.trim()) {
            return Some(message.to_string());
        }
    } else if draft.command.trim().is_empty() {
        return Some("A command server needs a command.".to_string());
    }
    for name in draft.names() {
        if !is_variable_name(name) {
            return Some(format!("{name} is not a header or variable name."));
        }
    }
    if draft.auth == "oauth" && !http {
        return Some("OAuth sign-in works for HTTP servers only.".to_string());
    }
    if draft.auth == "secret" && draft.names().len() != 1 {
        return Some(if http {
            "An API-key server declares exactly one header name (Advanced): the one \
             that carries the key."
                .to_string()
        } else {
            "An API-key server declares exactly one variable name (Advanced): the one \
             that carries the key."
                .to_string()
        });
    }
    None
}

/// settings-mcp.json `dialogLineCases` (web `mcpDialogLine`): the line the
/// dialog shows. A problem that is only a still-empty required field waits
/// until that field was touched or a submit was tried; anything else shows at
/// once.
fn dialog_line(draft: &Draft, touched: &HashSet<&'static str>, attempted: bool) -> Option<String> {
    let problem = validate(draft)?;
    if attempted {
        return Some(problem);
    }
    let empty = if draft.name.trim().is_empty() {
        Some("name")
    } else if draft.is_http() {
        draft.url.trim().is_empty().then_some("url")
    } else {
        draft.command.trim().is_empty().then_some("command")
    };
    match empty {
        Some(field) if !touched.contains(field) => None,
        _ => Some(problem),
    }
}

/// How members get in (web `mcpAuthLine`): the glyph and the line under the
/// form's first group.
fn auth_line(draft: &Draft) -> (&'static str, &'static str) {
    match draft.auth.as_str() {
        "oauth" => ("ui-sign-in", "Members sign in with their own account (OAuth)"),
        "secret" => ("ui-private", "Members paste their own API key"),
        _ => ("ui-success", "No sign-in needed"),
    }
}

/// The wire field set, exactly as the web's `save` builds it: the transport's
/// own half only, so a stdio row never carries a URL the router's `urlSchema`
/// would reject on the way past.
fn wire_fields(draft: &Draft) -> McpServerFields {
    let http = draft.is_http();
    McpServerFields {
        name: Some(draft.name.trim().to_string()),
        transport: Some(draft.transport.clone()),
        auth: Some(draft.auth.clone()),
        enabled_by_default: Some(draft.enabled_by_default),
        scopes: Some(draft.scopes.clone()),
        url: http.then(|| draft.url.trim().to_string()),
        header_names: http.then(|| draft.header_names.clone()),
        command: (!http).then(|| draft.command.trim().to_string()),
        args: (!http).then(|| draft.args.clone()),
        env_names: (!http).then(|| draft.env_names.clone()),
    }
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct McpDialogShot {
    pub edit: bool,
    pub name: &'static str,
    pub url: &'static str,
    pub command: &'static str,
    pub args: &'static str,
    pub names: &'static str,
    pub scopes: &'static str,
    /// `http` when empty.
    pub transport: &'static str,
    /// `none` when empty.
    pub auth: &'static str,
    pub enabled: bool,
    pub advanced: bool,
    /// A device row picked (`devices.id`) in the Runs on picker.
    pub device: Option<&'static str>,
    pub error: Option<&'static str>,
    pub attempted: bool,
    pub busy: bool,
}

pub struct McpServerDialogView {
    team_id: String,
    /// The row being edited; `None` = create.
    editing: Option<String>,
    /// The pane to refetch once the write lands.
    pane: WeakEntity<McpServersPane>,
    /// The text fields as typed: they outlive the field being drawn (the
    /// transport and the auth kind take rows away), so what was typed stays
    /// when a row comes back.
    fields: Fields,
    transport: String,
    auth: String,
    enabled_by_default: bool,
    /// VAPP-239: the device it runs on (`devices.id`), `None` = the server
    /// broker; `initial_device` = the row's, kept as the current value even
    /// when it is another owner's machine.
    device_id: Option<String>,
    initial_device: Option<McpServerDevice>,
    busy: bool,
    error: Option<SharedString>,
    /// `dialogLineCases`: the fields the person touched, and a tried submit.
    touched: HashSet<&'static str>,
    attempted: bool,
    /// The Advanced disclosure (web alike: open from the start for a command
    /// server).
    advanced: bool,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// The Runs on rows the pictures add (no store rows headless).
    #[cfg(test)]
    shot_devices: Vec<DevicePickerDevice>,
    /// Frames the bench's opened popup still fades in over (the headless
    /// clock stands still).
    #[cfg(test)]
    shot_settle: u8,
}

/// The six text fields (ONE names field: the transport decides which side of
/// the row it is, and switching keeps what was typed).
#[derive(Clone, Default)]
struct Fields {
    name: String,
    url: String,
    command: String,
    args: String,
    names: String,
    scopes: String,
}

impl Fields {
    fn slot(&mut self, field: &str) -> Option<&mut String> {
        match field {
            "name" => Some(&mut self.name),
            "url" => Some(&mut self.url),
            "command" => Some(&mut self.command),
            "args" => Some(&mut self.args),
            "names" => Some(&mut self.names),
            "scopes" => Some(&mut self.scopes),
            _ => None,
        }
    }
}

/// The field a press, an edit or a leave was on (`dialogLineCases`).
fn touch_key(field: &str) -> Option<&'static str> {
    match field {
        "name" => Some("name"),
        "url" => Some("url"),
        "command" => Some("command"),
        _ => None,
    }
}

impl McpServerDialogView {
    #[allow(clippy::too_many_arguments)]
    fn new(
        team_id: String,
        initial: Option<McpServerConfig>,
        device: Option<McpServerDevice>,
        pane: WeakEntity<McpServersPane>,
        prefill_name: Option<String>,
        viewport_height: f32,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let mut seed = initial.clone().unwrap_or_default();
        if initial.is_none() {
            if let Some(name) = prefill_name {
                seed.name = name;
            }
        }
        let http = seed.is_http();
        let fields = Fields {
            name: seed.name.clone(),
            url: seed.url.clone().unwrap_or_default(),
            command: seed.command.clone().unwrap_or_default(),
            args: format_list(&seed.args),
            names: format_list(if http { &seed.header_names } else { &seed.env_names }),
            scopes: format_list(&seed.scopes),
        };

        // Every action leaves the binding for the view (the writes and the
        // window's close need its window).
        let me = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new("/pane", Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["edit", "touch", "attempt", "toggle", "advanced", "device", "auth", "transport", "cancel", "submit"] {
                let me = me.clone();
                binding.on_action(name, move |_, event, cx| {
                    let context = event.context.clone();
                    let payload = event.payload.clone().unwrap_or(Value::Null);
                    let me = me.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = me.update(cx, |view, cx| view.action(name, &context, &payload, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let theme = cx.theme();
        let chrome = Chrome {
            radius: f32::from(theme.radius),
            danger: hex(theme.danger),
            border: hex(theme.border),
            button_fg: hex(theme.button_foreground),
            button_fill: hex(theme.input_background()),
            button_stroke: hex(theme.input),
            button_off_fg: hex(theme.muted_foreground.opacity(0.5)),
            primary: hex(theme.button_primary),
            primary_fg: hex(theme.button_primary_foreground),
            primary_hover: hex(theme.button_primary_hover),
        };
        let view = super::doc::mount_with("mcp-server-dialog", document(&Ink::of(cx), &chrome), &binding, Rc::new(super::doc::ide_plugin()), true, window, cx);
        // The content owns the window's height: the rows scroll, the footer
        // stays (`DialogContent::self_scrolling`).
        view.update(cx, |view, cx| {
            view.set_fill_height(true, cx);
            view.set_viewport_height(viewport_height, cx);
        });

        Self {
            team_id,
            editing: initial.as_ref().map(|row| row.id.clone()),
            pane,
            fields,
            transport: if http { "http".to_string() } else { "stdio".to_string() },
            auth: if initial.is_some() { seed.auth.clone() } else { "none".to_string() },
            // A new server starts pre-selected (web `EMPTY_MCP_SERVER_DRAFT`).
            enabled_by_default: initial.is_none() || seed.enabled_by_default,
            device_id: device.as_ref().map(|d| d.id.clone()),
            initial_device: device,
            busy: false,
            error: None,
            touched: HashSet::new(),
            attempted: false,
            advanced: !http,
            binding,
            view,
            #[cfg(test)]
            shot_devices: Vec::new(),
            #[cfg(test)]
            shot_settle: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new("t".into(), None, None, WeakEntity::new_invalid(), None, 0., window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: McpDialogShot, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.editing = shot.edit.then(|| "s1".to_string());
        self.fields = Fields { name: shot.name.into(), url: shot.url.into(), command: shot.command.into(), args: shot.args.into(), names: shot.names.into(), scopes: shot.scopes.into() };
        self.transport = if shot.transport.is_empty() { "http".to_string() } else { shot.transport.to_string() };
        self.auth = if shot.auth.is_empty() { "none".to_string() } else { shot.auth.to_string() };
        self.enabled_by_default = shot.enabled;
        self.advanced = shot.advanced;
        self.device_id = shot.device.map(str::to_string);
        self.error = shot.error.map(SharedString::from);
        self.attempted = shot.attempted;
        self.busy = shot.busy;
        let device = |id: &str, name: &str, icon: &str, server: bool| DevicePickerDevice { id: id.into(), name: name.into(), icon: Some(icon.into()), server, description: None, disabled: false };
        self.shot_devices = vec![device("d1", "studio-mac", "laptop", false), device("d2", "build-server", "server", true)];
        cx.notify();
    }

    /// The bench's press on a document node (a headless click never reaches
    /// an eUI popup), its popup's enter fade run out.
    #[cfg(test)]
    pub(crate) fn press_for_shots(&mut self, id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| {
            let index = view.index_of(id).expect("the pressed node");
            view.press(index, window, cx);
            view.advance_clock(std::time::Duration::from_millis(500), cx);
        });
        self.shot_settle = 4;
        cx.notify();
    }

    /// A document action: a field's edit, a leave or an Enter on it
    /// (`dialogLineCases`: an edit or a leave touches the field, Enter tries
    /// a submit), the Default switch, the Advanced fold, the footer.
    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let value = payload.get("value").and_then(Value::as_str).unwrap_or_default();
        match name {
            "edit" => {
                let field = context.get("field").and_then(Value::as_str).unwrap_or_default();
                if let Some(slot) = self.fields.slot(field) {
                    *slot = value.to_string();
                    if let Some(key) = touch_key(field) {
                        self.touched.insert(key);
                    }
                }
            }
            "touch" => {
                if let Some(key) = context.get("field").and_then(Value::as_str).and_then(touch_key) {
                    self.touched.insert(key);
                }
            }
            "attempt" => self.attempted = true,
            "toggle" => {
                if let Some(on) = payload.get("checked").and_then(Value::as_bool) {
                    self.enabled_by_default = on;
                }
            }
            "advanced" => self.advanced = !self.advanced,
            // A Select's `change` also reports `{open}` as its popup opens.
            "device" | "auth" | "transport" if payload.get("value").is_none() => return,
            "device" => self.device_id = (!value.is_empty()).then(|| value.to_string()),
            "auth" => self.auth = value.to_string(),
            "transport" => {
                self.transport = value.to_string();
                // OAuth is an HTTP-only kind; a stdio row falls back.
                if self.transport == "stdio" && self.auth == "oauth" {
                    self.auth = "none".to_string();
                }
            }
            "cancel" if !self.busy => native_dialog::close_dialog_window(window, cx),
            "submit" => self.submit(window, cx),
            _ => {}
        }
        cx.notify();
    }

    fn draft(&self) -> Draft {
        let text = |value: &str| value.trim().to_string();
        let names = parse_list(&self.fields.names);
        let http = self.transport != "stdio";
        Draft {
            name: text(&self.fields.name),
            transport: self.transport.clone(),
            url: text(&self.fields.url),
            header_names: if http { names.clone() } else { Vec::new() },
            command: text(&self.fields.command),
            args: parse_list(&self.fields.args),
            env_names: if http { Vec::new() } else { names },
            scopes: parse_list(&self.fields.scopes),
            auth: self.auth.clone(),
            enabled_by_default: self.enabled_by_default,
        }
    }

    /// A field's change reaches the view after the 150 ms debounce: Save
    /// reads the text as it is NOW.
    fn flush_fields(&mut self, cx: &App) {
        let view = self.view.read(cx);
        for id in ["name", "url", "command", "args", "names", "scopes"] {
            if let (Some(text), Some(slot)) = (view.field_text(&format!("{id}.field"), cx), self.fields.slot(id)) {
                *slot = text;
            }
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.busy {
            return;
        }
        self.flush_fields(cx);
        let draft = self.draft();
        if validate(&draft).is_some() {
            self.attempted = true;
            cx.notify();
            return;
        }
        let verb = if self.editing.is_some() { "save" } else { "add" };
        let Some(trpc) = queries::trpc_client(cx) else {
            crate::action_error::failed_action(verb, "the server", Some("Not signed in.".into()), cx);
            return;
        };
        self.busy = true;
        self.error = None;
        cx.notify();

        let fields = wire_fields(&draft);
        let team_id = self.team_id.clone();
        let editing = self.editing.clone();
        // VAPP-239: `mcpServers.setDevice` after the row's own save, only
        // when the pick changed.
        let device_change = (self.device_id != self.initial_device.as_ref().map(|d| d.id.clone())).then(|| self.device_id.clone());
        let pane = self.pane.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    let row = match editing {
                        Some(id) => api::mcp_servers::update(&trpc, &id, &fields)?,
                        None => api::mcp_servers::create(&trpc, &team_id, &fields)?,
                    };
                    if let Some(device_id) = device_change {
                        api::mcp_servers::set_device(&trpc, &row.id, device_id.as_deref())?;
                    }
                    Ok::<(), api::ApiError>(())
                })
                .await;
            let _ = this.update_in(window, |this, window, cx| match result {
                Ok(()) => {
                    // `mcpServers.list` is a server read — the pane only shows
                    // the row after a refetch.
                    native_dialog::close_then(window, cx, move |_, cx| {
                        let _ = pane.update(cx, |pane, cx| pane.refetch(cx));
                    });
                }
                // FEED-79: what the server refused about the INPUT (a bad
                // URL or header, BAD_REQUEST) stays under the form; anything
                // else is the error toast (an add can duplicate: no Retry).
                Err(err @ api::ApiError::Http { status: 400, .. }) => {
                    this.busy = false;
                    this.error = Some(
                        super::form_error(&err, "That didn\u{2019}t go through. Try again.")
                            .into(),
                    );
                    cx.notify();
                }
                Err(err) => {
                    this.busy = false;
                    crate::action_error::failed_action_err(verb, "the server", &err, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl McpServerDialogView {
    /// VAPP-239 (settings-mcp.json `runsOnLabel`): the Runs on options, the
    /// owner's own desktop apps and CLI daemons, first item `runsOnServer` =
    /// no device (the server broker calls it).
    fn runs_on_options(&self, cx: &App) -> Vec<Value> {
        let copy = &super::mcp::SETTINGS_MCP;
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let mut rows = vec![DevicePickerDevice {
            id: String::new(),
            name: copy.runs_on_server.to_string(),
            icon: None,
            server: true,
            description: None,
            disabled: false,
        }];
        if let Some(store) = sync::Store::try_global(cx) {
            for row in store.collections().devices.read(cx).iter() {
                let kind = row.kind.as_deref().unwrap_or("desktop");
                if me.is_none() || row.user_id != me || (kind != "desktop" && kind != "server") {
                    continue;
                }
                rows.push(DevicePickerDevice {
                    id: row.id.clone(),
                    name: row.label.clone().filter(|l| !l.is_empty()).or_else(|| row.device_id.clone()).unwrap_or_default(),
                    icon: row.icon.clone(),
                    server: kind == "server",
                    description: None,
                    disabled: false,
                });
            }
        }
        #[cfg(test)]
        rows.extend(self.shot_devices.iter().map(|d| DevicePickerDevice { id: d.id.clone(), name: d.name.clone(), icon: d.icon.clone(), server: d.server, description: None, disabled: false }));
        if let Some(current) = self.initial_device.as_ref().filter(|d| !rows.iter().any(|r| r.id == d.id)) {
            rows.push(DevicePickerDevice {
                id: current.id.clone(),
                name: current.label.clone(),
                icon: current.icon.clone(),
                server: current.kind == "server",
                description: None,
                disabled: false,
            });
        }
        rows.into_iter().map(|r| json!({"label": r.name, "value": r.id, "icon": crate::icons::device_icon_name(r.icon.as_deref(), r.server)})).collect()
    }

    /// The state as the document binds it.
    fn project(&self, cx: &App) -> Value {
        let draft = self.draft();
        let http = draft.is_http();
        let (auth_icon, auth_text) = auth_line(&draft);
        // The server's refusal wins over the client-side rule: it is the one
        // the person has not seen yet.
        let line = self.error.as_ref().map(ToString::to_string).or_else(|| dialog_line(&draft, &self.touched, self.attempted));
        let blocked = validate(&draft).is_some();
        let choices = |options: &[(&str, &str)]| options.iter().map(|(value, label)| json!({"label": label, "value": value})).collect::<Vec<_>>();
        let auth_options = if self.transport == "stdio" { vec![AUTH_NONE, AUTH_SECRET] } else { vec![AUTH_NONE, AUTH_OAUTH, AUTH_SECRET] };
        json!({
            "fields": {"name": self.fields.name, "url": self.fields.url, "command": self.fields.command, "args": self.fields.args, "names": self.fields.names, "scopes": self.fields.scopes},
            "http": http,
            "stdio": !http,
            "oauth": draft.auth == "oauth",
            "advanced": self.advanced,
            "advancedClosed": !self.advanced,
            "enabled": self.enabled_by_default,
            "pickers": {
                "runsOn": {"value": self.device_id.clone().unwrap_or_default(), "options": self.runs_on_options(cx)},
                "signIn": {"value": self.auth, "options": choices(&auth_options)},
                "transport": {"value": self.transport, "options": choices(TRANSPORT_OPTIONS)},
            },
            "namesLabel": match (draft.auth == "secret", http) {
                (true, true) => "Header that carries the key",
                (true, false) => "Variable that carries the key",
                (false, true) => "Header names",
                (false, false) => "Variable names",
            },
            "authIcon": auth_icon,
            "authText": auth_text,
            "hasError": line.is_some(),
            "error": line.unwrap_or_default(),
            "primary": if self.editing.is_some() { "Save" } else { "Add server" },
            "on": !blocked && !self.busy,
            "off": blocked && !self.busy,
            "busy": self.busy,
        })
    }
}

impl Render for McpServerDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "McpServerDialogView").entered();
        #[cfg(test)]
        if self.shot_settle > 0 {
            self.shot_settle -= 1;
            self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
            cx.notify();
        }
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        // The window owns the height (resizable): the document fills it.
        let surface = self.view.clone();
        div().size_full().min_w_0().on_prepaint(move |bounds, _, cx| {
            surface.update(cx, |view, cx| view.set_viewport_height(f32::from(bounds.size.height), cx));
        }).child(self.view.clone())
    }
}

/// The colours the document needs beyond [`Ink`]: the error ink, the footer
/// hairline and the outline / primary `Button`s' paints.
struct Chrome {
    radius: f32,
    danger: String,
    border: String,
    button_fg: String,
    button_fill: String,
    button_stroke: String,
    button_off_fg: String,
    primary: String,
    primary_fg: String,
    primary_hover: String,
}

/// One `glass_input_row`: genUI's row Input.
fn input_row(id: &str, label: Value, placeholder: &str, visible: Value) -> Value {
    json!({"id": id, "component": "Input", "visible": visible,
        "props": {"variant": "row", "label": label, "name": id, "placeholder": placeholder, "value": path(&format!("/pane/fields/{id}"))},
        "on": {"change": {"event": {"name": "edit", "context": {"field": id}}}, "blur": {"event": {"name": "touch", "context": {"field": id}}}, "submit": {"event": {"name": "attempt"}}}})
}

/// A `web_sm` `Button` (32 px, `px 12`, `text_sm` on a line of 1, the theme
/// radius; `gap_1` before a glyph).
#[allow(clippy::too_many_arguments)]
fn sm_button(ink: &Ink, id: &str, glyph: Option<Value>, label: Value, fill: &str, stroke: Option<&str>, fg: &str, event: &str, radius: f32) -> Value {
    let size = ink.r(SM);
    let text = json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": label.clone()}, "style": {"fontSize": size, "lineHeight": size, "color": fg, "flexShrink": 0}});
    let mut button = json!({"id": id, "component": "Box", "props": {"pressable": true}, "accessibility": {"label": label}, "on": {"press": {"event": {"name": event}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 32, "paddingHorizontal": 12, "borderRadius": radius, "backgroundColor": fill, "cursor": "pointer"},
        "children": glyph.into_iter().chain([text]).collect::<Vec<_>>()});
    if let Some(stroke) = stroke {
        button["style"]["borderWidth"] = json!(1);
        button["style"]["borderColor"] = json!(stroke);
    }
    button
}

/// The dialog's tree: the rows scroll (the IDE's overlay scrollbar), the
/// footer stays.
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let rule = |id: &str, visible: Value| ink.divider(id, visible);
    // The first group, web alike: what every server needs.
    let name = input_row("name", json!("Name"), "linear", json!(true));
    let url = input_row("url", json!("URL"), "https://mcp.example.com/mcp", path("/pane/http"));
    let device = select_row("runs_on", super::mcp::SETTINGS_MCP.runs_on_label, "pickers/runsOn", "device", json!({"popupMinWidth": 170}));
    let toggle = ink.shell("default", false, json!(true), vec![
        json!({"id": "default_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "flexShrink": 1, "minWidth": 0}, "children": [ink.text("default_label", json!(domain::skills::DEFAULT_LABEL), SM, &ink.foreground)]}),
        json!({"id": "default_switch", "component": "Switch", "props": {"label": "", "name": "enabledByDefault", "checked": path("/pane/enabled")}, "on": {"change": {"event": {"name": "toggle"}}}}),
    ]);
    let first = ink.group("group", vec![name, rule("url_rule", path("/pane/http")), url, rule("device_rule", json!(true)), device, rule("default_rule", json!(true)), toggle]);

    // How members get in (web `mcpAuthLine`).
    let auth_line = json!({"id": "auth_line", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.375), "paddingHorizontal": ink.r(0.25)},
        "children": [
            {"id": "auth_icon", "component": "Icon", "props": {"name": path("/pane/authIcon")}, "style": {"width": 14, "height": 14, "color": ink.muted, "flexShrink": 0}},
            ink.text("auth_text", path("/pane/authText"), XS, &ink.muted),
        ]});

    // The Advanced fold (`disclosure_header`, chevron leading): muted at
    // rest, the whole line brightening under the pointer. A node's `:hover`
    // can't reach its children and Text / Icon take their ink from their
    // own recipes, so the bright face is a layer over the muted one that
    // only shows while it is hovered; it also carries the press.
    let size = ink.r(XS);
    let face = |id: &str, color: &str| {
        let glyph = |suffix: &str, icon: &str, visible: Value| json!({"id": format!("{id}_{suffix}"), "component": "Icon", "visible": visible, "props": {"name": icon}, "style": {"width": size, "height": size, "color": color, "flexShrink": 0}});
        vec![
            glyph("closed", "ui-chevron-right", path("/pane/advancedClosed")),
            glyph("open", "ui-chevron-down", path("/pane/advanced")),
            json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": "Advanced"}, "style": {"fontSize": size, "lineHeight": size * LINE, "color": color}}),
        ]
    };
    let hover = json!({"id": "advanced_hover", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": "Advanced"}, "on": {"press": {"event": {"name": "advanced"}}},
        "style": {"position": "absolute", "top": 0, "left": 0, "right": 0, "bottom": 0, "display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.25), "cursor": "pointer", "opacity": 0, ":hover": {"opacity": 1}},
        "children": face("advanced_bright", &ink.foreground)});
    let mut children = face("advanced_muted", &ink.muted);
    children.push(hover);
    let fold = json!({"id": "advanced_toggle", "component": "Box",
        "style": {"position": "relative", "display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.25)},
        "children": children});

    // Behind the fold: how it signs in, how it runs, the transport's fields.
    let stdio = path("/pane/stdio");
    let oauth = path("/pane/oauth");
    let auth = select_row("sign_in", "Sign-in", "pickers/signIn", "auth", json!({"popupMinWidth": 170}));
    let transport = select_row("transport", "Transport", "pickers/transport", "transport", json!({"popupMinWidth": 170}));
    let command = input_row("command", json!("Command"), "npx", stdio.clone());
    let args = input_row("args", json!("Arguments"), "-y @acme/mcp", stdio.clone());
    let names = input_row("names", path("/pane/namesLabel"), "X-Api-Key", json!(true));
    let scopes = input_row("scopes", json!("Scopes"), "read", oauth.clone());
    let mut more = ink.group("advanced", vec![
        auth, rule("transport_rule", json!(true)), transport,
        rule("command_rule", stdio.clone()), command, rule("args_rule", stdio), args,
        rule("names_rule", json!(true)), names, rule("scopes_rule", oauth), scopes,
    ]);
    more["visible"] = path("/pane/advanced");

    let error = json!({"id": "error_row", "component": "Box", "visible": path("/pane/hasError"), "style": {"paddingHorizontal": ink.r(0.25), "width": "100%"},
        "children": [{"id": "error", "component": "Text", "props": {"text": path("/pane/error")}, "style": {"fontSize": crate::action_error::FIELD_ERROR_PX, "lineHeight": 20, "color": chrome.danger}}]});

    let body = json!({"id": "body", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%"},
        "children": [first, auth_line, fold, more, error]});
    let scroller = json!({"id": "body_scroll", "component": "Box", "style": {"flexGrow": 1, "flexShrink": 1, "minHeight": 0, "width": "100%", "overflowY": "auto"}, "children": [body]});

    // The footer: Cancel, then the primary (its face per state: a press on
    // the disabled one counts as trying to submit).
    let cancel = {
        let mut button = sm_button(ink, "cancel", None, json!("Cancel"), &chrome.button_fill, Some(&chrome.button_stroke), &chrome.button_fg, "cancel", chrome.radius);
        button["visible"] = not(path("/pane/busy"));
        button
    };
    let cancel_off = {
        let mut button = sm_button(ink, "cancel_off", None, json!("Cancel"), "#00000000", Some(&chrome.button_stroke), &chrome.button_off_fg, "cancel", chrome.radius);
        button["visible"] = path("/pane/busy");
        button["props"]["pressable"] = json!(false);
        button["style"]["cursor"] = json!("default");
        // The wash and the stroke at half (`Button::disabled`, outline).
        button["style"]["position"] = json!("relative");
        button["style"]["borderColor"] = json!("#00000000");
        button["children"].as_array_mut().expect("the button's children").insert(0, json!({"id": "cancel_off_fill", "component": "Box",
            "style": {"position": "absolute", "top": -1, "left": -1, "right": -1, "bottom": -1, "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": chrome.button_stroke, "backgroundColor": chrome.button_fill, "opacity": 0.5}}));
        button
    };
    let mut primary = sm_button(ink, "primary", None, path("/pane/primary"), &chrome.primary, None, &chrome.primary_fg, "submit", chrome.radius);
    primary["visible"] = path("/pane/on");
    primary["style"][":hover"] = json!({"backgroundColor": chrome.primary_hover});
    let off = |id: &str, glyph: Option<Value>, visible: Value| {
        let mut button = sm_button(ink, id, glyph, path("/pane/primary"), "#00000000", None, &chrome.primary_fg, "attempt", chrome.radius);
        button["visible"] = visible;
        button["style"]["cursor"] = json!("default");
        button["style"]["position"] = json!("relative");
        button["children"].as_array_mut().expect("the button's children").insert(0, json!({"id": format!("{id}_fill"), "component": "Box",
            "style": {"position": "absolute", "top": 0, "left": 0, "right": 0, "bottom": 0, "borderRadius": chrome.radius, "backgroundColor": chrome.primary, "opacity": 0.5}}));
        button
    };
    let row = json!({"id": "footer_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "flex-end", "gap": ink.r(0.5), "paddingTop": ink.r(0.75)},
        "children": [cancel, cancel_off, primary, off("primary_off", None, path("/pane/off")), off("primary_busy", None, path("/pane/busy"))]});
    let footer = json!({"id": "footer", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexShrink": 0, "width": "100%"},
        "children": [{"id": "footer_rule", "component": "Box", "style": {"height": 1, "flexShrink": 0, "backgroundColor": chrome.border}}, row]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "width": "100%", "height": "100%", "minWidth": 0}, "children": [scroller, footer]})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http_draft() -> Draft {
        Draft {
            name: "linear".into(),
            transport: "http".into(),
            url: "https://mcp.linear.app/mcp".into(),
            auth: "none".into(),
            ..Default::default()
        }
    }

    /// One field, whitespace OR comma separated, order kept and duplicates
    /// dropped — and it round-trips what an existing row was seeded from.
    #[test]
    fn list_fields_round_trip() {
        assert_eq!(parse_list("  -y   @acme/mcp "), ["-y", "@acme/mcp"]);
        assert_eq!(parse_list("X-Api-Key, X-Other"), ["X-Api-Key", "X-Other"]);
        assert_eq!(parse_list("dup dup"), ["dup"]);
        assert!(parse_list("  ,  ").is_empty());
        let values = vec!["read".to_string(), "write".to_string()];
        assert_eq!(parse_list(&format_list(&values)), values);
    }

    /// The router's `mcpVariableNameSchema` shape (web `MCP_VARIABLE_NAME_RE`).
    #[test]
    fn variable_names_follow_the_server_shape() {
        assert!(is_variable_name("X-Api-Key"));
        assert!(is_variable_name("_TOKEN"));
        assert!(is_variable_name("A1"));
        assert!(!is_variable_name(""));
        assert!(!is_variable_name("1TOKEN"));
        assert!(!is_variable_name("-lead"));
        assert!(!is_variable_name("has space"));
        assert!(!is_variable_name("has.dot"));
    }

    /// The router's `urlSchema`: https anywhere, http on loopback only.
    #[test]
    fn url_rule_mirrors_the_router() {
        assert!(check_url("https://mcp.example.com/mcp").is_ok());
        assert!(check_url("http://localhost:8123/mcp").is_ok());
        assert!(check_url("http://127.0.0.1:8123/mcp").is_ok());
        assert!(check_url("http://homeassistant.local:8123/mcp").is_ok());
        assert!(check_url("http://192.168.1.20/mcp").is_ok());
        assert_eq!(
            check_url("http://mcp.example.com/mcp"),
            Err("The URL must be https:// (http:// only for localhost or a LAN host).")
        );
        assert_eq!(
            check_url("ftp://mcp.example.com"),
            Err("The URL must be https:// (http:// only for localhost or a LAN host).")
        );
        assert_eq!(check_url("mcp.example.com"), Err("That URL does not parse."));
        assert_eq!(check_url("https://"), Err("That URL does not parse."));
    }

    /// The cross-field rules, in the order the web validator applies them —
    /// a submittable draft is one `mcpServers.create` accepts.
    #[test]
    fn validation_mirrors_the_web_rules() {
        assert_eq!(validate(&http_draft()), None);

        let nameless = Draft {
            name: "  ".into(),
            ..http_draft()
        };
        assert_eq!(validate(&nameless).as_deref(), Some("Give the server a name."));

        let no_url = Draft {
            url: String::new(),
            ..http_draft()
        };
        assert_eq!(
            validate(&no_url).as_deref(),
            Some("Paste the server's URL.")
        );

        let no_command = Draft {
            transport: "stdio".into(),
            url: String::new(),
            ..http_draft()
        };
        assert_eq!(
            validate(&no_command).as_deref(),
            Some("A command server needs a command.")
        );

        let bad_name = Draft {
            header_names: vec!["1bad".into()],
            ..http_draft()
        };
        assert_eq!(
            validate(&bad_name).as_deref(),
            Some("1bad is not a header or variable name.")
        );

        // OAuth is HTTP-only, and a secret row declares exactly one position.
        let stdio_oauth = Draft {
            transport: "stdio".into(),
            command: "npx".into(),
            auth: "oauth".into(),
            ..http_draft()
        };
        assert_eq!(
            validate(&stdio_oauth).as_deref(),
            Some("OAuth sign-in works for HTTP servers only.")
        );
        let two_secrets = Draft {
            auth: "secret".into(),
            header_names: vec!["A".into(), "B".into()],
            ..http_draft()
        };
        assert!(validate(&two_secrets)
            .as_deref()
            .is_some_and(|message| message.contains("exactly one header name")));
        let one_secret = Draft {
            auth: "secret".into(),
            header_names: vec!["X-Api-Key".into()],
            ..http_draft()
        };
        assert_eq!(validate(&one_secret), None);
        let stdio_secret = Draft {
            transport: "stdio".into(),
            command: "npx".into(),
            auth: "secret".into(),
            env_names: Vec::new(),
            ..http_draft()
        };
        assert!(validate(&stdio_secret)
            .as_deref()
            .is_some_and(|message| message.contains("exactly one variable name")));
    }

    /// The wire carries the transport's OWN half only: a stdio row with a
    /// leftover URL in the field must not send it (the router's `urlSchema`
    /// would refuse the request outright), and an http one sends no command.
    #[test]
    fn wire_fields_carry_one_transport_half() {
        let http = wire_fields(&Draft {
            header_names: vec!["X-Api-Key".into()],
            args: vec!["-y".into()],
            env_names: vec!["NOPE".into()],
            ..http_draft()
        });
        assert_eq!(http.url.as_deref(), Some("https://mcp.linear.app/mcp"));
        assert_eq!(http.header_names, Some(vec!["X-Api-Key".to_string()]));
        assert_eq!(http.command, None);
        assert_eq!(http.args, None);
        assert_eq!(http.env_names, None);

        let stdio = wire_fields(&Draft {
            transport: "stdio".into(),
            command: " npx ".into(),
            args: vec!["-y".into(), "@acme/mcp".into()],
            env_names: vec!["ACME_TOKEN".into()],
            ..http_draft()
        });
        assert_eq!(stdio.url, None);
        assert_eq!(stdio.header_names, None);
        assert_eq!(stdio.command.as_deref(), Some("npx"));
        assert_eq!(stdio.args, Some(vec!["-y".to_string(), "@acme/mcp".to_string()]));
        assert_eq!(stdio.env_names, Some(vec!["ACME_TOKEN".to_string()]));
        // Every write is a FULL row, so an update can never leave the merged
        // one half-normalized.
        assert_eq!(stdio.name.as_deref(), Some("linear"));
        assert_eq!(stdio.transport.as_deref(), Some("stdio"));
        assert_eq!(stdio.auth.as_deref(), Some("none"));
        assert_eq!(stdio.enabled_by_default, Some(false));
    }

    /// The hint under the group names what the auth kind means for the
    /// transport on screen (web parity).
    #[test]
    fn auth_line_follows_auth() {
        assert_eq!(auth_line(&http_draft()).1, "No sign-in needed");
        let oauth = Draft {
            auth: "oauth".into(),
            ..http_draft()
        };
        assert_eq!(auth_line(&oauth).1, "Members sign in with their own account (OAuth)");
        let secret = Draft {
            auth: "secret".into(),
            ..http_draft()
        };
        assert_eq!(auth_line(&secret).1, "Members paste their own API key");
    }

    /// settings-mcp.json `dialogLineCases`.
    #[test]
    fn dialog_line_replays_the_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../packages/domain-contract/fixtures/settings-mcp.json"
        ))
        .unwrap();
        let text = |value: &serde_json::Value, key: &str| value[key].as_str().unwrap().to_string();
        for case in fixture["dialogLineCases"].as_array().unwrap() {
            let raw = &case["draft"];
            let draft = Draft {
                name: text(raw, "name"),
                transport: text(raw, "transport"),
                url: text(raw, "url"),
                command: text(raw, "command"),
                ..http_draft()
            };
            let touched: HashSet<&'static str> = ["name", "url", "command"]
                .into_iter()
                .filter(|field| case["touched"].as_array().unwrap().iter().any(|t| t == field))
                .collect();
            assert_eq!(
                dialog_line(&draft, &touched, case["attempted"].as_bool().unwrap()).is_some(),
                case["shows"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}
