use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use dbx_core::docs::{CollectOptions, SchemaSnapshot};
use dbx_core::models::connection::ConnectionConfig;
use serde::Deserialize;

use crate::error::AppError;
use crate::state::WebState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocsSnapshotRequest {
    pub connection_id: String,
    pub database: String,
    #[serde(default)]
    pub schemas: Vec<String>,
    #[serde(default)]
    pub tables: Vec<String>,
    #[serde(default)]
    pub project_name: Option<String>,
}

async fn load_connection(state: &Arc<WebState>, connection_id: &str) -> Result<ConnectionConfig, AppError> {
    state
        .app
        .storage
        .load_connections()
        .await
        .map_err(AppError::from)?
        .into_iter()
        .find(|config| config.id == connection_id)
        .ok_or_else(|| AppError::from(format!("Connection with id '{connection_id}' not found")))
}

pub async fn collect_snapshot(
    State(state): State<Arc<WebState>>,
    Json(request): Json<DocsSnapshotRequest>,
) -> Result<Json<SchemaSnapshot>, AppError> {
    let connection = load_connection(&state, &request.connection_id).await?;

    let options = CollectOptions {
        database: request.database.clone(),
        schemas: request.schemas.clone(),
        tables: request.tables.clone(),
        project_name: request.project_name.clone().unwrap_or_else(|| connection.name.clone()),
    };

    let snapshot =
        dbx_core::docs::collect_snapshot(&state.app, &connection, &options, &|_progress| {}, &AtomicBool::new(false))
            .await
            .map_err(AppError::from)?;

    Ok(Json(snapshot))
}
