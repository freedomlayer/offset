use crypto::crypto_rand::system_random;
use crypto::uid::Uid;
pub use crypto::uid::UID_LEN;

use node::connect::{node_connect, NodeConnection};

/// Generate a random uid
pub fn gen_uid() -> Uid {
    // TODO: Is it safe to create a new `system_random` every time?
    // Obtain secure cryptographic random:
    let rng = system_random();

    Uid::new(&rng)
}
