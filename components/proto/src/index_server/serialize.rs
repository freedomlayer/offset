use common::int_convert::usize_to_u32;
use crate::capnp_common::{write_signature, read_signature,
                          write_custom_int128, read_custom_int128,
                          write_custom_u_int128, read_custom_u_int128,
                          write_rand_nonce, read_rand_nonce,
                          write_uid, read_uid,
                          write_invoice_id, read_invoice_id,
                          write_public_key, read_public_key,
                          write_relay_address, read_relay_address};
use index_capnp;

use super::messages::{RequestRoutes, RouteWithCapacity, ResponseRoutes,
                        UpdateFriend, IndexMutation, MutationsUpdate, TimeProofLink,
                        ForwardMutationsUpdate, IndexServerToClient, 
                        IndexClientToServer, IndexServerToServer};

use crate::serialize::SerializeError;

fn ser_request_routes(request_routes: &RequestRoutes,
                         request_routes_builder: &mut index_capnp::request_routes::Builder) {


    unimplemented!();
    /*
    let public_keys_len = usize_to_u32(friends_route.public_keys.len()).unwrap();
    let mut public_keys_builder = friends_route_builder.reborrow().init_public_keys(public_keys_len);

    for (index, public_key) in friends_route.public_keys.iter().enumerate() {
        let mut public_key_builder = public_keys_builder.reborrow().get(usize_to_u32(index).unwrap());
        write_public_key(public_key, &mut public_key_builder);
    }
    */
}

fn deser_request_routes(request_routes_reader: &index_capnp::request_routes::Reader)
    -> Result<RequestRoutes, SerializeError> {

    unimplemented!();

    /*
    let mut public_keys = Vec::new();
    for public_key_reader in friends_route_reader.get_public_keys()? {
        public_keys.push(read_public_key(&public_key_reader)?);
    }

    Ok(FriendsRoute {
        public_keys,
    })
    */
}
