use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::LazyLock;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Hash([u8; 32]);

impl Hash {
    pub fn new(data: [u8; 32]) -> Self {
        Self(data)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct MigrationDefinition {
    pub version: i64,
    pub name: &'static str,
    pub hash: Hash,
    pub sql: &'static str,
}

#[derive(Clone, Debug)]
pub struct MigrationRecord {
    pub version: i64,
    pub name: String,
    pub hash: Hash,
}

impl PartialEq<MigrationDefinition> for MigrationRecord {
    fn eq(&self, other: &MigrationDefinition) -> bool {
        self.version == other.version && self.name == other.name && self.hash == other.hash
    }
}

impl PartialEq<MigrationRecord> for MigrationDefinition {
    fn eq(&self, other: &MigrationRecord) -> bool {
        self.version == other.version && self.name == other.name && self.hash == other.hash
    }
}

#[derive(Debug, PartialEq)]
pub enum MigrationError {
    InternalHashWrong,
    MigrationMismatch,
    TooManyMigrations,
    Serialization,
    Database,
    UnversionedDatabase,
    NonConsecutiveVersions,
}

impl MigrationDefinition {
    fn compute_hash(&self) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update(self.sql);
        Hash(hasher.finalize().into())
    }

    pub fn verify_hash(&self) -> Result<(), MigrationError> {
        let hash = self.compute_hash();

        match self.hash == hash {
            true => Ok(()),
            false => Err(MigrationError::InternalHashWrong),
        }
    }
}

pub static MIGRATIONS: LazyLock<[MigrationDefinition; 2]> = LazyLock::new(|| {
    [
        MigrationDefinition {
            version: 1,
            name: "initial_schema",
            hash: "f08c7e2922ab85c365a355a1994dc590f45a5cbf1d014df2db3897661e1c4407"
                .try_into()
                .expect("migration hash must be valid hexadecimal"),
            sql: include_str!("001_initial_schema.sql"),
        },
        MigrationDefinition {
            version: 2,
            name: "add_null_checks",
            hash: "c69ac3da5546a17620c4d47b47e91573fdf7faf864d28ec89d61c46c62ebe02d"
                .try_into()
                .expect("migration hash must be valid hexadecimal"),
            sql: include_str!("002_add_null_checks.sql"),
        },
    ]
});

#[derive(Debug, PartialEq, Eq)]
pub enum HashParseError {
    InvalidLength,
    InvalidCharacter { index: usize },
}

impl fmt::Display for HashParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength => {
                write!(
                    formatter,
                    "hash must contain exactly 64 hexadecimal characters"
                )
            }
            Self::InvalidCharacter { index } => {
                write!(formatter, "invalid hexadecimal character at index {index}")
            }
        }
    }
}

impl std::error::Error for HashParseError {}

impl TryFrom<&str> for Hash {
    type Error = HashParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        const ENCODED_LENGTH: usize = 64;

        if value.len() != ENCODED_LENGTH {
            return Err(HashParseError::InvalidLength);
        }

        let mut hash = [0; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex_digit(pair[0])
                .ok_or(HashParseError::InvalidCharacter { index: index * 2 })?;
            let low = decode_hex_digit(pair[1]).ok_or(HashParseError::InvalidCharacter {
                index: index * 2 + 1,
            })?;
            hash[index] = high << 4 | low;
        }

        Ok(Self(hash))
    }
}

fn decode_hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Hash, HashParseError};

    #[test]
    fn hash_can_be_parsed_from_hexadecimal_string() {
        let hash: Hash = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
            .try_into()
            .unwrap();

        assert_eq!(hash, Hash(std::array::from_fn(|index| index as u8)));
    }

    #[test]
    fn hash_can_be_converted_to_lowercase_hexadecimal_string() {
        let hash = Hash(std::array::from_fn(|index| index as u8));

        assert_eq!(
            hash.to_string(),
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
        );
    }

    #[test]
    fn hash_rejects_wrong_length() {
        let result = Hash::try_from("00");

        assert_eq!(result, Err(HashParseError::InvalidLength));
    }

    #[test]
    fn hash_rejects_non_hexadecimal_character() {
        let result =
            Hash::try_from("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1g");

        assert_eq!(result, Err(HashParseError::InvalidCharacter { index: 63 }));
    }
}
