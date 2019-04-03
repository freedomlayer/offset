use crate::int_convert::usize_to_u64;
use byteorder::{BigEndian, WriteBytesExt};

/// Canonically serialize an object
/// This serialization is used for security related applications (For example, signatures and
/// hashing), therefore the serialization result must be the same on any system.
pub trait CanonicalSerialize {
    fn canonical_serialize(&self) -> Vec<u8>;
}

impl<T> CanonicalSerialize for Option<T>
where
    T: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_data = Vec::new();
        match &self {
            None => {
                res_data.push(0);
            }
            Some(t) => {
                res_data.push(1);
                res_data.extend_from_slice(&t.canonical_serialize());
            }
        };
        res_data
    }
}

impl<T> CanonicalSerialize for Vec<T>
where
    T: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_data = Vec::new();
        // Write length:
        res_data
            .write_u64::<BigEndian>(usize_to_u64(self.len()).unwrap())
            .unwrap();
        // Write all items:
        for t in self.iter() {
            res_data.extend_from_slice(&t.canonical_serialize());
        }
        res_data
    }
}

impl CanonicalSerialize for String {
    fn canonical_serialize(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

// Used mostly for testing:
impl CanonicalSerialize for u32 {
    fn canonical_serialize(&self) -> Vec<u8> {
        let mut res_data = Vec::new();
        res_data.write_u32::<BigEndian>(*self).unwrap();
        res_data
    }
}

impl<T, W> CanonicalSerialize for (T, W)
where
    T: CanonicalSerialize,
    W: CanonicalSerialize,
{
    fn canonical_serialize(&self) -> Vec<u8> {
        let (t, w) = self;
        let mut res_data = Vec::new();
        res_data.extend_from_slice(&t.canonical_serialize());
        res_data.extend_from_slice(&w.canonical_serialize());
        res_data
    }
}
