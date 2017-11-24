extern crate futures;

use self::futures::Future;
use self::futures::future::{loop_fn, Loop};

use std::cell::RefCell;
use super::{InnerChanneler, ChannelerError};

const CONN_ATTEMPT_TICKS: usize = 120;

struct TimerReader;

fn create_timer_reader_future<'a,R>(inner_channeler: RefCell<InnerChanneler<'a,R>>) 
    -> impl Future<Item=(), Error=ChannelerError> + 'a {

    loop_fn(TimerReader, move |timer_reader| {
        let mut b_inner_channeler = inner_channeler.borrow_mut();

        for (_, mut neighbor) in &mut b_inner_channeler.neighbors {
            let socket_addr = match neighbor.info.neighbor_address.socket_addr {
                None => continue,
                Some(socket_addr) => socket_addr,
            };
            // If there are already some attempts to add connections, 
            // we don't try to add a new connection ourselves.
            if neighbor.num_pending_out_conn > 0 {
                continue;
            }
            if neighbor.channels.len() == 0 {
                // This is an inactive neighbor.
                neighbor.ticks_to_next_conn_attempt -= 1;
                if neighbor.ticks_to_next_conn_attempt == 0 {
                    neighbor.ticks_to_next_conn_attempt = CONN_ATTEMPT_TICKS;
                } else {
                    continue;
                }
            }
            /*
            // Attempt a connection:
            TcpStream::connect(&socket_addr, &self.handle)
                .and_then(|stream| {
                    let (sink, stream) = stream.framed(PrefixFrameCodec::new()).split();

                    // TODO: Binary deserializtion of Channeler to Channeler messages.
            */
        }
        Ok(Loop::Break(()))
    })
}
