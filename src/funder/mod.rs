#![allow(dead_code, unused)]

use futures::prelude::{async, await};

pub mod messages;
// pub mod client;
mod liveness;
mod ephemeral;
mod credit_calc;
mod freeze_guard;
mod signature_buff; 
mod friend;
mod state;
mod types;
mod token_channel;
mod handler;
mod report;



#[async]
fn run_funder() -> Result<(),()> {
    Ok(())
}

