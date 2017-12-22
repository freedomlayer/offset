use std::io;

use indexer_capnp::{neighbors_route, chain_link};
use crypto::identity::{PublicKey, Signature};
use inner_messages::IndexingProviderStateHash;

use capnp::serialize_packed;
use bytes::{BigEndian, Bytes, BytesMut, Buf, BufMut};

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

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct NeighborsRoute {
    public_keys: Vec<PublicKey>,
}

impl<'a, 'b> Schema<'a, 'b> for NeighborsRoute {
    type Reader = neighbors_route::Reader<'a>;
    type Writer = neighbors_route::Builder<'b>;

    fn encode(&self) -> Result<Bytes, SchemaError> {
        let mut builder = ::capnp::message::Builder::new_default();

        match self.write(&mut builder.init_root()) {
            Ok(()) => {
                let mut serialized_msg = Vec::new();
                serialize_packed::write_message(&mut serialized_msg, &builder)?;

                Ok(Bytes::from(serialized_msg))
            }
            Err(e) => Err(e)
        }
    }

    fn decode(buffer: Bytes) -> Result<Self, SchemaError> {
        let mut buffer = io::Cursor::new(buffer);

        let reader = serialize_packed::read_message(
            &mut buffer,
            ::capnp::message::ReaderOptions::new()
        )?;

        Self::read(&reader.get_root()?)
    }

    fn write(&self, to: &'a mut Self::Writer) -> Result<(), SchemaError> {
        let mut public_keys = to.borrow().init_public_keys(self.public_keys.len() as u32);

        for (idx, public_key) in self.public_keys.iter().enumerate() {
            let mut target_cell = public_keys.borrow().get(idx as u32);
            let public_key_bytes = Bytes::from(public_key.as_bytes());
            write_custom_u_int256(&mut target_cell, &public_key_bytes)?;
        }

        Ok(())
    }

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
        let public_keys_reader = from.get_public_keys()?;

        let mut public_keys = Vec::with_capacity(public_keys_reader.len() as usize);

        for public_key_reader in public_keys_reader.iter() {
            let mut public_key_bytes = BytesMut::with_capacity(32);
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

    fn encode(&self) -> Result<Bytes, SchemaError> {
        let mut builder = ::capnp::message::Builder::new_default();

        match self.write(&mut builder.init_root()) {
            Ok(()) => {
                let mut serialized_msg = Vec::new();
                serialize_packed::write_message(&mut serialized_msg, &builder)?;

                Ok(Bytes::from(serialized_msg))
            }
            Err(e) => Err(e)
        }
    }

    fn decode(buffer: Bytes) -> Result<Self, SchemaError> {
        let mut buffer = io::Cursor::new(buffer);

        let reader = serialize_packed::read_message(
            &mut buffer,
            ::capnp::message::ReaderOptions::new()
        )?;

        Self::read(&reader.get_root()?)
    }

    fn write(&self, to: &'a mut Self::Writer) -> Result<(), SchemaError> {
        // Write the previousStateHash
        {
            let mut previous_state_hash = to.borrow().init_previous_state_hash();

            let state_hash_bytes = Bytes::from(self.previous_state_hash.as_bytes());

            write_custom_u_int256(&mut previous_state_hash, &state_hash_bytes)?;
        }
        // Write the newOwnersPublicKeys
        {
            let mut link_new_owners_public_public_keys = to.borrow().init_new_owners_public_keys(
                self.new_owners_public_keys.len() as u32
            );

            for (idx, public_key) in self.new_owners_public_keys.iter().enumerate() {
                let mut target_cell = link_new_owners_public_public_keys.borrow().get(idx as u32);
                let public_key_bytes = Bytes::from(public_key.as_bytes());
                write_custom_u_int256(&mut target_cell, &public_key_bytes)?;
            }
        }
        // Write the newIndexersPublicKeys
        {
            let mut new_indexers_public_keys = to.borrow().init_new_indexers_public_keys(
                self.new_indexers_public_keys.len() as u32
            );

            for (idx, public_key) in self.new_indexers_public_keys.iter().enumerate() {
                let mut target_cell = new_indexers_public_keys.borrow().get(idx as u32);
                let public_key_bytes = Bytes::from(public_key.as_bytes());
                write_custom_u_int256(&mut target_cell, &public_key_bytes)?;
            }
        }
        // Write the signaturesByOldOwners
        {
            let mut signatures_by_old_owners = to.borrow().init_signatures_by_old_owners(
                self.signatures_by_old_owners.len() as u32
            );

            for (idx, signature) in self.signatures_by_old_owners.iter().enumerate() {
                let mut target_cell = signatures_by_old_owners.borrow().get(idx as u32);
                let signature_bytes = Bytes::from(signature.as_bytes());
                write_custom_u_int512(&mut target_cell, &signature_bytes)?;
            }
        }

        Ok(())
    }

    fn read(from: &'b Self::Reader) -> Result<Self, SchemaError> {
        // Read the previousStateHash
        let previous_state_hash_reader = from.get_previous_state_hash()?;
        let mut state_hash_bytes = BytesMut::with_capacity(32);
        read_custom_u_int256(&mut state_hash_bytes, &previous_state_hash_reader)?;
        let previous_state_hash = IndexingProviderStateHash::from_bytes(&state_hash_bytes)
            .map_err(|_| SchemaError::Invalid)?;

        // Read the newOwnersPublicKeys
        let new_owners_public_keys_reader = from.get_new_owners_public_keys()?;

        let mut new_owners_public_keys = Vec::with_capacity(
            new_owners_public_keys_reader.len() as usize
        );

        for public_key_reader in new_owners_public_keys_reader.iter() {
            let mut public_key_bytes = BytesMut::with_capacity(32);
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
            let mut public_key_bytes = BytesMut::with_capacity(32);
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
            let mut signature_bytes = BytesMut::with_capacity(64);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neighbors_route() {
        let public_keys = (0..255u8).map(|byte| {
            PublicKey::from_bytes(&[byte; 32]).unwrap()
        }).collect::<Vec<_>>();

        let in_neighbors_route = NeighborsRoute { public_keys };

        let serialize_message = in_neighbors_route.encode().unwrap();

        let out_neighbors_route = NeighborsRoute::decode(serialize_message).unwrap();

        assert_eq!(in_neighbors_route, out_neighbors_route);
    }

    #[test]
    fn test_chain_link() {
        let previous_state_hash = IndexingProviderStateHash::from_bytes(&[0u8; 32]).unwrap();

        let new_owners_public_keys = (0..255u8).map(|byte| {
            PublicKey::from_bytes(&[byte; 32]).unwrap()
        }).collect::<Vec<_>>();

        let new_indexers_public_keys = (0..254u8).map(|byte| {
            PublicKey::from_bytes(&[byte; 32]).unwrap()
        }).collect::<Vec<_>>();

        let signatures_by_old_owners = (0..255u8).map(|byte| {
            Signature::from_bytes(&[byte; 64]).unwrap()
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
}
