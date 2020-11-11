use std::mem::size_of;
use std::os::unix::prelude::*;

pub fn c_timeval_new(t: std::time::Duration) -> libc::timeval {
    libc::timeval {
        tv_sec: t.as_secs() as libc::time_t,
        tv_usec: (t.subsec_nanos() / 1000) as libc::suseconds_t,
    }
}

/// `setsockopt` wrapper
///
/// The libc `setsockopt` function is set to set various options on a socket.
/// `set_socket_option` offers a somewhat type-safe wrapper that does not
/// require messing around with `*const c_void`s.
///
/// A proper `std::io::Error` will be returned on failure.
///
/// Example use:
///
/// ```text
/// let fd = ...;  // some file descriptor, this will be stdout
/// set_socket_option(fd, SOL_TCP, TCP_NO_DELAY, 1 as c_int)
/// ```
///
/// Note that the `val` parameter must be specified correctly; if an option
/// expects an integer, it is advisable to pass in a `c_int`, not the default
/// of `i32`.

pub(crate) fn set_socket_option<T>(
    fd: libc::c_int,
    level: libc::c_int,
    name: libc::c_int,
    val: &T,
) -> std::io::Result<()> {
    let result = unsafe {
        let val_ptr: *const T = val as *const T;

        libc::setsockopt(
            fd,
            level,
            name,
            val_ptr as *const libc::c_void,
            size_of::<T>() as libc::socklen_t,
        )
    };

    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}

pub fn set_nonblocking(fd: RawFd) -> std::io::Result<()> {
    let old_flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };

    if old_flags == -1 {
        return Err(std::io::Error::last_os_error());
    }

    let new_flags = old_flags | libc::O_NONBLOCK;

    let result = unsafe { libc::fcntl(fd, libc::F_SETFL, new_flags) };

    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
