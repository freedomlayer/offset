use common::conn::{FutTransform, BoxFuture, ConnPairVec};
use proto::funder::messages::TcpAddress;

pub struct TcpConnector;

impl FutTransform  for TcpConnector {
    type Input = TcpAddress;
    type Output = ConnPairVec;

    fn transform(&mut self, input: Self::Input)
        -> BoxFuture<'_, Self::Output> {

        // TODO:
        unimplemented!();
    }
}

