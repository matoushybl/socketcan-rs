extern crate socketcan;

use socketcan::canopen::{CANOpen, CANOpenNode, CANOpenNodeMessage};

struct TestNode {
    id: u8,
    node: CANOpenNode,
}

impl TestNode {
    fn new(bus: &CANOpen, id: u8) -> Self {
        let node = bus.create_device(id);
        let receiver = node.get_receiver().clone();
        let sender = node.get_sender().clone();
        std::thread::spawn(move || loop {
            while let Ok(Some(frame)) = receiver
                .recv()
                .map(|frame| Option::<CANOpenNodeMessage>::from(frame))
            {
                match frame {
                    CANOpenNodeMessage::SyncReceived => {
                        // sender.send(CANOpenNodeCommand::SendNMT(id, NMTCommand::ResetNode).into());
                    }
                    CANOpenNodeMessage::PDOReceived(_, _, _) => {}
                    CANOpenNodeMessage::NMTReceived(_) => {}
                    CANOpenNodeMessage::SDOReceived(_, _, _, _, _) => {}
                }
            }
        });

        TestNode { id, node }
    }
}

fn main() {
    let can_open = CANOpen::new("can0", 50000).unwrap();
    let _device = TestNode::new(&can_open, 5);
    loop {}
}
