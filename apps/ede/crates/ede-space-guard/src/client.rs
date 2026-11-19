//! Asking the guard: what `exponential-compositor space-enter` does
//! before it becomes a space's program.

use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

use crate::protocol::{parse_reply, Request, MAX_LINE};

/// How long one question may take (the first one starts the service).
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// Asks the guard at `socket`; `Err` says why it said no or could not
/// be asked.
pub fn ask(socket: &Path, request: &Request) -> Result<(), String> {
    let stream = UnixStream::connect(socket).map_err(|e| format!("the space guard ({}): {e}", socket.display()))?;
    stream.set_read_timeout(Some(TIMEOUT)).and_then(|()| stream.set_write_timeout(Some(TIMEOUT))).map_err(|e| e.to_string())?;
    (&stream).write_all(request.line().as_bytes()).map_err(|e| format!("the space guard: {e}"))?;
    let mut line = String::new();
    BufReader::new(Read::take(&stream, MAX_LINE as u64)).read_line(&mut line).map_err(|e| format!("the space guard: {e}"))?;
    parse_reply(&line).map_err(|reason| format!("the space guard: {reason}"))
}
