pub mod async_can;
pub mod socketcan;

use std::mem::size_of;
use std::os::unix::io::{AsRawFd, RawFd};

use crate::socketcan::{CANAddr, CANFilter, CANFrame};
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
        let interface_index =
            nix::net::if_::if_nametoindex(interface_name).map_err(OpenError::LookupError)?;
        let addr = CANAddr::new(interface_index);
        let sock_fd =
            unsafe { libc::socket(socketcan::PF_CAN, libc::SOCK_RAW, socketcan::CAN_RAW) };

        if sock_fd == -1 {
            return Err(OpenError::IOError(std::io::Error::last_os_error()));
        }

        let bind_result = unsafe {
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

    // TODO implement correct error handling.
    pub fn set_nonblocking(&self) {
        use nix::fcntl::{OFlag, F_GETFL, F_SETFL};

        let flags = nix::fcntl::fcntl(self.fd, F_GETFL).expect("fcntl(F_GETFD)");

        if flags < 0 {
            panic!(
                "bad return value from fcntl(F_GETFL): {} ({:?})",
                flags,
                nix::Error::last()
            );
        }

        let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;

        nix::fcntl::fcntl(self.fd, F_SETFL(flags)).expect("fcntl(F_SETFD)");
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
}

impl Drop for CANSocket {
    fn drop(&mut self) {
        self.close();
    }
}

impl AsRawFd for CANSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAN: &str = "can0";

    #[test]
    fn init() {
        let can = CANSocket::new(CAN);
        assert!(can.is_ok());
    }

    #[test]
    fn write() {
        let can = CANSocket::new(CAN).unwrap();
        can.write(&get_sample_frame()).unwrap();
    }

    #[test]
    fn read() {
        let can = CANSocket::new(CAN).unwrap();
        let _ = can.read().unwrap();
    }

    #[test]
    fn read_write() {
        let read_can = CANSocket::new(CAN).unwrap();
        let write_can = CANSocket::new(CAN).unwrap();

        write_can.write(&get_sample_frame()).unwrap();
        let frame = read_can.read().unwrap();
        assert_eq!(get_sample_frame().id(), frame.id());
    }

    #[test]
    fn filters() {
        let read_can = CANSocket::new(CAN).unwrap();
        let write_can = CANSocket::new(CAN).unwrap();

        write_can.write(&get_sample_frame()).unwrap();
        let frame = read_can.read().unwrap();

        read_can.setup_filters(Some(vec![]));
    }

    fn get_sample_frame() -> CANFrame {
        CANFrame::new(0x80, &[], false, false).unwrap()
    }
}
