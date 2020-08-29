use std::os::raw::c_short;

// constants stolen from C headers
pub const PF_CAN: i32 = 29;
pub const CAN_RAW: i32 = 1;
pub const CAN_BCM: i32 = 2;
pub const SOL_CAN_BASE: u32 = 100;
pub const SOL_CAN_RAW: u32 = SOL_CAN_BASE + CAN_RAW;
pub const CAN_RAW_FILTER: u32 = 1;
pub const CAN_RAW_ERR_FILTER: u32 = 2;

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

pub const BCM_SETTIMER: u16 = 0x0001;
pub const BCM_STARTTIMER: u16 = 0x0002;
pub const TX_SETUP: u32 = 1;