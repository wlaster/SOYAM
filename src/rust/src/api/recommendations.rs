use crate::api::models::AppError;
use crate::app::AppContext;
use crate::audio::commands::AudioMessage;

pub async fn get_ai_content_reduction(ctx: &AppContext) -> Result<bool, AppError> {
    ctx.core.api.get_ai_content_reduction().await.map_err(|e| AppError::ApiError(e.to_string()))
}

pub async fn set_ai_content_reduction(ctx: &AppContext, enabled: bool) -> Result<bool, AppError> {
    let value = ctx.core.api.set_ai_content_reduction(enabled).await
        .map_err(|e| AppError::ApiError(e.to_string()))?;
    ctx.audio.tx.send(AudioMessage::RefreshWavePreferences).await
        .map_err(|_| AppError::NotInitialized)?;
    Ok(value)
}
