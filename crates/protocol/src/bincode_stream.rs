use std::{fmt::Debug, marker::PhantomData};

use anyhow::Ok;
use serde::{de::DeserializeOwned, Serialize};
use tokio_util::{
    bytes::{Buf, BufMut},
    codec::{Decoder, Encoder},
};

pub struct BincodeCodec<Item> {
    _tmp: PhantomData<Item>,
}

impl<Item> Default for BincodeCodec<Item> {
    fn default() -> Self {
        Self { _tmp: Default::default() }
    }
}

impl<Item: Serialize> Encoder<Item> for BincodeCodec<Item> {
    type Error = bincode::Error;

    fn encode(&mut self, item: Item, dst: &mut tokio_util::bytes::BytesMut) -> Result<(), Self::Error> {
        let res = bincode::serialize_into(dst.writer(), &item);
        res
    }
}

impl<Item: DeserializeOwned + Debug> Decoder for BincodeCodec<Item> {
    type Error = bincode::Error;
    type Item = Item;

    fn decode(&mut self, src: &mut tokio_util::bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Result::<_, Self::Error>::Ok(Option::<Self::Item>::None);
        }
        let res = bincode::deserialize_from::<_, Item>(src.reader()).map(|o| Some(o));
        res
    }
}
