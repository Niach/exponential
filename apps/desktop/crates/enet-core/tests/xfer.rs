//! #FEED-295: files between two real nodes over localhost UDP.

use std::io::{Read, Write};
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

/// A scratch dir, removed when the guard drops (the big test leaves ~600 MB).
struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmp(name: &str) -> Tmp {
    let dir = std::env::temp_dir().join(format!("enet-xfer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Tmp(dir)
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i as u32).wrapping_mul(2654435761).rotate_left(seed as u32) as u8 ^ seed).collect()
}

/// Writes `mib` MiB of real (non-sparse) bytes: a patterned block with each
/// MiB's index stamped in, so no two MiB are alike.
fn write_big(path: &Path, mib: u64, seed: u8) {
    let block = pattern(1 << 20, seed);
    let mut out = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(path).unwrap());
    let mut chunk = block.clone();
    for i in 0..mib {
        chunk[..8].copy_from_slice(&i.to_le_bytes());
        out.write_all(&chunk).unwrap();
    }
    out.flush().unwrap();
}

fn refuse_keys() -> PathFilter {
    Arc::new(|p: &Path| p.extension().is_some_and(|e| e == "key"))
}

/// Every byte of `b` equals `a`'s (streamed, so big files stay cheap).
fn same(a: &Path, b: &Path) {
    let (mut fa, mut fb) = (std::fs::File::open(a).unwrap(), std::fs::File::open(b).unwrap_or_else(|e| panic!("{}: {e}", b.display())));
    assert_eq!(fa.metadata().unwrap().len(), fb.metadata().unwrap().len(), "{} has another size", b.display());
    let (mut ba, mut bb) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let n = fa.read(&mut ba).unwrap();
        if n == 0 {
            break;
        }
        fb.read_exact(&mut bb[..n]).unwrap();
        assert!(ba[..n] == bb[..n], "{} differs", b.display());
    }
}

fn offer(id: &str, peers: &[&Node], root: &Path) -> XferOffer {
    XferOffer { id: id.into(), peers: peers.iter().map(|n| n.endpoint_id()).collect(), root: root.to_path_buf(), refuse: refuse_keys() }
}

const BIG_MIB: u64 = 256;
const SMALL_DIRS: usize = 10;
const SMALL_PER_DIR: usize = 30;

/// 300 small files in nested dirs, an empty dir, an empty file, one 256 MiB
/// file, a refused file and (unix) a symlink out of the tree.
fn large_tree(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    std::fs::create_dir_all(root.join("empty")).unwrap();
    std::fs::create_dir_all(root.join("media")).unwrap();
    for d in 0..SMALL_DIRS {
        let rel = format!("tree/d{d:02}/sub{}", d % 3);
        std::fs::create_dir_all(root.join(&rel)).unwrap();
        for f in 0..SMALL_PER_DIR {
            let path = format!("{rel}/f{f:03}.bin");
            let len = (d * 7919 + f * 104_729) % 65_536;
            std::fs::write(root.join(&path), pattern(len, (d * SMALL_PER_DIR + f) as u8)).unwrap();
            files.push(path);
        }
    }
    std::fs::write(root.join("zero"), b"").unwrap();
    files.push("zero".into());
    write_big(&root.join("media/big.raw"), BIG_MIB, 7);
    files.push("media/big.raw".into());
    std::fs::write(root.join("tree/secret.key"), b"never").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/hostname", root.join("tree/escape")).unwrap();
    files
}

