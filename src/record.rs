use bytes::{BufMut, Bytes, BytesMut};

use crate::error::StreamError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub seq_num: u64,
    pub data: Bytes,
}

impl Record {
    pub fn serialise(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(8 + self.data.len());
        buf.put_u64(self.seq_num);
        buf.extend(self.data.iter());
        buf.freeze()
    }

    pub fn deserialise(buf: &[u8]) -> Result<Self, StreamError> {
        if buf.len() < 8 {
            return Err(StreamError::DeserialiseError(format!(
                "record should at least have 8 bytes, got {}",
                buf.len()
            )));
        }
        let (num_bytes, rest) = buf.split_at(8);
        let seq_num = u64::from_be_bytes(num_bytes.try_into().unwrap());
        Ok(Self {
            seq_num,
            data: Bytes::copy_from_slice(rest),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_serialize_roundtrip() {
        let record = Record {
            seq_num: 42,
            data: Bytes::from_static(b"hello"),
        };

        let decoded = Record::deserialise(&record.serialise()).unwrap();

        assert_eq!(record, decoded);
    }
}
