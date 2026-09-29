//! EXP-792 — the MCP server picker (web `@exp/ui` `McpServerPicker`): team
//! MCP servers by mark + name, the host (or the command) as the muted
//! second line. The launch composer's `⋯` picks the servers a run connects
//! to (multi); Settings → MCP servers wears the same glyph per row. A server
//! wears the REAL brand mark of the service it belongs to when its URL is
//! in the catalog (contract `mcpCatalog` → the generated brand set,
//! `registry::brand`), a terminal for a command, the plug for anything
//! else. Nobody draws a brand path by hand: the marks are vendored selfh.st
//! light SVGs, rasterized by gpui as a one-tint mask like every Lucide glyph.

use gpui::{AnyElement, SharedString};

use domain::contract::{MCP_CATALOG_IDS, MCP_CATALOG_MARKS, MCP_CATALOG_NAMES, MCP_CATALOG_URLS};

use crate::icons::{registry, ExpIcon};

use super::{OnPickerChange, Picker, PickerItem};

/// One well-known hosted MCP server (web `McpCatalogEntry`): a row of the
/// contract's parallel tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct McpCatalogEntry {
    pub id: &'static str,
    pub name: &'static str,
    /// The endpoint, or a TEMPLATE containing `{host}` (self-managed installs).
    pub url: &'static str,
    /// The brand slug (`registry::brand::by_slug`).
    pub mark: &'static str,
}

impl McpCatalogEntry {
    /// `url` is a template: never probed, never "Added".
    pub fn is_template(&self) -> bool {
        is_mcp_catalog_template(self.url)
    }
}

/// The catalog, entry for entry with the web `MCP_CATALOG` (the contract's
/// table). Dropped for lack of a selfh.st LIGHT mark (add one to icons.json
/// `brand` and the row to contract.json once it exists): Vercel, Asana,
/// Canva, Clerk, ClickUp, HubSpot, Hugging Face, Intercom, Miro, Mixpanel,
/// monday, Neon, PagerDuty, Railway, Render, Resend, WorkOS, Zapier,
/// Context7, DeepWiki.
pub(crate) fn mcp_catalog() -> impl Iterator<Item = McpCatalogEntry> {
    MCP_CATALOG_IDS
        .iter()
        .zip(MCP_CATALOG_NAMES)
        .zip(MCP_CATALOG_URLS)
        .zip(MCP_CATALOG_MARKS)
        .map(|(((id, name), url), mark)| McpCatalogEntry { id, name, url, mark })
}

/// A catalog URL with a `{host}` placeholder: the owner fills the host in.
pub(crate) fn is_mcp_catalog_template(url: &str) -> bool {
    url.contains("{host}")
}

/// A server row as the picker reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct McpPickerServer {
    pub id: String,
    pub name: String,
    pub url: Option<String>,
    pub command: Option<String>,
    /// The muted second line; `None` = the host (or the command).
    pub description: Option<String>,
    /// Rendered, never pickable.
    pub disabled: bool,
}

/// `(authority, path)` of an absolute `scheme://authority/path` URL, the
/// path without its query/fragment and `/` when the URL carries none (web
/// `new URL(url).pathname`). `None` for anything else.
fn split_url(url: &str) -> Option<(&str, &str)> {
    let (_, rest) = url.trim().split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let path = if tail.starts_with('/') {
        let end = tail.find(['?', '#']).unwrap_or(tail.len());
        &tail[..end]
    } else {
        "/"
    };
    Some((authority, path))
}

/// The URL's host (with any port), lowercased — web `new URL(url).host`.
/// `None` for anything that is not an absolute `scheme://host` URL.
pub(crate) fn mcp_host(url: Option<&str>) -> Option<String> {
    let (authority, _) = split_url(url?)?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    (!host.is_empty()).then(|| host.to_lowercase())
}

/// The URL's path (web `new URL(url).pathname`), `None` when it is no URL.
fn mcp_path(url: &str) -> Option<&str> {
    let (authority, path) = split_url(url)?;
    (!authority.is_empty()).then_some(path)
}

/// The path a template pins (`https://{host}/api/v4/mcp` → `/api/v4/mcp`).
fn template_path(template: &str) -> Option<String> {
    mcp_path(&template.replace("{host}", "template.invalid")).map(str::to_owned)
}

/// The catalog entry a server's URL belongs to (web `mcpCatalogEntryFor`):
/// a hosted entry by exact host, else a template whose path the URL carries
/// (a self-managed GitLab at any host). A team row added from the catalog
/// keeps its mark this way, whatever it was renamed to.
pub(crate) fn mcp_catalog_entry_for(url: Option<&str>) -> Option<McpCatalogEntry> {
    let url = url?;
    let host = mcp_host(Some(url))?;
    let hosted = mcp_catalog()
        .find(|entry| !entry.is_template() && mcp_host(Some(entry.url)).as_deref() == Some(host.as_str()));
    if hosted.is_some() {
        return hosted;
    }
    let path = mcp_path(url)?;
    mcp_catalog().find(|entry| entry.is_template() && template_path(entry.url).as_deref() == Some(path))
}

