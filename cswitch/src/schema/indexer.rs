//! The message needed by Indexer.
//!
//! The following types are defined in `inner_messages`:
//!
//! - `NeighborsRoute`
//! - `FriendsRoute`
//! - `RequestNeighborsRoute`
//! - `ResponseNeighborsRoute`
//! - `RequestFriendsRoute`
//! - `ResponseFriendsRoute`

use std::io;

use crypto::dh::DhPublicKey;
use crypto::rand_values::RandValue;
use crypto::identity::{PublicKey, Signature};

use inner_messages::{
    IndexingProviderID,
    IndexingProviderStateHash,
    NeighborsRoute,
    FriendsRoute,
    RequestNeighborsRoute,
    ResponseNeighborsRoute,
    RequestFriendsRoute,
    ResponseFriendsRoute,
};

use indexer_capnp::*;

use bytes::Bytes;
use capnp::serialize_packed;

use super::{
    Schema,
    SchemaError,
    read_custom_u_int128,
    read_custom_u_int256,
    write_custom_u_int128,
    write_custom_u_int256,
    read_public_key,
    write_public_key,
    read_signature,
    write_signature,
    read_rand_value,
    write_rand_value,
};

impl<'a> Schema<'a> for NeighborsRoute {
    type Reader = neighbors_route::Reader<'a>;
    type Writer = neighbors_route::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let public_keys_reader = from.get_public_keys()?;

        let mut public_keys = Vec::with_capacity(public_keys_reader.len() as usize);

        for public_key_reader in public_keys_reader.iter() {
            public_keys.push(read_public_key(&public_key_reader)?);
        }

        Ok(NeighborsRoute { public_keys })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError>
    {
        let mut public_keys_writer = to.borrow().init_public_keys(self.public_keys.len() as u32);

        for (idx, ref_public_key) in self.public_keys.iter().enumerate() {
            let mut public_key_writer = public_keys_writer.borrow().get(idx as u32);
            write_public_key(ref_public_key, &mut public_key_writer)?;
        }

        Ok(())
    }
}

impl<'a> Schema<'a> for FriendsRoute {
    type Reader = friends_route::Reader<'a>;
    type Writer = friends_route::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let public_keys_reader = from.get_public_keys()?;

        let mut public_keys = Vec::with_capacity(public_keys_reader.len() as usize);

        for public_key_reader in public_keys_reader.iter() {
            public_keys.push(read_public_key(&public_key_reader)?);
        }

        let capacity = from.get_capacity();

        Ok(FriendsRoute {
            public_keys,
            capacity,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        {
            let mut public_keys_writer = to.borrow().init_public_keys(self.public_keys.len() as u32);

            for (idx, ref_public_key) in self.public_keys.iter().enumerate() {
                let mut public_key_writer = public_keys_writer.borrow().get(idx as u32);
                write_public_key(ref_public_key, &mut public_key_writer)?;
            }
        }

        to.set_capacity(self.capacity);

        Ok(())
    }
}

impl<'a> Schema<'a> for RequestNeighborsRoute {
    type Reader = request_neighbors_route::Reader<'a>;
    type Writer = request_neighbors_route::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let source_node_public_key = read_public_key(&from.get_source_node_public_key()?)?;

        let destination_node_public_key = read_public_key(
            &from.get_destination_node_public_key()?
        )?;

        Ok(RequestNeighborsRoute {
            source_node_public_key,
            destination_node_public_key
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        {
            let mut source_node_public_key = to.borrow().init_source_node_public_key();
            write_public_key(&self.source_node_public_key, &mut source_node_public_key, )?;
        }

        {
            let mut destination_node_public_key = to.borrow().init_destination_node_public_key();
            write_public_key(
                &self.destination_node_public_key,
                &mut destination_node_public_key
            )?;
        }

        Ok(())
    }
}

impl<'a> Schema<'a> for ResponseNeighborsRoute {
    type Reader = response_neighbors_route::Reader<'a>;
    type Writer = response_neighbors_route::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        // Read the routes
        let routes_reader = from.get_routes()?;

        let mut routes = Vec::with_capacity(routes_reader.len() as usize);

