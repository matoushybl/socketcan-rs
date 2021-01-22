use crate::socketcan::CANFrame;
use crate::OpenError;
use tokio::io::unix::AsyncFd;

pub struct CANSocket {
    async_fd: AsyncFd<super::CANSocket>,
}

impl CANSocket {
    pub fn new(interface_name: &str) -> Result<Self, OpenError> {
        let socket = super::CANSocket::new(interface_name)?;
        socket.set_nonblocking().map_err(OpenError::IOError)?;
        Ok(Self {
            async_fd: AsyncFd::new(socket).unwrap(),
        })
    }

    pub async fn read(&self) -> std::io::Result<CANFrame> {
        match self
            .async_fd
            .readable()
            .await?
            .try_io(|fd| fd.get_ref().read())
        {
            Ok(maybe_frame) => maybe_frame,
            Err(_) => std::io::Result::Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "Tokio poll failed. Try again.",
            )),
        }
    }

    pub async fn write(&self, frame: &CANFrame) -> std::io::Result<()> {
        match self
            .async_fd
            .writable()
            .await?
            .try_io(|fd| fd.get_ref().write(frame))
        {
            Ok(maybe_frame) => maybe_frame,
            Err(_) => std::io::Result::Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "Tokio poll failed. Try again.",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::time::Duration;

    const CAN: &str = "vcan0";

    #[tokio::test]
    #[serial]
    async fn async_bidirectional() {
        let a = tokio::task::spawn({
            let socket = CANSocket::new(CAN).unwrap();
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
            let socket = CANSocket::new(CAN).unwrap();
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
