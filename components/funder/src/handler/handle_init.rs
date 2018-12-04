use std::fmt::Debug;
use crypto::crypto_rand::CryptoRandom;

use crate::handler::MutableFunderHandler;
use crate::types::{ChannelerConfig, FriendStatus, 
    FunderOutgoingComm, FunderOutgoingControl};

use crate::report::create_report;


#[allow(unused)]
impl<A,R> MutableFunderHandler<A,R> 
where
    A: Clone + Debug + PartialEq + Eq + 'static,
    R: CryptoRandom,
{

    pub fn handle_init(&mut self) {
        let mut enabled_friends = Vec::new();
        for (friend_public_key, friend) in &self.state.friends {
            match friend.status {
                FriendStatus::Enable => {
                    enabled_friends.push((friend.remote_public_key.clone(),
                        friend.remote_address.clone()));
                },
                FriendStatus::Disable => continue,
            };
        }

        // Send a report of the current FunderState:
        // This is a base report. Later reports are differential, and should be built on this base
        // report.
        let report = create_report(&self.state, &self.ephemeral);
        self.add_outgoing_control(FunderOutgoingControl::Report(report));

        for enabled_friend in enabled_friends {
            // Notify Channeler:
            let channeler_config = ChannelerConfig::AddFriend(enabled_friend);
            self.add_outgoing_comm(FunderOutgoingComm::ChannelerConfig(channeler_config));
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    use crate::handler::gen_mutable;
    use crate::state::{FunderState, FunderMutation};
    use crate::types::AddFriend;
    use crate::ephemeral::Ephemeral;
    use crate::friend::FriendMutation;

    use futures::executor::ThreadPool;
    use futures::{future, FutureExt};
    use futures::task::SpawnExt;
    use identity::{create_identity, IdentityClient};

    use crypto::test_utils::DummyRandom;
    use crypto::identity::{SoftwareEd25519Identity,
                            generate_pkcs8_key_pair, PUBLIC_KEY_LEN,
                            PublicKey};
    use crypto::crypto_rand::RngContainer;



    async fn task_handle_init_basic(identity_client: IdentityClient) {

        let local_pk = await!(identity_client.request_public_key()).unwrap();
        let pk_b = PublicKey::from(&[0xbb; PUBLIC_KEY_LEN]);

        let mut state = FunderState::new(&local_pk, Some(&1337u32));

        // Add a remote friend:
        let add_friend = AddFriend {
            friend_public_key: pk_b.clone(),
            address: 3u32,
            name: "pk_b".into(),
            balance: 0i128,
        };
        let f_mutation = FunderMutation::AddFriend(add_friend);
        state.mutate(&f_mutation);

        // Enable the remote friend:
        let friend_mutation = FriendMutation::SetStatus(FriendStatus::Enable);
        let funder_mutation = FunderMutation::FriendMutation((pk_b.clone(), friend_mutation));
        state.mutate(&funder_mutation);

        let ephemeral = Ephemeral::new(&state);
        let rng = DummyRandom::new(&[2u8]);

        let mut mutable_funder_handler = gen_mutable(identity_client,
                    RngContainer::new(rng),
                    &state,
                    &ephemeral);

        mutable_funder_handler.handle_init();

        let mut funder_handler_output = mutable_funder_handler.done();
        assert!(funder_handler_output.funder_mutations.is_empty());
        assert_eq!(funder_handler_output.outgoing_control.len(), 1);
        assert_eq!(funder_handler_output.outgoing_comms.len(),1);
        let out_comm = funder_handler_output.outgoing_comms.pop().unwrap();

        let channeler_config = match out_comm {
            FunderOutgoingComm::ChannelerConfig(channeler_config) => channeler_config,
            _ => unreachable!(),
        };
        match channeler_config {
            ChannelerConfig::AddFriend((pk, addr)) => {
                assert_eq!(addr, 3u32);
                assert_eq!(pk, pk_b);
            },
            _ => unreachable!(),
        };
    }

    #[test]
    fn test_handle_init_basic() {
        // Start identity service:
        let mut thread_pool = ThreadPool::new().unwrap();

        let rng = DummyRandom::new(&[1u8]);
        let pkcs8 = generate_pkcs8_key_pair(&rng);
        let identity = SoftwareEd25519Identity::from_pkcs8(&pkcs8).unwrap();
        let (requests_sender, identity_server) = create_identity(identity);
        let identity_client = IdentityClient::new(requests_sender);
        thread_pool.spawn(identity_server.then(|_| future::ready(()))).unwrap();

        thread_pool.run(task_handle_init_basic(identity_client));
    }
}
