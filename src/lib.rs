use std::os::raw::{c_int, c_short};
use std::os::unix::io::{AsRawFd, RawFd};
use tokio::io::unix::AsyncFd;

struct RawCANSocket {
    fd: RawFd,
}

impl RawCANSocket {
    pub fn new(interface_name: &str) -> Self {
        let interface_index = nix::net::if_::if_nametoindex(interface_name).unwrap();
        let addr = CANAddr {
            _af_can: AF_CAN as c_short,
            if_index: if_index as c_int,
            rx_id: 0, // ?
            tx_id: 0, // ?
        };
    }
}

impl AsRawFd for RawCANSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

struct CANSocket {
    async_fd: AsyncFd<RawCANSocket>,
}

impl CANSocket {}

// models
#[derive(Debug)]
#[repr(C, align(8))]
struct CANAddr {
    _af_can: c_short,
    if_index: c_int, // address familiy,
    rx_id: u32,
    tx_id: u32,
}
