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

use crate::controls::PointerMenuButtonExt as _;
use std::collections::HashSet;

use gpui::{
    div, px, size, App, AppContext as _, Div, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, InputState},
    scroll::{Scrollbar, ScrollbarAxis},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};

use api::mcp_servers::{McpServerConfig, McpServerDevice, McpServerFields};

use crate::icons::{registry, ExpIcon};
use crate::controls::{disclosure_header, glass_input, ChevronSide, WebControl as _};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;
use crate::surface;

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
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| McpServerDialogView::new(team_id, initial, device, pane, name, window, cx));
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
fn auth_line(draft: &Draft) -> (ExpIcon, &'static str) {
    match draft.auth.as_str() {
        "oauth" => (registry::UI_SIGN_IN, "Members sign in with their own account (OAuth)"),
        "secret" => (registry::UI_PRIVATE, "Members paste their own API key"),
        _ => (registry::UI_SUCCESS, "No sign-in needed"),
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

pub struct McpServerDialogView {
    team_id: String,
    /// The row being edited; `None` = create.
    editing: Option<String>,
    /// The pane to refetch once the write lands.
    pane: WeakEntity<McpServersPane>,
    name: Entity<InputState>,
    url: Entity<InputState>,
    command: Entity<InputState>,
    args: Entity<InputState>,
    /// The header names (http) or variable names (stdio) — ONE field: the
    /// transport decides which side of the row it is, and switching keeps
    /// what was typed rather than hiding it in a second, invisible list.
    names: Entity<InputState>,
    scopes: Entity<InputState>,
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
    body_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

/// `dialogLineCases`: an edit or a leave touches the field, Enter tries a submit.
fn subscriptions_touch(
    subscriptions: &mut Vec<Subscription>,
    field: &'static str,
    input: &Entity<InputState>,
    cx: &mut gpui::Context<McpServerDialogView>,
) {
    subscriptions.push(cx.subscribe(input, move |this, _, event: &InputEvent, cx| {
        match event {
            InputEvent::Change | InputEvent::Blur => {
                this.touched.insert(field);
            }
            InputEvent::PressEnter { .. } => this.attempted = true,
            _ => return,
        }
        cx.notify();
    }));
}

impl McpServerDialogView {
    fn new(
        team_id: String,
        initial: Option<McpServerConfig>,
        device: Option<McpServerDevice>,
        pane: WeakEntity<McpServersPane>,
        prefill_name: Option<String>,
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
        let mut touch_subscriptions = Vec::new();
        let mut field = |placeholder: &'static str, value: String, cx: &mut gpui::Context<Self>| {
            cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder(placeholder);
                if !value.is_empty() {
                    state.set_value(value, window, cx);
                }
                state
            })
        };
        let name = field("linear", seed.name.clone(), cx);
        let url = field(
            "https://mcp.example.com/mcp",
            seed.url.clone().unwrap_or_default(),
            cx,
        );
        let command = field("npx", seed.command.clone().unwrap_or_default(), cx);
        let args = field("-y @acme/mcp", format_list(&seed.args), cx);
        let names = field(
            "X-Api-Key",
            format_list(if http { &seed.header_names } else { &seed.env_names }),
            cx,
        );
        let scopes = field("read", format_list(&seed.scopes), cx);

        for (field, input) in [("name", &name), ("url", &url), ("command", &command)] {
            subscriptions_touch(&mut touch_subscriptions, field, input, cx);
        }
        // The validation line and the submit gate re-evaluate per keystroke.
        let mut subscriptions = touch_subscriptions;
        for input in [&name, &url, &command, &args, &names, &scopes] {
            subscriptions.push(cx.subscribe(input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }));
        }

        Self {
            team_id,
            editing: initial.as_ref().map(|row| row.id.clone()),
            pane,
            name,
            url,
            command,
            args,
            names,
            scopes,
            transport: if http {
                "http".to_string()
            } else {
                "stdio".to_string()
            },
            auth: if initial.is_some() {
                seed.auth.clone()
            } else {
                "none".to_string()
            },
            // A new server starts pre-selected (web `EMPTY_MCP_SERVER_DRAFT`).
            enabled_by_default: initial.is_none() || seed.enabled_by_default,
            device_id: device.as_ref().map(|d| d.id.clone()),
            initial_device: device,
            busy: false,
            error: None,
            touched: HashSet::new(),
            attempted: false,
            advanced: !http,
            body_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    fn draft(&self, cx: &App) -> Draft {
        let text = |state: &Entity<InputState>| state.read(cx).value().trim().to_string();
        let names = parse_list(self.names.read(cx).value().as_ref());
        let http = self.transport != "stdio";
        Draft {
            name: text(&self.name),
            transport: self.transport.clone(),
            url: text(&self.url),
            header_names: if http { names.clone() } else { Vec::new() },
            command: text(&self.command),
            args: parse_list(self.args.read(cx).value().as_ref()),
            env_names: if http { Vec::new() } else { names },
            scopes: parse_list(self.scopes.read(cx).value().as_ref()),
            auth: self.auth.clone(),
            enabled_by_default: self.enabled_by_default,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.busy {
            return;
        }
        let draft = self.draft(cx);
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

    // -- render pieces --------------------------------------------------------

    fn input_row(
        &self,
        label: &'static str,
        input: &Entity<InputState>,
        window: &Window,
        cx: &App,
    ) -> Div {
        surface::glass_input_row(
            label,
            surface::glass_row_input(glass_input(input, window, cx)).into_any_element(),
            cx,
        )
    }

    /// A closed-vocabulary picker row (Transport · Auth): the label leading,
    /// the picked label trailing behind a caret, writing `value` back through
    /// `apply`.
    fn choice_row(
        &self,
        label: &'static str,
        id: &'static str,
        options: Vec<(&'static str, &'static str)>,
        current: &str,
        apply: fn(&mut Self, &str, &mut gpui::Context<Self>),
        cx: &mut gpui::Context<Self>,
    ) -> Div {
        let picked: SharedString = options
            .iter()
            .find(|(value, _)| *value == current)
            .map(|(_, label)| *label)
            .unwrap_or(current)
            .to_string()
            .into();
        let current = current.to_string();
        let view = cx.entity().downgrade();
        let control = crate::trigger_editor::picker_trigger(id.into(), picked, cx)
            .pointer_menu(move |mut menu, _window, _cx| {
                menu = crate::pickers::single_select_menu(menu);
                for (value, label) in &options {
                    let view = view.clone();
                    let value = value.to_string();
                    let on = value == current;
                    menu = menu.item(
                        crate::controls::pointer_label_item(SharedString::from(*label), false)
                            .checked(on)
                            .on_click(move |_, _, cx| {
                                if let Some(view) = view.upgrade() {
                                    let value = value.clone();
                                    view.update(cx, |view, cx| {
                                        apply(view, &value, cx);
                                        cx.notify();
                                    });
                                }
                            }),
                    );
                }
                menu
            })
            .into_any_element();
        surface::glass_picker_row(label, None, control, cx)
    }

    fn footer(&self, blocked: bool, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().flex_1())
            .child(
                Button::new("mcp-edit-cancel")
                    .outline()
                    .cursor_pointer()
                    .web_sm()
                    .label("Cancel")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.busy {
                            return;
                        }
                        native_dialog::close_dialog_window(window, cx);
                    })),
            )
            .child(
                // A press on the disabled button counts as trying to submit.
                div()
                    .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                        this.attempted = true;
                        cx.notify();
                    }))
                    .child(
                        Button::new("mcp-edit-save")
                            .primary()
                            .cursor_pointer()
                            .web_sm()
                            .label(if self.editing.is_some() {
                                "Save"
                            } else {
                                "Add server"
                            })
                            .loading(self.busy)
                            .disabled(blocked || self.busy)
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }
}

