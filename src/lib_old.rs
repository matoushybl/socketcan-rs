//! socketCAN support.
//!
//! The Linux kernel supports using CAN-devices through a network-like API
//! (see https://www.kernel.org/doc/Documentation/networking/can.txt). This
//! crate allows easy access to this functionality without having to wrestle
//! libc calls.
//!
//! # An introduction to CAN
//!
//! The CAN bus was originally designed to allow microcontrollers inside a
//! vehicle to communicate over a single shared bus. Messages called
//! *frames* are multicast to all devices on the bus.
//!
//! Every frame consists of an ID and a payload of up to 8 bytes. If two
//! devices attempt to send a frame at the same time, the device with the
//! higher ID will notice the conflict, stop sending and reattempt to sent its
//! frame in the next time slot. This means that the lower the ID, the higher
//! the priority. Since most devices have a limited buffer for outgoing frames,
//! a single device with a high priority (== low ID) can block communication
//! on that bus by sending messages too fast.
//!
//! The Linux socketcan subsystem makes the CAN bus available as a regular
//! networking device. Opening an network interface allows receiving all CAN
//! messages received on it. A device CAN be opened multiple times, every
//! client will receive all CAN frames simultaneously.
//!
//! Similarly, CAN frames can be sent to the bus by multiple client
//! simultaneously as well.
//!
//! # Hardware and more information
//!
//! More information on CAN [can be found on Wikipedia](). When not running on
//! an embedded platform with already integrated CAN components,
//! [Thomas Fischl's USBtin](http://www.fischl.de/usbtin/) (see
//! [section 2.4](http://www.fischl.de/usbtin/#socketcan)) is one of many ways
//! to get started.
//!
//! # RawFd
//!
//! Raw access to the underlying file descriptor and construction through
//! is available through the `AsRawFd`, `IntoRawFd` and `FromRawFd`
//! implementations.

mod err;
pub use crate::err::{CANError, CANErrorDecodingFailure};
pub mod canopen;
pub mod dump;

#[cfg(test)]
mod tests;

use itertools::Itertools;
use libc::{
    bind, c_int, c_long, c_short, c_uint, c_void, close, connect, fcntl, read, setsockopt,
    sockaddr, socket, suseconds_t, time_t, timeval, write, EINPROGRESS, F_GETFL, F_SETFL,
    O_NONBLOCK, SOCK_DGRAM, SOCK_RAW, SOL_SOCKET, SO_RCVTIMEO, SO_SNDTIMEO,
};
use nix::net::if_::if_nametoindex;
use std::convert::TryFrom;
use std::mem::size_of;
use std::os::unix::io::{AsRawFd, IntoRawFd, RawFd};
use std::{error, fmt, io, time};
use tokio::prelude::*;
use tokio_fd::AsyncFd;

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

