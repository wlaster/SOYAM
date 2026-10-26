use super::{ApiService, Result};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Duration;
use yandex_music::model::rotor::session::Session;

fn parse_ai_preference(value: &Value) -> Result<bool> {
    value.pointer("/result/aiContentReductionEnabled").and_then(Value::as_bool)
        .ok_or_else(|| "Сервер не вернул настройку «Меньше ИИ»".into())
}

fn session_body(seeds: Vec<String>, enabled: bool) -> Value {
    let mut body = json!({"seeds": seeds, "queue": [], "includeTracksInResponse": true,
        "includeWaveModel": true, "interactive": true});
    // The official web client sends this parameter only when enabled.
    if enabled { body["aiContentReductionEnabled"] = Value::Bool(true); }
    body
}

impl ApiService {
    fn remember_ai_preference(&self, enabled: bool) {
        let mut current = self.ai_content_reduction.write();
        if *current != Some(enabled) {
            *current = Some(enabled);
            self.ai_revision.fetch_add(1, Ordering::AcqRel);
        }
    }

    async fn read_ai_preference(&self) -> Result<bool> {
        let value: Value = self.http_client.get("https://api.music.yandex.net/account/settings")
            .timeout(Duration::from_secs(10)).send().await?.error_for_status()?.json().await?;
        parse_ai_preference(&value)
    }

    pub async fn get_ai_content_reduction(&self) -> Result<bool> {
        let _guard = self.ai_settings_lock.lock().await;
        let enabled = self.read_ai_preference().await?;
        self.remember_ai_preference(enabled);
        Ok(enabled)
    }

    pub async fn set_ai_content_reduction(&self, enabled: bool) -> Result<bool> {
        let _guard = self.ai_settings_lock.lock().await;
        // Partial update, matching the official client: do not overwrite other settings.
        let value: Value = self.http_client.post("https://api.music.yandex.net/account/settings")
            .query(&[("aiContentReductionEnabled", enabled.to_string())])
            .timeout(Duration::from_secs(10)).send().await?.error_for_status()?.json().await?;
        let confirmed = parse_ai_preference(&value)?;
        let readback = self.read_ai_preference().await?;
        self.remember_ai_preference(readback);
        if confirmed != enabled || readback != enabled {
            return Err("Яндекс Музыка не подтвердила изменение настройки".into());
        }
        Ok(readback)
    }

    pub(crate) fn ai_preference_revision(&self) -> u64 { self.ai_revision.load(Ordering::Acquire) }

    pub async fn create_session(&self, seeds: Vec<String>) -> Result<Session> {
        let cached = *self.ai_content_reduction.read();
        let enabled = match cached { Some(value) => value, None => self.get_ai_content_reduction().await? };
        let response: Value = self.http_client.post("https://api.music.yandex.net/rotor/session/new")
            .json(&session_body(seeds, enabled)).send().await?.error_for_status()?.json().await?;
        let result = response.get("result").ok_or("Сервер не вернул сессию Моей волны")?;
        Ok(serde_json::from_value(result.clone())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ai_preference_distinguishes_disabled_from_missing() {
        assert_eq!(parse_ai_preference(&json!({"result":{"aiContentReductionEnabled":false}})).unwrap(), false);
        assert!(parse_ai_preference(&json!({"result":{}})).is_err());
        assert!(parse_ai_preference(&json!({"result":{"aiContentReductionEnabled":"true"}})).is_err());
    }
    #[test]
    fn wave_request_matches_official_ai_parameter() {
        let enabled = session_body(vec!["user:onyourwave".into()], true);
        assert_eq!(enabled["aiContentReductionEnabled"], true);
        assert_eq!(enabled["seeds"][0], "user:onyourwave");
        assert_eq!(enabled["interactive"], true);
        assert!(session_body(vec![], false).get("aiContentReductionEnabled").is_none());
    }
    #[tokio::test]
    #[ignore = "saved account, real server; temporarily toggles and restores AI preference"]
    async fn live_ai_preference_roundtrip_and_wave_sessions() {
        let path = std::path::PathBuf::from(std::env::var("APPDATA").unwrap())
            .join("darkplayoff.yayma.app/yayma/yamusic_v2.db");
        let db = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let raw: String = db.query_row("SELECT value FROM app_settings WHERE key='auth_token'", [], |row| row.get(0)).unwrap();
        let (token, uid): (String, u64) = serde_json::from_str(&raw).unwrap();
        drop(db);
        let api = ApiService::new(token, Some(uid)).await.unwrap();
        let initial = api.get_ai_content_reduction().await.unwrap();
        let outcome: Result<()> = async {
            for enabled in [!initial, initial] {
                let confirmed = api.set_ai_content_reduction(enabled).await?;
                if confirmed != enabled { return Err("Preference mismatch".into()); }
                let session = api.create_session(vec!["user:onyourwave".into()]).await?;
                if !crate::audio::fetcher::is_usable_wave_session(&session) || session.sequence.is_empty() {
                    return Err("Wave session rejected AI preference".into());
                }
                println!("AI preference={enabled}, confirmed; wave session accepted, tracks={}", session.sequence.len());
            }
            Ok(())
        }.await;
        let restored = api.set_ai_content_reduction(initial).await;
        assert_eq!(restored.unwrap(), initial, "Initial account preference must be restored");
        outcome.unwrap();
    }

}
