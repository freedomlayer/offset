use funder::FunderState;
use proto::net::messages::NetAddress;

use quickcheck::QuickCheck;

#[test]
fn qc_funder_state_json() {
    fn ser_de(state: FunderState<NetAddress>) -> bool {
        let ser_str = serde_json::to_string(&state).unwrap();
        let state2: FunderState<NetAddress> = serde_json::from_str(&ser_str).unwrap();
        state2 == state
    }
    QuickCheck::new()
        .max_tests(10)
        .quickcheck(ser_de as fn(FunderState<NetAddress>) -> bool);
}
