use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde::de::DeserializeOwned;

use futures::task::{Spawn, SpawnExt};
use futures::channel::mpsc;
use futures::{future, FutureExt, StreamExt, SinkExt};

use crypto::identity::{SoftwareEd25519Identity, generate_pkcs8_key_pair, 
    PublicKey};
use crypto::test_utils::DummyRandom;

use identity::{create_identity, IdentityClient};

use crate::state::{FunderState, FunderMutation};
use crate::funder::inner_funder_loop;
use crate::types::{FunderOutgoingComm, IncomingCommMessage, 
    ChannelerConfig, FunderOutgoingControl, IncomingControlMessage,
    IncomingLivenessMessage, ResponseReceived, AddFriend, FriendStatus,
    SetFriendStatus};
use crate::database::AtomicDb;
use crate::report::{FunderReport, FunderReportMutation};

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

pub struct NodeControl<A: Clone> {
    pub public_key: PublicKey,
    send_control: mpsc::Sender<IncomingControlMessage<A>>,
    recv_control: mpsc::Receiver<FunderOutgoingControl<A>>,
    pub report: FunderReport<A>,
}

#[derive(Debug)]
enum NodeRecv<A> {
    ReportMutations(Vec<FunderReportMutation<A>>),
    ResponseReceived(ResponseReceived),
}

impl<A: Clone> NodeControl<A> {
    pub async fn send(&mut self, msg: IncomingControlMessage<A>) -> Option<()> {
        await!(self.send_control.send(msg))
            .ok()
            .map(|_| ())
    }

    pub async fn recv(&mut self) -> Option<NodeRecv<A>> {
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

    pub async fn recv_until<'a, P: 'a>(&'a mut self, predicate: P)
    where
        P: Fn(&FunderReport<A>) -> bool,
    {
        while !predicate(&self.report) {
            match await!(self.recv()).unwrap() {
                NodeRecv::ReportMutations(_) => {},
                NodeRecv::ResponseReceived(_) => unreachable!(),
            };
        }
    }

    pub async fn recv_until_response(&mut self) -> Option<ResponseReceived> {
        loop {
            match await!(self.recv())? {
                NodeRecv::ReportMutations(_) => {},
                NodeRecv::ResponseReceived(response_received) => return Some(response_received),
            };
        }
    }

    pub async fn add_friend<'a>(&'a mut self, 
                         friend_public_key: &'a PublicKey,
                         address: A,
                         name: &'a str,
                         balance: i128) {

        let add_friend = AddFriend {
            friend_public_key: friend_public_key.clone(),
            address,
            name: name.into(),
            balance, 
        };
        await!(self.send(IncomingControlMessage::AddFriend(add_friend))).unwrap();
        let pred = |report: &FunderReport<_>| report.friends.contains_key(&friend_public_key);
        await!(self.recv_until(pred));
    }

    pub async fn set_friend_status<'a>(&'a mut self, 
                         friend_public_key: &'a PublicKey,
                         status: FriendStatus) {

        let set_friend_status = SetFriendStatus {
            friend_public_key: friend_public_key.clone(),
            status: status.clone(),
        };
        await!(self.send(IncomingControlMessage::SetFriendStatus(set_friend_status))).unwrap();
        let pred = |report: &FunderReport<_>| {
           match report.friends.get(&friend_public_key) {
               None => false,
               Some(friend) => friend.status == status,
           }
        };
        await!(self.recv_until(pred));
    }
}

/// Create a few node_controls, together with a router connecting them all.
/// This allows having a conversation between any two nodes.
pub async fn create_node_controls<A>(num_nodes: usize, 
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

