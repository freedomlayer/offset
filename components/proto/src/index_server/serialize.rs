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

use crate::funder::serialize::{ser_friends_route, deser_friends_route};

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


fn ser_route_with_capacity(route_with_capacity: &RouteWithCapacity,
                           route_with_capacity_builder: &mut index_capnp::route_with_capacity::Builder) {

    ser_friends_route(&route_with_capacity.route, &mut route_with_capacity_builder.reborrow().init_route());
    write_custom_u_int128(route_with_capacity.capacity, &mut route_with_capacity_builder.reborrow().init_capacity());
}

fn deser_route_with_capacity(route_with_capacity_reader: &index_capnp::route_with_capacity::Reader) 
    -> Result<RouteWithCapacity, SerializeError> {

    Ok(RouteWithCapacity {
        route: deser_friends_route(&route_with_capacity_reader.get_route()?)?,
        capacity: read_custom_u_int128(&route_with_capacity_reader.get_capacity()?)?,
    })
}

fn ser_response_routes(response_routes: &ResponseRoutes,
                       response_routes_builder: &mut index_capnp::response_routes::Builder) {

    write_uid(&response_routes.request_id, &mut response_routes_builder.reborrow().init_request_id());
    let routes_len = usize_to_u32(response_routes.routes.len()).unwrap();
    let mut routes_builder = response_routes_builder.reborrow().init_routes(routes_len);

    for (index, route) in response_routes.routes.iter().enumerate() {
        let mut route_with_capacity_builder = routes_builder.reborrow().get(usize_to_u32(index).unwrap());
        ser_route_with_capacity(&route, &mut route_with_capacity_builder);
    }
}

fn deser_response_routes(response_routes_reader: &index_capnp::response_routes::Reader)
    -> Result<ResponseRoutes, SerializeError> {
    
    let mut routes = Vec::new();
    for route_with_capacity in response_routes_reader.get_routes()? {
        routes.push(deser_route_with_capacity(&route_with_capacity)?);
    }

    Ok(ResponseRoutes {
        request_id: read_uid(&response_routes_reader.get_request_id()?)?,
        routes,
    })
}

fn ser_update_friend(update_friend: &UpdateFriend,
                       update_friend_builder: &mut index_capnp::update_friend::Builder) {

    write_public_key(&update_friend.public_key, &mut update_friend_builder.reborrow().init_public_key());
    write_custom_u_int128(update_friend.send_capacity, &mut update_friend_builder.reborrow().init_send_capacity());
    write_custom_u_int128(update_friend.recv_capacity, &mut update_friend_builder.reborrow().init_recv_capacity());
}

fn deser_update_friend(update_friend_reader: &index_capnp::update_friend::Reader)
    -> Result<UpdateFriend, SerializeError> {

    Ok(UpdateFriend {
        public_key: read_public_key(&update_friend_reader.get_public_key()?)?,
        send_capacity: read_custom_u_int128(&update_friend_reader.get_send_capacity()?)?,
        recv_capacity: read_custom_u_int128(&update_friend_reader.get_recv_capacity()?)?,
    })
}

