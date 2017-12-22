use std::io;

use indexer_capnp::*;
use crypto::identity::{
    PublicKey, PUBLIC_KEY_LEN,
    Signature, SIGNATURE_LEN,
};
use inner_messages::{
    IndexingProviderID, INDEXING_PROVIDER_ID_LEN,
    IndexingProviderStateHash, INDEXING_PROVIDER_STATE_HASH_LEN,
};

use capnp::serialize_packed;
use bytes::{Bytes, BytesMut};

use super::{
    Schema,
    SchemaError,
    read_custom_u_int128,
    read_custom_u_int256,
    read_custom_u_int512,
    write_custom_u_int128,
    write_custom_u_int256,
    write_custom_u_int512
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
        let mut public_keys = to.borrow().init_public_keys(self.public_keys.len() as u32);

        for (idx, ref_public_key) in self.public_keys.iter().enumerate() {
            let mut target_cell = public_keys.borrow().get(idx as u32);
            write_custom_u_int256(&mut target_cell, ref_public_key)?;
        }

        Ok(())
    }

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
        let public_keys_reader = from.get_public_keys()?;

        let mut public_keys = Vec::with_capacity(public_keys_reader.len() as usize);

        for public_key_reader in public_keys_reader.iter() {
            let mut public_key_bytes = BytesMut::with_capacity(PUBLIC_KEY_LEN);
            read_custom_u_int256(&mut public_key_bytes, &public_key_reader)?;

            public_keys.push(
                PublicKey::from_bytes(&public_key_bytes).map_err(|_| SchemaError::Invalid)?
            );
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
            write_custom_u_int256(&mut previous_state_hash, &self.previous_state_hash)?;
        }
        // Write the newOwnersPublicKeys
        {
            let mut link_new_owners_public_public_keys = to.borrow().init_new_owners_public_keys(
                self.new_owners_public_keys.len() as u32
            );

            for (idx, ref_public_key) in self.new_owners_public_keys.iter().enumerate() {
                let mut target_cell = link_new_owners_public_public_keys.borrow().get(idx as u32);
                write_custom_u_int256(&mut target_cell, ref_public_key)?;
            }
        }
        // Write the newIndexersPublicKeys
        {
            let mut new_indexers_public_keys = to.borrow().init_new_indexers_public_keys(
                self.new_indexers_public_keys.len() as u32
            );

            for (idx, ref_public_key) in self.new_indexers_public_keys.iter().enumerate() {
                let mut target_cell = new_indexers_public_keys.borrow().get(idx as u32);
                write_custom_u_int256(&mut target_cell, ref_public_key)?;
            }
        }
        // Write the signaturesByOldOwners
        {
            let mut signatures_by_old_owners = to.borrow().init_signatures_by_old_owners(
                self.signatures_by_old_owners.len() as u32
            );

            for (idx, ref_signature) in self.signatures_by_old_owners.iter().enumerate() {
                let mut target_cell = signatures_by_old_owners.borrow().get(idx as u32);
                write_custom_u_int512(&mut target_cell, ref_signature)?;
            }
        }

        Ok(())
    }

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
        // Read the previousStateHash
        let previous_state_hash_reader = from.get_previous_state_hash()?;
        let mut state_hash_bytes = BytesMut::with_capacity(INDEXING_PROVIDER_STATE_HASH_LEN);
        read_custom_u_int256(&mut state_hash_bytes, &previous_state_hash_reader)?;
        let previous_state_hash = IndexingProviderStateHash::from_bytes(&state_hash_bytes)
            .map_err(|_| SchemaError::Invalid)?;

        // Read the newOwnersPublicKeys
        let new_owners_public_keys_reader = from.get_new_owners_public_keys()?;

        let mut new_owners_public_keys = Vec::with_capacity(
            new_owners_public_keys_reader.len() as usize
        );

        for public_key_reader in new_owners_public_keys_reader.iter() {
            let mut public_key_bytes = BytesMut::with_capacity(PUBLIC_KEY_LEN);
            read_custom_u_int256(&mut public_key_bytes, &public_key_reader)?;

            new_owners_public_keys.push(
                PublicKey::from_bytes(&public_key_bytes).map_err(|_| SchemaError::Invalid)?
            );
        }

        // Read the newOwnersPublicKeys
        let new_indexers_public_keys_reader = from.get_new_indexers_public_keys()?;

        let mut new_indexers_public_keys = Vec::with_capacity(
            new_indexers_public_keys_reader.len() as usize
        );

        for public_key_reader in new_indexers_public_keys_reader.iter() {
            let mut public_key_bytes = BytesMut::with_capacity(PUBLIC_KEY_LEN);
            read_custom_u_int256(&mut public_key_bytes, &public_key_reader)?;

            new_indexers_public_keys.push(
                PublicKey::from_bytes(&public_key_bytes).map_err(|_| SchemaError::Invalid)?
            );
        }

        // Read the signaturesByOldOwners
        let signatures_by_old_owners_reader = from.get_signatures_by_old_owners()?;

        let mut signatures_by_old_owners = Vec::with_capacity(
            signatures_by_old_owners_reader.len() as usize
        );

        for signature_reader in signatures_by_old_owners_reader.iter() {
            let mut signature_bytes = BytesMut::with_capacity(SIGNATURE_LEN);
            read_custom_u_int512(&mut signature_bytes, &signature_reader)?;

            signatures_by_old_owners.push(
                Signature::from_bytes(&signature_bytes).map_err(|_| SchemaError::Invalid)?
            );
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
            let mut indexing_provider_id = to.borrow().init_indexing_provider_id();

            write_custom_u_int128(&mut indexing_provider_id, &self.indexing_provider_id)?;
        }

        // Writer the indexingProviderStatesChain
        {
            let mut indexing_provider_states_chain = to.borrow().init_indexing_provider_states_chain(
                self.indexing_provider_states_chain.len() as u32
            );

            for (idx, ref_chain_link) in self.indexing_provider_states_chain.iter().enumerate() {
                let mut target_cell = indexing_provider_states_chain.borrow().get(idx as u32);
                ref_chain_link.write(&mut target_cell)?;
            }
        }

        Ok(())
    }

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
        // Read the indexingProviderID
        let indexing_provider_id_reader = from.get_indexing_provider_id()?;
        let mut indexing_provider_id_bytes = BytesMut::with_capacity(INDEXING_PROVIDER_ID_LEN);
        read_custom_u_int128(&mut indexing_provider_id_bytes, &indexing_provider_id_reader)?;
        let indexing_provider_id = IndexingProviderID::from_bytes(&indexing_provider_id_bytes)
            .map_err(|_| SchemaError::Invalid)?;

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
        let mut state_hash = to.borrow().init_state_hash();

        write_custom_u_int256(&mut state_hash, &self.state_hash)?;

        Ok(())
    }

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
        let state_hash_reader = from.get_state_hash()?;
        let mut state_hash_bytes = BytesMut::with_capacity(INDEXING_PROVIDER_STATE_HASH_LEN);
        read_custom_u_int256(&mut state_hash_bytes, &state_hash_reader)?;
        let state_hash = IndexingProviderStateHash::from_bytes(&state_hash_bytes)
            .map_err(|_| SchemaError::Invalid)?;

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
            let mut target_cell = routes.borrow().get(idx as u32);
            ref_neighbors_route.write(&mut target_cell)?;
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neighbors_route() {
        let public_keys = (0..255u8).map(|byte| {
            PublicKey::from_bytes(&[byte; PUBLIC_KEY_LEN]).unwrap()
        }).collect::<Vec<_>>();

        let in_neighbors_route = NeighborsRoute { public_keys };

        let serialize_message = in_neighbors_route.encode().unwrap();

        let out_neighbors_route = NeighborsRoute::decode(serialize_message).unwrap();

        assert_eq!(in_neighbors_route, out_neighbors_route);
    }

    #[test]
    fn test_chain_link() {
        let previous_state_hash =
            IndexingProviderStateHash::from_bytes(&[0u8; INDEXING_PROVIDER_STATE_HASH_LEN]).unwrap();

        let new_owners_public_keys = (0..255u8).map(|byte| {
            PublicKey::from_bytes(&[byte; PUBLIC_KEY_LEN]).unwrap()
        }).collect::<Vec<_>>();

        let new_indexers_public_keys = (0..254u8).map(|byte| {
            PublicKey::from_bytes(&[byte; PUBLIC_KEY_LEN]).unwrap()
        }).collect::<Vec<_>>();

        let signatures_by_old_owners = (0..255u8).map(|byte| {
            Signature::from_bytes(&[byte; SIGNATURE_LEN]).unwrap()
        }).collect::<Vec<_>>();

        let in_chain_link = ChainLink {
            previous_state_hash,
            new_owners_public_keys,
            new_indexers_public_keys,
            signatures_by_old_owners,
        };

        let serialize_message = in_chain_link.encode().unwrap();

        let out_chain_link = ChainLink::decode(serialize_message).unwrap();

        assert_eq!(in_chain_link, out_chain_link);
    }

    #[test]
    fn test_request_update_state() {
        let indexing_provider_id =
            IndexingProviderID::from_bytes(&[11u8; INDEXING_PROVIDER_ID_LEN]).unwrap();

        let mut indexing_provider_states_chain = Vec::new();

        let previous_state_hash =
            IndexingProviderStateHash::from_bytes(&[0u8; INDEXING_PROVIDER_STATE_HASH_LEN]).unwrap();

        let new_owners_public_keys = (0..255u8).map(|byte| {
            PublicKey::from_bytes(&[byte; PUBLIC_KEY_LEN]).unwrap()
        }).collect::<Vec<_>>();

        let new_indexers_public_keys = (0..254u8).map(|byte| {
            PublicKey::from_bytes(&[byte; PUBLIC_KEY_LEN]).unwrap()
        }).collect::<Vec<_>>();

        let signatures_by_old_owners = (0..255u8).map(|byte| {
            Signature::from_bytes(&[byte; SIGNATURE_LEN]).unwrap()
        }).collect::<Vec<_>>();

        let chain_link = ChainLink {
            previous_state_hash,
            new_owners_public_keys,
            new_indexers_public_keys,
            signatures_by_old_owners,
        };

        for _ in 0..128 {
            indexing_provider_states_chain.push(chain_link.clone());
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
        let state_hash =
            IndexingProviderStateHash::from_bytes(&[0u8; INDEXING_PROVIDER_STATE_HASH_LEN]).unwrap();

        let in_response_update_state = ResponseUpdateState { state_hash };

        let serialize_message = in_response_update_state.encode().unwrap();

        let out_response_update_state = ResponseUpdateState::decode(serialize_message).unwrap();

        assert_eq!(in_response_update_state, out_response_update_state);
    }

    #[test]
    fn test_routes_to_indexer() {
        let public_keys = (0..255u8).map(|byte| {
            PublicKey::from_bytes(&[byte; PUBLIC_KEY_LEN]).unwrap()
        }).collect::<Vec<_>>();

        let neighbors_route = NeighborsRoute { public_keys };

        let mut routes = Vec::new();

        for _ in 0..500 {
            routes.push(neighbors_route.clone());
        }

        let in_routes_to_indexer = RoutesToIndexer { routes };

        let serialize_message = in_routes_to_indexer.encode().unwrap();

        let out_routes_to_indexer = RoutesToIndexer::decode(serialize_message).unwrap();

        assert_eq!(in_routes_to_indexer, out_routes_to_indexer);
    }
}
