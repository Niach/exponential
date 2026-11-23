//! #AXC-12: the Express inference format. An agent may write Express (a
//! compact notation, `a2ui-express`) instead of A2UI JSON; this module
//! compiles it against the core catalog into the A2UI 1.0 messages the
//! router and the reducer already take, and writes messages back as Express
//! (`decompile`). Express compiles to the existing catalogue only: a
//! component the catalog does not define is a compile error.
//!
//! The catalog is `generated/express.catalog.json`: `catalog/core.schema.json`
//! without its prose (packages/exponential-ui/scripts/generate.ts).

use std::sync::OnceLock;

use a2ui_express::compile::Compiler;
use a2ui_express::decompile::{decompile as decompile_with, DecompileOptions, Order};
use a2ui_express::schema::Catalog;
use serde_json::Value;

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
