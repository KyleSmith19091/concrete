use bytes::{Buf, BufMut, Bytes, BytesMut};
use enum_ordinalize::Ordinalize;
use super::KeyType;

use crate::kv::{DeserializationError, stream_id::StreamID};

const KEY_LEN: usize = 1 + StreamID::LEN;
const VALUE_LEN: usize = 8 + 8 + 4;

/// ser_key serialises the stream identifier which acts as the ID
/// for the stream tail positon
pub fn ser_key(stream_id: StreamID) -> Bytes {
    // allocate buffer
    let mut buf = BytesMut::with_capacity(KEY_LEN);
    // key type -> single byte
    buf.put_u8(KeyType::StreamTailPosition.ordinal()); 
    // stream identifier
    buf.put_slice(stream_id.as_bytes()); 
    debug_assert_eq!(buf.len(), KEY_LEN, "serialized length mismatch");
    // remove mut
    buf.freeze()
}

pub fn deser_key(mut bytes: Bytes) -> Result<StreamID, DeserializationError> {
    // confirm that the buffer has enough bytes for us to extract
    super::check_exact_size(&bytes, KEY_LEN)?;
    // extract ordinal from buffer (advances internal pointer)
    let ordinal = bytes.get_u8(); 
    if ordinal != KeyType::StreamTailPosition.ordinal() {
        return Err(DeserializationError::InvalidOrdinal(ordinal));
    }
    // allocate buffer to hold streamID
    let mut stream_id_bytes = [0u8; StreamID::LEN];

    // copy data from buffer
    bytes.copy_to_slice(&mut stream_id_bytes);

    // cast stream id to StreamID type
    Ok(stream_id_bytes.into())
}