impl McpServerDialogView {
    /// VAPP-239 (settings-mcp.json `runsOnLabel`): THE device picker over the
    /// owner's own desktop apps and CLI daemons, first item `runsOnServer` =
    /// no device (the server broker calls it).
    fn runs_on_row(&self, cx: &mut gpui::Context<Self>) -> Div {
        let copy = &super::mcp::SETTINGS_MCP;
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let mut rows = vec![crate::picker::device_picker::DevicePickerDevice {
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
                rows.push(crate::picker::device_picker::DevicePickerDevice {
                    id: row.id.clone(),
                    name: row.label.clone().filter(|l| !l.is_empty()).or_else(|| row.device_id.clone()).unwrap_or_default(),
                    icon: row.icon.clone(),
                    server: kind == "server",
                    description: None,
                    disabled: false,
                });
            }
        }
        if let Some(current) = self.initial_device.as_ref().filter(|d| !rows.iter().any(|r| r.id == d.id)) {
            rows.push(crate::picker::device_picker::DevicePickerDevice {
                id: current.id.clone(),
                name: current.label.clone(),
                icon: current.icon.clone(),
                server: current.kind == "server",
                description: None,
                disabled: false,
            });
        }
        let picked = self.device_id.clone().unwrap_or_default();
        let label = rows.iter().find(|r| r.id == picked).map(|r| r.name.clone()).unwrap_or_else(|| copy.runs_on_server.to_string());
        let trigger = crate::trigger_editor::picker_trigger("mcp-edit-runs-on".into(), label, cx).into_any_element();
        let view = cx.entity().downgrade();
        let on_pick: crate::picker::OnPickerChange<String> = std::rc::Rc::new(move |next: Vec<String>, _window: &mut Window, cx: &mut App| {
            let Some(id) = next.into_iter().next() else { return };
            if let Some(view) = view.upgrade() {
                view.update(cx, |view, cx| {
                    view.device_id = (!id.is_empty()).then_some(id);
                    cx.notify();
                });
            }
        });
        let control = crate::picker::deferred(move |window, cx| {
            crate::picker::device_picker::device_picker(&rows, Some(picked), trigger, on_pick)
                .id(SharedString::from("mcp-edit-runs-on-picker"))
                .render(window, cx)
        })
        .into_any_element();
        surface::glass_picker_row(copy.runs_on_label, None, control, cx)
    }
}

