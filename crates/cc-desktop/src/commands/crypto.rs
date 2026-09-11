#[tauri::command(rename_all = "snake_case")]
pub fn encrypt_value(value: String) -> Result<String, String> {
    let master_key = cc_core::crypto::get_or_create_master_key()
        .map_err(|e| e.to_string())?;
    cc_core::crypto::encrypt(&value, &master_key).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn decrypt_value(envelope: String) -> Result<String, String> {
    let master_key = cc_core::crypto::get_or_create_master_key()
        .map_err(|e| e.to_string())?;
    cc_core::crypto::decrypt(&envelope, &master_key).map_err(|e| e.to_string())
}
