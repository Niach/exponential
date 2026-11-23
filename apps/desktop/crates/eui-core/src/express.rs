//! #AXC-12: the Express inference format. An agent may write Express (a
//! compact notation, `a2ui-express`) instead of A2UI JSON; this module
//! compiles it against the core catalog into the A2UI 1.0 messages the
//! router and the reducer already take, and writes messages back as Express
//! (`decompile`). Express compiles to the existing catalogue only: a
//! component the catalog does not define is a compile error.
//!
//! The catalog is `generated/express.catalog.json`: `catalog/core.schema.json`
//! without its prose (packages/exponential-ui/scripts/generate.ts).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use a2ui_express::compile::Compiler;
use a2ui_express::decompile::{decompile as decompile_with, DecompileOptions, Order};
use a2ui_express::response::{AfterRejectedBlock, ExpressParser, Part, ResponseStream};
use a2ui_express::schema::Catalog;
use serde_json::Value;

use crate::host::decoders::{DecodeIssue, Decoded};

/// The compiler crate itself, for a host that extends the catalog (eDE's
/// shell documents add its host functions, #EDE-216) or reads the syntax
/// tree (an issue at its line).
pub use a2ui_express;

/// The core catalog as the A2UI catalog definition the compiler loads.
pub const EXPRESS_CATALOG_JSON: &str = include_str!("generated/express.catalog.json");

/// One Express problem: the category (`parse` | `validation` | the
/// compiler's own kind) and the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressError {
    pub kind: String,
    pub message: String,
}

impl std::fmt::Display for ExpressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ExpressError {}

/// The compiler over the core catalog, built once per process.
pub fn compiler() -> &'static Compiler {
    static COMPILER: OnceLock<Compiler> = OnceLock::new();
    COMPILER.get_or_init(|| {
        let catalog = Catalog::from_json_str(EXPRESS_CATALOG_JSON).expect("the generated Express catalog loads");
        Compiler::new(vec![catalog], None).expect("the generated Express catalog is usable")
    })
}

/// One Express block (`<a2ui>` tags optional) → A2UI messages; statements
/// before any `surface("…")` line target `surface_id`.
pub fn compile(source: &str, surface_id: &str) -> Result<Vec<Value>, ExpressError> {
    compiler()
        .compile(source, surface_id)
        .map_err(|e| ExpressError { kind: format!("{:?}", e.kind), message: e.message })
}

/// A2UI messages → one Express block (without the `<a2ui>` tags), keyword
/// arguments, components in message order.
pub fn decompile(messages: &[Value]) -> Result<String, ExpressError> {
    let options = DecompileOptions { keyword_args: true, order: Order::Upstream };
    decompile_with(compiler().catalogs(), messages, &options).map_err(|e| match e {
        a2ui_express::Error::Parse(message) => ExpressError { kind: "parse".into(), message },
        a2ui_express::Error::Validation(message) => ExpressError { kind: "validation".into(), message },
    })
}

/// The core catalog WITH its prose (descriptions, the catalog rules) for the
/// prompt; behind `descriptions` like the rest of the model-facing text.
#[cfg(feature = "descriptions")]
pub const EXPRESS_CATALOG_DESCRIBED_JSON: &str = include_str!("generated/express.catalog.described.json");

/// The Express section of an agent's system prompt over the core catalog:
/// the notation, the rules and every component's signature in the compact
/// style (~10k tokens; the upstream style is ~114k on this catalog).
#[cfg(feature = "descriptions")]
pub fn express_prompt() -> Result<String, ExpressError> {
    use a2ui_express::prompt::{generate_prompt, PromptOptions, PromptStyle};
    let catalog = Catalog::from_json_str(EXPRESS_CATALOG_DESCRIBED_JSON).map_err(|e| ExpressError { kind: "catalog".into(), message: e.to_string() })?;
    let compiler = Compiler::new(vec![catalog], None).map_err(|e| ExpressError { kind: "catalog".into(), message: e.to_string() })?;
    let options = PromptOptions { style: PromptStyle::Compact, ..Default::default() };
    generate_prompt(compiler.catalogs(), &options).map_err(|e| ExpressError { kind: "prompt".into(), message: e.to_string() })
}

