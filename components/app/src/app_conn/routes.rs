use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};

use common::multi_consumer::MultiConsumerClient;

use proto::crypto::{PublicKey, Uid};

use crypto::rand::{CryptoRandom, OffstSystemRandom, RandGen};

use proto::funder::messages::Currency;
use proto::app_server::messages::{AppRequest, AppToAppServer};
use proto::index_client::messages::{ClientResponseRoutes, ResponseRoutesResult};
use proto::index_server::messages::{Edge, MultiRoute, RequestRoutes};

#[derive(Debug)]
pub struct AppRoutesError;

#[derive(Clone)]
pub struct AppRoutes<R = OffstSystemRandom> {
    sender: mpsc::Sender<AppToAppServer>,
    routes_mc: MultiConsumerClient<ClientResponseRoutes>,
    rng: R,
}

/*
pub struct RequestRoutes {
    pub request_id: Uid,
    /// Wanted capacity for the route.
    /// 0 means we want to optimize for capacity??
    pub capacity: u128,
    pub source: PublicKey,
    pub destination: PublicKey,
    /// This directed edge must not show up in the route.
    /// Useful for finding non trivial directed loops.
    pub opt_exclude: Option<(PublicKey, PublicKey)>,
}


pub enum ResponseRoutesResult {
    Success(Vec<RouteWithCapacity>),
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientResponseRoutes {
    pub request_id: Uid,
    pub result: ResponseRoutesResult,
}

*/

impl<R> AppRoutes<R>
where
    R: CryptoRandom,
{
    pub(super) fn new(
        sender: mpsc::Sender<AppToAppServer>,
        routes_mc: MultiConsumerClient<ClientResponseRoutes>,
        rng: R,
    ) -> Self {
        AppRoutes {
            sender,
            routes_mc,
            rng,
        }
    }

    pub async fn request_routes(
        &mut self,
        currency: Currency,
        capacity: u128,
        source: PublicKey,
        destination: PublicKey,
        opt_exclude: Option<(PublicKey, PublicKey)>,
    ) -> Result<Vec<MultiRoute>, AppRoutesError> {
        let request_routes_id = Uid::rand_gen(&self.rng);
        let opt_exclude = opt_exclude.map(|(from_public_key, to_public_key)| Edge {
            from_public_key,
            to_public_key,
        });
        let request_routes = RequestRoutes {
            request_id: request_routes_id.clone(),
            currency,
            capacity,
            source,
            destination,
            opt_exclude,
        };

        let app_request = AppRequest::RequestRoutes(request_routes);
        let to_app_server = AppToAppServer::new(Uid::rand_gen(&self.rng), app_request);

        // Start listening for incoming response routes messages:
        let mut incoming_routes = self
            .routes_mc
            .request_stream()
            .await
            .map_err(|_| AppRoutesError)?;

        // Send our request to offst node:
        self.sender
            .send(to_app_server)
            .await
            .map_err(|_| AppRoutesError)?;

        while let Some(client_response_routes) = incoming_routes.next().await {
            if client_response_routes.request_id != request_routes_id {
                // This is not our request
                continue;
            }
            match client_response_routes.result {
                ResponseRoutesResult::Success(multi_routes) => return Ok(multi_routes),
                ResponseRoutesResult::Failure => return Err(AppRoutesError),
            }
        }
        Err(AppRoutesError)
    }
}
