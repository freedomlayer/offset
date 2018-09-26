use std::{io, cmp, mem};
use std::marker::PhantomData;
use futures::{Async, AsyncSink, Stream, Sink, Poll};
use tokio_io::{AsyncRead, AsyncWrite};


pub struct AsyncReader<M,E> {
    opt_receiver: Option<M>,
    pending_in: Vec<u8>,
    phantom_error: PhantomData<E>,
}

impl<M,E> AsyncReader<M,E> {
    pub fn new(receiver: M) -> Self {
        AsyncReader {
            opt_receiver: Some(receiver),
            pending_in: Vec::new(),
            phantom_error: PhantomData,
        }
    }
}

impl<M,E> io::Read for AsyncReader<M,E> 
where
    M: Stream<Item=Vec<u8>, Error=E>,
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

            if buf.is_empty() {
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
                        Err(_) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
                    };
                },
                None => return Ok(total_read),
            }
        }
    }
}


impl<M,E> AsyncRead for AsyncReader<M,E> where M: Stream<Item=Vec<u8>, Error=E> {}


pub struct AsyncWriter<K,E> {
    opt_sender: Option<K>,
    pending_out: Vec<u8>,
    max_frame_len: usize,
    phantom_error: PhantomData<E>,
}


impl<K,E> AsyncWriter<K,E> {
    pub fn new(sender: K, max_frame_len: usize) -> Self {
        AsyncWriter {
            opt_sender: Some(sender),
            pending_out: Vec::new(),
            max_frame_len,
            phantom_error: PhantomData,
        }
    }
}


impl<K,E> io::Write for AsyncWriter<K,E> 
where
    K: Sink<SinkItem=Vec<u8>, SinkError=E>,
{
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let mut sender = match self.opt_sender.take() {
            Some(sender) => sender,
            None => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
        };
        let mut total_write = 0;

        loop {
            if buf.is_empty() {
                self.opt_sender = Some(sender);
                return Ok(total_write);
            }

            // Buffer as much as possible:
            let free_bytes = self.max_frame_len.checked_sub(self.pending_out.len()).unwrap();
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
                Err(_) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
            };
            if !is_ready {
                self.opt_sender = Some(sender);
                if total_write > 0 {
                    return Ok(total_write);
                } else {
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
            Err(_) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
        };

        self.opt_sender = Some(sender);
        if !is_ready {
            Err(io::Error::new(io::ErrorKind::WouldBlock, "WouldBlock"))
        } else {
            Ok(())
        }
    }
}

impl<K,E> AsyncWrite for AsyncWriter<K,E> where K: Sink<SinkItem=Vec<u8>, SinkError=E> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        match self.opt_sender.take() {
            Some(mut sender) => {
                match sender.close() {
                    Ok(Async::Ready(())) => Ok(Async::Ready(())),
                    Ok(Async::NotReady) => {
                        self.opt_sender = Some(sender);
                        Ok(Async::NotReady)
                    },
                    Err(_) => Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
                }
            },
            None => Err(io::Error::new(io::ErrorKind::BrokenPipe, "BrokenPipe")),
        }
    }
}

#[cfg(test)]
#[allow(unused)]
mod tests {
    use super::*;
    use futures::sync::mpsc;
    use futures::prelude::{async, await};
    use futures::Future;
    use tokio_core::reactor::Core;

    enum ReceiverRes {
        Ready((usize, Vec<u8>)),
        NotReady,
        Error,
    }

    struct TestReceiver {
        receiver: mpsc::Receiver<Vec<u8>>,
        results: Vec<ReceiverRes>,
    }

    /*
    impl Future for TestReceiver {
        fn poll(&mut self) -> Poll<(), Self::Error> {
            let my_buff = [0; 0x100];
            match self.receiver.poll_read(&mut my_buff) {
                Ok(Async::Ready(size)) => self.results.push(
                    (ReceiverRes::Ready(size), my_buff[0..size].to_vec())),
                Ok(Async::NotReady) => self.results.push(ReceiverRes::NotReady),
                Err(_e) => self.results.push(ReceiverRes::Error),
            }
        }
    }
    */

    // TODO: Continue tests here

    #[async]
    fn basic_stream_receiver() -> Result<(), ()> {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let async_reader: AsyncReader<_, ()> = AsyncReader::new(receiver);
        Ok(())
    }

    #[test]
    fn test_basic_stream_receiver() {

        let mut core = Core::new().unwrap();
        core.run(basic_stream_receiver());
    }
}
