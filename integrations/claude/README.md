# Exponential for Claude

[Exponential](https://exponential.at) is an open-source (Apache-2.0) realtime issue tracker with coding agents. Teams keep issues on boards, discuss them in comments, and close them through pull requests that coding agents open from the team's own machines. This plugin brings Exponential into Claude: in Claude chat, in Cowork and in Claude Code you can read and change your team's work, open the interactive issue list, and start a coding agent on one of your devices.

## What the plugin contains

- **The Exponential connector**: a remote MCP server at `https://app.exponential.at/api/mcp` (Streamable HTTP, OAuth sign-in). Every area of the tracker is a pair of tools: a read tool such as `exponential_issues` that never changes anything, and its write twin such as `exponential_issues_write`. Each tool takes an `op` plus that operation's fields. The issue list, an issue's detail, your notifications inbox and your devices open as interactive views where Claude supports MCP Apps.
- **Four skills**:
  - `exponential`: how to work the tracker well (resolving ids, Markdown, mentions, the views, starting coding agents).
  - `exponential-genui`: writing genUI surfaces, the cards Exponential draws on every platform.
  - `exponential-apps`: building and changing Exponential apps (declarative genUI packages with their own records).
  - `exponential-motion`: Motion animations, the readable dialect that compiles to Lottie.

There are no hooks, no commands, no agents, no scripts and no executables in this plugin.

## Install

- **Claude (chat and Cowork)**: open the Claude directory, find **Exponential**, and add it.
- **Claude Code**:

  ```
  /plugin marketplace add Niach/exponential-claude-plugin
  /plugin install exponential@exponential
  ```

## Sign in

The first call opens Exponential's OAuth sign-in in your browser. Sign in to your Exponential account, then choose on the consent screen which teams, or which boards of a team, Claude may reach. Claude sees only what you grant there, and never more than your own membership allows. You can revoke the grant at any time in Exponential under Settings.

## Self-hosting

Exponential runs on your own server too. This plugin points at the hosted service, `app.exponential.at`. For a self-hosted instance, add a custom connector in Claude with your instance's own `https://<your-host>/api/mcp` URL instead; the skills work the same against it.

## What the plugin sends and fetches

- The only network traffic is the MCP connection to `https://app.exponential.at/api/mcp`: the tool calls Claude makes on your behalf (with their arguments, such as an issue's title or a comment you asked Claude to write) and their results, plus the OAuth sign-in with Exponential.
- The interactive views are served by the same server as MCP Apps resources.
- Nothing runs on your machine: no hooks, no scripts, no local commands, no background processes, and no telemetry or analytics of any kind. The skills are plain Markdown and JSON read by Claude.
- Starting a coding agent through the connector runs it on one of your own devices that already has the Exponential desktop app or CLI signed in, never on the machine where Claude runs.

## Privacy, terms and support

- Privacy policy: https://exponential.at/privacy
- Terms of service: https://exponential.at/terms
- Support: support@exponential.at, or open an issue in this repository.

## License

Apache-2.0, see [LICENSE](LICENSE). The skills `exponential-genui`, `exponential-apps` and `exponential-motion` are generated from Exponential's SDK documentation in the main repository and mirrored here.
