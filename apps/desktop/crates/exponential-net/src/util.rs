//! Small shared helpers: randomness, wall clock, length-prefixed frames.

use iroh::endpoint::{RecvStream, SendStream};

use crate::error::{transport, NetError, Result};

pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("system randomness");
    b
}

/// Milliseconds since the Unix epoch (the browser's clock on wasm).
pub(crate) fn now_ms() -> u64 {
    use n0_future::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Writes `[u32 BE len][body]`.
pub(crate) async fn write_frame(send: &mut SendStream, body: &[u8]) -> Result<()> {
    let len = u32::try_from(body.len()).map_err(|_| NetError::Invalid("frame too large".into()))?;
    send.write_all(&len.to_be_bytes()).await.map_err(transport)?;
    send.write_all(body).await.map_err(transport)?;
    Ok(())
}

/// Reads one `[u32 BE len][body]` frame; `Ok(None)` = the stream ended cleanly
/// before a new frame.
pub(crate) async fn read_frame(recv: &mut RecvStream, max: usize) -> Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match recv.read_exact(&mut len).await {
        Ok(()) => {}
        Err(iroh::endpoint::ReadExactError::FinishedEarly(0)) => return Ok(None),
        Err(e) => return Err(transport(e)),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > max {
        return Err(NetError::Invalid(format!("frame of {len} bytes over the {max} limit")));
    }
    let mut body = vec![0u8; len];
    recv.read_exact(&mut body).await.map_err(transport)?;
    Ok(Some(body))
}

/// A tiny cursor for the hand-rolled wire formats.
pub(crate) struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.buf.len() < n {
            return Err(NetError::Invalid("truncated".into()));
        }
        let (head, rest) = self.buf.split_at(n);
        self.buf = rest;
        Ok(head)
    }
    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("length checked"))
    }
    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }
    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }
    pub(crate) fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.array()?))
    }
    pub(crate) fn rest(&self) -> &'a [u8] {
        self.buf
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}
