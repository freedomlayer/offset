use std::{io, cmp};
use futures::sync::mpsc;
use futures::{Async, Stream};


struct StreamReceiver<M> {
    opt_receiver: Option<M>,
    pending_in: Vec<u8>,
}

impl<M> StreamReceiver<M> {
    fn new(receiver: M) -> Self {
        StreamReceiver {
            opt_receiver: Some(receiver),
            pending_in: Vec::new(),
        }
    }
}

impl<M> io::Read for StreamReceiver<M> 
where
    M: Stream<Item=Vec<u8>, Error=()>,
{
    fn read(&mut self, mut buf: &mut [u8]) -> io::Result<usize> {
        let mut total_read = 0; // Total amount of bytes read
        loop {
            // pending_in --> buf (As many bytes as possible)
            let min_len = cmp::min(buf.len(), self.pending_in.len());
            buf[.. min_len].copy_from_slice(&self.pending_in[.. min_len]);
            let _ = self.pending_in.drain(.. min_len);
            buf = &mut buf[min_len ..];
            total_read += min_len;

            if buf.len() == 0 {
                return Ok(total_read);
            }

            match self.opt_receiver.take() {
                Some(mut receiver) => {
                    match receiver.poll() {
                        Ok(Async::Ready(Some(data))) => {
                            self.opt_receiver = Some(receiver);
                            self.pending_in = data;
                        },
                        Ok(Async::Ready(None)) => return Ok(total_read), // End of incoming data
                        Ok(Async::NotReady) => {
                            self.opt_receiver = Some(receiver);
                            if total_read > 0 {
                                return Ok(total_read)
                            } else {
                                return Err(io::Error::new(io::ErrorKind::WouldBlock, "WouldBlock"))
                            }
                        },
                        Err(()) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
                    };
                },
                None => return Ok(total_read),
            }
        }
    }
}