        for neighbors_route_reader in routes_reader.iter() {
            routes.push(NeighborsRoute::read(&neighbors_route_reader)?);
        }

        // Read the destinationCommPublicKey
        let destination_comm_public_key = read_public_key(
            &from.get_destination_comm_public_key()?
        )?;

        // Read the destinationRecentTimesStamp
        let destination_recent_timestamp = read_rand_value(
            &from.get_destination_recent_timestamp()?
        )?;

        Ok(ResponseNeighborsRoute {
            routes,
            destination_comm_public_key,
            destination_recent_timestamp
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        // Write the routes
        {
            let mut routes_writer = to.borrow().init_routes(self.routes.len() as u32);

            for (idx, ref_neighbors_route) in self.routes.iter().enumerate() {
                let mut neighbors_route_writer = routes_writer.borrow().get(idx as u32);
                ref_neighbors_route.write(&mut neighbors_route_writer)?;
            }
        }
        // Write the destinationCommPublicKey
        {
            let mut destination_comm_public_key_writer =
                to.borrow().init_destination_comm_public_key();

            write_public_key(
                &self.destination_comm_public_key,
                &mut destination_comm_public_key_writer
            )?;
        }
        // Write the destinationRecentTimesStamp
        {
            let mut destination_recent_timestamp_writer =
                to.borrow().init_destination_recent_timestamp();

            write_rand_value(
                &self.destination_recent_timestamp,
                &mut destination_recent_timestamp_writer,
            )?;
        }

        Ok(())
    }
}

impl<'a> Schema<'a> for RequestFriendsRoute {
    type Reader = request_friends_route::Reader<'a>;
    type Writer = request_friends_route::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        use self::request_friends_route::route_type::Which::*;

        let route_type_reader = from.borrow().get_route_type();

        match route_type_reader.which()? {
            Direct(wrapped_direct_reader) => {
                let direct_reader = wrapped_direct_reader?;

                let source_node_public_key =
                    read_public_key(&direct_reader.get_source_node_public_key()?)?;
                let destination_node_public_key =
                    read_public_key(&direct_reader.get_destination_node_public_key()?)?;
                Ok(RequestFriendsRoute::Direct {
                    source_node_public_key,
                    destination_node_public_key
                })
            }
            LoopFromFriend(wrapped_loop_from_friend_reader) => {
                let loop_from_friend_reader = wrapped_loop_from_friend_reader?;

                let friend_public_key =
                    read_public_key(&loop_from_friend_reader.get_friend_public_key()?)?;
                Ok(RequestFriendsRoute::LoopFromFriend {
                    friend_public_key
                })
            }
            LoopToFriend(wrapped_loop_to_friend_reader) => {
                let loop_to_friend_reader = wrapped_loop_to_friend_reader?;

                let friend_public_key =
                    read_public_key(&loop_to_friend_reader.get_friend_public_key()?)?;
                Ok(RequestFriendsRoute::LoopToFriend {
                    friend_public_key
                })
            }
        }
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        let mut route_type_writer = to.borrow().init_route_type();

        match *self {
            RequestFriendsRoute::Direct {
                ref source_node_public_key,
                ref destination_node_public_key
            } => {
                let mut direct_writer = route_type_writer.borrow().init_direct();

                // Write the sourceNodePublicKey
                {
                    let mut source_node_public_key_writer
                    = direct_writer.borrow().init_source_node_public_key();
                    write_public_key(
                        source_node_public_key,
                        &mut source_node_public_key_writer
                    )?;
                }
                // Write the destinationNodePublicKey
                {
                    let mut destination_node_public_key_writer
                    = direct_writer.borrow().init_destination_node_public_key();
                    write_public_key(
                        destination_node_public_key,
                        &mut destination_node_public_key_writer
                    )?;
                }
            }
            RequestFriendsRoute::LoopFromFriend {
                ref friend_public_key
            } => {
                let mut loop_from_friend_writer =
                    route_type_writer.borrow().init_loop_from_friend();

                let mut friend_public_key_writer =
                    loop_from_friend_writer.borrow().init_friend_public_key();

                write_public_key(friend_public_key, &mut friend_public_key_writer)?;
            }
            RequestFriendsRoute::LoopToFriend {
                ref friend_public_key
            } => {
                let mut loop_to_friend_writer =
                    route_type_writer.borrow().init_loop_to_friend();

                let mut friend_public_key_writer =
                    loop_to_friend_writer.borrow().init_friend_public_key();

                write_public_key(friend_public_key, &mut friend_public_key_writer)?;
            }
        }

