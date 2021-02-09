// extern crate socketcan;
//
// use socketcan::canopen::{CANOpen, CANOpenNode, CANOpenNodeMessage};
//
// struct TestNode {
//     id: u8,
//     node: CANOpenNode,
// }
//
// impl TestNode {
//     fn new(bus: &CANOpen, id: u8) -> Self {
//         let node = bus.create_device(id);
//         let receiver = node.get_receiver().clone();
//         let sender = node.get_sender().clone();
//         std::thread::spawn(move || loop {
//             while let Ok(Some(frame)) = receiver
//                 .recv()
//                 .map(|frame| Option::<CANOpenNodeMessage>::from(frame))
//             {
//                 match frame {
//                     CANOpenNodeMessage::SyncReceived => {
//                         // sender.send(CANOpenNodeCommand::SendNMT(id, NMTCommand::ResetNode).into());
//                     }
//                     CANOpenNodeMessage::PDOReceived(_, _, _) => {}
//                     CANOpenNodeMessage::NMTReceived(_) => {}
//                     CANOpenNodeMessage::SDOReceived(_, _, _, _, _) => {}
//                 }
//             }
//         });
//
//         TestNode { id, node }
//     }
// }

use socketcan::bcm::BCMSocket;
use socketcan::CANFrame;
use socketcan::CANSocket;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let bus_name = "can0";
    let socket = CANSocket::new(bus_name)?;
    let bcm = BCMSocket::new(bus_name)?;

    let frame = CANFrame::new(0x80, &[], false, false).unwrap();
    bcm.send_periodically(50000, frame)?;

    Ok(())
}
