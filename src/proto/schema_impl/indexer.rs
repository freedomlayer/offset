use std::io;

use bytes::Bytes;
use capnp::serialize_packed;

include_schema!(indexer_capnp, "indexer_capnp");

use proto::{Schema, SchemaError};
use proto::indexer::{
    FriendsRouteWithCapacity,
    IndexerRoute,
    NeighborsRoute,
    RequestFriendsRoutes,
    RequestNeighborsRoutes,
    RequestUpdateState,
    ResponseFriendsRoutes,
    ResponseNeighborsRoutes,
    ResponseUpdateState,
    RoutesToIndexer,
    StateChainLink,
};

use super::common::{
    read_indexing_provider_id,
    read_indexing_provider_state_hash,
    read_public_key,
    read_public_key_list,
    read_signature,
    write_indexing_provider_id,
    write_indexing_provider_state_hash,
    write_public_key,
    write_public_key_list,
    write_signature,
};


impl<'a> Schema<'a> for NeighborsRoute {
    type Reader = neighbors_route::Reader<'a>;
    type Writer = neighbors_route::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let public_keys = read_public_key_list(&from.get_public_keys()?)?;

        Ok(NeighborsRoute { public_keys })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        write_public_key_list(
            &self.public_keys,
            &mut to.borrow().init_public_keys(self.public_keys.len() as u32),
        )?;

        Ok(())
    }
}

impl<'a> Schema<'a> for FriendsRouteWithCapacity {
    type Reader = friends_route_with_capacity::Reader<'a>;
    type Writer = friends_route_with_capacity::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let public_keys = read_public_key_list(&from.get_public_keys()?)?;

        let capacity = from.get_capacity();

        Ok(FriendsRouteWithCapacity {
            public_keys,
            capacity,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        write_public_key_list(
            &self.public_keys,
            &mut to.borrow().init_public_keys(self.public_keys.len() as u32),
        )?;

        to.set_capacity(self.capacity);

        Ok(())
    }
}

impl<'a> Schema<'a> for RequestNeighborsRoutes {
    type Reader = request_neighbors_route::Reader<'a>;
    type Writer = request_neighbors_route::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let source_node_public_key = read_public_key(&from.get_source_node_public_key()?)?;

        let destination_node_public_key =
            read_public_key(&from.get_destination_node_public_key()?)?;

        Ok(RequestNeighborsRoutes {
            source_node_public_key,
            destination_node_public_key,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        write_public_key(
            &self.source_node_public_key,
            &mut to.borrow().init_source_node_public_key(),
        )?;

        write_public_key(
            &self.destination_node_public_key,
            &mut to.borrow().init_destination_node_public_key(),
        )?;

        Ok(())
    }
}

impl<'a> Schema<'a> for ResponseNeighborsRoutes {
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

        Ok(ResponseNeighborsRoutes { routes })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        // Write the routes
        {
            let mut routes_writer = to.borrow().init_routes(self.routes.len() as u32);

            for (idx, ref_neighbors_route) in self.routes.iter().enumerate() {
                ref_neighbors_route.write(&mut routes_writer.borrow().get(idx as u32))?;
            }
        }

