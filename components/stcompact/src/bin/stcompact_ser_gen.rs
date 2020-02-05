#![deny(trivial_numeric_casts, warnings)]
#![allow(intra_doc_link_resolution_failure)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

/*
#[macro_use]
extern crate log;
*/

use quickcheck::Arbitrary;
use rand::{self, rngs::StdRng};
use serde::Serialize;
use structopt::StructOpt;

use stcompact::messages::{ServerToUser, UserToServer};

/// stcompact_ser_gen: Offst Compact serialization generator
///
/// Generates example serialized messages, for integration tests
/// with a user application
///
#[derive(Debug, StructOpt)]
#[structopt(name = "stcompact_ser_gen")]
pub struct StCompactSerGenCmd {
    /// Amount of instances to generate (For each message type)
    #[structopt(short = "i", long = "iters")]
    pub iters: usize,
}

fn gen_print_instances<T, G>(type_name: &str, iters: usize, gen: &mut G)
where
    T: Arbitrary + Serialize,
    G: quickcheck::Gen,
{
    println!("final {} = [", type_name);
    for _ in 0..iters {
        let msg = T::arbitrary(gen);
        let ser_str = serde_json::to_string_pretty(&msg).unwrap();
        println!("r'''");
        println!("{}", ser_str);
        println!("''',");
    }
    println!("];\n");
}

fn main() {
    env_logger::init();

    // Load argumnets:
    let st_compact_ser_gen_cmd = StCompactSerGenCmd::from_args();

    // Create a random generator for quickcheck:
    let size = 3;
    let rng_seed: [u8; 32] = [1; 32];
    let rng: StdRng = rand::SeedableRng::from_seed(rng_seed);
    let mut gen = quickcheck::StdGen::new(rng, size);

    // Print randomly generated instances to console:
    gen_print_instances::<ServerToUser, _>("serverToUser", st_compact_ser_gen_cmd.iters, &mut gen);
    println!("// -------------------------------------");
    gen_print_instances::<UserToServer, _>("userToServer", st_compact_ser_gen_cmd.iters, &mut gen);
}