impl Render for McpServerDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "McpServerDialogView").entered();
        let draft = self.draft(cx);
        let http = draft.is_http();
        let validation = validate(&draft);
        let muted = cx.theme().muted_foreground;

        // The form's rows, web alike: what every server needs first, the
        // transport and its details behind Advanced.
        let mut rows: Vec<Div> = vec![self.input_row("Name", &self.name, window, cx)];
        if http {
            rows.push(self.input_row("URL", &self.url, window, cx));
        }
        rows.push(self.runs_on_row(cx));
        rows.push(surface::glass_toggle_row(
            domain::skills::DEFAULT_LABEL,
            None,
            crate::controls::web_switch("mcp-edit-default")
                .checked(self.enabled_by_default)
                .on_click(cx.listener(|this, on: &bool, _, cx| {
                    this.enabled_by_default = *on;
                    cx.notify();
                }))
                .into_any_element(),
            cx,
        ));

        let (auth_icon, auth_text) = auth_line(&draft);
        let mut body = v_flex()
            .w_full()
            .gap_2()
            .child(surface::glass_group_rows(rows))
            .child(
                h_flex()
                    .px_1()
                    .gap_1p5()
                    .items_center()
                    .text_xs()
                    .text_color(muted)
                    .child(gpui_component::Icon::from(auth_icon).with_size(px(14.)))
                    .child(auth_text),
            )
            .child(
                disclosure_header(
                    "mcp-edit-advanced",
                    self.advanced,
                    ChevronSide::Leading,
                    div().text_xs().child("Advanced"),
                    cx,
                )
                .px_1()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.advanced = !this.advanced;
                    cx.notify();
                })),
            );
        if self.advanced {
            let auth_options = if http {
                vec![AUTH_NONE, AUTH_OAUTH, AUTH_SECRET]
            } else {
                vec![AUTH_NONE, AUTH_SECRET]
            };
            let mut advanced: Vec<Div> = vec![
                self.choice_row(
                    "Sign-in",
                    "mcp-edit-auth",
                    auth_options,
                    &self.auth,
                    |this, value, _cx| this.auth = value.to_string(),
                    cx,
                ),
                self.choice_row(
                    "Transport",
                    "mcp-edit-transport",
                    TRANSPORT_OPTIONS.to_vec(),
                    &self.transport,
                    |this, value, _cx| {
                        this.transport = value.to_string();
                        // OAuth is an HTTP-only kind; a stdio row falls back.
                        if this.transport == "stdio" && this.auth == "oauth" {
                            this.auth = "none".to_string();
                        }
                    },
                    cx,
                ),
            ];
            if !http {
                advanced.push(self.input_row("Command", &self.command, window, cx));
                advanced.push(self.input_row("Arguments", &self.args, window, cx));
            }
            advanced.push(self.input_row(
                match (draft.auth == "secret", http) {
                    (true, true) => "Header that carries the key",
                    (true, false) => "Variable that carries the key",
                    (false, true) => "Header names",
                    (false, false) => "Variable names",
                },
                &self.names,
                window,
                cx,
            ));
            if draft.auth == "oauth" {
                advanced.push(self.input_row("Scopes", &self.scopes, window, cx));
            }
            body = body.child(surface::glass_group_rows(advanced));
        }
        // The server's refusal wins over the client-side rule: it is the one
        // the person has not seen yet.
        if let Some(message) = self
            .error
            .clone()
            .or_else(|| dialog_line(&draft, &self.touched, self.attempted).map(SharedString::from))
        {
            body = body.child(crate::action_error::field_error(message, cx).px_1());
        }

        let body_scroll = self.body_scroll.clone();
        v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        v_flex()
                            .id("mcp-edit-body-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&body_scroll)
                            .child(body),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(Scrollbar::new(&body_scroll).axis(ScrollbarAxis::Vertical)),
                    ),
            )
            .child(self.footer(validation.is_some(), cx))
    }
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
