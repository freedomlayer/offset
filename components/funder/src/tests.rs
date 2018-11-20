use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde::de::DeserializeOwned;

use futures::channel::mpsc;
use futures::executor::ThreadPool;
use futures::task::{Spawn, SpawnExt};
use futures::{future, FutureExt, StreamExt, SinkExt};

use crypto::identity::{SoftwareEd25519Identity, generate_pkcs8_key_pair, 
    PublicKey};
use crypto::test_utils::DummyRandom;
use identity::{create_identity, IdentityClient};

use crate::state::{FunderState, FunderMutation};
use crate::funder::inner_funder_loop;
use crate::types::{FunderOutgoingComm, IncomingCommMessage, 
    ChannelerConfig, FunderOutgoingControl, IncomingControlMessage,
    IncomingLivenessMessage, AddFriend, ResponseReceived, FriendStatus,
    SetFriendStatus, RequestsStatus, SetRequestsStatus};
use crate::database::AtomicDb;
use crate::report::{FunderReport, FunderReportMutation, FriendLivenessReport};

// This is required to make sure the tests are not stuck.
//
// We could instead have CHANNEL_SIZE = 0 with some kind of (event_sender, event_receiver) pair, to make
// sure an asynchronous event was fully processed before continuing with the next one, but this
// approach makes tests difficult to write.
const CHANNEL_SIZE: usize = 64;

#[derive(Debug)]
struct Node {
    friends: HashSet<PublicKey>,
    comm_out: mpsc::Sender<IncomingCommMessage>,
}

#[derive(Debug)]
struct NewNode<A> {
    public_key: PublicKey,
    comm_in: mpsc::Receiver<FunderOutgoingComm<A>>,
    comm_out: mpsc::Sender<IncomingCommMessage>,
}

#[derive(Debug)]
enum RouterEvent<A> {
    NewNode(NewNode<A>),
    OutgoingComm((PublicKey, FunderOutgoingComm<A>)), // (src_public_key, outgoing_comm)
}

async fn router_handle_outgoing_comm<A: 'static>(nodes: &mut HashMap<PublicKey, Node>, 
                                        src_public_key: PublicKey,
                                        outgoing_comm: FunderOutgoingComm<A>) {
    match outgoing_comm {
        FunderOutgoingComm::FriendMessage((dest_public_key, friend_message)) => {
            let node = nodes.get_mut(&dest_public_key).unwrap();
            assert!(node.friends.contains(&src_public_key));
            let incoming_comm_message = IncomingCommMessage::Friend((src_public_key.clone(), friend_message));
            await!(node.comm_out.send(incoming_comm_message)).unwrap();
        },
        FunderOutgoingComm::ChannelerConfig(channeler_config) => {
            match channeler_config {
                ChannelerConfig::AddFriend((friend_public_key, _address)) => {
                    let node = nodes.get_mut(&src_public_key).unwrap();
                    assert!(node.friends.insert(friend_public_key.clone()));
                    let mut comm_out = node.comm_out.clone();

                    let remote_node = nodes.get(&friend_public_key).unwrap();
                    let mut remote_node_comm_out = remote_node.comm_out.clone();
                    if remote_node.friends.contains(&src_public_key) {
                        // If there is a match, notify both sides about online state:
                        let incoming_comm_message = IncomingCommMessage::Liveness(
                            IncomingLivenessMessage::Online(src_public_key.clone()));
                        await!(remote_node_comm_out.send(incoming_comm_message)).unwrap();

                        let incoming_comm_message = IncomingCommMessage::Liveness(
                            IncomingLivenessMessage::Online(friend_public_key.clone()));
                        await!(comm_out.send(incoming_comm_message)).unwrap();
                    }
                },
                ChannelerConfig::RemoveFriend(friend_public_key) => {
                    let node = nodes.get_mut(&src_public_key).unwrap();
                    assert!(node.friends.remove(&friend_public_key));
                    let mut comm_out = node.comm_out.clone();

                    if nodes.get(&friend_public_key).unwrap().friends.contains(&src_public_key) {
                        let incoming_comm_message = IncomingCommMessage::Liveness(
                            IncomingLivenessMessage::Offline(friend_public_key.clone()));
                        await!(comm_out.send(incoming_comm_message)).unwrap();
                    }
                },
            }
        },
    }
}

