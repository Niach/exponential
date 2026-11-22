//! #EDE-172 (P13): the services daemon's intake. With `[session] helpers =
//! "compositor"` the compositor starts `exponential-services serve` with
//! one end of a `SOCK_SEQPACKET` pair at fd 3 ([`crate::CONTROL_FD_ENV`])
//! and binds no `services.sock`. For each [`crate::Request::OpenServices`]
//! it sends one message over it: a [`Handed`] (JSON) with the daemon's end
//! of a new connection riding on it (`SCM_RIGHTS`); the asker gets the
//! other end. The daemon serves that connection with exactly the
//! capabilities the message names: whoever can reach the daemon asked the
//! compositor, as a principal the compositor knows.

use std::os::fd::{AsFd, OwnedFd};

use serde::{Deserialize, Serialize};

use crate::{Capability, SessionId};

/// The largest message on the intake.
pub const MAX_MESSAGE: usize = 4096;

/// What comes with each connection the compositor hands the daemon.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handed {
    /// The session of the grant that asked (`root` for the shell's).
    pub session_id: SessionId,
    /// What the connection's `call` holds.
    pub capabilities: Vec<Capability>,
}

/// A new intake: (the compositor's end, non-blocking: a daemon that stops
/// reading never holds up its event loop; the daemon's), both
/// close-on-exec (the spawn hands the daemon's on at fd 3).
pub fn pair() -> std::io::Result<(OwnedFd, OwnedFd)> {
    use rustix::net::{socketpair, AddressFamily, SocketFlags, SocketType};
    let (compositor, daemon) = socketpair(AddressFamily::UNIX, SocketType::SEQPACKET, SocketFlags::CLOEXEC, None)?;
    rustix::io::ioctl_fionbio(&compositor, true)?;
    Ok((compositor, daemon))
}

/// Hands `connection` over `intake` with `handed`: one message, whole or
/// not at all (`WouldBlock` on a non-blocking intake whose daemon does not
/// read).
pub fn send(intake: impl AsFd, handed: &Handed, connection: impl AsFd) -> std::io::Result<()> {
    use rustix::net::{SendAncillaryBuffer, SendAncillaryMessage, SendFlags};
    let bytes = serde_json::to_vec(handed).map_err(std::io::Error::other)?;
    let fds = [connection.as_fd()];
    let mut space = [std::mem::MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    if !control.push(SendAncillaryMessage::ScmRights(&fds)) {
        return Err(std::io::Error::other("no room for the descriptor"));
    }
    let sent = rustix::net::sendmsg(intake, &[std::io::IoSlice::new(&bytes)], &mut control, SendFlags::NOSIGNAL)?;
    if sent != bytes.len() {
        return Err(std::io::Error::other("the intake took part of a message"));
    }
    Ok(())
}

/// The next connection on `intake` (blocks): `None` = the compositor closed
/// it (it ended, or started another daemon). A message without a
/// descriptor, or one that does not parse, is an error.
pub fn receive(intake: impl AsFd) -> std::io::Result<Option<(Handed, OwnedFd)>> {
    use rustix::net::{RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags};
    let mut buf = vec![0u8; MAX_MESSAGE];
    let mut space = [std::mem::MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(4))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let received = rustix::net::recvmsg(&intake, &mut [std::io::IoSliceMut::new(&mut buf)], &mut control, RecvFlags::CMSG_CLOEXEC)?;
    let mut fds: Vec<OwnedFd> = Vec::new();
    for message in control.drain() {
        if let RecvAncillaryMessage::ScmRights(received) = message {
            fds.extend(received);
        }
    }
    if received.bytes == 0 && fds.is_empty() {
        return Ok(None);
    }
    let handed: Handed = serde_json::from_slice(&buf[..received.bytes]).map_err(std::io::Error::other)?;
    let mut fds = fds.into_iter();
    let connection = fds.next().ok_or_else(|| std::io::Error::other("a message on the intake without its connection"))?;
    Ok(Some((handed, connection)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        os::unix::net::UnixStream,
    };

    #[test]
    fn a_connection_rides_on_its_message_and_the_close_ends_the_intake() {
        let (compositor, daemon) = pair().unwrap();
        let (asker, theirs) = UnixStream::pair().unwrap();
        let handed = Handed { session_id: SessionId::new("root").unwrap(), capabilities: vec![Capability::Ui, Capability::System] };
        send(&compositor, &handed, &theirs).unwrap();
        drop(theirs);
        let (got, connection) = receive(&daemon).unwrap().expect("a connection");
        assert_eq!(got, handed);
        let mut connection = UnixStream::from(connection);
        connection.write_all(b"served").unwrap();
        let mut asker = asker;
        let mut said = [0u8; 6];
        asker.read_exact(&mut said).unwrap();
        assert_eq!(&said, b"served", "the asker's end and the daemon's are one connection");
        drop(compositor);
        assert!(receive(&daemon).unwrap().is_none(), "the compositor's close ends the intake");
    }
}