        Ok(())
    }
}

impl<'a> Schema<'a> for RequestFriendsRoutes {
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
                Ok(RequestFriendsRoutes::Direct {
                    source_node_public_key,
                    destination_node_public_key,
                })
            }
            LoopFromFriend(wrapped_loop_from_friend_reader) => {
                let loop_from_friend_reader = wrapped_loop_from_friend_reader?;

                let friend_public_key =
                    read_public_key(&loop_from_friend_reader.get_friend_public_key()?)?;
                Ok(RequestFriendsRoutes::LoopFromFriend { friend_public_key })
            }
            LoopToFriend(wrapped_loop_to_friend_reader) => {
                let loop_to_friend_reader = wrapped_loop_to_friend_reader?;

                let friend_public_key =
                    read_public_key(&loop_to_friend_reader.get_friend_public_key()?)?;
                Ok(RequestFriendsRoutes::LoopToFriend { friend_public_key })
            }
        }
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        let mut route_type_writer = to.borrow().init_route_type();

        match *self {
            RequestFriendsRoutes::Direct {
                ref source_node_public_key,
                ref destination_node_public_key,
            } => {
                let mut direct_writer = route_type_writer.borrow().init_direct();

                // Write the sourceNodePublicKey
                write_public_key(
                    source_node_public_key,
                    &mut direct_writer.borrow().init_source_node_public_key(),
                )?;

                // Write the destinationNodePublicKey
                write_public_key(
                    destination_node_public_key,
                    &mut direct_writer.borrow().init_destination_node_public_key(),
                )?;
            }
            RequestFriendsRoutes::LoopFromFriend {
                ref friend_public_key,
            } => {
                let mut loop_from_friend_writer =
                    route_type_writer.borrow().init_loop_from_friend();

                let mut friend_public_key_writer =
                    loop_from_friend_writer.borrow().init_friend_public_key();

                write_public_key(friend_public_key, &mut friend_public_key_writer)?;
            }
            RequestFriendsRoutes::LoopToFriend {
                ref friend_public_key,
            } => {
                let mut loop_to_friend_writer = route_type_writer.borrow().init_loop_to_friend();

                let mut friend_public_key_writer =
                    loop_to_friend_writer.borrow().init_friend_public_key();

                write_public_key(friend_public_key, &mut friend_public_key_writer)?;
            }
        }

        Ok(())
    }
}

impl<'a> Schema<'a> for ResponseFriendsRoutes {
    type Reader = response_friends_route::Reader<'a>;
    type Writer = response_friends_route::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        // Read the routes
        let routes_reader = from.get_routes()?;

        let mut routes = Vec::with_capacity(routes_reader.len() as usize);

        for friends_route_reader in routes_reader.iter() {
            routes.push(FriendsRouteWithCapacity::read(&friends_route_reader)?);
        }

        Ok(ResponseFriendsRoutes { routes })
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

        Ok(())
    }
}

impl<'a> Schema<'a> for StateChainLink {
    type Reader = chain_link::Reader<'a>;
    type Writer = chain_link::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        // Read the previousStateHash
        let previous_state_hash =
            read_indexing_provider_state_hash(&from.get_previous_state_hash()?)?;

        // Read the newOwnersPublicKeys
        let new_owners_public_keys = read_public_key_list(&from.get_new_owners_public_keys()?)?;

        // Read the newOwnersPublicKeys
        let new_indexers_public_keys = read_public_key_list(&from.get_new_indexers_public_keys()?)?;

        // Read the signaturesByOldOwners
        let signatures_by_old_owners_reader = from.get_signatures_by_old_owners()?;

        let mut signatures_by_old_owners =
            Vec::with_capacity(signatures_by_old_owners_reader.len() as usize);

        for signature_reader in signatures_by_old_owners_reader.iter() {
            signatures_by_old_owners.push(read_signature(&signature_reader)?);
        }

        Ok(StateChainLink {
            previous_state_hash,
            new_owners_public_keys,
            new_indexers_public_keys,
            signatures_by_old_owners,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        // Write the previousStateHash
        write_indexing_provider_state_hash(
            &self.previous_state_hash,
            &mut to.borrow().init_previous_state_hash(),
        )?;
        // Write the newOwnersPublicKeys
        {
            let mut new_owners_public_keys_writer = to.borrow()
                .init_new_owners_public_keys(self.new_owners_public_keys.len() as u32);

            write_public_key_list(
                &self.new_owners_public_keys,
                &mut new_owners_public_keys_writer,
            )?;
        }
        // Write the newIndexersPublicKeys
        {
            let mut new_indexers_public_keys_writer = to.borrow()
                .init_new_indexers_public_keys(self.new_indexers_public_keys.len() as u32);

            write_public_key_list(
                &self.new_indexers_public_keys,
                &mut new_indexers_public_keys_writer,
            )?;
        }
        // Write the signaturesByOldOwners
        {
            let mut signatures_by_old_owners = to.borrow()
                .init_signatures_by_old_owners(self.signatures_by_old_owners.len() as u32);

            for (idx, ref_signature) in self.signatures_by_old_owners.iter().enumerate() {
                write_signature(
                    ref_signature,
                    &mut signatures_by_old_owners.borrow().get(idx as u32),
                )?;
            }
        }

        Ok(())
    }
}

impl<'a> Schema<'a> for RequestUpdateState {
    type Reader = request_update_state::Reader<'a>;
    type Writer = request_update_state::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        // Read the indexingProviderId
        let indexing_provider_id = read_indexing_provider_id(&from.get_indexing_provider_id()?)?;

