//! VAPP-271: the Photobook sample prepares for export: a core-catalog
//! package declaring exactly photos, files.write, print and share, offline.
//! Until the pinned signer signs it, only `--allow-unsigned` takes it.

use eengine_bundle::prepare;
use serde_json::json;

const FILE: &[u8] = include_bytes!("../../../../../samples/apps/photobook.app.json");

#[test]
fn the_photobook_prepares() {
    let toml = json!({"id": "at.exponential.photobook", "name": "Photobook", "version": "1.0.0"});
    let signed = serde_json::from_slice::<serde_json::Value>(FILE).unwrap().get("signature").is_some();
    if !signed {
        assert_eq!(prepare(FILE, &toml, false).err().map(|r| r.code), Some("unsigned"));
    }
    let prepared = prepare(FILE, &toml, !signed).expect("the Photobook prepares");
    assert_eq!(prepared.scopes, ["files.write", "photos.read", "print.pdf", "share.send"]);
    assert_eq!(prepared.package["catalogId"], "https://ui.exponential.at/catalogs/core/v1");
}
