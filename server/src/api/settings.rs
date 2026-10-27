//! The Owner's settings: `GET`/`PUT /api/v1/settings` and
//! `POST /api/v1/settings/preview`.

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use serde::{Deserialize, Serialize};

use crate::auth::Owner;
use crate::error::ApiError;
use crate::image_type::ImageType;
use crate::path_template::{PRESETS, Template, UploadFacts};
use crate::random::SeededRandom;
use crate::settings;
use crate::state::AppState;

/// The made-up upload every example Path is rendered for.
const EXAMPLE_FILENAME: &str = "holiday photo.jpg";
const EXAMPLE_SHA256: &str = "3a7bd3e2360a3d29eea436fcfb7e44c735d117c42d1c1835420b6b9942dd4f1b";
/// Fixed, so an example stays the same while the Owner types.
const EXAMPLE_SEED: u64 = 0x1bed;

#[derive(Serialize)]
pub struct PresetView {
    pub id: &'static str,
    pub name: &'static str,
    pub template: &'static str,
    pub example: String,
}

#[derive(Serialize)]
pub struct SettingsView {
    /// The current Path Template.
    pub path_template: String,
    /// The id of the Preset it is, or null for a Custom template.
    pub preset: Option<&'static str>,
    /// An example Path the current template makes.
    pub example: String,
    pub presets: Vec<PresetView>,
}

#[derive(Serialize)]
pub struct PreviewView {
    pub path_template: String,
    pub preset: Option<&'static str>,
    pub example: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsUpdate {
    /// Omitted = unchanged.
    pub path_template: Option<String>,
}

#[derive(Deserialize)]
pub struct PreviewRequest {
    pub path_template: String,
}

/// An example Path for `template`, for an upload of `holiday photo.jpg` now.
fn example(state: &AppState, template: &Template) -> String {
    let facts = UploadFacts {
        image_type: ImageType::Jpeg,
        original_filename: EXAMPLE_FILENAME,
        sha256: EXAMPLE_SHA256,
        uploaded_at: state.now(),
    };
    template.render(&facts, &SeededRandom::new(EXAMPLE_SEED))
}

async fn view(state: &AppState) -> SettingsView {
    let template = settings::path_template(state).await;
    SettingsView {
        path_template: template.as_str().to_owned(),
        preset: template.preset().map(|p| p.id),
        example: example(state, &template),
        presets: PRESETS
            .iter()
            .map(|p| PresetView {
                id: p.id,
                name: p.name,
                template: p.template,
                example: example(
                    state,
                    &Template::parse(p.template).expect("Presets are valid"),
                ),
            })
            .collect(),
    }
}

fn body<T>(body: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    body.map(|Json(b)| b)
        .map_err(|e| ApiError::BadRequest(e.body_text()))
}

/// `GET /api/v1/settings`.
pub async fn get(State(state): State<AppState>, _owner: Owner) -> Json<SettingsView> {
    Json(view(&state).await)
}

/// `PUT /api/v1/settings`: change the given settings. A new Path Template
/// applies from the next upload on; existing Paths never change.
pub async fn update(
    State(state): State<AppState>,
    _owner: Owner,
    update: Result<Json<SettingsUpdate>, JsonRejection>,
) -> Result<Json<SettingsView>, ApiError> {
    let update = body(update)?;
    if let Some(source) = update.path_template {
        let template = Template::parse(&source)?;
        settings::set_path_template(&state, template).await?;
    }
    Ok(Json(view(&state).await))
}

/// `POST /api/v1/settings/preview`: validate a Path Template without saving
/// it and answer an example Path, or the error saving it would give.
pub async fn preview(
    State(state): State<AppState>,
    _owner: Owner,
    request: Result<Json<PreviewRequest>, JsonRejection>,
) -> Result<Json<PreviewView>, ApiError> {
    let request = body(request)?;
    let template = Template::parse(&request.path_template)?;
    Ok(Json(PreviewView {
        path_template: request.path_template,
        preset: template.preset().map(|p| p.id),
        example: example(&state, &template),
    }))
}