        // Read the indexingProviderStatesChain
        let indexing_provider_states_chain_reader = from.get_indexing_provider_states_chain()?;

        let mut indexing_provider_states_chain =
            Vec::with_capacity(indexing_provider_states_chain_reader.len() as usize);

        for chain_link_reader in indexing_provider_states_chain_reader.iter() {
            indexing_provider_states_chain.push(StateChainLink::read(&chain_link_reader)?);
        }

        Ok(RequestUpdateState {
            indexing_provider_id,
            indexing_provider_states_chain,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        // Write the indexingProviderId
        write_indexing_provider_id(
            &self.indexing_provider_id,
            &mut to.borrow().init_indexing_provider_id(),
        )?;

        // Writer the indexingProviderStatesChain
        {
            let mut indexing_provider_states_chain = to.borrow()
                .init_indexing_provider_states_chain(self.indexing_provider_states_chain.len()
                    as u32);

            for (idx, ref_chain_link) in self.indexing_provider_states_chain.iter().enumerate() {
                let mut chain_link_writer = indexing_provider_states_chain.borrow().get(idx as u32);
                ref_chain_link.write(&mut chain_link_writer)?;
            }
        }

        Ok(())
    }
}

impl<'a> Schema<'a> for ResponseUpdateState {
    type Reader = response_update_state::Reader<'a>;
    type Writer = response_update_state::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        // Read the stateHash
        let state_hash = read_indexing_provider_state_hash(&from.get_state_hash()?)?;

        Ok(ResponseUpdateState { state_hash })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        write_indexing_provider_state_hash(&self.state_hash, &mut to.borrow().init_state_hash())?;

        Ok(())
    }
}

impl<'a> Schema<'a> for IndexerRoute {
    type Reader = indexer_route::Reader<'a>;
    type Writer = indexer_route::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        let neighbors_route = NeighborsRoute::read(&from.get_neighbors_route()?)?;

        Ok(IndexerRoute {
            neighbors_route,
            app_port: from.get_app_port(),
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        self.neighbors_route
            .write(&mut to.borrow().init_neighbors_route())?;

        to.set_app_port(self.app_port);

        Ok(())
    }
}

impl<'a> Schema<'a> for RoutesToIndexer {
    type Reader = routes_to_indexers::Reader<'a>;
    type Writer = routes_to_indexers::Builder<'a>;

    inject_default_impl!();

    fn read(from: &Self::Reader) -> Result<Self, SchemaError> {
        // Read the indexingProviderId
        let indexing_provider_id = read_indexing_provider_id(&from.get_indexing_provider_id()?)?;

        // Read the routes
        let routes_reader = from.get_routes()?;

        let mut routes = Vec::with_capacity(routes_reader.len() as usize);

        for indexer_route_reader in routes_reader.iter() {
            routes.push(IndexerRoute::read(&indexer_route_reader)?);
        }

        // Write the requestPrice
        let request_price = from.get_request_price();

        Ok(RoutesToIndexer {
            indexing_provider_id,
            routes,
            request_price,
        })
    }

