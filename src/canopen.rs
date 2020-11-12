use crate::bcm::BCMSocket;
use crate::socketcan::CANFrame;
use crate::{CANSocket, OpenError};
use std::convert::TryFrom;

pub enum CANOpenNodeCommand {
    SendPDO(u8, PDO, [u8; 8], usize),
    SendNMT(u8, NMTCommand),
    SendSDO(u8, SDOControlByte, u16, u8, [u8; 4], usize),
}

#[derive(Debug)]
pub enum CANOpenNodeMessage {
    SyncReceived,
    PDOReceived(PDO, [u8; 8], usize),
    NMTReceived(NMTState),
    SDOReceived(SDOControlByte, u16, u8, [u8; 4], u8),
}

#[derive(Debug, Error)]
pub enum ReadError {
    #[error("Failed to read frame from the bus.")]
    IO(std::io::Error),
    #[error("Failed to parse message from the bus.")]
    Parse(MessageParseError),
}

pub struct CANOpenSocket {
    socket: CANSocket,
    _bcm: BCMSocket,
}

impl CANOpenSocket {
    pub fn new(bus_name: &str, sync_period: Option<u64>) -> Result<CANOpenSocket, OpenError> {
        let socket = CANSocket::new(bus_name)?;
        let bcm = BCMSocket::new(bus_name)?;
        if let Some(sync_period) = sync_period {
            let frame = CANFrame::new(0x80, &[], false, false).unwrap();
            bcm.send_periodically(sync_period, frame)
                .map_err(OpenError::IOError)?;
        }

        Ok(CANOpenSocket { socket, _bcm: bcm })
    }

    pub fn read(&self) -> Result<CANOpenNodeMessage, ReadError> {
        let frame = self.socket.read().map_err(ReadError::IO)?;
        CANOpenNodeMessage::try_from(frame).map_err(ReadError::Parse)
    }

    pub fn write(&self, command: CANOpenNodeCommand) -> Result<(), std::io::Error> {
        let frame = CANFrame::from(command);
        self.socket.write(&frame)
    }
}

pub mod async_canopen {
    use crate::async_can::CANSocket;
    use crate::bcm::BCMSocket;
    use crate::socketcan::CANFrame;

    use super::{CANOpenNodeCommand, CANOpenNodeMessage, ReadError};
    use crate::OpenError;
    use std::convert::TryFrom;

    pub struct CANOpenSocket {
        socket: CANSocket,
        _bcm: BCMSocket,
    }

    impl CANOpenSocket {
        pub fn new(bus_name: &str, sync_period: Option<u64>) -> Result<CANOpenSocket, OpenError> {
            let socket = CANSocket::new(bus_name)?;
            let bcm = BCMSocket::new(bus_name)?;
            if let Some(sync_period) = sync_period {
                let frame = CANFrame::new(0x80, &[], false, false).unwrap();
                bcm.send_periodically(sync_period, frame)
                    .map_err(OpenError::IOError)?;
            }

            Ok(CANOpenSocket { socket, _bcm: bcm })
        }

        pub async fn read(&self) -> Result<CANOpenNodeMessage, ReadError> {
            let frame = self.socket.read().await.map_err(ReadError::IO)?;
            CANOpenNodeMessage::try_from(frame).map_err(ReadError::Parse)
        }

        pub async fn write(&self, command: CANOpenNodeCommand) -> Result<(), std::io::Error> {
            let frame = CANFrame::from(command);
            self.socket.write(&frame).await
        }
    }
}

#[derive(Debug)]
pub enum PDO {
    PDO1,
    PDO2,
    PDO3,
    PDO4,
}

impl PDO {
    #[allow(unused)]
    fn get_from_device_id(&self) -> u32 {
        match self {
            PDO::PDO1 => 0x180,
            PDO::PDO2 => 0x280,
            PDO::PDO3 => 0x380,
            PDO::PDO4 => 0x480,
        }
    }

    fn get_to_device_id(&self) -> u32 {
        match self {
            PDO::PDO1 => 0x200,
            PDO::PDO2 => 0x300,
            PDO::PDO3 => 0x400,
            PDO::PDO4 => 0x500,
        }
    }
}

#[derive(Debug)]
pub enum NMTState {
    Initializing,
    Stopped,
    Operational,
    PreOperational,
}

impl From<u8> for NMTState {
    fn from(raw: u8) -> Self {
        match raw {
            0x04 => NMTState::Stopped,
            0x05 => NMTState::Operational,
            0x7f => NMTState::PreOperational,
            _ => NMTState::Initializing,
        }
    }
}

#[derive(Debug)]
pub enum NMTCommand {
    GoToOperational,
    GoToStopped,
    GoToPreOperational,
    ResetNode,
    ResetCommunication,
}

impl From<NMTCommand> for u8 {
    fn from(command: NMTCommand) -> Self {
        match command {
            NMTCommand::GoToOperational => 0x01,
            NMTCommand::GoToStopped => 0x02,
            NMTCommand::GoToPreOperational => 0x80,
            NMTCommand::ResetNode => 0x81,
            NMTCommand::ResetCommunication => 0x82,
        }
    }
}

#[derive(Debug)]
pub struct SDOControlByte {
    ccs: u8,
    bytes_not_containing_data: u8,
    expedited: bool,
    data_size_in_control_byte: bool,
}
// TODO from and into traits
impl SDOControlByte {}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessageParseError {
    #[error("Invalid ID {0} found in the frame.")]
    InvalidID(u32),
}

impl TryFrom<CANFrame> for CANOpenNodeMessage {
    type Error = MessageParseError;

    fn try_from(frame: CANFrame) -> Result<Self, Self::Error> {
        let frame_id = frame.id() & 0xf80;
        match frame_id {
            0x80 => Ok(CANOpenNodeMessage::SyncReceived),
            0x180 => Ok(CANOpenNodeMessage::PDOReceived(
                PDO::PDO1,
                frame.raw_data(),
                frame.len(),
            )),
            0x280 => Ok(CANOpenNodeMessage::PDOReceived(
                PDO::PDO2,
                frame.raw_data(),
                frame.len(),
            )),
            0x380 => Ok(CANOpenNodeMessage::PDOReceived(
                PDO::PDO3,
                frame.raw_data(),
                frame.len(),
            )),
            0x480 => Ok(CANOpenNodeMessage::PDOReceived(
                PDO::PDO4,
                frame.raw_data(),
                frame.len(),
            )),
            // FIXME implement later
            // 0x580 => Ok(CANOpenNodeMessage::SDOReceived(
            //     LittleEndian::read_u16(&frame._data[0..2]),
            //     frame._data[2],
            // )),
            0x700 => Ok(CANOpenNodeMessage::NMTReceived(frame.data()[0].into())),
            _ => Err(MessageParseError::InvalidID(frame_id)),
        }
    }
}

impl From<CANOpenNodeCommand> for CANFrame {
    fn from(command: CANOpenNodeCommand) -> Self {
        match command {
            CANOpenNodeCommand::SendPDO(id, pdo, data, size) => CANFrame::new(
                pdo.get_to_device_id() | id as u32,
                &data[..size],
                false,
                false,
            )
            .unwrap(),
            CANOpenNodeCommand::SendNMT(id, command) => {
                CANFrame::new(0x700 | id as u32, &[command.into()], false, false).unwrap()
            }
            CANOpenNodeCommand::SendSDO(id, _, _, _, _, _) => {
                CANFrame::new(0x580 | id as u32, &[], false, false).unwrap()
            }
        }
    }
}
