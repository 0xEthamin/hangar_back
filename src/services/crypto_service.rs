use aes_gcm::{
    aead::{Aead, KeyInit, OsRng, AeadCore},
    Aes256Gcm, Nonce, Key
};
use crate::error::AppError;

const NONCE_SIZE: usize = 12; // 96 bits, standard pour AES-GCM


pub fn encrypt(plaintext: &str, key: &[u8]) -> Result<Vec<u8>, AppError>
{
    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher.encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e|
        {
            tracing::error!("Encryption failed: {}", e);
            AppError::InternalServerError
        })?;
    
    let mut result = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    result.extend_from_slice(nonce.as_slice());
    result.extend_from_slice(&ciphertext);

    Ok(result)
}

pub fn decrypt(ciphertext_with_nonce: &[u8], key: &[u8]) -> Result<String, AppError>
{
    if ciphertext_with_nonce.len() < NONCE_SIZE
    {
        tracing::error!("Ciphertext is too short to contain a nonce.");
        return Err(AppError::InternalServerError);
    }

    let key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(key);

    let (nonce_bytes, ciphertext) = ciphertext_with_nonce.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext_bytes = cipher.decrypt(nonce, ciphertext)
        .map_err(|e|
        {
            tracing::error!("Decryption failed: {}. This might happen if the key is wrong or the data is corrupted.", e);
            AppError::InternalServerError
        })?;

    String::from_utf8(plaintext_bytes)
        .map_err(|_| AppError::InternalServerError)
}