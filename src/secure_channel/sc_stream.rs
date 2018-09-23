use std::{io, cmp, mem};
use futures::sync::mpsc;
use futures::{Async, AsyncSink, Stream, Sink};


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

struct StreamSender<K> {
    opt_sender: Option<K>,
    pending_out: Vec<u8>,
    max_frame_len: usize,
}


impl<K> StreamSender<K> {
    fn new(sender: K, max_frame_len: usize) -> Self {
        StreamSender {
            opt_sender: Some(sender),
            pending_out: Vec::new(),
            max_frame_len,
        }
    }
}


impl<K> io::Write for StreamSender<K> 
where
    K: Sink<SinkItem=Vec<u8>, SinkError=()>,
{
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let mut sender = match self.opt_sender.take() {
            Some(sender) => sender,
            None => return return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
        };
        let mut total_write = 0;

        loop {
            if buf.len() == 0 {
                self.opt_sender = Some(sender);
                return Ok(total_write);
            }

            // Buffer as much as possible:
            let free_bytes = self.max_frame_len - self.pending_out.len();
            let min_len = cmp::min(buf.len(), free_bytes);
            self.pending_out.extend_from_slice(&buf[.. min_len]);
            buf = &buf[min_len ..];
            total_write += min_len;

            let pending_out = mem::replace(&mut self.pending_out, Vec::new());
            let is_ready = match sender.start_send(pending_out) {
                Ok(AsyncSink::Ready) => true,
                Ok(AsyncSink::NotReady(pending_out)) => {
                    self.pending_out = pending_out;
                    false
                },
                Err(()) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
            };
            if !is_ready {
                if total_write > 0 {
                    self.opt_sender = Some(sender);
                    return Ok(total_write);
                } else {
                    self.opt_sender = Some(sender);
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, "WouldBlock"));
                }
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut sender = match self.opt_sender.take() {
            Some(sender) => sender,
            None => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
        };

        let is_ready = match sender.poll_complete() {
            Ok(Async::Ready(())) => true,
            Ok(Async::NotReady) => false, 
            Err(()) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
        };

        self.opt_sender = Some(sender);
        if !is_ready {
            Err(io::Error::new(io::ErrorKind::WouldBlock, "WouldBlock"))
        } else {
            Ok(())
        }
    }
}
