use libc::{c_int, c_short};
use std::fmt::Display;

use thiserror::Error;

// constants stolen from C headers
pub(crate) const AF_CAN: c_int = 29;
pub(crate) const PF_CAN: c_int = 29;
pub(crate) const CAN_RAW: c_int = 1;
pub(crate) const CAN_BCM: c_int = 2;
pub(crate) const SOL_CAN_BASE: c_int = 100;
pub(crate) const SOL_CAN_RAW: c_int = SOL_CAN_BASE + CAN_RAW;
pub(crate) const CAN_RAW_FILTER: c_int = 1;
pub(crate) const CAN_RAW_ERR_FILTER: c_int = 2;

/// if set, indicate 29 bit extended format
pub const EFF_FLAG: u32 = 0x80000000;

/// remote transmission request flag
pub const RTR_FLAG: u32 = 0x40000000;

/// error flag
pub const ERR_FLAG: u32 = 0x20000000;

/// valid bits in standard frame id
pub const SFF_MASK: u32 = 0x000007ff;

/// valid bits in extended frame id
pub const EFF_MASK: u32 = 0x1fffffff;

/// valid bits in error frame
pub const ERR_MASK: u32 = 0x1fffffff;

// BCM
pub(crate) const BCM_SETTIMER: u16 = 0x0001;
pub(crate) const BCM_STARTTIMER: u16 = 0x0002;
pub(crate) const TX_SETUP: u32 = 1;

#[derive(Debug)]
#[repr(C, align(8))]
pub(crate) struct CANAddr {
    af_can: c_short,
    if_index: c_int,
}

impl CANAddr {
    pub fn new(interface_index: u32) -> Self {
        Self {
            af_can: AF_CAN as c_short,
            if_index: interface_index as c_int,
        }
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
    id: u32,
    /// data length. Bytes beyond are not valid
    data_len: u8,
    /// padding
    pad: u8,
    /// reserved
    res0: u8,
    /// reserved
    res1: u8,
    /// buffer for data
    data: [u8; 8],
}

impl Default for CANFrame {
    fn default() -> Self {
        Self {
            id: 0,
            data_len: 0,
            pad: 0,
            res0: 0,
            res1: 0,
            data: [0; 8],
        }
    }
}

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("Provided slice of data was longer than 8 bytes.")]
    TooMuchData,
    #[error("Provided ID was greater than EFF_MASK.")]
    IDTooLarge,
}

impl CANFrame {
    pub fn new(mut id: u32, data: &[u8], rtr: bool, err: bool) -> Result<CANFrame, FrameError> {
        if data.len() > 8 {
            return Err(FrameError::TooMuchData);
        }
        if id > EFF_MASK {
            return Err(FrameError::IDTooLarge);
        }
        // set EFF_FLAG on large message
        if id > SFF_MASK {
            id |= EFF_FLAG;
        }
        if rtr {
            id |= RTR_FLAG;
        }
        if err {
            id |= ERR_FLAG;
        }

        let mut full_data = [0; 8];

        // not cool =/
        for (n, c) in data.iter().enumerate() {
            full_data[n] = *c;
        }

        Ok(CANFrame {
            id,
            data_len: data.len() as u8,
            pad: 0,
            res0: 0,
            res1: 0,
            data: full_data,
        })
    }

    /// Return the actual CAN ID (without EFF/RTR/ERR flags)
    #[inline(always)]
    pub fn id(&self) -> u32 {
        if self.is_extended() {
            self.id & EFF_MASK
        } else {
            self.id & SFF_MASK
        }
    }

    pub fn err(&self) -> u32 {
        return self.id & ERR_MASK;
    }

    pub fn is_extended(&self) -> bool {
        self.id & EFF_FLAG != 0
    }

    pub fn is_error(&self) -> bool {
        self.id & ERR_FLAG != 0
    }

    pub fn is_rtr(&self) -> bool {
        self.id & RTR_FLAG != 0
    }

    pub fn data(&self) -> &[u8] {
        &self.data[..(self.data_len as usize)]
    }

    pub fn raw_data(&self) -> [u8; 8] {
        self.data
    }

    pub fn len(&self) -> usize {
        self.data_len as usize
    }

    // #[inline(always)]
    // pub fn error(&self) -> Result<CANError, CANErrorDecodingFailure> {
    //     CANError::from_frame(self)
    // }
}

impl Display for CANFrame {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(
            f,
            "ID: {:#x} RTR: {} DATA: {:?}",
            self.id(),
            self.is_rtr(),
            self.data()
        )
    }
}

impl core::fmt::UpperHex for CANFrame {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> Result<(), core::fmt::Error> {
        write!(f, "{:X}#", self.id())?;

        let parts: Vec<String> = self.data().iter().map(|v| format!("{:02X}", v)).collect();

        write!(f, "{}", parts.join(" "))
    }
}

/// CANFilter
///
/// Uses the same memory layout as the underlying kernel struct for performance
/// reasons.
#[derive(Debug, Copy, Clone)]
#[repr(C, align(8))]
pub struct CANFilter {
    id: u32,
    mask: u32,
}

impl CANFilter {
    pub fn new(id: u32, mask: u32) -> Result<CANFilter, FrameError> {
        // TODO return error on wrong id
        Ok(CANFilter { id, mask })
    }
}

#[repr(C, align(8))]
pub struct BCMInterval {
    pub tv_sec: libc::c_long,
    pub tv_usec: libc::c_long,
}

#[repr(C, align(8))]
pub struct BCMMessageHeader {
    pub opcode: u32,
    pub flags: u32,
    pub count: u32,
    pub ival1: BCMInterval,
    pub ival2: BCMInterval,
    pub can_id: u32,
    pub nframes: u32,
    pub frames: CANFrame,
}
