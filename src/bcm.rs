use crate::socketcan::{BCMInterval, BCMMessageHeader, CANAddr, CANFrame};
use crate::{socketcan, OpenError};
use std::mem::size_of;
use std::os::unix::prelude::*;

pub struct BCMSocket {
    fd: RawFd,
}

impl BCMSocket {
    pub fn new(interface_name: &str) -> Result<Self, OpenError> {
        let interface_index =
            nix::net::if_::if_nametoindex(interface_name).map_err(OpenError::LookupError)?;
        let sock_fd =
            unsafe { libc::socket(socketcan::PF_CAN, libc::SOCK_DGRAM, socketcan::CAN_BCM) };

        if sock_fd == -1 {
            return Err(OpenError::IOError(std::io::Error::last_os_error()));
        }

        let connect_result = unsafe {
            let addr = CANAddr::new(interface_index);
            let sockaddr_ptr = &addr as *const CANAddr;
            libc::connect(
                sock_fd,
                sockaddr_ptr as *const libc::sockaddr,
                std::mem::size_of::<CANAddr>() as u32,
            )
        };

        if connect_result == -1 {
            let e = std::io::Error::last_os_error();
            unsafe {
                libc::close(sock_fd);
            }
            return Err(OpenError::IOError(e));
        }

        Ok(Self { fd: sock_fd })
    }

    pub fn send_periodically(&self, microseconds: u64, frame: CANFrame) -> std::io::Result<()> {
        let bcm_message = BCMMessageHeader {
            opcode: socketcan::TX_SETUP,
            flags: (socketcan::BCM_SETTIMER | socketcan::BCM_STARTTIMER) as u32,
            count: 0,
            ival1: BCMInterval {
                tv_sec: 0,
                tv_usec: 0,
            },
            ival2: BCMInterval {
                tv_sec: 0,
                tv_usec: microseconds as libc::c_long,
            },
            can_id: frame.id(),
            nframes: 1,
            frames: frame,
        };

        let write_result = unsafe {
            let message_ptr = &bcm_message as *const BCMMessageHeader;
            libc::write(
                self.fd,
                message_ptr as *const libc::c_void,
                size_of::<BCMMessageHeader>() as usize,
            )
        };

        if write_result == -1 {
            return Err(std::io::Error::last_os_error());
        }

        return Ok(());
    }
}
