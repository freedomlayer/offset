use rand::{self, rngs::StdRng};

use funder::FunderState;
use proto::file::{
    FriendAddressFile, FriendFile, IdentityFile, IndexServerFile, NodeAddressFile,
    RelayAddressFile, TrustedAppFile,
};
use proto::net::messages::NetAddress;
use stcompact::compact_node::CompactState;

use quickcheck::QuickCheck;

/// Define conversion to/from String:
macro_rules! ser_de_test {
    ($test_name:ident, $msg_type:ty) => {
        #[test]
        fn $test_name() {
            fn ser_de(msg: $msg_type) -> bool {
                let ser_str = serde_json::to_string_pretty(&msg).unwrap();
                let msg2: $msg_type = serde_json::from_str(&ser_str).unwrap();
                msg == msg2
            }

            let rng_seed: [u8; 32] = [1; 32];
            let rng: StdRng = rand::SeedableRng::from_seed(rng_seed);

            // Limit size, to avoid blowup to type size:
            let size = 3usize;
            QuickCheck::with_gen(quickcheck::StdGen::new(rng, size))
                .max_tests(100)
                .quickcheck(ser_de as fn($msg_type) -> bool);
        }
    };
}

ser_de_test!(qc_ser_de_funder_state, FunderState<NetAddress>);
ser_de_test!(qc_ser_de_friend_address_file, FriendAddressFile);
ser_de_test!(qc_ser_de_friend_file, FriendFile);
ser_de_test!(qc_ser_de_identity_file, IdentityFile);
ser_de_test!(qc_ser_de_index_server_file, IndexServerFile);
ser_de_test!(qc_ser_de_node_address_file, NodeAddressFile);
ser_de_test!(qc_ser_de_relay_address_file, RelayAddressFile);
ser_de_test!(qc_ser_de_trusted_app_file, TrustedAppFile);

ser_de_test!(qc_ser_de_compact_state, CompactState);

/*
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
        .max_tests(100)
        .quickcheck(ser_de as fn(FunderState<NetAddress>) -> bool);
}
*/

// TODO: Add serialization tests for proto::file
// Possibly create a macro to do all the boilerplate for quickcheck tests.
// use proto::file::*;
