use super::*;
use crossbeam;

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
        let frame = CANFrame::new(0x80, &[], false, false).unwrap();
        bus.bcm_send_periodically(sync_period, frame).unwrap();
        
        let (frame_received_sender, frame_received_receiver) = crossbeam::unbounded::<CANFrame>();
        let (frame_to_send_sender, frame_to_send_receiver) = crossbeam::unbounded::<CANFrame>();

        let thread_handle = std::thread::spawn(move || {
            loop {
                while let Ok(frame) = bus.read_frame() {
                    frame_received_sender.send(frame);
                }
                while let Ok(frame) = frame_to_send_receiver.try_recv() {
                    bus.write_frame(&frame).expect("failed to write frame.");
                }
            }
        });

        Ok(
            CANOpen {
                sender: frame_to_send_sender,
                receiver: frame_received_receiver,
                thread_handle: Some(thread_handle),
            }
        )
    }

    pub fn create_handle(&self) -> CANOpenHandle {
        CANOpenHandle {
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
        }
    }
}

impl Drop for CANOpen {
    fn drop(&mut self) {
        self.thread_handle.take().unwrap().join().unwrap();
    }
}