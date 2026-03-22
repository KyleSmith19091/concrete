use super::bash::Bash;

/// Unique identifier for a stream scoped by its namespace
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamID(Bash);

impl std::fmt::Display for StreamID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for StreamID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StreamId({})", self.0)
    }
}

impl StreamID {
    pub const LEN: usize = 32;
    const SEPARATOR: u8 = 0;

    pub fn new(namespace: &str, stream: &str) -> Self {
        Self(Bash::delimited(
            &[namespace.as_bytes(), stream.as_bytes()],
            Self::SEPARATOR,
        ))
    }

    pub fn as_bytes(&self) -> &[u8; Self::LEN] {
        self.0.as_bytes()
    }
}

impl From<[u8; StreamID::LEN]> for StreamID {
    fn from(bytes: [u8; StreamID::LEN]) -> Self {
        Self(bytes.into())
    }
}

impl From<StreamID> for [u8; StreamID::LEN] {
    fn from(id: StreamID) -> Self {
        id.0.into()
    }
}