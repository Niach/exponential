/* EXP-1492: exponential.at/mcp/, eMCP's one docs page. Facts:
   apps/desktop/crates/emcp (the server, the client, OAuth + the vault, the
   contract modules) and packages/emcp (the fixtures, EXP-1490); the names
   are docs/libraries.md's. Tag emcp-v0.1.0 (2026-10-31); ePilot serves its
   tools through it since #1499. */
import { DocsCode, DocsLayout, DocsSection, type DocsSection as DocsSectionType } from "@exp/site-shell"
import { StatusChip } from "../../../src/components/blog/StatusChip"
import { DocsTable, IcArrow, PageHero } from "../components/Content"
import { LIBRARIES_NAV } from "../lib/libraries"
import type { PageProps } from "../lib/routes"
import { LINKS } from "../lib/site"

const SECTIONS: DocsSectionType[] = [
  { id: `status`, num: `01`, label: `Status` },
  { id: `install`, num: `02`, label: `Install` },
  { id: `server`, num: `03`, label: `Serve tools` },
  { id: `client`, num: `04`, label: `Call a server` },
  { id: `oauth`, num: `05`, label: `OAuth and the vault` },
  { id: `contract`, num: `06`, label: `The contract` },
]

const CONTRACT: [string, string][] = [
  [`exposes`, `An app's MCP tools as declarative bindings: validation, input resolution, naming and collisions, provenance.`],
  [`access`, `Who sees and calls which app tool: the member × app policy, none, read or write, and what revoking it ends.`],
  [`audit`, `The audit row of every call: canonical JSON, the arguments hash, the status rule, 90 days of retention.`],
  [`vault`, `Encryption at rest for every secret: the ev1 format, AES-256-GCM, bound to its row and key.`],
  [`upstream`, `One member's connection to one upstream server: its states, transitions and what is callable.`],
]

