use std::io::{Error, ErrorKind, Result};
use std::fmt::Debug;

/// Check an error return value for timeouts.
///
/// Due to the fact that timeouts are reported as errors, calling `read_frame`
/// on a socket with a timeout that does not receive a frame in time will
/// result in an error being returned. This trait adds a `should_retry` method
/// to `Error` and `Result` to check for this condition.
pub trait ShouldRetry {
    /// Check for timeout
    ///
    /// If `true`, the error is probably due to a timeout.
    fn should_retry(&self) -> bool;
}

impl ShouldRetry for Error {
    fn should_retry(&self) -> bool {
        match self.kind() {
            // EAGAIN, EINPROGRESS and EWOULDBLOCK are the three possible codes
            // returned when a timeout occurs. the stdlib already maps EAGAIN
            // and EWOULDBLOCK os WouldBlock
            ErrorKind::WouldBlock => true,
            // however, EINPROGRESS is also valid
            ErrorKind::Other => {
                if let Some(i) = self.raw_os_error() {
                    i == nix::errno::Errno::EINPROGRESS.into()
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl<E: Debug> ShouldRetry for Result<E> {
    fn should_retry(&self) -> bool {
        if let &Err(ref e) = self {
            e.should_retry()
        } else {
            false
        }
    }
}