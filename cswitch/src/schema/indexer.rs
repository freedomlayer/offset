use std::io;

use indexer_capnp::*;
use crypto::identity::{PublicKey, Signature};
//use crypto::dh::DhPublicKey;
use inner_messages::{IndexingProviderID, IndexingProviderStateHash};

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
    write_signature
};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct NeighborsRoute {
    public_keys: Vec<PublicKey>,
}

impl<'a, 'b> Schema<'a, 'b> for NeighborsRoute {
    type Reader = neighbors_route::Reader<'a>;
    type Writer = neighbors_route::Builder<'b>;

    inject_default_en_de_impl!();

    fn write(&self, to: &'a mut Self::Writer) -> Result<(), SchemaError> {
        let mut public_keys_writer = to.borrow().init_public_keys(self.public_keys.len() as u32);

        for (idx, ref_public_key) in self.public_keys.iter().enumerate() {
            let mut public_key_writer = public_keys_writer.borrow().get(idx as u32);
            write_public_key(ref_public_key, &mut public_key_writer)?;
        }

        Ok(())
    }

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
        let public_keys_reader = from.get_public_keys()?;

        let mut public_keys = Vec::with_capacity(public_keys_reader.len() as usize);

        for public_key_reader in public_keys_reader.iter() {
            public_keys.push(read_public_key(&public_key_reader)?);
        }

        Ok(NeighborsRoute { public_keys })
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ChainLink {
    previous_state_hash: IndexingProviderStateHash,
    new_owners_public_keys: Vec<PublicKey>,
    new_indexers_public_keys: Vec<PublicKey>,
    signatures_by_old_owners: Vec<Signature>,
}

impl<'a, 'b> Schema<'a, 'b> for ChainLink {
    type Reader = chain_link::Reader<'a>;
    type Writer = chain_link::Builder<'b>;

    inject_default_en_de_impl!();

    fn write(&self, to: &'a mut Self::Writer) -> Result<(), SchemaError> {
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

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
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
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RequestUpdateState {
    indexing_provider_id: IndexingProviderID,
    indexing_provider_states_chain: Vec<ChainLink>,
}

impl<'a, 'b> Schema<'a, 'b> for RequestUpdateState {
    type Reader = request_update_state::Reader<'a>;
    type Writer = request_update_state::Builder<'b>;

    inject_default_en_de_impl!();

    fn write(&self, to: &'a mut Self::Writer) -> Result<(), SchemaError> {
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

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
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
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ResponseUpdateState {
    state_hash: IndexingProviderStateHash
}

impl<'a, 'b> Schema<'a, 'b> for ResponseUpdateState {
    type Reader = response_update_state::Reader<'a>;
    type Writer = response_update_state::Builder<'b>;

    inject_default_en_de_impl!();

    fn write(&self, to: &'a mut Self::Writer) -> Result<(), SchemaError> {
        let mut state_hash_writer = to.borrow().init_state_hash();

        write_custom_u_int256(&self.state_hash, &mut state_hash_writer)?;

        Ok(())
    }

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
        let state_hash_reader = from.get_state_hash()?;

        let state_hash = IndexingProviderStateHash::from_bytes(
            &read_custom_u_int256(&state_hash_reader)?
        ).map_err(|_| SchemaError::Invalid)?;

        Ok(ResponseUpdateState { state_hash })
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RoutesToIndexer {
    routes: Vec<NeighborsRoute>,
}

impl <'a, 'b> Schema<'a, 'b> for RoutesToIndexer {
    type Reader = routes_to_indexer::Reader<'a>;
    type Writer = routes_to_indexer::Builder<'b>;

    inject_default_en_de_impl!();

    fn write(&self, to: &'a mut Self::Writer) -> Result<(), SchemaError> {
        // Write the routes
        let mut routes = to.borrow().init_routes(self.routes.len() as u32);

        for (idx, ref_neighbors_route) in self.routes.iter().enumerate() {
            let mut neighbors_route_writer = routes.borrow().get(idx as u32);
            ref_neighbors_route.write(&mut neighbors_route_writer)?;
        }

        Ok(())
    }

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
        // Read the routes
        let routes_reader = from.get_routes()?;

        let mut routes = Vec::with_capacity(routes_reader.len() as usize);

        for neighbors_route_reader in routes_reader.iter() {
            routes.push(NeighborsRoute::read(&neighbors_route_reader)?);
        }

        Ok(RoutesToIndexer { routes })
    }
}

//#[derive(Debug, Eq, PartialEq, Clone)]
//pub struct ConnectedFriend {
//    push_credits: u128,
//    public_key: PublicKey,
//}

//#[derive(Debug, Eq, PartialEq, Clone)]
//pub struct ResponseIndexerInfo {
//    connected_neighbors_list: Vec<PublicKey>,
//    neighbors_comm_public_key: DhPublicKey,
//    neighbors_recent_timestamp: ,
//    friends_comm_public_key: DhPublicKey,
//    friends_recent_timestamp:
//}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct RequestNeighborsRoute {
    source_node_public_key: PublicKey,
    destination_node_public_key: PublicKey,
}

impl<'a, 'b> Schema<'a, 'b> for RequestNeighborsRoute {
    type Reader = request_neighbors_route::Reader<'a>;
    type Writer = request_neighbors_route::Builder<'b>;

    inject_default_en_de_impl!();

    fn write(&self, to: &'a mut Self::Writer) -> Result<(), SchemaError> {
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

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
        let source_node_public_key = read_public_key(&from.get_source_node_public_key()?)?;

        let destination_node_public_key = read_public_key(
            &from.get_destination_node_public_key()?
        )?;

        Ok(RequestNeighborsRoute {
            source_node_public_key,
            destination_node_public_key
        })
    }
}

//#[derive(Debug, Eq, PartialEq, Clone)]
//pub struct ResponseNeighborsRoute {
//    routes: Vec<NeighborsRoute>,
//    destination_comm_public_key: DhPublicKey,
//    destination_recent_timestamp:
//}


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
}
