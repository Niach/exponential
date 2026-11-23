//! #FEED-295: files between two real nodes over localhost UDP.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use enet_core::{Identity, NetConfig, NetError, Node, PathFilter, XferEnd, XferOffer, XferProgress, PART_SUFFIX};

async fn node() -> Node {
    Node::start(NetConfig::local_only(), Identity::generate()).await.expect("node starts")
}

fn pair(a: &Node, b: &Node) {
    a.add_ticket(&b.ticket());
    b.add_ticket(&a.ticket());
    a.access().allow(b.endpoint_id());
    b.access().allow(a.endpoint_id());
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("enet-xfer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i as u32).wrapping_mul(2654435761).rotate_left(seed as u32) as u8 ^ seed).collect()
}

fn source_tree(root: &Path) {
    std::fs::create_dir_all(root.join("sounds/deep")).unwrap();
    std::fs::create_dir_all(root.join("empty")).unwrap();
    std::fs::write(root.join("a.txt"), b"hello").unwrap();
    std::fs::write(root.join("zero"), b"").unwrap();
    std::fs::write(root.join("sounds/big.ogg"), pattern(24 << 20, 3)).unwrap();
    std::fs::write(root.join("sounds/deep/mid.ogg"), pattern(3 << 20, 9)).unwrap();
    std::fs::write(root.join("secret.key"), b"never").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/hostname", root.join("escape")).unwrap();
}

fn refuse_keys() -> PathFilter {
    Arc::new(|p: &Path| p.extension().is_some_and(|e| e == "key"))
}

fn same(a: &Path, b: &Path) {
    assert_eq!(std::fs::read(a).unwrap(), std::fs::read(b).unwrap(), "{} differs", b.display());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_directory_arrives_whole_and_checked() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("whole");
    let root = dir.join("src");
    source_tree(&root);
    let mut end = src.xfer_offer(XferOffer { id: "t1".into(), peer: dst.endpoint_id(), root: root.clone(), refuse: refuse_keys() });
    let dest = dir.join("out/copy");
    let progress = Arc::new(XferProgress::default());
    let summary = dst.xfer_fetch(src.endpoint_id(), "t1", &dest, &dir.join("journal"), progress.clone()).await.expect("fetch");
    assert!(summary.dir);
    assert_eq!(summary.files, 4);
    assert_eq!(summary.dirs, 3);
    assert_eq!(summary.bytes, 5 + (24 << 20) + (3 << 20));
    let mut skipped = summary.skipped.clone();
    skipped.sort();
    #[cfg(unix)]
    assert_eq!(skipped, vec!["escape".to_string(), "secret.key".to_string()]);
    for f in ["a.txt", "zero", "sounds/big.ogg", "sounds/deep/mid.ogg"] {
        same(&root.join(f), &dest.join(f));
    }
    assert!(dest.join("empty").is_dir());
    assert!(!dest.join("secret.key").exists() && !dest.join("escape").exists());
    assert_eq!(progress.bytes_done.load(Ordering::Relaxed), summary.bytes);
    tokio::time::timeout(Duration::from_secs(10), end.wait_for(|e| e.is_some())).await.unwrap().unwrap();
    assert_eq!(*end.borrow(), Some(XferEnd::Done));
    // The same transfer again is a no-op resume; a NEW one refuses the existing destination.
    let again = dst.xfer_fetch(src.endpoint_id(), "t1", &dest, &dir.join("journal"), Arc::new(XferProgress::default())).await.unwrap();
    assert_eq!(again.resumed_files, 4);
    src.xfer_offer(XferOffer { id: "t2".into(), peer: dst.endpoint_id(), root: root.clone(), refuse: refuse_keys() });
    let err = dst.xfer_fetch(src.endpoint_id(), "t2", &dest, &dir.join("journal2"), Arc::new(XferProgress::default())).await.unwrap_err();
    assert!(matches!(err, NetError::Refused(_)), "{err:?}");
    src.shutdown().await.unwrap();
    dst.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_single_file_lands_at_the_destination_path() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("file");
    std::fs::write(dir.join("one.bin"), pattern(1 << 20, 5)).unwrap();
    src.xfer_offer(XferOffer { id: "f".into(), peer: dst.endpoint_id(), root: dir.join("one.bin"), refuse: refuse_keys() });
    let dest = dir.join("nested/dir/two.bin");
    let summary = dst.xfer_fetch(src.endpoint_id(), "f", &dest, &dir.join("j"), Arc::new(XferProgress::default())).await.unwrap();
    assert!(!summary.dir);
    same(&dir.join("one.bin"), &dest);
    src.shutdown().await.unwrap();
    dst.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_transfer_resumes_where_it_stopped() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("resume");
    let root = dir.join("src");
    std::fs::create_dir_all(&root).unwrap();
    for i in 0..6u8 {
        std::fs::write(root.join(format!("f{i}.bin")), pattern(16 << 20, i)).unwrap();
    }
    src.xfer_offer(XferOffer { id: "r".into(), peer: dst.endpoint_id(), root: root.clone(), refuse: refuse_keys() });
    let dest = dir.join("copy");
    let journal = dir.join("journal");
    let dst = Arc::new(dst);
    let progress = Arc::new(XferProgress::default());
    let task = {
        let (dst, progress, dest, journal, peer) = (dst.clone(), progress.clone(), dest.clone(), journal.clone(), src.endpoint_id());
        tokio::spawn(async move { dst.xfer_fetch(peer, "r", &dest, &journal, progress).await })
    };
    // Cut the link mid-way: the source goes away (its restart re-offers).
    tokio::time::timeout(Duration::from_secs(30), async {
        while progress.bytes_done.load(Ordering::Relaxed) < 40 << 20 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("progress");
    let src_id = src.identity().clone();
    src.shutdown().await.unwrap();
    let err = task.await.unwrap().unwrap_err();
    assert!(enet_core::xfer_retryable(&err), "{err:?}");
    let parts = std::fs::read_dir(&dest).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(PART_SUFFIX)).count();
    assert!(parts > 0, "a part file waits for the resume");

    let src = Node::start(NetConfig::local_only(), src_id).await.unwrap();
    pair(&src, &dst);
    src.xfer_offer(XferOffer { id: "r".into(), peer: dst.endpoint_id(), root: root.clone(), refuse: refuse_keys() });
    let progress = Arc::new(XferProgress::default());
    let summary = dst.xfer_fetch(src.endpoint_id(), "r", &dest, &journal, progress.clone()).await.expect("resumed fetch");
    assert_eq!(summary.files, 6);
    assert!(summary.resumed_files < 6);
    for i in 0..6u8 {
        same(&root.join(format!("f{i}.bin")), &dest.join(format!("f{i}.bin")));
    }
    assert!(std::fs::read_dir(&dest).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(PART_SUFFIX)));
    src.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_named_peer_reads_and_only_what_was_offered() {
    let (src, dst, other) = (node().await, node().await, node().await);
    pair(&src, &dst);
    pair(&src, &other);
    let dir = tmp("peer");
    std::fs::write(dir.join("x"), b"x").unwrap();
    src.xfer_offer(XferOffer { id: "p".into(), peer: dst.endpoint_id(), root: dir.join("x"), refuse: refuse_keys() });
    let err = other.xfer_fetch(src.endpoint_id(), "p", &dir.join("stolen"), &dir.join("j"), Arc::new(XferProgress::default())).await.unwrap_err();
    assert!(matches!(err, NetError::NotFound(_)), "{err:?}");
    assert!(!dir.join("stolen").exists());
    // A root the app refuses is refused as a whole.
    std::fs::write(dir.join("id.key"), b"k").unwrap();
    src.xfer_offer(XferOffer { id: "k".into(), peer: dst.endpoint_id(), root: dir.join("id.key"), refuse: refuse_keys() });
    let err = dst.xfer_fetch(src.endpoint_id(), "k", &dir.join("k2"), &dir.join("jk"), Arc::new(XferProgress::default())).await.unwrap_err();
    assert!(matches!(err, NetError::Refused(_)), "{err:?}");
    for n in [src, dst, other] {
        n.shutdown().await.unwrap();
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_symlinked_directory_inside_the_destination_is_never_followed() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("dest-link");
    let root = dir.join("src");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("sub/f"), b"data").unwrap();
    src.xfer_offer(XferOffer { id: "s".into(), peer: dst.endpoint_id(), root: root.clone(), refuse: refuse_keys() });
    // A resumed transfer whose `sub` was swapped for a link elsewhere.
    let dest = dir.join("copy");
    let outside = dir.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(&dest).unwrap();
    std::os::unix::fs::symlink(&outside, dest.join("sub")).unwrap();
    std::fs::write(dir.join("j"), "{\"v\":1,\"id\":\"s\",\"dir\":true}\n").unwrap();
    let err = dst.xfer_fetch(src.endpoint_id(), "s", &dest, &dir.join("j"), Arc::new(XferProgress::default())).await.unwrap_err();
    assert!(matches!(err, NetError::Invalid(_)), "{err:?}");
    assert!(!outside.join("f").exists());
    src.shutdown().await.unwrap();
    dst.shutdown().await.unwrap();
}