/// A server's glyph (web `getMcpServerIcon`): its catalog entry's brand
/// mark (matched by URL); a terminal for a command server (no url); else
/// the plug. A mark the build does not vendor falls back to the plug too.
pub(crate) fn mcp_server_icon(url: Option<&str>, command: Option<&str>) -> ExpIcon {
    let url = url.filter(|url| !url.trim().is_empty());
    if let Some(icon) = mcp_catalog_entry_for(url).and_then(|entry| registry::brand::by_slug(entry.mark)) {
        return icon;
    }
    if url.is_none() && command.is_some_and(|command| !command.trim().is_empty()) {
        registry::SESSION_SHELL
    } else {
        registry::UI_MCP
    }
}

/// What a server's second line says when nothing more pressing does: its
/// host, else its command.
fn mcp_target(server: &McpPickerServer) -> Option<String> {
    mcp_host(server.url.as_deref())
        .or_else(|| server.command.clone().filter(|command| !command.is_empty()))
}

pub(crate) fn mcp_server_picker_items(servers: &[McpPickerServer]) -> Vec<PickerItem<String>> {
    servers
        .iter()
        .map(|server| {
            let target = mcp_target(server);
            let mut keywords = vec![SharedString::from(server.name.clone())];
            keywords.extend(target.clone().map(SharedString::from));
            let mut item = PickerItem::new(server.id.clone(), server.name.clone())
                .icon(gpui_component::Icon::from(mcp_server_icon(
                    server.url.as_deref(),
                    server.command.as_deref(),
                )))
                .disabled(server.disabled)
                .keywords(keywords);
            if let Some(description) = server.description.clone().or(target) {
                item = item.description(description);
            }
            item
        })
        .collect()
}

