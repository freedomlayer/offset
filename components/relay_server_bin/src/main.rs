#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#[macro_use]
extern crate log;

use std::time::{Duration, Instant};
use futures::executor::ThreadPool;

use common::int_convert::usize_to_u64;
use timer::create_timer;
use relay::{relay_server, RelayServerError};
use net::TcpListener;
use proto::consts::{TICK_MS, KEEPALIVE_TICKS};


fn main() {
    let thread_pool = match ThreadPool::new() {
        Ok(thread_pool) => thread_pool,
        Err(_) => {
            error!("Could not create a ThreadPool! Aborting.");
            return;
        },
    };

    let dur = Duration::from_millis(usize_to_u64(TICK_MS).unwrap()); 
    let timer_client = match create_timer(dur, thread_pool.clone()) {
        Ok(timer_client) => timer_client,
        Err(_) => {
            error!("Failed to create timer! Aborting.");
            return;
        }
    };


    /*
    relay_server(incoming_conns,
                timer_client,
                conn_timeout_ticks,
                keepalive_ticks,
                thread_pool);
    */


    println!("Hello world!");
}
