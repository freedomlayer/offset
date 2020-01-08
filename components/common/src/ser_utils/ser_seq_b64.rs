use std::convert::TryFrom;
use std::fmt;
use std::marker::PhantomData;
use std::string::ToString;

use serde::de::{Deserialize, Error, SeqAccess, Visitor};
use serde::ser::{Serialize, Serializer};
use serde::Deserializer;

use base64::{self, URL_SAFE_NO_PAD};

pub fn serialize<T, V, S>(input_vec: V, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize + AsRef<[u8]>,
    V: IntoIterator<Item = T>,
{
    let items_iter = input_vec
        .into_iter()
        .map(|item| base64::encode_config(item.as_ref(), URL_SAFE_NO_PAD));
    serializer.collect_seq(items_iter)
}

pub fn deserialize<'de, T, V, D>(deserializer: D) -> Result<V, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + for<'t> TryFrom<&'t [u8]>,
    // The `IntoIterator<Item=T>` bound is a hack for solving ambiguity.
    // See:
    // https://users.rust-lang.org/t/serde-type-annotations-needed-cannot-resolve-serde-deserialize/36482?u=realcr
    V: Default + Extend<T> + IntoIterator<Item = T>,
{
    struct SeqVisitor<T, V> {
        item: PhantomData<T>,
        seq: PhantomData<V>,
    }

    impl<'de, T, V> Visitor<'de> for SeqVisitor<T, V>
    where
        T: Deserialize<'de> + for<'t> TryFrom<&'t [u8]>,
        V: Default + Extend<T>,
    {
        type Value = V;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("A vector")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut res_vec: V = V::default();
            while let Some(str_item) = seq.next_element::<String>()? {
                let item_vec: Vec<u8> = base64::decode_config(&str_item, URL_SAFE_NO_PAD)
                    .map_err(|err| Error::custom(err.to_string()))?;
                let item = T::try_from(&item_vec).map_err(|_| Error::custom("Length mismatch"))?;
                // Extend::<T>::extend(&mut res_vec, Some(item));
                res_vec.extend(Some(item));
            }
            Ok(res_vec)
        }
    }

    let visitor = SeqVisitor {
        item: PhantomData,
        seq: PhantomData,
    };
    deserializer.deserialize_seq(visitor)
}
