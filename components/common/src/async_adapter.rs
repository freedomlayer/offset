use std::{cmp, mem};
use std::marker::PhantomData;
use core::pin::Pin;

use futures;
use futures::{Poll, Stream, StreamExt, Sink};
use futures::io::{AsyncRead, AsyncWrite};
use futures::task::Waker;

pub struct AsyncReader<M> {
    opt_receiver: Option<M>,
    pending_in: Vec<u8>,
}

impl<M> AsyncReader<M> {
    pub fn new(receiver: M) -> Self {
        AsyncReader {
            opt_receiver: Some(receiver),
            pending_in: Vec::new(),
        }
    }
}


impl<M> AsyncRead for AsyncReader<M> where M: Stream<Item=Vec<u8>> + std::marker::Unpin {
    fn poll_read(&mut self, waker: &Waker, mut buf: &mut [u8]) -> 
        Poll<Result<usize, futures::io::Error>> {


        let mut total_read = 0; // Total amount of bytes read
        loop {
            // pending_in --> buf (As many bytes as possible)
            let min_len = cmp::min(buf.len(), self.pending_in.len());
            buf[.. min_len].copy_from_slice(&self.pending_in[.. min_len]);
            let _ = self.pending_in.drain(.. min_len);
            buf = &mut buf[min_len ..];
            total_read += min_len;

            if buf.is_empty() {
                return Poll::Ready(Ok(total_read));
            }

            match self.opt_receiver.take() {
                Some(mut receiver) => {
                    match receiver.poll_next_unpin(waker) {
                        Poll::Ready(Some(data)) => {
                            self.opt_receiver = Some(receiver);
                            self.pending_in = data;
                        },
                        Poll::Ready(None) => return Poll::Ready(Ok(total_read)), // End of incoming data
                        Poll::Pending => {
                            self.opt_receiver = Some(receiver);
                            if total_read > 0 {
                                return Poll::Ready(Ok(total_read));
                            } else {
                                return Poll::Pending;
                            }
                        },
                    };
                },
                None => return Poll::Ready(Ok(total_read)),
            }
        }
    }
}


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


impl<K,E> AsyncWrite for AsyncWriter<K,E> where K: Sink<SinkItem=Vec<u8>, SinkError=E> + std::marker::Unpin {
    fn poll_write(&mut self, lw: &Waker, mut buf: &[u8]) 
        -> Poll<Result<usize, futures::io::Error>> {

        let mut sender = match self.opt_sender.take() {
            Some(sender) => sender,
            None => return Poll::Pending,
        };
        let mut total_write = 0;

        loop {
            if buf.is_empty() {
                self.opt_sender = Some(sender);
                return Poll::Ready(Ok(total_write));
            }

            // Buffer as much as possible:
            let free_bytes = self.max_frame_len.checked_sub(self.pending_out.len()).unwrap();
            let min_len = cmp::min(buf.len(), free_bytes);
            self.pending_out.extend_from_slice(&buf[.. min_len]);
            buf = &buf[min_len ..];
            total_write += min_len;

            match Pin::new(&mut sender).poll_ready(lw) {
                Poll::Ready(Ok(())) => {
                    let pending_out = mem::replace(&mut self.pending_out, Vec::new());
                    match Pin::new(&mut sender).start_send(pending_out) {
                        Ok(()) => {},
                        Err(_) => return Poll::Ready(Err(futures::io::Error::new(
                                futures::io::ErrorKind::BrokenPipe, "BrokenPipe"))),
                    };
                },
                Poll::Pending => {
                    self.opt_sender = Some(sender);
                    if total_write > 0 {
                        return Poll::Ready(Ok(total_write));
                    } else {
                        return Poll::Pending;
                    }
                },
                Poll::Ready(Err(_)) => return Poll::Ready(Err(futures::io::Error::new(
                        futures::io::ErrorKind::BrokenPipe, "BrokenPipe"))),
            };
        }
    }
    