/// One parser per default surface id (a parser is cheap to clone, its
/// compiler is not to build).
fn parser(surface_id: &str) -> ExpressParser {
    static PARSERS: OnceLock<Mutex<HashMap<String, ExpressParser>>> = OnceLock::new();
    let mut map = PARSERS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    map.entry(surface_id.to_string()).or_insert_with(|| ExpressParser::new(compiler().clone(), surface_id)).clone()
}

/// The Express wire decoder, beside `JsonlDecoder` and `SseDecoder`: model
/// output in (any chunking), A2UI messages out as each block's statements
/// become ready, so a surface renders while the agent is still writing it.
///
/// - `wrapped`: prose with `<a2ui>…</a2ui>` blocks (a model's whole
///   response); the prose comes back in `Decoded::text` (append the pieces;
///   a run after a block starts with a `"\n\n"` piece). Unwrapped: the
///   whole stream is ONE Express body without tags.
/// - `surface_id`: the surface of statements before any `surface("…")`
///   line (the stream never mints one).
/// - A block that does not compile is an issue (`at` = the chunk it failed
///   in, 1-based), never fatal: the stream reads on past it.
pub struct ExpressDecoder {
    stream: ResponseStream,
    chunks: u32,
    /// The stream failed (an unclosed block at the end, a broken body): it
    /// takes no more input.
    failed: bool,
    /// Prose has come out (the next run after a block starts a paragraph).
    texted: bool,
    /// A block passed since the last prose.
    blocked: bool,
    /// #VAPP-300: the surfaces the OPEN block has written to, in order.
    block_surfaces: Vec<String>,
}

impl ExpressDecoder {
    pub fn new(wrapped: bool, surface_id: &str) -> Self {
        let parser = parser(surface_id);
        let state = parser.new_state();
        ExpressDecoder { stream: parser.open_stream_with(wrapped, AfterRejectedBlock::Continue, state), chunks: 0, failed: false, texted: false, blocked: false, block_surfaces: Vec::new() }
    }

    pub fn push(&mut self, chunk: &str) -> Decoded {
        if self.failed {
            return Decoded::default();
        }
        self.chunks += 1;
        let result = self.stream.push(chunk);
        let mut out = self.collect(result);
        self.blocked |= self.stream.is_in_block();
        if self.failed || !self.stream.is_in_block() {
            self.block_surfaces.clear();
        }
        out.open = self.block_surfaces.clone();
        out
    }

    pub fn end(&mut self) -> Decoded {
        if self.failed {
            return Decoded::default();
        }
        let result = self.stream.finish();
        self.failed = true;
        let out = self.collect(result);
        self.block_surfaces.clear();
        out
    }

    /// #VAPP-300: the surfaces whose block is still open (what the last
    /// read reported as `open`).
    pub fn open(&self) -> Vec<String> {
        self.block_surfaces.clone()
    }

    fn collect(&mut self, result: Result<Vec<Part>, a2ui_express::response::ResponseError>) -> Decoded {
        let mut out = Decoded::default();
        let at = self.chunks.max(1);
        let parts = match result {
            Ok(parts) => parts,
            Err(e) => {
                self.failed = true;
                let mut parts = e.partial;
                parts.push(Part::Error(e.error));
                parts
            }
        };
        for part in parts {
            match part {
                Part::Text(text) => {
                    // The stream trims each run's edges: a run after a block
                    // is a new paragraph.
                    if self.blocked && self.texted {
                        out.text.push("\n\n".to_string());
                    }
                    // Prose: the block before it closed.
                    self.block_surfaces.clear();
                    self.blocked = false;
                    self.texted = true;
                    out.text.push(text)
                }
                Part::A2ui(messages) => {
                    self.blocked = true;
                    for m in &messages {
                        let id = m.as_object().map(crate::host::router::surface_id_of).unwrap_or_default();
                        if !id.is_empty() && !self.block_surfaces.contains(&id) {
                            self.block_surfaces.push(id);
                        }
                    }
                    out.messages.extend(messages)
                }
                Part::Error(e) => {
                    // A rejected block is over.
                    self.block_surfaces.clear();
                    self.blocked = true;
                    out.issues.push(DecodeIssue { at, message: e.to_string() })
                }
            }
        }
        out
    }
}
