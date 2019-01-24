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

    write_uid(&request_routes.request_id, &mut request_routes_builder.reborrow().init_request_id());
    write_custom_u_int128(request_routes.capacity, &mut request_routes_builder.reborrow().init_capacity());
    write_public_key(&request_routes.source, &mut request_routes_builder.reborrow().init_source());
    write_public_key(&request_routes.destination, &mut request_routes_builder.reborrow().init_destination());

    let mut opt_exclude_builder = request_routes_builder.reborrow().init_opt_exclude();
    match &request_routes.opt_exclude {
        Some((from_public_key, to_public_key)) => {
            let mut edge_builder = opt_exclude_builder.init_edge();
            write_public_key(from_public_key, &mut edge_builder.reborrow().init_from_public_key());
            write_public_key(to_public_key, &mut edge_builder.reborrow().init_to_public_key());
        },
        None => {
            opt_exclude_builder.set_empty(());
        },
    }
}

fn deser_request_routes(request_routes_reader: &index_capnp::request_routes::Reader)
    -> Result<RequestRoutes, SerializeError> {

    let opt_exclude = match request_routes_reader.get_opt_exclude().which()? {
        index_capnp::request_routes::opt_exclude::Edge(res_edge_reader) => {
            let edge_reader = res_edge_reader?;
            let from_public_key = read_public_key(&edge_reader.get_from_public_key()?)?;
            let to_public_key = read_public_key(&edge_reader.get_to_public_key()?)?;
            Some((from_public_key, to_public_key))
        },
        index_capnp::request_routes::opt_exclude::Empty(()) => None,
    };

    Ok(RequestRoutes {
        request_id: read_uid(&request_routes_reader.get_request_id()?)?,
        capacity: read_custom_u_int128(&request_routes_reader.get_capacity()?)?,
        source: read_public_key(&request_routes_reader.get_source()?)?,
        destination: read_public_key(&request_routes_reader.get_destination()?)?,
        opt_exclude,
    })
}