    fn poll_flush(&mut self, lw: &Waker) 
        -> Poll<Result<(), futures::io::Error>> {

        let mut sender = match self.opt_sender.take() {
            Some(sender) => sender,
            None => return Poll::Ready(Err(futures::io::Error::new(
                    futures::io::ErrorKind::BrokenPipe, "BrokenPipe"))),
        };

        // Try to send whatever pending bytes we have:
        if !self.pending_out.is_empty() {
            match Pin::new(&mut sender).poll_ready(lw) {
                Poll::Ready(Ok(())) => {
                    let pending_out = mem::replace(&mut self.pending_out, Vec::new());
                    match Pin::new(&mut sender).start_send(pending_out) {
                        Ok(()) => {},
                        Err(_) => return Poll::Ready(Err(futures::io::Error::new(
                            futures::io::ErrorKind::BrokenPipe, "BrokenPipe"))),
                    }
                },
                Poll::Pending => {
                    self.opt_sender = Some(sender);
                    return Poll::Pending;
                },
                Poll::Ready(Err(_)) => return Poll::Ready(Err(futures::io::Error::new(
                        futures::io::ErrorKind::BrokenPipe, "BrokenPipe"))),
            }
        }

        match Pin::new(&mut sender).poll_flush(lw) {
            Poll::Ready(Ok(())) => {
                self.opt_sender = Some(sender);
                Poll::Ready(Ok(()))
            },
            Poll::Pending => {
                self.opt_sender = Some(sender);
                Poll::Pending
            },
            Poll::Ready(Err(_)) => Poll::Ready(Err(futures::io::Error::new(
                    futures::io::ErrorKind::BrokenPipe, "BrokenPipe"))),
        }
    }

