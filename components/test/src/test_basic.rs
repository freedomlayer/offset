use std::collections::HashMap;

use futures::channel::mpsc;
use futures::task::Spawn;
use futures::executor::ThreadPool;
use futures::SinkExt;

use tempfile::tempdir;

use timer::{create_timer_incoming};
use proto::app_server::messages::AppPermissions;

use crate::utils::{create_node, create_app, SimDb,
                    create_relay, create_index_server,
                    relay_address, named_relay_address, 
                    named_index_server_address, node_public_key};
use crate::sim_network::create_sim_network;

const TIMER_CHANNEL_LEN: usize = 128;

async fn task_basic<S>(mut spawner: S) 
where
    S: Spawn + Clone + Send + Sync + 'static,
{
    let _ = env_logger::init();
    // Create a temporary directory.
    // Should be deleted when gets out of scope:
    let temp_dir = tempdir().unwrap();

    // Create a database manager at the temporary directory:
    let sim_db = SimDb::new(temp_dir.path().to_path_buf());

    // A network simulator:
    let sim_net_client = create_sim_network(&mut spawner);

    // Create timer_client:
    let (mut tick_sender, tick_receiver) = mpsc::channel(TIMER_CHANNEL_LEN);
    let timer_client = create_timer_incoming(tick_receiver, spawner.clone()).unwrap();


    // Create initial database for node 0:
    sim_db.init_db(0);

    let mut trusted_apps = HashMap::new();
    trusted_apps.insert(0, AppPermissions {
        routes: true,
        send_funds: true,
        config: true,
    });
    await!(create_node(0, 
              sim_db.clone(),
              timer_client.clone(),
              sim_net_client.clone(),
              trusted_apps,
              spawner.clone()));

    let app0 = await!(create_app(0,
                    sim_net_client.clone(),
                    timer_client.clone(),
                    0,
                    spawner.clone()));


    // Create initial database for node 1:
    sim_db.init_db(1);

    let mut trusted_apps = HashMap::new();
    trusted_apps.insert(1, AppPermissions {
        routes: true,
        send_funds: true,
        config: true,
    });
    await!(create_node(1, 
              sim_db.clone(),
              timer_client.clone(),
              sim_net_client.clone(),
              trusted_apps,
              spawner.clone()));

    let app1 = await!(create_app(1,
                    sim_net_client.clone(),
                    timer_client.clone(),
                    1,
                    spawner.clone()));

    // Create relays:
    await!(create_relay(0,
                 timer_client.clone(),
                 sim_net_client.clone(),
                 spawner.clone()));

    await!(create_relay(1,
                 timer_client.clone(),
                 sim_net_client.clone(),
                 spawner.clone()));
    
    // Create three index servers:
    // 0 -- 2 -- 1
    // The only way for information to flow between the two index servers
    // is by having the middle server forward it.
    await!(create_index_server(2,
                             timer_client.clone(),
                             sim_net_client.clone(),
                             vec![0,1],
                             spawner.clone()));

    await!(create_index_server(0,
                             timer_client.clone(),
                             sim_net_client.clone(),
                             vec![2],
                             spawner.clone()));

    await!(create_index_server(1,
                             timer_client.clone(),
                             sim_net_client.clone(),
                             vec![2],
                             spawner.clone()));

    let mut config0 = app0.config().unwrap();
    let mut config1 = app1.config().unwrap();
    let mut report0 = app0.report();
    let mut report1 = app1.report();

    // Configure relays:
    // await!(config0.add_relay(named_relay_address(0))).unwrap();
    // await!(config1.add_relay(named_relay_address(1))).unwrap();

    // Configure index servers:
    // await!(config0.add_index_server(named_index_server_address(0))).unwrap();
    // await!(config1.add_index_server(named_index_server_address(1))).unwrap();

    // Wait some time:
    for i in 0 .. 0x100usize {
        await!(tick_sender.send(())).unwrap();
    }

    dbg!("Add node1 as a friend");
    // Node0: Add node1 as a friend:
    await!(config0.add_friend(node_public_key(1),
                              vec![relay_address(1)],
                              String::from("node1"),
                              100));

    dbg!("Add node0 as a friend");
    // Node1: Add node0 as a friend:
    await!(config1.add_friend(node_public_key(0),
                              vec![relay_address(0)],
                              String::from("node0"),
                              -100));

    // Node0: Wait until node1 is online:
    loop {
        dbg!("Node0 iter");
        await!(tick_sender.send(())).unwrap();
        let (node_report, _receiver) = await!(report0.incoming_reports()).unwrap();
        let friend_report = match node_report.funder_report.friends.get(&node_public_key(1)) {
            None => continue,
            Some(friend_report) => friend_report,
        };
        dbg!("Check friend online:");
        if friend_report.liveness.is_online() {
            break;
        }
    }

    unimplemented!();

}

#[test]
fn test_basic() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_basic(thread_pool.clone()));
}