/// ALWAYS multi: a run connects to a SET of servers.
pub(crate) fn mcp_server_picker(
    servers: &[McpPickerServer],
    value: Vec<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::multi(mcp_server_picker_items(servers), value, trigger, on_change)
        .search(true)
        .empty_text("No servers found.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_component::IconNamed as _;

    /// The generated `ExpIcon` has no `PartialEq`; its asset path is its
    /// identity.
    fn icon(url: Option<&str>, command: Option<&str>) -> SharedString {
        mcp_server_icon(url, command).path()
    }

    fn path(icon: ExpIcon) -> SharedString {
        icon.path()
    }

    /// The contract's parallel tables line up and every mark is vendored:
    /// a catalog row can never fall back to the plug.
    #[test]
    fn every_catalog_mark_is_a_vendored_brand() {
        assert_eq!(MCP_CATALOG_IDS.len(), MCP_CATALOG_NAMES.len());
        assert_eq!(MCP_CATALOG_IDS.len(), MCP_CATALOG_URLS.len());
        assert_eq!(MCP_CATALOG_IDS.len(), MCP_CATALOG_MARKS.len());
        assert!(!MCP_CATALOG_IDS.is_empty());
        for slug in MCP_CATALOG_MARKS {
            assert!(registry::brand::by_slug(slug).is_some(), "mark {slug:?} is not vendored");
            assert!(registry::brand::SLUGS.contains(slug), "mark {slug:?} missing from SLUGS");
        }
        // Templates pin a path; hosted entries a host.
        for entry in mcp_catalog() {
            if entry.is_template() {
                assert!(template_path(entry.url).is_some(), "{}", entry.id);
            } else {
                assert!(mcp_host(Some(entry.url)).is_some(), "{}", entry.id);
            }
        }
    }

    /// Every hosted catalog URL resolves to its own entry's brand mark.
    #[test]
    fn hosted_catalog_urls_wear_their_brand_mark() {
        for entry in mcp_catalog().filter(|entry| !entry.is_template()) {
            let expected = registry::brand::by_slug(entry.mark).expect(entry.mark);
            assert_eq!(icon(Some(entry.url), None), path(expected), "{}", entry.id);
            assert_eq!(mcp_catalog_entry_for(Some(entry.url)), Some(entry));
        }
        assert_eq!(icon(Some("https://mcp.linear.app/mcp"), None), path(registry::brand::LINEAR));
        assert_eq!(icon(Some("https://gitlab.com/api/v4/mcp"), None), path(registry::brand::GITLAB));
        assert_eq!(icon(Some("https://mcp.atlassian.com/v1/mcp"), None), path(registry::brand::JIRA));
    }

    /// Matched by HOST: another path, a query, casing, whitespace — still
    /// the brand mark (a renamed catalog row keeps it too).
    #[test]
    fn a_catalog_host_matches_whatever_the_path() {
        assert_eq!(icon(Some("  HTTPS://MCP.Linear.app/sse?x=1 "), None), path(registry::brand::LINEAR));
        assert_eq!(icon(Some("https://mcp.notion.com"), Some("ignored")), path(registry::brand::NOTION));
        assert_eq!(icon(Some("https://mcp.stripe.com/"), None), path(registry::brand::STRIPE));
        // A different host (or port) is not the catalog's.
        assert_eq!(icon(Some("https://mcp.linear.app:8443/mcp"), None), path(registry::UI_MCP));
        assert_eq!(icon(Some("https://linear.app/mcp"), None), path(registry::UI_MCP));
    }

    /// A template entry matches by the PATH it pins, at any host: a
    /// self-managed GitLab; never by its placeholder host.
    #[test]
    fn a_template_matches_by_path_at_any_host() {
        let self_managed = mcp_catalog_entry_for(Some("https://git.acme.dev/api/v4/mcp")).unwrap();
        assert_eq!(self_managed.id, "gitlab-self-managed");
        assert!(self_managed.is_template());
        assert_eq!(icon(Some("https://git.acme.dev/api/v4/mcp"), None), path(registry::brand::GITLAB));
        assert_eq!(icon(Some("HTTP://localhost:8929/api/v4/mcp?x=1#f"), None), path(registry::brand::GITLAB));
        // The hosted gitlab.com entry wins over the template for its host.
        assert_eq!(mcp_catalog_entry_for(Some("https://gitlab.com/api/v4/mcp")).unwrap().id, "gitlab");
        // Another path at that host is nobody's (the match is exact).
        assert_eq!(icon(Some("https://git.acme.dev/mcp"), None), path(registry::UI_MCP));
        assert_eq!(icon(Some("https://git.acme.dev/api/v4/mcp/"), None), path(registry::UI_MCP));
    }

    #[test]
    fn a_command_server_is_a_shell_everything_else_the_plug() {
        assert_eq!(icon(None, Some("npx -y @acme/mcp")), path(registry::SESSION_SHELL));
        assert_eq!(icon(Some(""), Some("uvx acme-mcp")), path(registry::SESSION_SHELL));
        // A url wins over a command, even an unknown one.
        assert_eq!(icon(Some("https://mcp.acme.dev/mcp"), Some("npx acme")), path(registry::UI_MCP));
        assert_eq!(icon(Some("not a url"), None), path(registry::UI_MCP));
        assert_eq!(icon(None, None), path(registry::UI_MCP));
        assert_eq!(icon(None, Some("  ")), path(registry::UI_MCP));
        assert_eq!(mcp_catalog_entry_for(Some("not a url")), None);
        assert_eq!(mcp_catalog_entry_for(None), None);
    }

    #[test]
    fn mcp_host_is_the_url_host_with_its_port() {
        assert_eq!(mcp_host(Some("https://Mcp.Stripe.com")).as_deref(), Some("mcp.stripe.com"));
        assert_eq!(mcp_host(Some("http://user:pw@localhost:3000/mcp")).as_deref(), Some("localhost:3000"));
        assert_eq!(mcp_host(Some("mcp.stripe.com")), None);
        assert_eq!(mcp_host(Some("https://")), None);
        assert_eq!(mcp_host(None), None);
    }

    #[test]
    fn mcp_path_is_the_url_pathname() {
        assert_eq!(mcp_path("https://mcp.stripe.com"), Some("/"));
        assert_eq!(mcp_path("https://mcp.stripe.com?x=1"), Some("/"));
        assert_eq!(mcp_path("https://api.githubcopilot.com/mcp/"), Some("/mcp/"));
        assert_eq!(mcp_path("https://a.b/api/v4/mcp?x=1#frag"), Some("/api/v4/mcp"));
        assert_eq!(mcp_path("https://"), None);
        assert_eq!(mcp_path("no url"), None);
        assert_eq!(template_path("https://{host}/api/v4/mcp").as_deref(), Some("/api/v4/mcp"));
    }

    #[test]
    fn rows_read_host_or_command_unless_told_otherwise() {
        let servers = vec![
            McpPickerServer {
                id: "a".into(),
                name: "Linear".into(),
                url: Some("https://mcp.linear.app/mcp".into()),
                ..Default::default()
            },
            McpPickerServer {
                id: "b".into(),
                name: "Local".into(),
                command: Some("npx acme".into()),
                ..Default::default()
            },
            McpPickerServer {
                id: "c".into(),
                name: "Sentry".into(),
                url: Some("https://mcp.sentry.dev/mcp".into()),
                description: Some("Connect first".into()),
                disabled: true,
                ..Default::default()
            },
        ];
        let items = mcp_server_picker_items(&servers);
        let descriptions: Vec<Option<&str>> =
            items.iter().map(|item| item.description.as_ref().map(|d| d.as_ref())).collect();
        assert_eq!(descriptions, vec![Some("mcp.linear.app"), Some("npx acme"), Some("Connect first")]);
        assert_eq!(
            items[0].keywords.iter().map(|k| k.as_ref()).collect::<Vec<_>>(),
            vec!["Linear", "mcp.linear.app"]
        );
        assert!(!items[0].disabled && items[2].disabled);
        assert!(items.iter().all(|item| item.icon.is_some()));
    }
}
