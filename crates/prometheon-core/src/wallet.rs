//! Wallet loading.
//!
//! Reads the standard Solana CLI keypair file (a JSON array of 64 bytes) — the same format
//! `solana-keygen` writes and `preflight` reads — so there is one on-disk shape across the
//! toolchain. The path comes from `WALLET_KEYPAIR_PATH(_MAINNET)`; the key material is never logged.

use std::path::Path;

use solana_sdk::signature::Keypair;

/// Load a Solana keypair from a CLI keypair JSON file (a 64-byte array).
pub fn load_keypair(path: impl AsRef<Path>) -> anyhow::Result<Keypair> {
    let path = path.as_ref();
    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read keypair {}: {e}", path.display()))?;
    let bytes: Vec<u8> = serde_json::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("parse keypair {}: {e}", path.display()))?;
    if bytes.len() != 64 {
        anyhow::bail!(
            "keypair {} has {} bytes, expected 64",
            path.display(),
            bytes.len()
        );
    }
    Keypair::try_from(bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("invalid keypair {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signer::Signer;

    #[test]
    fn round_trips_a_keypair_through_the_cli_json_format() {
        let kp = Keypair::new();
        let json = serde_json::to_string(&kp.to_bytes().to_vec()).unwrap();
        let path = std::env::temp_dir().join("prometheon_wallet_test.json");
        std::fs::write(&path, json).unwrap();

        let loaded = load_keypair(&path).unwrap();
        assert_eq!(loaded.pubkey(), kp.pubkey());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_wrong_length() {
        let path = std::env::temp_dir().join("prometheon_wallet_bad.json");
        std::fs::write(&path, "[1,2,3]").unwrap();
        assert!(load_keypair(&path).unwrap_err().to_string().contains("64"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(load_keypair("/nonexistent/prometheon/keypair.json").is_err());
    }
}