#[tokio::test(flavor = "multi_thread")]
async fn a_large_directory_arrives_whole_and_checked() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("large");
    let root = dir.0.join("src");
    let files = large_tree(&root);
    let mut end = src.xfer_offer(offer("t1", &[&dst], &root));
    let dest = dir.0.join("out/copy");
    let progress = Arc::new(XferProgress::default());
    let started = std::time::Instant::now();
    let summary = dst.xfer_fetch(src.endpoint_id(), "t1", &dest, &dir.0.join("journal"), progress.clone()).await.expect("fetch");
    eprintln!("large tree: {} files, {} bytes in {:?}", summary.files, summary.bytes, started.elapsed());
    assert!(summary.dir);
    assert_eq!(summary.files as usize, files.len());
    assert_eq!(summary.resumed_files, 0);
    let mut skipped = summary.skipped.clone();
    skipped.sort();
    #[cfg(unix)]
    assert_eq!(skipped, vec!["tree/escape".to_string(), "tree/secret.key".to_string()]);
    #[cfg(not(unix))]
    assert_eq!(skipped, vec!["tree/secret.key".to_string()]);
    for f in &files {
        same(&root.join(f), &dest.join(f));
    }
    assert!(dest.join("empty").is_dir());
    assert_eq!(std::fs::metadata(dest.join("zero")).unwrap().len(), 0);
    assert!(!dest.join("tree/secret.key").exists() && std::fs::symlink_metadata(dest.join("tree/escape")).is_err());
    assert_eq!(progress.bytes_done.load(Ordering::Relaxed), summary.bytes);
    assert_eq!(progress.bytes_streamed.load(Ordering::Relaxed), summary.bytes);
    assert_eq!(progress.files_done.load(Ordering::Relaxed), summary.files);
    tokio::time::timeout(Duration::from_secs(10), end.wait_for(|e| e.is_some())).await.unwrap().unwrap();
    assert_eq!(*end.borrow(), Some(XferEnd::Done));

    // The same transfer again is a no-op resume (nothing streams)...
    let again_progress = Arc::new(XferProgress::default());
    let again = dst.xfer_fetch(src.endpoint_id(), "t1", &dest, &dir.0.join("journal"), again_progress.clone()).await.unwrap();
    assert_eq!(again.resumed_files, again.files);
    assert_eq!(again_progress.bytes_streamed.load(Ordering::Relaxed), 0);
    // ...and a NEW one refuses the existing destination, touching nothing.
    src.xfer_offer(offer("t2", &[&dst], &root.join("zero")));
    let err = dst.xfer_fetch(src.endpoint_id(), "t2", &dest.join("zero"), &dir.0.join("journal2"), Arc::new(XferProgress::default())).await.unwrap_err();
    assert!(matches!(err, NetError::Refused(_)), "{err:?}");
    assert!(!dir.0.join("journal2").exists(), "a refused fresh transfer leaves no journal");
    src.shutdown().await.unwrap();
    dst.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_single_file_lands_at_the_destination_path() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("file");
    std::fs::write(dir.0.join("one.bin"), pattern(1 << 20, 5)).unwrap();
    src.xfer_offer(offer("f", &[&dst], &dir.0.join("one.bin")));
    let dest = dir.0.join("nested/dir/two.bin");
    let summary = dst.xfer_fetch(src.endpoint_id(), "f", &dest, &dir.0.join("j"), Arc::new(XferProgress::default())).await.unwrap();
    assert!(!summary.dir);
    same(&dir.0.join("one.bin"), &dest);
    src.shutdown().await.unwrap();
    dst.shutdown().await.unwrap();
}

