use std::convert::TryFrom;
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use std::string::ToString;

use serde::de::Error;
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serializer};

use base64::{self, URL_SAFE_NO_PAD};

pub trait SerBase64<'de>: Sized {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer;
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

impl<'de, T> SerBase64<'de> for T
where
    T: AsRef<[u8]> + for<'t> TryFrom<&'t [u8]>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let base64_str = base64::encode_config(&self.as_ref(), URL_SAFE_NO_PAD);
        serializer.serialize_str(&base64_str)
    }

    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
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
}

/// Serializes `buffer` to a lowercase hex string.
pub fn to_base64<T, S>(to_base64: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: AsRef<[u8]>,
    S: Serializer,
{
    let base64_str = base64::encode_config(&to_base64.as_ref(), URL_SAFE_NO_PAD);
    serializer.serialize_str(&base64_str)
}

/// Deserializes a lowercase hex string to a `Vec<u8>`.
pub fn from_base64<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: for<'t> TryFrom<&'t [u8]>,
    D: Deserializer<'de>,
{
    let string = String::deserialize(deserializer)?;
    let vec = base64::decode_config(&string, URL_SAFE_NO_PAD)
        .map_err(|err| Error::custom(err.to_string()))?;
    T::try_from(&vec).map_err(|_| Error::custom("Length mismatch"))
}

/// Serializes value as a string
pub fn to_string<T, S>(input: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: ToString,
    S: Serializer,
{
    serializer.serialize_str(&input.to_string())
}

/// Deserializes a string into a value
pub fn from_string<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: FromStr,
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)?
        .parse()
        .map_err(|_| Error::custom("Failed to parse as string"))
}
