use bytes::Bytes;

pub struct Record {
    pub seq_num: u64,
    pub data: Bytes,
}