    fn write(&self, to: &mut Self::Writer) -> Result<(), SchemaError> {
        // Write the indexingProviderId
        write_indexing_provider_id(
            &self.indexing_provider_id,
            &mut to.borrow().init_indexing_provider_id(),
        )?;

        // Write the routes
        {
            let mut routes = to.borrow().init_routes(self.routes.len() as u32);

            for (idx, ref_indexer_route) in self.routes.iter().enumerate() {
                ref_indexer_route.write(&mut routes.borrow().get(idx as u32))?;
            }
        }

        // Write the requestPrice
        to.set_request_price(self.request_price);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use super::*;
    use rand::random;

    use crypto::identity::{PublicKey, Signature, PUBLIC_KEY_LEN, SIGNATURE_LEN};

    use proto::indexer::{IndexingProviderId, INDEXING_PROVIDER_ID_LEN,
                         IndexingProviderStateHash, INDEXING_PROVIDER_STATE_HASH_LEN};

    const MAX_NUM: usize = 512;
    const MAX_COUNT_FOR_TEST: i32 = 20;

    // TODO: Move the create_dummy_* functions to a appropriate place.

    // fn create_dummy_rand_value() -> RandValue {
    //     let fixed_byte = random::<u8>();
    //     RandValue::from_bytes(&[fixed_byte; RAND_VALUE_LEN]).unwrap()
    // }

    fn create_dummy_public_key() -> PublicKey {
        let fixed_byte = random::<u8>();
        PublicKey::from_bytes(&[fixed_byte; PUBLIC_KEY_LEN]).unwrap()
    }

    fn create_dummy_public_keys_list() -> Vec<PublicKey> {
        let num_keys = random::<usize>() % MAX_NUM + 1;

        (0..num_keys).map(|_| create_dummy_public_key()).collect()
    }

    // fn create_dummy_dh_public_key() -> DhPublicKey {
    //     let fixed_byte = random::<u8>();
    //     DhPublicKey::from_bytes(&[fixed_byte; DH_PUBLIC_KEY_LEN]).unwrap()
    // }

    fn create_dummy_signatures_list() -> Vec<Signature> {
        let num_signatures = random::<usize>() % MAX_NUM + 1;

        (0..num_signatures)
            .map(|_| {
                let fixed_byte = random::<u8>();
                Signature::from_bytes(&[fixed_byte; SIGNATURE_LEN]).unwrap()
            })
            .collect()
    }

    fn create_dummy_indexing_provider_id() -> IndexingProviderId {
        let fixed_byte = random::<u8>();
        IndexingProviderId::try_from(&[fixed_byte; INDEXING_PROVIDER_ID_LEN][..]).unwrap()
    }

    fn create_dummy_neighbors_route() -> NeighborsRoute {
        NeighborsRoute {
            public_keys: create_dummy_public_keys_list(),
        }
    }

    fn create_dummy_friends_route_with_capacity() -> FriendsRouteWithCapacity {
        FriendsRouteWithCapacity {
            public_keys: create_dummy_public_keys_list(),
            capacity: random::<u64>(),
        }
    }

    fn create_dummy_chain_link() -> StateChainLink {
        let fixed_byte = random::<u8>();
        let previous_state_hash = IndexingProviderStateHash::try_from(
            &[fixed_byte; INDEXING_PROVIDER_STATE_HASH_LEN][..],
        ).unwrap();

        let new_owners_public_keys = create_dummy_public_keys_list();
        let new_indexers_public_keys = create_dummy_public_keys_list();
        let signatures_by_old_owners = create_dummy_signatures_list();

        StateChainLink {
            previous_state_hash,
            new_owners_public_keys,
            new_indexers_public_keys,
            signatures_by_old_owners,
        }
    }

    fn create_dummy_indexer_route() -> IndexerRoute {
        IndexerRoute {
            neighbors_route: create_dummy_neighbors_route(),
            app_port: random::<u32>(),
        }
    }

    #[test]
    fn test_neighbors_route() {
        let in_neighbors_route = create_dummy_neighbors_route();

        test_encode_decode!(NeighborsRoute, in_neighbors_route);
    }

    #[test]
    fn test_friends_route() {
        let in_friends_route = create_dummy_friends_route_with_capacity();

        test_encode_decode!(FriendsRouteWithCapacity, in_friends_route);
    }

    #[test]
    fn test_request_neighbors_route() {
        let source_node_public_key = create_dummy_public_key();
        let destination_node_public_key = create_dummy_public_key();

        let in_request_neighbors_route = RequestNeighborsRoutes {
            source_node_public_key,
            destination_node_public_key,
        };

        test_encode_decode!(RequestNeighborsRoutes, in_request_neighbors_route);
    }

    #[test]
    fn test_response_neighbors_route() {
        let in_response_neighbors_route = ResponseNeighborsRoutes {
            routes: (0..MAX_COUNT_FOR_TEST)
                .map(|_| create_dummy_neighbors_route())
                .collect(),
        };

        test_encode_decode!(ResponseNeighborsRoutes, in_response_neighbors_route);
    }

    #[test]
    fn test_request_friends_route() {
        let in_request_friends_route_direct = RequestFriendsRoutes::Direct {
            source_node_public_key: create_dummy_public_key(),
            destination_node_public_key: create_dummy_public_key(),
        };

        let serialized_message = in_request_friends_route_direct.encode().unwrap();

        let out_request_friends_route_direct =
            RequestFriendsRoutes::decode(serialized_message).unwrap();

        assert_eq!(
            in_request_friends_route_direct,
            out_request_friends_route_direct
        );

        let in_request_friends_route_loop_from_friend = RequestFriendsRoutes::LoopFromFriend {
            friend_public_key: create_dummy_public_key(),
        };

        let serialized_message = in_request_friends_route_loop_from_friend.encode().unwrap();

        let out_request_friends_route_loop_from_friend =
            RequestFriendsRoutes::decode(serialized_message).unwrap();

        assert_eq!(
            in_request_friends_route_loop_from_friend,
            out_request_friends_route_loop_from_friend
        );

        let in_request_friends_route_loop_to_friend = RequestFriendsRoutes::LoopToFriend {
            friend_public_key: create_dummy_public_key(),
        };

        let serialized_message = in_request_friends_route_loop_to_friend.encode().unwrap();

        let out_request_friends_route_loop_to_friend =
            RequestFriendsRoutes::decode(serialized_message).unwrap();

        assert_eq!(
            in_request_friends_route_loop_to_friend,
            out_request_friends_route_loop_to_friend
        );
    }

    #[test]
    fn test_response_friends_route() {
        let in_response_friends_route = ResponseFriendsRoutes {
            routes: (0..MAX_COUNT_FOR_TEST).map(|_| create_dummy_friends_route_with_capacity()).collect(),
        };

        test_encode_decode!(ResponseFriendsRoutes, in_response_friends_route);
    }

    #[test]
    fn test_state_chain_link() {
        let in_state_chain_link = create_dummy_chain_link();

        test_encode_decode!(StateChainLink, in_state_chain_link);
    }

    #[test]
    fn test_request_update_state() {
        let indexing_provider_id = create_dummy_indexing_provider_id();
        let indexing_provider_states_chain = (0..MAX_COUNT_FOR_TEST)
            .map(|_| create_dummy_chain_link())
            .collect::<Vec<_>>();
        let in_request_update_state = RequestUpdateState {
            indexing_provider_id,
            indexing_provider_states_chain,
        };
        test_encode_decode!(RequestUpdateState, in_request_update_state);
    }

    #[test]
    fn test_response_update_state() {
        let fixed_byte = random::<u8>();
        let state_hash = IndexingProviderStateHash::try_from(
            &[fixed_byte; INDEXING_PROVIDER_STATE_HASH_LEN][..],
        ).unwrap();

        let in_response_update_state = ResponseUpdateState { state_hash };

        test_encode_decode!(ResponseUpdateState, in_response_update_state);
    }

    #[test]
    fn test_indexer_route() {
        let in_indexer_route = create_dummy_indexer_route();

        test_encode_decode!(IndexerRoute, in_indexer_route);
    }

    #[test]
    fn test_routes_to_indexer() {
        let in_routes_to_indexer = RoutesToIndexer {
            indexing_provider_id: create_dummy_indexing_provider_id(),
            routes: (0..MAX_COUNT_FOR_TEST)
                .map(|_| create_dummy_indexer_route())
                .collect::<Vec<_>>(),
            request_price: random::<u64>(),
        };

        test_encode_decode!(RoutesToIndexer, in_routes_to_indexer);
    }
}
