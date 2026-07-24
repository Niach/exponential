# gpui-markdown-editor (vendored)

Vendored WYSIWYG markdown editor core for gpui — the engine behind the desktop
issue-description editor (EXP-261). Typora-style editing: markers concealed
except around the caret, per-block heading sizes, task checkboxes, links.

**Third-party code, Apache-2.0** — see `LICENSE` and `NOTICE` (provenance:
yuWorm/gpui-markdeditor, an extraction of manyougz/velotype v0.7.0). Not under
the repository's ESTL license. Every local modification is logged in `NOTICE`.

Host integration lives in `crates/ui/src/wysiwyg/`; this crate stays
host-agnostic (events out, environment in).
