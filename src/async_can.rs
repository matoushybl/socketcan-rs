use crate::socketcan::CANFrame;
use tokio::prelude::io::unix::AsyncFd;

struct CANSocket {
    async_fd: AsyncFd<super::CANSocket>,
}

impl CANSocket {
    pub fn new(interface_name: &str) -> Self {
        let socket = super::CANSocket::new(interface_name).unwrap();
        socket.set_nonblocking();
        Self {
            async_fd: AsyncFd::new(socket).unwrap(),
        }
    }

    pub async fn read(&self) -> std::io::Result<CANFrame> {
        self.async_fd
            .readable()
            .await?
            .with_io(|| self.async_fd.get_ref().read())
    }

    pub async fn write(&self, frame: &CANFrame) -> std::io::Result<()> {
        self.async_fd
            .writable()
            .await?
            .with_io(|| self.async_fd.get_ref().write(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const CAN: &str = "can0";

    #[tokio::test]
    async fn async_read() {
        let socket = CANSocket::new(CAN);
        let _ = socket.read().await;
    }

    #[tokio::test]
    async fn async_bidirectional() {
        let a = tokio::task::spawn({
            let socket = CANSocket::new(CAN);
            async move {
                loop {
                    match tokio::time::timeout(Duration::from_secs(2), socket.read()).await {
                        Ok(r) => match r {
                            Ok(_) => {
                                println!("frame received");
                            }
                            Err(_) => {
                                break;
                            }
                        },
                        Err(_) => {
                            break;
                        }
                    }
                }
            }
        });

        let b = tokio::spawn({
            let socket = CANSocket::new(CAN);
            async move {
                let mut interval = tokio::time::interval(Duration::from_millis(10));
                for _ in 0i8..100i8 {
                    let frame = CANFrame::new(0x80, &[], false, false).unwrap();
                    let _ = socket.write(&frame).await;
                    interval.tick().await;
                }
            }
        });

        let _ = tokio::join!(a, b);
    }
}
