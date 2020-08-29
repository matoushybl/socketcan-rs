use super::*;
use byteorder::{ByteOrder, LittleEndian};
use crossbeam;
use std::time::Duration;

pub struct CANOpen {
    sender: crossbeam::Sender<CANFrame>,
    receiver: crossbeam::Receiver<CANFrame>,
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

pub struct CANOpenHandle {
    pub sender: crossbeam::Sender<CANFrame>,
    pub receiver: crossbeam::Receiver<CANFrame>,
}

impl CANOpen {
    pub fn new(bus_name: &str, sync_period: u64) -> Result<CANOpen, CANSocketOpenError> {
        let bus = CANSocket::open(bus_name)?;
        bus.set_nonblocking(true)?;
        let frame = CANFrame::new(0x80, &[], false, false).unwrap();
        bus.bcm_send_periodically(sync_period, frame).unwrap();

        let (frame_received_sender, frame_received_receiver) = crossbeam::unbounded::<CANFrame>();
        let (frame_to_send_sender, frame_to_send_receiver) = crossbeam::unbounded::<CANFrame>();

        let thread_handle = std::thread::spawn(move || loop {
            while let Ok(frame) = bus.read_frame() {
                frame_received_sender.send(frame);
            }
            while let Ok(frame) = frame_to_send_receiver.try_recv() {
                if bus.write_frame(&frame).is_err() {
                    println!("Failed to send message over CAN.");
                }
            }
            std::thread::sleep(Duration::from_micros(500));
        });

        Ok(CANOpen {
            sender: frame_to_send_sender.clone(),
            receiver: frame_received_receiver.clone(),
            thread_handle: Some(thread_handle),
        })
    }

    pub fn create_device(&self, id: u8) -> CANOpenNode {
        CANOpenNode {
            id,
            handle: CANOpenHandle {
                sender: self.sender.clone(),
                receiver: self.receiver.clone(),
            },
        }
    }
}

impl Drop for CANOpen {
    fn drop(&mut self) {
        self.thread_handle.take().unwrap().join().unwrap();
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

#[derive(Debug)]
pub enum CANOpenNodeMessage {
    SyncReceived,
    PDOReceived(PDO, [u8; 8], u8),
    NMTReceived(NMTState),
    SDOReceived(SDOControlByte, u16, u8, [u8; 4], u8),
}

impl From<CANFrame> for Option<CANOpenNodeMessage> {
    fn from(frame: CANFrame) -> Self {
        let frame_id = frame.id() & 0xf80;
        match frame_id {
            0x80 => Some(CANOpenNodeMessage::SyncReceived),
            0x180 => Some(CANOpenNodeMessage::PDOReceived(
                PDO::PDO1,
                frame._data,
                frame._data_len,
            )),
            0x280 => Some(CANOpenNodeMessage::PDOReceived(
                PDO::PDO2,
                frame._data,
                frame._data_len,
            )),
            0x380 => Some(CANOpenNodeMessage::PDOReceived(
                PDO::PDO3,
                frame._data,
                frame._data_len,
            )),
            0x480 => Some(CANOpenNodeMessage::PDOReceived(
                PDO::PDO4,
                frame._data,
                frame._data_len,
            )),
            // FIXME implement later
            // 0x580 => Some(CANOpenNodeMessage::SDOReceived(
            //     LittleEndian::read_u16(&frame._data[0..2]),
            //     frame._data[2],
            // )),
            0x700 => Some(CANOpenNodeMessage::NMTReceived(frame._data[0].into())),
            _ => None,
        }
    }
}

impl From<CANOpenNodeCommand> for CANFrame {
    fn from(command: CANOpenNodeCommand) -> Self {
        match command {
            CANOpenNodeCommand::SendPDO(id, pdo, data, size) => {
                CANFrame::new(pdo.get_to_device_id() | id as u32, &data[..size], false, false).unwrap()
            }
            CANOpenNodeCommand::SendNMT(id, command) => {
                CANFrame::new(0x700 | id as u32, &[command.into()], false, false).unwrap()
            }
            CANOpenNodeCommand::SendSDO(id, _, _, _, _, _) => {
                CANFrame::new(0x580 | id as u32, &[], false, false).unwrap()
            }
        }
    }
}

pub enum CANOpenNodeCommand {
    SendPDO(u8, PDO, [u8; 8], usize),
    SendNMT(u8, NMTCommand),
    SendSDO(u8, SDOControlByte, u16, u8, [u8; 4], usize),
}

pub struct CANOpenNode {
    id: u8,
    handle: CANOpenHandle,
}

impl CANOpenNode {
    pub fn get_receiver(&self) -> crossbeam::Receiver<CANFrame> {
        self.handle.receiver.clone()
    }

    pub fn get_sender(&self) -> crossbeam::Sender<CANFrame> {
        self.handle.sender.clone()
    }
}
