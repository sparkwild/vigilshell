use ssh_key::PrivateKey;

use crate::keys::Error;

/// Decode an unencrypted secret key in the OpenSSH format.
///
/// Encrypted OpenSSH keys are rejected because this fork does not enable the
/// `ssh-key` private-key encryption feature. The password argument is ignored.
pub fn decode_openssh(secret: &[u8], _password: Option<&str>) -> Result<PrivateKey, Error> {
    let pk = PrivateKey::from_bytes(secret)?;
    if pk.is_encrypted() {
        return Err(Error::KeyIsEncrypted);
    }
    Ok(pk)
}
