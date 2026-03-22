/// Blake3 hash (32 bytes) 
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bash(blake3::Hash);

impl Bash {
    pub const LEN: usize = 32;

    /// Hashes components separated by a delimiter byte.
    /// Callers must ensure components do not contain the delimiter.
    pub fn delimited(components: &[&[u8]], delimiter: u8) -> Self {
        let mut hasher = blake3::Hasher::new();
        for component in components {
            hasher.update(component);
            hasher.update(&[delimiter]);
        }
        Self(hasher.finalize())
    }

    pub fn as_bytes(&self) -> &[u8; Bash::LEN] {
        self.0.as_bytes()
    }
}

impl std::fmt::Display for Bash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.to_hex().as_str())
    }
}

impl From<Bash> for [u8; Bash::LEN] {
    fn from(bash: Bash) -> Self {
        bash.0.into()
    }
}

impl From<[u8; Bash::LEN]> for Bash {
    fn from(bytes: [u8; Bash::LEN]) -> Self {
        Self(blake3::Hash::from_bytes(bytes))
    }
}
