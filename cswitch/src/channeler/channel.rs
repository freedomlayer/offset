extern crate futures;

use self::futures::sync::mpsc;
use self::futures::{Future, IntoFuture};
use ::inner_messages::ChannelerAddress;

use super::ToChannel;

pub enum ChannelError {
}

pub fn create_channel(channeler_address: ChannelerAddress) 
        -> (mpsc::Sender<ToChannel>, impl Future<Item=(), Error=ChannelError>) {

    // Create an mpsc channel that will be used to signal this channel future.
    let (channel_sender, channel_receiver) = mpsc::channel(0);

    (channel_sender, Ok(()).into_future())
}
