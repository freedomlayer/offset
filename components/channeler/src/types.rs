use common::access_control::{AccessControl, AccessControlOp};
use common::conn::ConnPair;
use crypto::identity::PublicKey;

pub type RawConn = ConnPair<Vec<u8>, Vec<u8>>;

pub type AccessControlPk = AccessControl<PublicKey>;
pub type AccessControlOpPk = AccessControlOp<PublicKey>;
