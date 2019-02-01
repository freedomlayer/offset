#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

use futures::executor::ThreadPool;
use relay::{relay_server, RelayServerError};

fn main() {
    let thread_pool = match ThreadPool::new() {
        Ok(thread_pool) => thread_pool,
        Err(_) => {
            error!("Could not create a ThreadPool! Aborting.");
            return;
        },
    };

    /*
    relay_server(mut timer_client: TimerClient, 
                incoming_conns: S,
                keepalive_ticks: usize,
                mut spawner: impl Spawn + Clone) -> Result<(), RelayServerError> 
    */

    println!("Hello world!");
}