        Ok(())
    }
}

impl<'a> Schema<'a> for ResponseFriendsRoute {
    type Reader = response_friends_route::Reader<'a>;
    type Writer = response_friends_route::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        // Read the routes
        let routes_reader = from.get_routes()?;

        let mut routes = Vec::with_capacity(routes_reader.len() as usize);

        for friends_route_reader in routes_reader.iter() {
            routes.push(FriendsRoute::read(&friends_route_reader)?);
        }

        // Read the destinationCommPublicKey
        let destination_comm_public_key = read_public_key(
            &from.get_destination_comm_public_key()?
        )?;

        // Read the destinationRecentTimesStamp
        let destination_recent_timestamp = read_rand_value(
            &from.get_destination_recent_timestamp()?
        )?;

        Ok(ResponseFriendsRoute {
            routes,
            destination_comm_public_key,
            destination_recent_timestamp
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        // Write the routes
        {
            let mut routes_writer = to.borrow().init_routes(self.routes.len() as u32);

            for (idx, ref_neighbors_route) in self.routes.iter().enumerate() {
                let mut neighbors_route_writer = routes_writer.borrow().get(idx as u32);
                ref_neighbors_route.write(&mut neighbors_route_writer)?;
            }
        }
        // Write the destinationCommPublicKey
        {
            let mut destination_comm_public_key_writer =
                to.borrow().init_destination_comm_public_key();

            write_public_key(
                &self.destination_comm_public_key,
                &mut destination_comm_public_key_writer
            )?;
        }
        // Write the destinationRecentTimesStamp
        {
            let mut destination_recent_timestamp_writer =
                to.borrow().init_destination_recent_timestamp();

            write_rand_value(
                &self.destination_recent_timestamp,
                &mut destination_recent_timestamp_writer,
            )?;
        }

        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RequestUpdateState {
    indexing_provider_id: IndexingProviderID,
    indexing_provider_states_chain: Vec<ChainLink>,
}

impl<'a> Schema<'a> for RequestUpdateState {
    type Reader = request_update_state::Reader<'a>;
    type Writer = request_update_state::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        // Read the indexingProviderID
        let indexing_provider_id_reader = from.get_indexing_provider_id()?;
        let indexing_provider_id = IndexingProviderID::from_bytes(
            &read_custom_u_int128(&indexing_provider_id_reader)?
        ).map_err(|_| SchemaError::Invalid)?;

        // Read the indexingProviderStatesChain
        let indexing_provider_states_chain_reader = from.get_indexing_provider_states_chain()?;

        let mut indexing_provider_states_chain = Vec::with_capacity(
            indexing_provider_states_chain_reader.len() as usize
        );

        for chain_link_reader in indexing_provider_states_chain_reader.iter() {
            indexing_provider_states_chain.push(ChainLink::read(&chain_link_reader)?);
        }

        Ok(RequestUpdateState {
            indexing_provider_id,
            indexing_provider_states_chain
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        // Write the indexingProviderID
        {
            let mut indexing_provider_id_writer = to.borrow().init_indexing_provider_id();

            write_custom_u_int128(&self.indexing_provider_id, &mut indexing_provider_id_writer)?;
        }

        // Writer the indexingProviderStatesChain
        {
            let mut indexing_provider_states_chain = to.borrow().init_indexing_provider_states_chain(
                self.indexing_provider_states_chain.len() as u32
            );

            for (idx, ref_chain_link) in self.indexing_provider_states_chain.iter().enumerate() {
                let mut chain_link_writer = indexing_provider_states_chain.borrow().get(idx as u32);
                ref_chain_link.write(&mut chain_link_writer)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ResponseUpdateState {
    state_hash: IndexingProviderStateHash
}

impl<'a> Schema<'a> for ResponseUpdateState {
    type Reader = response_update_state::Reader<'a>;
    type Writer = response_update_state::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let state_hash_reader = from.get_state_hash()?;

        let state_hash = IndexingProviderStateHash::from_bytes(
            &read_custom_u_int256(&state_hash_reader)?
        ).map_err(|_| SchemaError::Invalid)?;

        Ok(ResponseUpdateState { state_hash })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        let mut state_hash_writer = to.borrow().init_state_hash();

        write_custom_u_int256(&self.state_hash, &mut state_hash_writer)?;

        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RoutesToIndexer {
    routes: Vec<NeighborsRoute>,
}

impl<'a> Schema<'a> for RoutesToIndexer {
    type Reader = routes_to_indexer::Reader<'a>;
    type Writer = routes_to_indexer::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        // Read the routes
        let routes_reader = from.get_routes()?;

        let mut routes = Vec::with_capacity(routes_reader.len() as usize);

        for neighbors_route_reader in routes_reader.iter() {
            routes.push(NeighborsRoute::read(&neighbors_route_reader)?);
        }

        Ok(RoutesToIndexer { routes })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        // Write the routes
        let mut routes = to.borrow().init_routes(self.routes.len() as u32);

        for (idx, ref_neighbors_route) in self.routes.iter().enumerate() {
            let mut neighbors_route_writer = routes.borrow().get(idx as u32);
            ref_neighbors_route.write(&mut neighbors_route_writer)?;
        }

        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ChainLink {
    previous_state_hash: IndexingProviderStateHash,
    new_owners_public_keys: Vec<PublicKey>,
    new_indexers_public_keys: Vec<PublicKey>,
    signatures_by_old_owners: Vec<Signature>
}

impl<'a> Schema<'a> for ChainLink {
    type Reader = chain_link::Reader<'a>;
    type Writer = chain_link::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        // Read the previousStateHash
        let previous_state_hash_reader = from.get_previous_state_hash()?;
        let previous_state_hash = IndexingProviderStateHash::from_bytes(
            &read_custom_u_int256(&previous_state_hash_reader)?
        ).map_err(|_| SchemaError::Invalid)?;

        // Read the newOwnersPublicKeys
        let new_owners_public_keys_reader = from.get_new_owners_public_keys()?;

        let mut new_owners_public_keys = Vec::with_capacity(
            new_owners_public_keys_reader.len() as usize
        );

        for public_key_reader in new_owners_public_keys_reader.iter() {
            new_owners_public_keys.push(read_public_key(&public_key_reader)?);
        }

        // Read the newOwnersPublicKeys
        let new_indexers_public_keys_reader = from.get_new_indexers_public_keys()?;

        let mut new_indexers_public_keys = Vec::with_capacity(
            new_indexers_public_keys_reader.len() as usize
        );

        for public_key_reader in new_indexers_public_keys_reader.iter() {
            new_indexers_public_keys.push(read_public_key(&public_key_reader)?);
        }

        // Read the signaturesByOldOwners
        let signatures_by_old_owners_reader = from.get_signatures_by_old_owners()?;

        let mut signatures_by_old_owners = Vec::with_capacity(
            signatures_by_old_owners_reader.len() as usize
        );

        for signature_reader in signatures_by_old_owners_reader.iter() {
            signatures_by_old_owners.push(read_signature(&signature_reader)?);
        }

        Ok(ChainLink {
            previous_state_hash,
            new_owners_public_keys,
            new_indexers_public_keys,
            signatures_by_old_owners
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        // Write the previousStateHash
        {
            let mut previous_state_hash = to.borrow().init_previous_state_hash();
            write_custom_u_int256(&self.previous_state_hash, &mut previous_state_hash)?;
        }
        // Write the newOwnersPublicKeys
        {
            let mut link_new_owners_public_public_keys_writer = to.borrow().init_new_owners_public_keys(
                self.new_owners_public_keys.len() as u32
            );

            for (idx, ref_public_key) in self.new_owners_public_keys.iter().enumerate() {
                let mut public_key_writer = link_new_owners_public_public_keys_writer.borrow().get(idx as u32);
                write_public_key(ref_public_key, &mut public_key_writer)?;
            }
        }
        // Write the newIndexersPublicKeys
        {
            let mut new_indexers_public_keys = to.borrow().init_new_indexers_public_keys(
                self.new_indexers_public_keys.len() as u32
            );

            for (idx, ref_public_key) in self.new_indexers_public_keys.iter().enumerate() {
                let mut public_key_writer = new_indexers_public_keys.borrow().get(idx as u32);
                write_public_key(ref_public_key, &mut public_key_writer)?;
            }
        }
        // Write the signaturesByOldOwners
        {
            let mut signatures_by_old_owners = to.borrow().init_signatures_by_old_owners(
                self.signatures_by_old_owners.len() as u32
            );

            for (idx, ref_signature) in self.signatures_by_old_owners.iter().enumerate() {
                let mut signature_writer = signatures_by_old_owners.borrow().get(idx as u32);
                write_signature(ref_signature, &mut signature_writer)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ConnectedFriend {
    send_capacity: u128,
    recv_capacity: u128,
    public_key: PublicKey,
}

// XXX: Don't forget to add test
impl<'a> Schema<'a> for ConnectedFriend {
    type Reader = connected_friend::Reader<'a>;
    type Writer = connected_friend::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        unimplemented!()
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        unimplemented!()
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ResponseIndexerInfo {
    connected_neighbors_list: Vec<PublicKey>,
    neighbors_comm_public_key: DhPublicKey,
    neighbors_recent_timestamp: RandValue,
    friends_comm_public_key: DhPublicKey,
    friends_recent_timestamp: RandValue,
}

// XXX: Don't forget to add test
impl<'a> Schema<'a> for ResponseIndexerInfo {
    type Reader = response_indexer_info::Reader<'a>;
    type Writer = response_indexer_info::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        unimplemented!()
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        unimplemented!()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crypto::identity::{PUBLIC_KEY_LEN, SIGNATURE_LEN};
    use inner_messages::{INDEXING_PROVIDER_STATE_HASH_LEN, INDEXING_PROVIDER_ID_LEN};

    use rand::random;

    const MAX_NUM: usize = 512;

    #[inline]
    fn create_dummy_public_key() -> PublicKey {
        let fixed_byte = random::<u8>();
        PublicKey::from_bytes(&[fixed_byte; PUBLIC_KEY_LEN]).unwrap()
    }

    #[inline]
    fn create_dummy_public_keys() -> Vec<PublicKey> {
        let num_keys = random::<usize>() % MAX_NUM + 1;

        (0..num_keys).map(|_| create_dummy_public_key()).collect::<Vec<_>>()
    }

    #[inline]
    fn create_dummy_signatures() -> Vec<Signature> {
        let num_signatures = random::<usize>() % MAX_NUM + 1;

        let mut signatures = (0..num_signatures).map(|_| {
            let fixed_byte = random::<u8>();
            Signature::from_bytes(&[fixed_byte; SIGNATURE_LEN]).unwrap()
        }).collect::<Vec<_>>();

        signatures
    }

    #[inline]
    fn create_dummy_neighbors_route() -> NeighborsRoute {
        NeighborsRoute {
            public_keys: create_dummy_public_keys()
        }
    }

    #[inline]
    fn create_dummy_chain_link() -> ChainLink {
        let fixed_byte = random::<u8>();
        let previous_state_hash = IndexingProviderStateHash::from_bytes(
            &[fixed_byte; INDEXING_PROVIDER_STATE_HASH_LEN]
        ).unwrap();

        let new_owners_public_keys = create_dummy_public_keys();
        let new_indexers_public_keys = create_dummy_public_keys();
        let signatures_by_old_owners = create_dummy_signatures();

        ChainLink {
            previous_state_hash,
            new_owners_public_keys,
            new_indexers_public_keys,
            signatures_by_old_owners,
        }
    }

    #[test]
    fn test_neighbors_route() {
        let in_neighbors_route = create_dummy_neighbors_route();

        let serialize_message = in_neighbors_route.encode().unwrap();

        let out_neighbors_route = NeighborsRoute::decode(serialize_message).unwrap();

        assert_eq!(in_neighbors_route, out_neighbors_route);
    }

    #[test]
    fn test_chain_link() {
        let in_chain_link = create_dummy_chain_link();

        let serialize_message = in_chain_link.encode().unwrap();

        let out_chain_link = ChainLink::decode(serialize_message).unwrap();

        assert_eq!(in_chain_link, out_chain_link);
    }

    #[test]
    fn test_request_update_state() {
        let fixed_byte = random::<u8>();
        let indexing_provider_id = IndexingProviderID::from_bytes(
            &[fixed_byte; INDEXING_PROVIDER_ID_LEN]
        ).unwrap();

        let mut indexing_provider_states_chain = Vec::new();
        for _ in 0..MAX_NUM {
            indexing_provider_states_chain.push(create_dummy_chain_link());
        }

        let in_request_update_state = RequestUpdateState {
            indexing_provider_id,
            indexing_provider_states_chain,
        };

        let serialize_message = in_request_update_state.encode().unwrap();

        let out_request_update_state = RequestUpdateState::decode(serialize_message).unwrap();

        assert_eq!(in_request_update_state, out_request_update_state);
    }

    #[test]
    fn test_response_update_state() {
        let fixed_byte = random::<u8>();
        let state_hash = IndexingProviderStateHash::from_bytes(
            &[fixed_byte; INDEXING_PROVIDER_STATE_HASH_LEN]
        ).unwrap();

        let in_response_update_state = ResponseUpdateState { state_hash };

        let serialize_message = in_response_update_state.encode().unwrap();

        let out_response_update_state = ResponseUpdateState::decode(serialize_message).unwrap();

        assert_eq!(in_response_update_state, out_response_update_state);
    }

    #[test]
    fn test_routes_to_indexer() {
        let mut routes = Vec::new();
        for _ in 0..MAX_NUM {
            routes.push(create_dummy_neighbors_route());
        }

        let in_routes_to_indexer = RoutesToIndexer { routes };

        let serialize_message = in_routes_to_indexer.encode().unwrap();

        let out_routes_to_indexer = RoutesToIndexer::decode(serialize_message).unwrap();

        assert_eq!(in_routes_to_indexer, out_routes_to_indexer);
    }

    #[test]
    fn test_request_neighbors_route() {
        let source_node_public_key = create_dummy_public_key();
        let destination_node_public_key = create_dummy_public_key();

        let in_request_neighbors_route = RequestNeighborsRoute {
            source_node_public_key,
            destination_node_public_key
        };

        let serialize_message = in_request_neighbors_route.encode().unwrap();

        let out_request_neighbors_route = RequestNeighborsRoute::decode(serialize_message).unwrap();

        assert_eq!(in_request_neighbors_route, out_request_neighbors_route);
    }

    #[test]
    fn test_request_friends_route() {
        let in_request_friends_route_direct = RequestFriendsRoute::Direct {
            source_node_public_key: create_dummy_public_key(),
            destination_node_public_key: create_dummy_public_key(),
        };

        let serialized_message = in_request_friends_route_direct.encode().unwrap();

        let out_request_friends_route_direct =
            RequestFriendsRoute::decode(serialized_message).unwrap();

        assert_eq!(in_request_friends_route_direct, out_request_friends_route_direct);

        let in_request_friends_route_loop_from_friend = RequestFriendsRoute::LoopFromFriend {
            friend_public_key: create_dummy_public_key(),
        };

        let serialized_message = in_request_friends_route_loop_from_friend.encode().unwrap();

        let out_request_friends_route_loop_from_friend =
            RequestFriendsRoute::decode(serialized_message).unwrap();

        assert_eq!(
            in_request_friends_route_loop_from_friend,
            out_request_friends_route_loop_from_friend
        );

        let in_request_friends_route_loop_to_friend = RequestFriendsRoute::LoopToFriend {
            friend_public_key: create_dummy_public_key(),
        };

        let serialized_message = in_request_friends_route_loop_to_friend.encode().unwrap();

        let out_request_friends_route_loop_to_friend =
            RequestFriendsRoute::decode(serialized_message).unwrap();

        assert_eq!(
            in_request_friends_route_loop_to_friend,
            out_request_friends_route_loop_to_friend
        );
    }
}