export default function Mcp({ path }: PageProps) {
  return (
    <>
      <PageHero
        eyebrow="Library"
        title="eMCP"
        cta={
          <a className="btn btn-ghost" href={LINKS.source(`apps/desktop/crates/emcp`)}>
            Source <IcArrow />
          </a>
        }
      >
        MCP for devices: a loopback server for your own tools, a client for any server, OAuth with a vault for its credentials.
      </PageHero>
      <DocsLayout nav={LIBRARIES_NAV} title="Libraries" sections={SECTIONS} currentPath={path}>
        <DocsSection id="status" num="01" label="Status">
          <h2>Status</h2>
          <p>
            eMCP runs today inside the Exponential desktop app and the <code>exponential</code> CLI daemon: code mode serves
            its tools through it, ePilot serves its tools through it, and every call into the instance&apos;s own MCP
            endpoint goes through its client. It is built on rmcp, the official Rust MCP SDK. The first release is{` `}
            <code>emcp-v0.1.0</code>. On the server, the same contract runs the broker for{` `}
            <a className="content-link" href="/docs/mcp/">app MCP tools</a>.
          </p>
          <DocsTable
            head={[`Part`, `Where`, ``]}
            rows={[
              [<strong key="p">Server and client</strong>, `Code mode, ePilot and the MCP calls of the desktop app and the CLI daemon`, <StatusChip key="s" status="in-progress">in the app</StatusChip>],
              [<strong key="p">App tool broker</strong>, <span key="w">An app&apos;s <code>exposes</code> as MCP tools at <code>/api/mcp/apps</code>, with consent, access and audit</span>, <StatusChip key="s" status="in-progress">in the app</StatusChip>],
              [<strong key="p">The contract</strong>, <span key="w">The fixtures in <code>packages/emcp</code>, replayed in Rust and TypeScript</span>, <StatusChip key="s" status="in-progress">built</StatusChip>],
              [<strong key="p">The library</strong>, <span key="w"><code>emcp</code>, a git dependency</span>, <StatusChip key="s" status="shipped">0.1.0</StatusChip>],
            ]}
          />
        </DocsSection>
        <DocsSection id="install" num="02" label="Install">
          <h2>Install</h2>
          <p>
            One crate in the Exponential repo, taken as a git dependency on an <code>emcp-v&lt;version&gt;</code> tag, never
            from crates.io: the contract is compiled in from the repo&apos;s fixtures, which only a checkout carries. Pin the
            tag, never a branch; a tag is never moved.
          </p>
          <DocsCode language="toml">{`[dependencies]
emcp = { git = "https://github.com/Niach/exponential", tag = "emcp-v0.1.0" }`}</DocsCode>
          <p>
            Everything is blocking from your side: the SDK runs on eMCP&apos;s own threads, never under your executor.
          </p>
        </DocsSection>
        <DocsSection id="server" num="03" label="Serve tools">
          <h2>Serve tools</h2>
          <p>
            Implement <code>ToolProvider</code> and serve it. The server listens on <code>127.0.0.1</code>, answers plain
            JSON, and admits each session with its own bearer token: an unknown or revoked token is refused before any tool
            runs. A refusal the agent should read is content in the result, never a protocol error.
          </p>
          <DocsCode language="rust">{`use std::sync::Arc;
use emcp::{Caller, Server, ServerInfo, ToolProvider};
use serde_json::{json, Value};

struct Clock;

impl ToolProvider for Clock {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(vec![json!({ "name": "now", "description": "The time", "inputSchema": { "type": "object" } })])
    }

    fn call(&self, caller: &Caller, _name: &str, _arguments: Value) -> Value {
        json!({ "content": [{ "type": "text", "text": format!("noon, {}", caller.label) }] })
    }
}

let info = ServerInfo { name: "clock".into(), version: "0.1.0".into(), instructions: None };
let server = Server::serve(info, Arc::new(Clock))?;
let token = server.grant("session-1", "EXP-12");
// MCP config: { "clock": { "type": "http", "url": server.url(),
//               "headers": { "Authorization": "Bearer <token>" } } }
server.revoke("session-1");`}</DocsCode>
        </DocsSection>
        <DocsSection id="client" num="04" label="Call a server">
          <h2>Call a server</h2>
          <p>
            <code>Client::connect</code> does the handshake against any Streamable HTTP server with the headers you resolved.
            Header values are never logged and never appear in an error.
          </p>
          <DocsCode language="rust">{`use std::time::Duration;
use emcp::{Client, Upstream};
use serde_json::json;

let upstream = Upstream {
    name: "exponential".into(),
    url: "https://your-instance/api/mcp".into(),
    headers: vec![("Authorization".into(), format!("Bearer {key}"))],
};
let client = Client::connect(&upstream, "my-app", Duration::from_secs(10))?;
let tools = client.list_tools(Duration::from_secs(10))?;
let issue = client.call("exponential_issues", json!({ "op": "get", "id": "EXP-12" }), Duration::from_secs(30))?;`}</DocsCode>
        </DocsSection>
        <DocsSection id="oauth" num="05" label="OAuth and the vault">
          <h2>OAuth and the vault</h2>
          <p>
            For a server behind OAuth, <code>emcp::oauth</code> discovers the authorization server, registers the client,
            runs PKCE and refreshes expired tokens. The credentials live in a <code>Vault</code> you implement: the OS
            keychain on a device, a server-backed store for a member&apos;s connection. <code>SealedVault</code> encrypts
            every value with the vault format before it reaches your store; <code>MemoryVault</code> is for tests.
          </p>
          <DocsCode language="rust">{`use emcp::oauth::{self, OAuthConfig};

let authorization = oauth::begin(&config, vault.clone(), "linear")?;
open_browser(authorization.url());
authorization.finish(&code, &state)?;          // the callback's code and state

let client = oauth::connect(&config.url, vault, "linear", "my-app", timeout)?;
oauth::disconnect(&*vault, "linear")?;          // the credential is deleted`}</DocsCode>
        </DocsSection>
        <DocsSection id="contract" num="06" label="The contract">
          <h2>The contract</h2>
          <p>
            <code>emcp::contract</code> holds the rules an app tool broker follows, the same on the server and on a device.
            Each module mirrors one fixture in <code>packages/emcp</code> and replays every case of it.
          </p>
          <DocsTable head={[`Module`, `What`]} rows={CONTRACT.map(([name, what]) => [<code key="n">{name}</code>, what])} />
        </DocsSection>
      </DocsLayout>
    </>
  )
}
