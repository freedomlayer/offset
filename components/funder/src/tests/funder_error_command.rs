use common::test_executor::TestExecutor;

use proto::crypto::PublicKey;
use proto::funder::messages::FriendStatus;

use super::utils::{create_node_controls, dummy_relay_address};

async fn task_funder_error_command(test_executor: TestExecutor) {
    let num_nodes = 2;
    let mut node_controls = create_node_controls(num_nodes, test_executor.clone()).await;

    let public_keys = node_controls
        .iter()
        .map(|nc| nc.public_key.clone())
        .collect::<Vec<PublicKey>>();

    let relays0 = vec![dummy_relay_address(0)];
    let relays1 = vec![dummy_relay_address(1)];
    node_controls[0]
        .add_friend(&public_keys[1], relays1, "node1")
        .await;
    node_controls[1]
        .add_friend(&public_keys[0], relays0, "node0")
        .await;
    assert_eq!(node_controls[0].report.friends.len(), 1);
    assert_eq!(node_controls[1].report.friends.len(), 1);

    // This command should cause an error, because node0 is not friend of itself.
    // We expect that the Funder will be able to handle this error, and not crash:
    node_controls[0]
        .set_friend_status(&public_keys[0], FriendStatus::Enabled)
        .await;

    // This command should work correctly:
    node_controls[0]
        .set_friend_status(&public_keys[1], FriendStatus::Enabled)
        .await;
}

#[test]
fn test_funder_error_command() {
    let test_executor = TestExecutor::new();
    let res = test_executor.run(task_funder_error_command(test_executor.clone()));
    assert!(res.is_output());
}
