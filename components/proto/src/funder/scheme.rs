use std::fmt::Debug;
use serde::Serialize;
use serde::de::DeserializeOwned;
use common::canonical_serialize::CanonicalSerialize;

/// A trait that describes generic types that the Funder works with
/// and the relationships between them.
pub trait FunderScheme: Clone + PartialEq + Eq + Debug {
    /// An anonymous address
    type Address: Send + Sync + Clone + CanonicalSerialize + PartialEq + Eq + Debug + Serialize + DeserializeOwned;
    /// An address that contains a name (Provided by the user of this node)
    type NamedAddress: Send + Sync + Clone + PartialEq + Eq + Debug + Serialize + DeserializeOwned;

    /// A function to convert a NamedAddress to Address
    fn anonymize_address(named_address: Self::NamedAddress) -> Self::Address;
}