/// A future that forwards communication between nodes. Used for testing.
/// Simulates the Channeler interface
async fn router<A: Send + 'static + std::fmt::Debug>(incoming_new_node: mpsc::Receiver<NewNode<A>>, 
                                                     mut spawner: impl Spawn + Clone) {
    let mut nodes: HashMap<PublicKey, Node> = HashMap::new();
    let (comm_sender, comm_receiver) = mpsc::channel::<(PublicKey, FunderOutgoingComm<A>)>(0);

    let incoming_new_node = incoming_new_node
        .map(|new_node| RouterEvent::NewNode(new_node));
    let comm_receiver = comm_receiver
        .map(|tuple| RouterEvent::OutgoingComm(tuple));

    let mut events = incoming_new_node.select(comm_receiver);

    while let Some(event) = await!(events.next()) {
        match event {
            RouterEvent::NewNode(new_node) => {
                let NewNode {public_key, comm_in, comm_out} = new_node;
                nodes.insert(public_key.clone(), Node { friends: HashSet::new(), comm_out});

                let c_public_key = public_key.clone();
                let mut c_comm_sender = comm_sender.clone();
                let mut mapped_comm_in = comm_in.map(move |funder_outgoing_comm|
                                                 (c_public_key.clone(), funder_outgoing_comm));
                let fut = async move {
                    await!(c_comm_sender.send_all(&mut mapped_comm_in)).unwrap()
                };
                spawner.spawn(fut.then(|_| future::ready(()))).unwrap();
            },
            RouterEvent::OutgoingComm((src_public_key, outgoing_comm)) => {
                await!(router_handle_outgoing_comm(&mut nodes, src_public_key, outgoing_comm));
            }
        };
    }
}

struct MockDb<A: Clone> {
    state: FunderState<A>,
}

impl<A: Clone + std::fmt::Debug> MockDb<A> {
    fn new(state: FunderState<A>) -> MockDb<A> {
        MockDb { state }
    }
}

impl<A: Clone + 'static + std::fmt::Debug> AtomicDb for MockDb<A> {
    type State = FunderState<A>;
    type Mutation = FunderMutation<A>;
    type Error = ();

    fn get_state(&self) -> &FunderState<A> {
        &self.state
    }

    fn mutate(&mut self, mutations: Vec<FunderMutation<A>>) -> Result<(), ()> {
        for mutation in mutations {
            self.state.mutate(&mutation);
        }
        Ok(())
    }
}

struct NodeControl<A: Clone> {
    pub public_key: PublicKey,
    send_control: mpsc::Sender<IncomingControlMessage<A>>,
    recv_control: mpsc::Receiver<FunderOutgoingControl<A>>,
    report: FunderReport<A>,
}

#[derive(Debug)]
enum NodeRecv<A> {
    ReportMutations(Vec<FunderReportMutation<A>>),
    ResponseReceived(ResponseReceived),
}

impl<A: Clone> NodeControl<A> {
    async fn send(&mut self, msg: IncomingControlMessage<A>) -> Option<()> {
        await!(self.send_control.send(msg))
            .ok()
            .map(|_| ())
    }

    async fn recv(&mut self) -> Option<NodeRecv<A>> {
        let funder_outgoing_control = await!(self.recv_control.next())?;
        match funder_outgoing_control {
            FunderOutgoingControl::Report(_) => unreachable!(),
            FunderOutgoingControl::ReportMutations(mutations) => {
                for mutation in &mutations {
                    self.report.mutate(&mutation).unwrap();
                }
                Some(NodeRecv::ReportMutations(mutations))
            },
            FunderOutgoingControl::ResponseReceived(response_received) =>
                Some(NodeRecv::ResponseReceived(response_received)),
        }
    }
}

