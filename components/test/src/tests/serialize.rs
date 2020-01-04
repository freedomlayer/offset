use rand::{self, rngs::StdRng};

use funder::FunderState;
use proto::net::messages::NetAddress;

use quickcheck::QuickCheck;

#[test]
fn qc_ser_de_funder_state_json() {
    fn ser_de(state: FunderState<NetAddress>) -> bool {
        let ser_str = serde_json::to_string_pretty(&state).unwrap();
        let state2: FunderState<NetAddress> = serde_json::from_str(&ser_str).unwrap();
        state2 == state
    }

    let rng_seed: [u8; 32] = [1; 32];
    let rng: StdRng = rand::SeedableRng::from_seed(rng_seed);

    // Limit size, to avoid blowup to type size:
    let size = 3usize;
    QuickCheck::with_gen(quickcheck::StdGen::new(rng, size))
        .max_tests(10)
        .quickcheck(ser_de as fn(FunderState<NetAddress>) -> bool);
}
