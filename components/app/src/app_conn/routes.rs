use proto::crypto::{PublicKey, Uid};

use proto::app_server::messages::AppRequest;
use proto::funder::messages::Currency;
use proto::index_server::messages::{Edge, RequestRoutes};

pub fn request_routes(
    request_routes_id: Uid,
    currency: Currency,
    capacity: u128,
    source: PublicKey,
    destination: PublicKey,
    opt_exclude: Option<(PublicKey, PublicKey)>,
) -> AppRequest {
    let opt_exclude = opt_exclude.map(|(from_public_key, to_public_key)| Edge {
        from_public_key,
        to_public_key,
    });

    let request_routes = RequestRoutes {
        request_id: request_routes_id,
        currency,
        capacity,
        source,
        destination,
        opt_exclude,
    };

    AppRequest::RequestRoutes(request_routes)
}