/// Create a few node_controls, together with a router connecting them all.
/// This allows having a conversation between any two nodes.
async fn create_node_controls<A>(num_nodes: usize, 
                              mut spawner: impl Spawn + Clone + Send + 'static) 
                                -> Vec<NodeControl<A>>
where 
    A: Serialize + DeserializeOwned + Send + Sync + Clone + std::fmt::Debug + 'static,
{

    let (mut send_new_node, recv_new_node) = mpsc::channel::<NewNode<A>>(0);
    spawner.spawn(router(recv_new_node, spawner.clone())).unwrap();

    // Avoid problems with casting to u8:
    assert!(num_nodes < 256);
    let mut node_controls: Vec<NodeControl<A>> = Vec::new();

    for i in 0 .. num_nodes {
        let rng = DummyRandom::new(&[i as u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng);
        let identity1 = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender, identity_server) = create_identity(identity1);
        let identity_client = IdentityClient::new(requests_sender);
        spawner.spawn(identity_server.then(|_| future::ready(()))).unwrap();


        let public_key = await!(identity_client.request_public_key()).unwrap();
        let funder_state = FunderState::new(&public_key);

        let mock_db = MockDb::<A>::new(funder_state);

        let (send_control, incoming_control) = mpsc::channel(CHANNEL_SIZE);
        let (control_sender, mut recv_control) = mpsc::channel(CHANNEL_SIZE);

        let (send_comm, incoming_comm) = mpsc::channel(CHANNEL_SIZE);
        let (comm_sender, recv_comm) = mpsc::channel(CHANNEL_SIZE);

        let funder_fut = inner_funder_loop(
            identity_client.clone(),
            DummyRandom::new(&[i as u8]),
            incoming_control,
            incoming_comm,
            control_sender,
            comm_sender,
            mock_db,
            None);

        spawner.spawn(funder_fut.then(|_| future::ready(()))).unwrap();

        let base_report = match await!(recv_control.next()).unwrap() {
            FunderOutgoingControl::Report(report) => report,
            _ => unreachable!(),
        };

        let new_node = NewNode {
            public_key: public_key.clone(),
            comm_in: recv_comm,
            comm_out: send_comm,
        };
        await!(send_new_node.send(new_node)).unwrap();

        node_controls.push(NodeControl {
            public_key: await!(identity_client.request_public_key()).unwrap(),
            send_control,
            recv_control,
            report: base_report,
        });
    }
    node_controls
}

async fn task_funder_basic(spawner: impl Spawn + Clone + Send + 'static) {
    let num_nodes = 2;
    let mut node_controls = await!(create_node_controls(num_nodes, spawner));

    // --------------
    assert_eq!(node_controls[0].report.friends.len(), 0);
    let add_friend = AddFriend {
        friend_public_key: node_controls[1].public_key.clone(),
        address: 1u32,
        name: "node1".into(),
        balance: 8, 
    };
    await!(node_controls[0].send(IncomingControlMessage::AddFriend(add_friend))).unwrap();
    await!(node_controls[0].recv()).unwrap();
    assert_eq!(node_controls[0].report.friends.len(), 1);

    // --------------
    assert_eq!(node_controls[1].report.friends.len(), 0);
    let add_friend = AddFriend {
        friend_public_key: node_controls[0].public_key.clone(),
        address: 0u32,
        name: "node0".into(),
        balance: -8, 
    };

    await!(node_controls[1].send(IncomingControlMessage::AddFriend(add_friend))).unwrap();
    await!(node_controls[1].recv()).unwrap();

    assert_eq!(node_controls[1].report.friends.len(), 1);

    // --------------
    let set_friend_status = SetFriendStatus {
        friend_public_key: node_controls[1].public_key.clone(),
        status: FriendStatus::Enable,
    };
    await!(node_controls[0].send(IncomingControlMessage::SetFriendStatus(set_friend_status))).unwrap();
    await!(node_controls[0].recv()).unwrap();

    // --------------
    let set_friend_status = SetFriendStatus {
        friend_public_key: node_controls[0].public_key.clone(),
        status: FriendStatus::Enable,
    };
    await!(node_controls[1].send(IncomingControlMessage::SetFriendStatus(set_friend_status))).unwrap();
    await!(node_controls[1].recv()).unwrap();
    await!(node_controls[0].recv()).unwrap();
    await!(node_controls[1].recv()).unwrap();

    // Both sides should be online:
    let friend_report1 = node_controls[1].report.friends.get(&node_controls[0].public_key).unwrap();
    if let FriendLivenessReport::Online = friend_report1.liveness {
    } else {
        unreachable!();
    }

    let friend_report0 = node_controls[0].report.friends.get(&node_controls[1].public_key).unwrap();
    if let FriendLivenessReport::Online = friend_report0.liveness {
    } else {
        unreachable!();
    }

    // --------
    let set_requests_status = SetRequestsStatus {
        friend_public_key: node_controls[1].public_key.clone(),
        status: RequestsStatus::Open,
    };
    await!(node_controls[0].send(IncomingControlMessage::SetRequestsStatus(set_requests_status))).unwrap();

    // --------
    let set_requests_status = SetRequestsStatus {
        friend_public_key: node_controls[0].public_key.clone(),
        status: RequestsStatus::Open,
    };
    await!(node_controls[1].send(IncomingControlMessage::SetRequestsStatus(set_requests_status))).unwrap();

    await!(node_controls[0].recv()).unwrap();
    await!(node_controls[1].recv()).unwrap();
    await!(node_controls[0].recv()).unwrap();
    await!(node_controls[1].recv()).unwrap();

}

#[test]
fn test_funder_basic() {
    let mut thread_pool = ThreadPool::new().unwrap();
    thread_pool.run(task_funder_basic(thread_pool.clone()));
}
