use std::convert::TryFrom;
use std::fmt;
use std::marker::PhantomData;
use std::string::ToString;

use serde::de::{Error, Visitor};
use serde::ser::Serializer;
use serde::Deserializer;

use base64::{self, URL_SAFE_NO_PAD};

pub fn serialize<T, S>(item: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: AsRef<[u8]>,
{
    let base64_str = base64::encode_config(&item.as_ref(), URL_SAFE_NO_PAD);
    serializer.serialize_str(&base64_str)
}

pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: for<'t> TryFrom<&'t [u8]>,
{
    struct ItemVisitor<T> {
        item: PhantomData<T>,
    }

    impl<'de, T> Visitor<'de> for ItemVisitor<T>
    where
        T: for<'t> TryFrom<&'t [u8]>,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("A bytes like item")
        }

        fn visit_str<E>(self, str_item: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            let vec = base64::decode_config(&str_item, URL_SAFE_NO_PAD)
                .map_err(|err| Error::custom(err.to_string()))?;
            T::try_from(&vec).map_err(|_| Error::custom("Length mismatch"))
        }
    }

    let visitor = ItemVisitor { item: PhantomData };
    deserializer.deserialize_str(visitor)
}