    fn poll_close(&mut self, lw: &Waker) 
        -> Poll<Result<(), futures::io::Error>> {

        // TODO: Should we try to flush anything here?

        let mut sender = match self.opt_sender.take() {
            Some(sender) => sender,
            None => return Poll::Ready(Err(futures::io::Error::new(
                    futures::io::ErrorKind::BrokenPipe, "BrokenPipe"))),
        };

        match Pin::new(&mut sender).poll_close(lw) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Pending => {
                self.opt_sender = Some(sender);
                Poll::Pending
            },
            Poll::Ready(Err(_)) => Poll::Ready(Err(futures::io::Error::new(
                    futures::io::ErrorKind::BrokenPipe, "BrokenPipe"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc;
    use futures::SinkExt;
    use futures::executor::LocalPool;
    use futures::io::{AsyncReadExt, AsyncWriteExt};

    // TODO: Tests here are very basic.
    // More tests are required.

    /*
    async fn plain_sender(sender: impl Sink<SinkItem=Vec<u8>, SinkError=()> + std::marker::Unpin + 'static, 
                     reader_done_send: oneshot::Sender<bool>)  {
        let mut vec = vec![0u8,0,0,0x90];
        vec.extend(vec![0; 0x90]);
        await!(sender.send(vec)).unwrap();

        let mut vec = vec![0u8,0,0,0x75];
        vec.extend(vec![1; 0x75]);
        await!(sender.send(vec)).unwrap();
        reader_done_send.send(true);
    }

    async fn frames_receiver(receiver: impl Stream<Item=Vec<u8>> + std::marker::Unpin + 'static, 
                       writer_done_send: oneshot::Sender<bool>)  {
        let (opt_data, receiver) = await!(receiver.into_future());
        assert_eq!(opt_data.unwrap(), vec![0; 0x90]);
        let (opt_data, receiver) = await!(receiver.into_future());
        assert_eq!(opt_data.unwrap(), vec![1; 0x75]);
        let (opt_data, receiver) = await!(receiver.into_future());
        assert_eq!(opt_data, None);
        writer_done_send.send(true);
    }

    #[test]
    fn test_basic_async_reader() {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let async_reader = AsyncReader::new(receiver);
        let reader = FramedRead::new(async_reader.compat(), FrameCodec::new())
            .map_err(|_| ());
        let sender = sender.sink_map_err(|_| ());

        let (reader_done_send, reader_done_recv) = oneshot::channel::<bool>();
        let (writer_done_send, writer_done_recv) = oneshot::channel::<bool>();

        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();
        spawner.spawn(frames_receiver(reader, reader_done_send));
        spawner.spawn(plain_sender(sender, writer_done_send));
        assert_eq!(true, local_pool.run_until(reader_done_recv).unwrap());
        assert_eq!(true, local_pool.run_until(writer_done_recv).unwrap());
    }

    async fn frames_sender(sender: impl Sink<SinkItem=Vec<u8>, SinkError=()> + std::marker::Unpin + 'static, 
                     reader_done_send: oneshot::Sender<bool>) {
        await!(sender.send(vec![0; 0x90])).unwrap();
        await!(sender.send(vec![1; 0x75])).unwrap();
        reader_done_send.send(true);
    }

    async fn plain_receiver(mut receiver: impl Stream<Item=Vec<u8>> + std::marker::Unpin + 'static, 
                       writer_done_send: oneshot::Sender<bool>) {

        let mut total_buf = Vec::new();
        while let (Some(data), new_receiver) = await!(receiver.into_future()) {
            receiver = new_receiver;
            assert!(data.len() <= 0x11);
            total_buf.extend(data);
        }

        let mut expected_buf = Vec::new();
        expected_buf.extend(vec![0,0,0,0x90]);
        expected_buf.extend(vec![0; 0x90]);
        expected_buf.extend(vec![0,0,0,0x75]);
        expected_buf.extend(vec![1; 0x75]);
        assert_eq!(total_buf, expected_buf);
        writer_done_send.send(true);
    }

    #[test]
    fn test_basic_async_writer() {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let async_writer = AsyncWriter::new(sender, 0x11);
        let writer = FramedWrite::new(async_writer, FrameCodec::new())
            .sink_map_err(|_| ());

        let (reader_done_send, reader_done_recv) = oneshot::channel::<bool>();
        let (writer_done_send, writer_done_recv) = oneshot::channel::<bool>();

        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();
        spawner.spawn(plain_receiver(receiver, reader_done_send));
        spawner.spawn(frames_sender(writer, writer_done_send));
        assert_eq!(true, local_pool.run_until(reader_done_recv).unwrap());
        assert_eq!(true, local_pool.run_until(writer_done_recv).unwrap());
    }

    #[test]
    fn test_basic_async_reader_writer() {
        let (sender, receiver) = mpsc::channel::<Vec<u8>>(0);
        let async_writer = AsyncWriter::new(sender, 0x11);
        let writer = FramedWrite::new(async_writer, FrameCodec::new())
            .sink_map_err(|_| ());
        let async_reader = AsyncReader::new(receiver);
        let reader = FramedRead::new(async_reader, FrameCodec::new())
            .map_err(|_| ());

        let (reader_done_send, reader_done_recv) = oneshot::channel::<bool>();
        let (writer_done_send, writer_done_recv) = oneshot::channel::<bool>();

        let mut local_pool = LocalPool::new();
        let spawner = local_pool.spawner();
        spawner.spawn(frames_receiver(reader, reader_done_send));
        spawner.spawn(frames_sender(writer, writer_done_send));
        assert_eq!(true, local_pool.run_until(reader_done_recv).unwrap());
        assert_eq!(true, local_pool.run_until(writer_done_recv).unwrap());
    }
    */

    async fn task_basic_async_write() {
        // We use a channel with a large buffer so that we don't need two tasks to run this test.
        let (sender, mut receiver) = mpsc::channel::<Vec<u8>>(100);
        let mut async_writer = AsyncWriter::new(sender, 0x11);

        await!(async_writer.write_all(&[0; 0x90])).unwrap();
        await!(async_writer.write_all(&[0; 0x75])).unwrap();
        drop(async_writer);

        let mut total_buf = Vec::new();
        while let (Some(data), new_receiver) = await!(receiver.into_future()) {
            receiver = new_receiver;
            assert!(data.len() <= 0x11);
            total_buf.extend(data);
        }

        let mut expected_buf = Vec::new();
        expected_buf.extend(vec![0; 0x90 + 0x75]);

        assert_eq!(expected_buf, total_buf);
    }


    #[test]
    fn test_basic_async_write() {
        let mut local_pool = LocalPool::new();
        local_pool.run_until(task_basic_async_write());
    }


    async fn task_basic_async_read() {
        // We use a channel with a large buffer so that we don't need two tasks to run this test.
        let (mut sender, receiver) = mpsc::channel::<Vec<u8>>(100);
        let mut async_reader = AsyncReader::new(receiver);

        for i in 0 .. 20 {
            await!(sender.send(vec![i; 0x10])).unwrap();
        }
        drop(sender);

        let mut total_buf = [0u8; 20 * 0x10];
        let mut buf_pos = &mut total_buf[..];
        while let Ok(num_read) = await!(async_reader.read(buf_pos)) {
            if num_read == 0 {
                break;
            }
            buf_pos = &mut buf_pos[num_read..];
        }

        let mut expected_buf = Vec::new();
        for i in 0 .. 20 {
            expected_buf.extend(vec![i; 0x10]);
        }

        assert_eq!(&expected_buf[..], &total_buf[..]);
    }


    #[test]
    fn test_basic_async_read() {
        let mut local_pool = LocalPool::new();
        local_pool.run_until(task_basic_async_read());
    }
}
