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
//! is available through the `AsRawFd`, `IntoRawFd`
//! implementations.

pub mod async_can;
pub mod bcm;
pub mod canopen;
mod socketcan;
mod util;

pub use socketcan::CANFrame;

use std::mem::size_of;
use std::os::unix::prelude::*;

use crate::socketcan::{CANAddr, CANFilter};
use colored::Color;
use fern::colors::ColoredLevelConfig;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenError {
    #[error("Target CAN network couldn't be found.")]
    LookupError(nix::Error),
    #[error("Failed to access or set-up CAN network socket.")]
    IOError(std::io::Error),
}

pub struct CANSocket {
    fd: RawFd,
}

impl CANSocket {
    pub fn new(interface_name: &str) -> Result<Self, OpenError> {
        Self::setup_logging();
        let interface_index =
            nix::net::if_::if_nametoindex(interface_name).map_err(OpenError::LookupError)?;
        let sock_fd =
            unsafe { libc::socket(socketcan::PF_CAN, libc::SOCK_RAW, socketcan::CAN_RAW) };

        if sock_fd == -1 {
            return Err(OpenError::IOError(std::io::Error::last_os_error()));
        }

        let bind_result = unsafe {
            let addr = CANAddr::new(interface_index);
            let sockaddr_ptr = &addr as *const CANAddr;
            libc::bind(
                sock_fd,
                sockaddr_ptr as *const libc::sockaddr,
                std::mem::size_of::<CANAddr>() as u32,
            )
        };

        if bind_result == -1 {
            let e = std::io::Error::last_os_error();
            unsafe {
                libc::close(sock_fd);
            }
            return Err(OpenError::IOError(e));
        }

        Ok(Self { fd: sock_fd })
    }

    pub fn set_nonblocking(&self) -> std::io::Result<()> {
        util::set_nonblocking(self.fd)
    }

    pub fn read(&self) -> Result<CANFrame, std::io::Error> {
        let mut frame = CANFrame::default();
        let read_result = unsafe {
            let frame_ptr = &mut frame as *mut CANFrame;
            libc::read(
                self.fd,
                frame_ptr as *mut libc::c_void,
                size_of::<CANFrame>(),
            )
        };

        if read_result as usize != size_of::<CANFrame>() {
            return Err(std::io::Error::last_os_error());
        }

        Ok(frame)
    }

    pub fn write(&self, frame: &CANFrame) -> Result<(), std::io::Error> {
        let write_result = unsafe {
            let frame_ptr = frame as *const CANFrame;
            libc::write(
                self.fd,
                frame_ptr as *const libc::c_void,
                size_of::<CANFrame>(),
            )
        };

        if write_result as usize != size_of::<CANFrame>() {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }

    fn close(&mut self) -> std::io::Result<()> {
        let result = unsafe { libc::close(self.fd) };

        if result != -1 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn setup_filters(&self, filters: Option<Vec<CANFilter>>) -> std::io::Result<()> {
        let return_value = match filters {
            // clear filters
            None => unsafe {
                libc::setsockopt(
                    self.fd,
                    socketcan::SOL_CAN_RAW,
                    socketcan::CAN_RAW_FILTER,
                    0 as *const libc::c_void,
                    0,
                )
            },
            Some(filters) => unsafe {
                let filters_ptr = &filters[0] as *const CANFilter;
                libc::setsockopt(
                    self.fd,
                    socketcan::SOL_CAN_RAW,
                    socketcan::CAN_RAW_FILTER,
                    filters_ptr as *const libc::c_void,
                    (size_of::<CANFilter>() * filters.len()) as u32,
                )
            },
        };

        if return_value != 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }

    pub fn setup_accept_all_filter(&self) -> std::io::Result<()> {
        self.setup_filters(Some(vec![CANFilter::new(0, 0).unwrap()]))
    }

    pub fn setup_drop_all_filter(&self) -> std::io::Result<()> {
        self.setup_filters(None)
    }

    pub fn set_error_filter(&self, mask: u32) -> std::io::Result<()> {
        let result = unsafe {
            libc::setsockopt(
                self.fd,
                socketcan::SOL_CAN_RAW,
                socketcan::CAN_RAW_ERR_FILTER,
                (&mask as *const u32) as *const libc::c_void,
                size_of::<u32>() as u32,
            )
        };

        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }

    pub fn error_filter_drop_all(&self) -> std::io::Result<()> {
        self.set_error_filter(0)
    }

    pub fn error_filter_accept_all(&self) -> std::io::Result<()> {
        self.set_error_filter(socketcan::ERR_MASK)
    }

    /// Sets the read timeout on the socket
    pub fn set_read_timeout(&self, duration: std::time::Duration) -> std::io::Result<()> {
        util::set_socket_option(
            self.fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &util::c_timeval_new(duration),
        )
    }

    /// Sets the write timeout on the socket
    pub fn set_write_timeout(&self, duration: std::time::Duration) -> std::io::Result<()> {
        util::set_socket_option(
            self.fd,
            libc::SOL_SOCKET,
            libc::SO_SNDTIMEO,
            &util::c_timeval_new(duration),
        )
    }

    fn setup_logging() {
        let colors_line = ColoredLevelConfig::new()
            .error(Color::Red)
            .warn(Color::Yellow)
            .info(Color::White)
            .debug(Color::Green)
            .trace(Color::Blue);

        let _ = fern::Dispatch::new()
            .format(move |out, message, record| {
                out.finish(format_args!(
                    "{}{}[{}][{}] {}",
                    format_args!(
                        "\x1B[{}m",
                        colors_line.get_color(&record.level()).to_fg_str()
                    ),
                    chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                    record.target(),
                    record.level(),
                    message
                ))
            })
            .chain(std::io::stdout())
            .apply();
    }
}

impl Drop for CANSocket {
    fn drop(&mut self) {
        self.close().ok();
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tokio::time::Duration;

    const CAN: &str = "can0";

    #[test]
    #[serial]
    fn init() {
        let can = CANSocket::new(CAN);
        assert!(can.is_ok());
    }

    #[test]
    #[serial]
    fn init_nonexistent() {
        let can = CANSocket::new("invalid");
        assert!(can.is_err());
    }

    #[test]
    #[serial]
    fn write() {
        let can = CANSocket::new(CAN).unwrap();
        can.write(&get_sample_frame()).unwrap();
    }

    #[test]
    #[serial]
    fn read_write() {
        let read_can = CANSocket::new(CAN).unwrap();
        let write_can = CANSocket::new(CAN).unwrap();

        write_can.write(&get_sample_frame()).unwrap();
        let frame = read_can.read().unwrap();
        assert_eq!(get_sample_frame().id(), frame.id());
    }

    #[test]
    #[serial]
    fn filters() {
        let read_can = CANSocket::new(CAN).unwrap();
        read_can
            .set_read_timeout(Duration::from_millis(100))
            .unwrap();

        let write_can = CANSocket::new(CAN).unwrap();

        write_can.write(&get_sample_frame()).unwrap();
        assert!(read_can.read().is_ok());

        read_can
            .setup_filters(Some(vec![CANFilter::new(0x80, 0xff).unwrap()]))
            .unwrap();

        write_can
            .write(&CANFrame::new(0x80, &[], false, false).unwrap())
            .unwrap();

        assert!(read_can.read().is_ok());

        write_can
            .write(&CANFrame::new(0x00, &[], false, false).unwrap())
            .unwrap();

        assert!(read_can.read().is_err());
    }

    fn get_sample_frame() -> CANFrame {
        CANFrame::new(0x80, &[], false, false).unwrap()
    }
}
