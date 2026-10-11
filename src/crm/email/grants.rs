//! AES-256-GCM grants, bound to the operator with authenticated additional data.
use super::{EmailError, SCOPES};
use crate::{db::turso_http::TursoDb, handlers::helpers::get_str};
use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::RngCore;

pub fn encrypt(key: &[u8; 32], operator: &str, token: &str) -> Result<String, EmailError> {
    let mut nonce = [0; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EmailError::Storage)?;
    let data = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: token.as_bytes(),
                aad: operator.as_bytes(),
            },
        )
        .map_err(|_| EmailError::Storage)?;
    Ok(STANDARD.encode([nonce.to_vec(), data].concat()))
}
pub fn decrypt(key: &[u8; 32], operator: &str, encoded: &str) -> Result<String, EmailError> {
    let bytes = STANDARD.decode(encoded).map_err(|_| EmailError::Storage)?;
    if bytes.len() < 28 {
        return Err(EmailError::Storage);
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| EmailError::Storage)?;
    let plain = cipher
        .decrypt(
            Nonce::from_slice(&bytes[..12]),
            Payload {
                msg: &bytes[12..],
                aad: operator.as_bytes(),
            },
        )
        .map_err(|_| EmailError::Storage)?;
    String::from_utf8(plain).map_err(|_| EmailError::Storage)
}
pub fn has_scopes(scopes: &str) -> bool {
    SCOPES
        .iter()
        .all(|needed| scopes.split_whitespace().any(|s| s == *needed))
}
pub fn get(t: &TursoDb, operator: &str) -> Result<Option<String>, String> {
    let rows = t.query("SELECT encrypted_refresh_token, scopes FROM crm_gmail_grants WHERE operator_email = ?1 AND revoked_at IS NULL", &[&operator])?;
    Ok(rows
        .first()
        .filter(|r| has_scopes(&get_str(r, "scopes")))
        .map(|r| get_str(r, "encrypted_refresh_token")))
}
pub fn save(t: &TursoDb, operator: &str, encrypted: &str, scopes: &str) -> Result<(), String> {
    let now = super::super::pending::now_iso();
    t.execute(
        "UPDATE crm_gmail_reply_status SET expired_grant_at=NULL WHERE operator_email=?1",
        &[&operator],
    )?;
    t.execute("INSERT INTO crm_gmail_grants (operator_email, encrypted_refresh_token, scopes, granted_at, revoked_at) VALUES (?1, ?2, ?3, ?4, NULL) ON CONFLICT(operator_email) DO UPDATE SET encrypted_refresh_token = excluded.encrypted_refresh_token, scopes = excluded.scopes, granted_at = excluded.granted_at, revoked_at = NULL", &[&operator, &encrypted, &scopes, &now])
}
pub fn revoke(t: &TursoDb, operator: &str) -> Result<(), String> {
    let now = super::super::pending::now_iso();
    t.execute("UPDATE crm_gmail_grants SET revoked_at = ?1, encrypted_refresh_token = '' WHERE operator_email = ?2", &[&now, &operator])
}
