use common::access_control::{AccessControl, AccessControlOp};

use proto::crypto::PublicKey;

pub type AccessControlPk = AccessControl<PublicKey>;
pub type AccessControlOpPk = AccessControlOp<PublicKey>;
