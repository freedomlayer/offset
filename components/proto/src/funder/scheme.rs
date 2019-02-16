use common::mutable_state::MutableState;

/// A trait that describes generic types that the Funder works with
/// and the relationships between them.
pub trait FunderScheme {
    /// An anonymous address
    type Address;
    /// An address that contains a name (Provided by the user of this node)
    type NamedAddress: Default + MutableState<Mutation=Self::NamedAddressMutation, MutateError=!>;
    /// A mutation that can be applied to NamedAddress
    type NamedAddressMutation;

    /// A function to convert a NamedAddress to Address
    fn anonymize_address(named_address: Self::NamedAddress) -> Self::Address;
}
