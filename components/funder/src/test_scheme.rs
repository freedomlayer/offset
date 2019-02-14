use proto::funder::scheme::FunderScheme;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestFunderScheme;

impl FunderScheme for TestFunderScheme {
    /// An anonymous address
    type Address = u32;
    /// An address that contains a name (Provided by the user of this node)
    type NamedAddress = (String, u32);

    /// A function to convert a NamedAddress to Address
    fn anonymize_address((_name, address_num): Self::NamedAddress) -> Self::Address {
        address_num
    }
}
