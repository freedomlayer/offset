use std::convert::TryFrom;
use std::fmt;
use std::marker::PhantomData;
use std::string::ToString;

use serde::de::{Deserialize, Error, Visitor};
use serde::ser::{Serialize, Serializer};
use serde::Deserializer;

use base64::{self, URL_SAFE_NO_PAD};

pub fn serialize<T, S>(opt_item: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize + AsRef<[u8]>,
{
    match opt_item {
        Some(item) => {
            let string_item = base64::encode_config(item.as_ref(), URL_SAFE_NO_PAD);
            serializer.serialize_some(&string_item)
        }
        None => serializer.serialize_none(),
    }
}

pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + for<'t> TryFrom<&'t [u8]>,
{
    struct ItemVisitor<T> {
        item: PhantomData<T>,
    }

    impl<'de, T> Visitor<'de> for ItemVisitor<T>
    where
        T: Deserialize<'de> + for<'t> TryFrom<&'t [u8]>,
    {
        type Value = Option<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("An option")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            struct B64Visitor<T> {
                item: PhantomData<T>,
            }

            impl<'de, T> Visitor<'de> for B64Visitor<T>
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

            let b64_visitor = B64Visitor { item: PhantomData };
            Ok(Some(deserializer.deserialize_string(b64_visitor)?))
        }
    }

    let visitor = ItemVisitor { item: PhantomData };
    deserializer.deserialize_option(visitor)
}
