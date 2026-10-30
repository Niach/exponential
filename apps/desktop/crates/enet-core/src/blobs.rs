//! Files: content-addressed blobs over iroh-blobs (BLAKE3 verified
//! streaming, ALPN `iroh_blobs::ALPN`). In-memory store by default; the
//! `fs-store` feature persists to `NetConfig::blob_dir` on native hosts.

use iroh::EndpointId;
use iroh_blobs::{api::Store, ticket::BlobTicket, BlobFormat, Hash};
use iroh_tickets::Ticket as _;

use crate::{
    error::{transport, NetError, Result},
    node::Shared,
};

/// A blob added to (or fetched into) this node's store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRef {
    /// BLAKE3 hash, 32 bytes.
    pub hash: [u8; 32],
    pub size: u64,
    /// An iroh `blob…` ticket (this node's address + the hash): anyone the
    /// node's access policy lets in can fetch it.
    pub ticket: String,
}

impl BlobRef {
    pub fn hash_hex(&self) -> String {
        Hash::from_bytes(self.hash).to_hex()
    }
}

pub(crate) async fn open_store(config: &crate::NetConfig) -> Result<Store> {
    #[cfg(all(feature = "fs-store", not(wasm_browser)))]
    if let Some(dir) = &config.blob_dir {
        let fs = iroh_blobs::store::fs::FsStore::load(dir).await.map_err(|e| NetError::Bind(e.to_string()))?;
        return Ok((*fs).clone());
    }
    let _ = config;
    let mem = iroh_blobs::store::mem::MemStore::new();
    Ok((*mem).clone())
}

pub(crate) fn blob_ref(shared: &Shared, hash: Hash, size: u64) -> BlobRef {
    let ticket = BlobTicket::new(shared.ticket().to_addr(), hash, BlobFormat::Raw);
    BlobRef { hash: *hash.as_bytes(), size, ticket: ticket.encode_string() }
}

pub(crate) async fn add_bytes(shared: &Shared, data: Vec<u8>) -> Result<BlobRef> {
    let size = data.len() as u64;
    let tag = shared.store.add_bytes(data).await.map_err(transport)?;
    Ok(blob_ref(shared, tag.hash, size))
}

#[cfg(not(wasm_browser))]
pub(crate) async fn add_path(shared: &Shared, path: &std::path::Path) -> Result<BlobRef> {
    let path = std::path::absolute(path).map_err(|e| NetError::Invalid(e.to_string()))?;
    let size = std::fs::metadata(&path).map_err(|e| NetError::Invalid(e.to_string()))?.len();
    let tag = shared.store.add_path(path).await.map_err(transport)?;
    Ok(blob_ref(shared, tag.hash, size))
}

pub(crate) async fn get(shared: &Shared, hash: [u8; 32]) -> Result<Option<Vec<u8>>> {
    let hash = Hash::from_bytes(hash);
    if !shared.store.has(hash).await.map_err(transport)? {
        return Ok(None);
    }
    let bytes = shared.store.get_bytes(hash).await.map_err(transport)?;
    Ok(Some(bytes.to_vec()))
}

pub(crate) async fn fetch(shared: &Shared, peer: EndpointId, hash: [u8; 32]) -> Result<Vec<u8>> {
    let h = Hash::from_bytes(hash);
    if let Some(local) = get(shared, hash).await? {
        return Ok(local);
    }
    let conn = shared.connect(peer, iroh_blobs::ALPN).await?;
    if let Err(e) = shared.store.remote().fetch(conn.clone(), h).complete().await {
        return Err(crate::node::classify(&conn, transport(e)));
    }
    get(shared, hash).await?.ok_or_else(|| NetError::NotFound(format!("blob {}", h.to_hex())))
}

pub(crate) async fn fetch_ticket(shared: &Shared, ticket: &str) -> Result<Vec<u8>> {
    let t = BlobTicket::decode_string(ticket.trim()).map_err(|e| NetError::Ticket(e.to_string()))?;
    let addr = t.addr().clone();
    let peer = addr.id;
    shared.add_peer(addr);
    fetch(shared, peer, *t.hash().as_bytes()).await
}
