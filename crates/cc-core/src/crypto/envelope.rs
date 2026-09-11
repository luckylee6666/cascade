use anyhow::{Result, bail};

pub const ENVELOPE_PREFIX: &str = "cc-enc:v1:";

pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENVELOPE_PREFIX)
}

pub fn wrap_envelope(ciphertext: &[u8], nonce: &[u8]) -> String {
    use base64::Engine;
    let combined = [nonce, ciphertext].concat();
    let encoded = base64::engine::general_purpose::STANDARD.encode(combined);
    format!("{}{}", ENVELOPE_PREFIX, encoded)
}

pub fn unwrap_envelope(envelope: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    if !envelope.starts_with(ENVELOPE_PREFIX) {
        bail!("Invalid envelope prefix");
    }

    use base64::Engine;
    let encoded = envelope
        .strip_prefix(ENVELOPE_PREFIX)
        .expect("prefix verified by starts_with above");
    let combined = base64::engine::general_purpose::STANDARD.decode(encoded)?;

    if combined.len() < 12 {
        bail!("Invalid envelope: too short");
    }

    let nonce = combined[..12].to_vec();
    let ciphertext = combined[12..].to_vec();

    Ok((nonce, ciphertext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_roundtrip() {
        let nonce = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let ciphertext = vec![10, 20, 30, 40, 50];

        let envelope = wrap_envelope(&ciphertext, &nonce);
        assert!(is_encrypted(&envelope));

        let (nonce2, ciphertext2) = unwrap_envelope(&envelope).unwrap();
        assert_eq!(nonce, nonce2);
        assert_eq!(ciphertext, ciphertext2);
    }
}
