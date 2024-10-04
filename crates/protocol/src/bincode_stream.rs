use std::marker::PhantomData;

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
        bincode::serialize_into(dst.writer(), &item)
    }
}

impl<Item: DeserializeOwned> Decoder for BincodeCodec<Item> {
    type Error = bincode::Error;
    type Item = Item;

    fn decode(&mut self, src: &mut tokio_util::bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        bincode::deserialize_from::<_, Item>(src.reader()).map(|o| Some(o))
    }
}