impl ShouldRetry for io::Error {
    fn should_retry(&self) -> bool {
        match self.kind() {
            // EAGAIN, EINPROGRESS and EWOULDBLOCK are the three possible codes
            // returned when a timeout occurs. the stdlib already maps EAGAIN
            // and EWOULDBLOCK os WouldBlock
            io::ErrorKind::WouldBlock => true,
            // however, EINPROGRESS is also valid
            io::ErrorKind::Other => {
                if let Some(i) = self.raw_os_error() {
                    i == EINPROGRESS
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl<E: fmt::Debug> ShouldRetry for io::Result<E> {
    fn should_retry(&self) -> bool {
        if let &Err(ref e) = self {
            e.should_retry()
        } else {
            false
        }
    }
}

fn c_timeval_new(t: time::Duration) -> timeval {
    timeval {
        tv_sec: t.as_secs() as time_t,
        tv_usec: (t.subsec_nanos() / 1000) as suseconds_t,
    }
}

#[repr(C)]
struct BCMInterval {
    tv_sec: c_long,
    tv_usec: c_long,
}

const BCM_SETTIMER: u16 = 0x0001;
const BCM_STARTTIMER: u16 = 0x0002;
const TX_SETUP: u32 = 1;

#[repr(C, align(8))]
struct BCMMessageHeader {
    opcode: u32,
    flags: u32,
    count: u32,
    ival1: BCMInterval,
    ival2: BCMInterval,
    can_id: u32,
    nframes: u32,
    frames: CANFrame,
}

/// A socket for a CAN device.
///
/// Will be closed upon deallocation. To close manually, use std::drop::Drop.
/// Internally this is just a wrapped file-descriptor.
#[derive(Debug)]
pub struct CANSocket {
    fd: AsyncFd,
    bcm_fd: AsyncFd,
}

impl CANSocket {
    /// Open a named CAN device.
    ///
    /// Usually the more common case, opens a socket can device by name, such
    /// as "vcan0" or "socan0".

    /// Open CAN device by interface number.
    ///
    /// Opens a CAN device by kernel interface number.
    pub fn open_if(if_index: c_uint) -> Result<CANSocket, CANSocketOpenError> {
        let addr = CANAddr {
            _af_can: AF_CAN as c_short,
            if_index: if_index as c_int,
            rx_id: 0, // ?
            tx_id: 0, // ?
        };

        // open socket
        let sock_fd;
        unsafe {
            sock_fd = socket(PF_CAN, SOCK_RAW, CAN_RAW);
        }
        let bcm_fd;
        unsafe {
            bcm_fd = socket(PF_CAN, SOCK_DGRAM, CAN_BCM);
        }

        if sock_fd == -1 || bcm_fd == -1 {
            return Err(CANSocketOpenError::from(io::Error::last_os_error()));
        }

        // bind it
        let bind_rv;
        unsafe {
            let sockaddr_ptr = &addr as *const CANAddr;
            bind_rv = bind(
                sock_fd,
                sockaddr_ptr as *const sockaddr,
                size_of::<CANAddr>() as u32,
            );
        }

        if bind_rv == -1 {
            let e = io::Error::last_os_error();
            unsafe {
                close(sock_fd);
            }
            return Err(CANSocketOpenError::from(e));
        }

        // connect BCM
        let bcm_addr = CANAddr {
            _af_can: PF_CAN as c_short,
            if_index: if_index as c_int,
            rx_id: 0, // ?
            tx_id: 0, // ?
        };

        let bcm_connect_result;
        unsafe {
            let addr_ptr = &bcm_addr as *const CANAddr;
            bcm_connect_result = connect(
                bcm_fd,
                addr_ptr as *const sockaddr,
                size_of::<CANAddr>() as u32,
            );
        }

        if bcm_connect_result == -1 {
            let e = io::Error::last_os_error();
            unsafe {
                close(bcm_fd);
            }
            return Err(CANSocketOpenError::from(e));
        }

        Ok(CANSocket {
            fd: AsyncFd::try_from(sock_fd)?,
            bcm_fd: AsyncFd::try_from(bcm_fd)?,
        })
    }

    pub fn bcm_send_periodically(&self, microseconds: u64, frame: CANFrame) -> io::Result<()> {
        let bcm_message = BCMMessageHeader {
            opcode: TX_SETUP,
            flags: (BCM_SETTIMER | BCM_STARTTIMER) as u32,
            count: 0,
            ival1: BCMInterval {
                tv_sec: 0,
                tv_usec: 0,
            },
            ival2: BCMInterval {
                tv_sec: 0,
                tv_usec: microseconds as c_long,
            },
            can_id: frame._id,
            nframes: 1,
            frames: frame,
        };

        let write_result;
        unsafe {
            let message_ptr = &bcm_message as *const BCMMessageHeader;
            write_result = write(
                self.bcm_fd,
                message_ptr as *const c_void,
                size_of::<BCMMessageHeader>() as usize,
            );
        }

        if write_result == -1 {
            return Err(io::Error::last_os_error());
        }

        return Ok(());
    }

    /// Disable reception of CAN frames.
    ///
    /// Sets a completely empty filter; disabling all CAN frame reception.
    #[inline(always)]
    pub fn filter_drop_all(&self) -> io::Result<()> {
        self.set_filter(&[])
    }

    /// Accept all frames, disabling any kind of filtering.
    ///
    /// Replace the current filter with one containing a single rule that
    /// acceps all CAN frames.
    pub fn filter_accept_all(&self) -> io::Result<()> {
        // safe unwrap: 0, 0 is a valid mask/id pair
        self.set_filter(&[CANFilter::new(0, 0).unwrap()])
    }

    #[inline(always)]
    pub fn set_error_filter(&self, mask: u32) -> io::Result<()> {
        let rv = unsafe {
            setsockopt(
                self.fd,
                SOL_CAN_RAW,
                CAN_RAW_ERR_FILTER,
                (&mask as *const u32) as *const c_void,
                size_of::<u32>() as u32,
            )
        };

        if rv != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }

    #[inline(always)]
    pub fn error_filter_drop_all(&self) -> io::Result<()> {
        self.set_error_filter(0)
    }

    #[inline(always)]
    pub fn error_filter_accept_all(&self) -> io::Result<()> {
        self.set_error_filter(ERR_MASK)
    }
}

impl AsRawFd for CANSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl IntoRawFd for CANSocket {
    fn into_raw_fd(self) -> RawFd {
        self.fd
    }
}

impl Drop for CANSocket {
    fn drop(&mut self) {
        self.close().ok(); // ignore result
    }
}

/// CANFrame
///
/// Uses the same memory layout as the underlying kernel struct for performance
/// reasons.
#[derive(Debug, Copy, Clone)]
#[repr(C, align(8))]
pub struct CANFrame {
    /// 32 bit CAN_ID + EFF/RTR/ERR flags
    _id: u32,

    /// data length. Bytes beyond are not valid
    _data_len: u8,

    /// padding
    _pad: u8,

    /// reserved
    _res0: u8,

    /// reserved
    _res1: u8,

    /// buffer for data
    _data: [u8; 8],
}

impl CANFrame {
    pub fn new(id: u32, data: &[u8], rtr: bool, err: bool) -> Result<CANFrame, ConstructionError> {
        let mut _id = id;

        if data.len() > 8 {
            return Err(ConstructionError::TooMuchData);
        }

        if id > EFF_MASK {
            return Err(ConstructionError::IDTooLarge);
        }

        // set EFF_FLAG on large message
        if id > SFF_MASK {
            _id |= EFF_FLAG;
        }

        if rtr {
            _id |= RTR_FLAG;
        }

        if err {
            _id |= ERR_FLAG;
        }

        let mut full_data = [0; 8];

        // not cool =/
        for (n, c) in data.iter().enumerate() {
            full_data[n] = *c;
        }

        Ok(CANFrame {
            _id: _id,
            _data_len: data.len() as u8,
            _pad: 0,
            _res0: 0,
            _res1: 0,
            _data: full_data,
        })
    }

    /// Return the actual CAN ID (without EFF/RTR/ERR flags)
    #[inline(always)]
    pub fn id(&self) -> u32 {
        if self.is_extended() {
            self._id & EFF_MASK
        } else {
            self._id & SFF_MASK
        }
    }

    /// Return the error message
    #[inline(always)]
    pub fn err(&self) -> u32 {
        return self._id & ERR_MASK;
    }

    /// Check if frame uses 29 bit extended frame format
    #[inline(always)]
    pub fn is_extended(&self) -> bool {
        self._id & EFF_FLAG != 0
    }

    /// Check if frame is an error message
    #[inline(always)]
    pub fn is_error(&self) -> bool {
        self._id & ERR_FLAG != 0
    }

    /// Check if frame is a remote transmission request
    #[inline(always)]
    pub fn is_rtr(&self) -> bool {
        self._id & RTR_FLAG != 0
    }

    /// A slice into the actual data. Slice will always be <= 8 bytes in length
    #[inline(always)]
    pub fn data(&self) -> &[u8] {
        &self._data[..(self._data_len as usize)]
    }

    #[inline(always)]
    pub fn error(&self) -> Result<CANError, CANErrorDecodingFailure> {
        CANError::from_frame(self)
    }
}

impl fmt::Display for CANFrame {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "ID: {:#x} RTR: {} DATA: {:?}",
            self.id(),
            self.is_rtr(),
            self.data()
        )
    }
}

impl fmt::UpperHex for CANFrame {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{:X}#", self.id())?;

        let mut parts = self.data().iter().map(|v| format!("{:02X}", v));

        let sep = if f.alternate() { " " } else { " " };
        write!(f, "{}", parts.join(sep))
    }
}

/// CANFilter
///
/// Uses the same memory layout as the underlying kernel struct for performance
/// reasons.
#[derive(Debug, Copy, Clone)]
#[repr(C, align(8))]
pub struct CANFilter {
    _id: u32,
    _mask: u32,
}

impl CANFilter {
    pub fn new(id: u32, mask: u32) -> Result<CANFilter, ConstructionError> {
        Ok(CANFilter {
            _id: id,
            _mask: mask,
        })
    }
}
