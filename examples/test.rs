extern crate socketcan;

use socketcan::CANSocket;
use socketcan::CANFrame;
use socketcan::canopen::CANOpen;

fn main() {
    // let can = CANSocket::open("can0").expect("Failed to open CAN.");
    // let frame = CANFrame::new(0x80, &[], false, false).unwrap();
    // can.bcm_send_periodically(50000, frame).unwrap();
    // while let Ok(frame) = can.read_frame() {
    //     println!("{}", frame);
    // }

    let canOpen = CANOpen::new("can0", 50000).unwrap();
    let handle = canOpen.create_handle();
    loop {
        if let Ok(frame) = handle.receiver.recv() {
            println!("{}", frame);
        }
    }
}