/// Bytes already at the destination: finished files plus `.exp-part` lengths.
fn bytes_on_disk(dest: &Path) -> u64 {
    std::fs::read_dir(dest).unwrap().map(|e| e.unwrap().metadata().unwrap().len()).sum()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_transfer_resumes_where_it_stopped() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("resume");
    let root = dir.0.join("src");
    std::fs::create_dir_all(&root).unwrap();
    // Two small files finish first; four big ones are cut mid-way.
    std::fs::write(root.join("a-small"), pattern(4096, 1)).unwrap();
    std::fs::write(root.join("b-small"), pattern(8192, 2)).unwrap();
    for i in 0..4u8 {
        write_big(&root.join(format!("f{i}.bin")), 24, i);
    }
    let total = 4096 + 8192 + 4 * (24u64 << 20);
    src.xfer_offer(offer("r", &[&dst], &root));
    let dest = dir.0.join("copy");
    let journal = dir.0.join("journal");
    let dst = Arc::new(dst);
    let progress = Arc::new(XferProgress::default());
    let task = {
        let (dst, progress, dest, journal, peer) = (dst.clone(), progress.clone(), dest.clone(), journal.clone(), src.endpoint_id());
        tokio::spawn(async move { dst.xfer_fetch(peer, "r", &dest, &journal, progress).await })
    };
    // Cut the link mid-way: the source goes away (its restart re-offers).
    tokio::time::timeout(Duration::from_secs(60), async {
        while progress.bytes_done.load(Ordering::Relaxed) < 30 << 20 {
            tokio::time::sleep(Duration::from_millis(2)).await;
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
    let have = bytes_on_disk(&dest);
    assert!(have > 0 && have < total, "{have} of {total}");

    let src = Node::start(NetConfig::local_only(), src_id).await.unwrap();
    pair(&src, &dst);
    src.xfer_offer(offer("r", &[&dst], &root));
    let progress = Arc::new(XferProgress::default());
    let summary = dst.xfer_fetch(src.endpoint_id(), "r", &dest, &journal, progress.clone()).await.expect("resumed fetch");
    assert_eq!(summary.files, 6);
    assert!(summary.resumed_files >= 2, "the small files were journaled: {summary:?}");
    // Only what was missing came over the wire again.
    let streamed = progress.bytes_streamed.load(Ordering::Relaxed);
    eprintln!("resume: {have} bytes were there, {streamed} streamed of {total}");
    assert_eq!(streamed, total - have);
    assert_eq!(progress.bytes_done.load(Ordering::Relaxed), total);
    for f in ["a-small", "b-small", "f0.bin", "f1.bin", "f2.bin", "f3.bin"] {
        same(&root.join(f), &dest.join(f));
    }
    assert!(std::fs::read_dir(&dest).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().ends_with(PART_SUFFIX)));
    src.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_part_that_no_longer_matches_is_caught_by_the_checksum() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("corrupt");
    let root = dir.0.join("src");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("f"), pattern(2 << 20, 4)).unwrap();
    src.xfer_offer(offer("c", &[&dst], &root));
    // An earlier attempt left 1 MiB of a part that is NOT the source's
    // prefix (corrupted on disk, or the source changed since).
    let dest = dir.0.join("copy");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join(format!("f{PART_SUFFIX}")), pattern(1 << 20, 99)).unwrap();
    std::fs::write(dir.0.join("j"), "{\"v\":1,\"id\":\"c\",\"dir\":true}\n").unwrap();
    let progress = Arc::new(XferProgress::default());
    let summary = dst.xfer_fetch(src.endpoint_id(), "c", &dest, &dir.0.join("j"), progress.clone()).await.expect("fetch");
    assert_eq!(summary.files, 1);
    same(&root.join("f"), &dest.join("f"));
    // The resumed tail (1 MiB) failed the whole-file check; the file came again whole.
    assert_eq!(progress.bytes_streamed.load(Ordering::Relaxed), (1 << 20) + (2 << 20));
    assert!(!dest.join(format!("f{PART_SUFFIX}")).exists());
    src.shutdown().await.unwrap();
    dst.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_listed_peers_read_and_only_what_was_offered() {
    let (src, dst, dst2, other) = (node().await, node().await, node().await, node().await);
    for n in [&dst, &dst2, &other] {
        pair(&src, n);
    }
    let dir = tmp("peer");
    std::fs::write(dir.0.join("x"), b"x").unwrap();
    // The other machine has two installs (the app and the daemon): either reads.
    src.xfer_offer(offer("p", &[&dst, &dst2], &dir.0.join("x")));
    dst2.xfer_fetch(src.endpoint_id(), "p", &dir.0.join("got"), &dir.0.join("j2"), Arc::new(XferProgress::default())).await.expect("a listed peer reads");
    same(&dir.0.join("x"), &dir.0.join("got"));
    // A third node, allowed onto the source but not in the offer: unknown.
    let err = other.xfer_fetch(src.endpoint_id(), "p", &dir.0.join("stolen"), &dir.0.join("j"), Arc::new(XferProgress::default())).await.unwrap_err();
    assert!(matches!(err, NetError::NotFound(_)), "{err:?}");
    assert!(enet_core::xfer_retryable(&err), "an unknown transfer is worth the next peer");
    assert!(!dir.0.join("stolen").exists());
    // A root the app refuses is refused as a whole.
    std::fs::write(dir.0.join("id.key"), b"k").unwrap();
    src.xfer_offer(offer("k", &[&dst], &dir.0.join("id.key")));
    let err = dst.xfer_fetch(src.endpoint_id(), "k", &dir.0.join("k2"), &dir.0.join("jk"), Arc::new(XferProgress::default())).await.unwrap_err();
    assert!(matches!(err, NetError::Refused(_)), "{err:?}");
    // A withdrawn offer is gone.
    assert!(src.xfer_withdraw("p"));
    let err = dst.xfer_fetch(src.endpoint_id(), "p", &dir.0.join("late"), &dir.0.join("jl"), Arc::new(XferProgress::default())).await.unwrap_err();
    assert!(matches!(err, NetError::NotFound(_)), "{err:?}");
    for n in [src, dst, dst2, other] {
        n.shutdown().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_receiver_tells_the_source() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("fail");
    std::fs::write(dir.0.join("x"), b"x").unwrap();
    let mut end = src.xfer_offer(offer("e", &[&dst], &dir.0.join("x")));
    dst.xfer_fail(src.endpoint_id(), "e", "cancelled".into()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), end.wait_for(|e| e.is_some())).await.unwrap().unwrap();
    assert_eq!(*end.borrow(), Some(XferEnd::Failed("cancelled".into())));
    src.shutdown().await.unwrap();
    dst.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_symlinked_directory_inside_the_destination_is_never_followed() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("dest-link");
    let root = dir.0.join("src");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("sub/f"), b"data").unwrap();
    src.xfer_offer(offer("s", &[&dst], &root));
    // A resumed transfer whose `sub` was swapped for a link elsewhere.
    let dest = dir.0.join("copy");
    let outside = dir.0.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(&dest).unwrap();
    std::os::unix::fs::symlink(&outside, dest.join("sub")).unwrap();
    std::fs::write(dir.0.join("j"), "{\"v\":1,\"id\":\"s\",\"dir\":true}\n").unwrap();
    let err = dst.xfer_fetch(src.endpoint_id(), "s", &dest, &dir.0.join("j"), Arc::new(XferProgress::default())).await.unwrap_err();
    assert!(matches!(err, NetError::Invalid(_)), "{err:?}");
    assert!(!outside.join("f").exists());
    src.shutdown().await.unwrap();
    dst.shutdown().await.unwrap();
}

/// A cancel aborts the fetch's task: the files in flight stop with it (the
/// e2e run on 2026-11-27 caught detached workers streaming on for 90 s).
#[tokio::test(flavor = "multi_thread")]
async fn an_aborted_fetch_stops_writing_at_once() {
    let (src, dst) = (node().await, node().await);
    pair(&src, &dst);
    let dir = tmp("abort");
    let root = dir.0.join("src");
    std::fs::create_dir_all(&root).unwrap();
    for i in 0..4u8 {
        write_big(&root.join(format!("f{i}.bin")), 64, i);
    }
    src.xfer_offer(offer("c", &[&dst], &root));
    let dest = dir.0.join("copy");
    let dst = Arc::new(dst);
    let progress = Arc::new(XferProgress::default());
    let task = {
        let (dst, progress, dest, journal, peer) = (dst.clone(), progress.clone(), dest.clone(), dir.0.join("journal"), src.endpoint_id());
        tokio::spawn(async move { dst.xfer_fetch(peer, "c", &dest, &journal, progress).await })
    };
    tokio::time::timeout(Duration::from_secs(60), async {
        while progress.bytes_done.load(Ordering::Relaxed) < 16 << 20 {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("progress");
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    // Let anything still running flush, then nothing may grow any more.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let at_abort = bytes_on_disk(&dest);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(bytes_on_disk(&dest), at_abort, "no byte lands after the abort");
    assert!(at_abort < 4 * (64u64 << 20), "{at_abort}");
    src.shutdown().await.unwrap();
}
