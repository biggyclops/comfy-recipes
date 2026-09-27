mod comfyui;
mod database;
mod recipes;
mod settings;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use comfyui::{ComfyClient, HealthCheck, ProgressUpdate};
use database::{Database, HistoryEntry};
use recipes::{FaceSwapParams, FaceSwapHairParams, Recipe, FACE_HAIR_REQUIRED_NODES, FACE_HAIR_REQUIRED_MODELS};
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::fs;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

struct AppState {
    db: Database,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunResult {
    history_id: i64,
    result_image_base64: String,
    prompt_id: String,
}

#[tauri::command]
fn get_settings() -> Settings {
    settings::load_settings()
}

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> {
    settings::save_settings(&settings)
}

#[tauri::command]
async fn test_connection(address: String) -> Result<HealthCheck, String> {
    let client = ComfyClient::new(&address);
    let required_nodes = recipes::get_required_nodes();
    Ok(client.health_check_extended(
        &required_nodes,
        FACE_HAIR_REQUIRED_NODES,
        FACE_HAIR_REQUIRED_MODELS,
    ).await)
}

#[tauri::command]
fn get_fix_instructions(missing_nodes: Vec<String>) -> Vec<String> {
    recipes::get_fix_instructions(&missing_nodes)
}

#[tauri::command]
fn get_recipes() -> Vec<Recipe> {
    recipes::get_recipes()
}

#[tauri::command]
async fn run_face_swap(
    app: AppHandle,
    state: State<'_, AppState>,
    source_image_base64: String,
    target_image_base64: String,
    face_restore_visibility: f32,
    codeformer_weight: f32,
    input_faces_index: String,
) -> Result<RunResult, String> {
    let settings = settings::load_settings();
    let client = ComfyClient::new(&settings.comfyui_address);

    app.emit("progress", ProgressUpdate {
        stage: "uploading".to_string(),
        value: 0,
        max: 100,
        percent: 0.0,
    }).ok();

    let source_data = BASE64
        .decode(&source_image_base64)
        .map_err(|e| format!("Invalid source image: {}", e))?;
    let target_data = BASE64
        .decode(&target_image_base64)
        .map_err(|e| format!("Invalid target image: {}", e))?;

    let source_filename = format!("source_{}.png", Uuid::new_v4());
    let target_filename = format!("target_{}.png", Uuid::new_v4());

    let source_upload = client
        .upload_image(&source_data, &source_filename)
        .await
        .map_err(|e| format!("Failed to upload source image: {}", e))?;

    app.emit("progress", ProgressUpdate {
        stage: "uploading".to_string(),
        value: 50,
        max: 100,
        percent: 50.0,
    }).ok();

    let target_upload = client
        .upload_image(&target_data, &target_filename)
        .await
        .map_err(|e| format!("Failed to upload target image: {}", e))?;

    app.emit("progress", ProgressUpdate {
        stage: "queued".to_string(),
        value: 0,
        max: 100,
        percent: 0.0,
    }).ok();

    let params = FaceSwapParams {
        source_image: source_upload.name.clone(),
        target_image: target_upload.name.clone(),
        face_restore_visibility,
        codeformer_weight,
        input_faces_index: input_faces_index.clone(),
    };

    let workflow = recipes::build_face_swap_workflow(&params);
    let prompt_response = client
        .queue_prompt(workflow)
        .await
        .map_err(|e| format!("Failed to queue prompt: {}", e))?;

    let prompt_id = prompt_response.prompt_id.clone();
    let app_clone = app.clone();

    client
        .watch_progress(&prompt_id, move |progress| {
            app_clone.emit("progress", progress).ok();
        })
        .await
        .map_err(|e| format!("Error during execution: {}", e))?;

    app.emit("progress", ProgressUpdate {
        stage: "fetching".to_string(),
        value: 0,
        max: 100,
        percent: 0.0,
    }).ok();

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let history = client
        .get_history(&prompt_id)
        .await
        .map_err(|e| format!("Failed to get history: {}", e))?
        .ok_or("No history found for prompt")?;

    let output_image = history
        .outputs
        .values()
        .filter_map(|o| o.images.as_ref())
        .flatten()
        .next()
        .ok_or("No output image found")?;

    let image_data = client
        .get_image(&output_image.filename, &output_image.subfolder, &output_image.r#type)
        .await
        .map_err(|e| format!("Failed to download result: {}", e))?;

    let images_dir = database::get_images_dir()?;
    let result_filename = format!("result_{}.png", Uuid::new_v4());
    let result_path = images_dir.join(&result_filename);
    fs::write(&result_path, &image_data).map_err(|e| format!("Failed to save result: {}", e))?;

    let source_path = images_dir.join(&source_filename);
    fs::write(&source_path, &source_data).map_err(|e| format!("Failed to save source: {}", e))?;

    let target_path = images_dir.join(&target_filename);
    fs::write(&target_path, &target_data).map_err(|e| format!("Failed to save target: {}", e))?;

    let settings_json = serde_json::json!({
        "face_restore_visibility": face_restore_visibility,
        "codeformer_weight": codeformer_weight,
        "input_faces_index": input_faces_index,
    });

    let history_id = state.db.add_entry(
        "face_swap",
        Some(source_path.to_str().unwrap_or("")),
        Some(target_path.to_str().unwrap_or("")),
        Some(result_path.to_str().unwrap_or("")),
        &settings_json,
        Some(&prompt_id),
        Some("face_only"),
    )?;

    state.db.cleanup_old(50)?;

    let result_base64 = BASE64.encode(&image_data);

    app.emit("progress", ProgressUpdate {
        stage: "complete".to_string(),
        value: 100,
        max: 100,
        percent: 100.0,
    }).ok();

    Ok(RunResult {
        history_id,
        result_image_base64: result_base64,
        prompt_id,
    })
}

#[tauri::command]
async fn run_face_swap_hair(
    app: AppHandle,
    state: State<'_, AppState>,
    source_image_base64: String,
    target_image_base64: String,
    blend_edges: f32,
    seed: i64,
    face_restore_visibility: f32,
    codeformer_weight: f32,
    input_faces_index: String,
) -> Result<RunResult, String> {
    let settings = settings::load_settings();
    let client = ComfyClient::new(&settings.comfyui_address);

    app.emit("progress", ProgressUpdate {
        stage: "uploading".to_string(),
        value: 0,
        max: 100,
        percent: 0.0,
    }).ok();

    let source_data = BASE64
        .decode(&source_image_base64)
        .map_err(|e| format!("Invalid source image: {}", e))?;
    let target_data = BASE64
        .decode(&target_image_base64)
        .map_err(|e| format!("Invalid target image: {}", e))?;

    let source_filename = format!("source_{}.png", Uuid::new_v4());
    let target_filename = format!("target_{}.png", Uuid::new_v4());

    let source_upload = client
        .upload_image(&source_data, &source_filename)
        .await
        .map_err(|e| format!("Failed to upload source image: {}", e))?;

    app.emit("progress", ProgressUpdate {
        stage: "uploading".to_string(),
        value: 50,
        max: 100,
        percent: 50.0,
    }).ok();

    let target_upload = client
        .upload_image(&target_data, &target_filename)
        .await
        .map_err(|e| format!("Failed to upload target image: {}", e))?;

    app.emit("progress", ProgressUpdate {
        stage: "queued".to_string(),
        value: 0,
        max: 100,
        percent: 0.0,
    }).ok();

    let params = FaceSwapHairParams {
        source_image: source_upload.name.clone(),
        target_image: target_upload.name.clone(),
        blend_edges,
        seed,
        face_restore_visibility,
        codeformer_weight,
        input_faces_index: input_faces_index.clone(),
    };

    let workflow = recipes::build_face_swap_hair_workflow(&params);
    let prompt_response = client
        .queue_prompt(workflow)
        .await
        .map_err(|e| format!("Failed to queue prompt: {}", e))?;

    let prompt_id = prompt_response.prompt_id.clone();
    let app_clone = app.clone();

    client
        .watch_progress(&prompt_id, move |progress| {
            app_clone.emit("progress", progress).ok();
        })
        .await
        .map_err(|e| format!("Error during execution: {}", e))?;

    app.emit("progress", ProgressUpdate {
        stage: "fetching".to_string(),
        value: 0,
        max: 100,
        percent: 0.0,
    }).ok();

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let history = client
        .get_history(&prompt_id)
        .await
        .map_err(|e| format!("Failed to get history: {}", e))?
        .ok_or("No history found for prompt")?;

    let output_image = history
        .outputs
        .values()
        .filter_map(|o| o.images.as_ref())
        .flatten()
        .next()
        .ok_or("No output image found")?;

    let image_data = client
        .get_image(&output_image.filename, &output_image.subfolder, &output_image.r#type)
        .await
        .map_err(|e| format!("Failed to download result: {}", e))?;

    let images_dir = database::get_images_dir()?;
    let result_filename = format!("result_{}.png", Uuid::new_v4());
    let result_path = images_dir.join(&result_filename);
    fs::write(&result_path, &image_data).map_err(|e| format!("Failed to save result: {}", e))?;

    let source_path = images_dir.join(&source_filename);
    fs::write(&source_path, &source_data).map_err(|e| format!("Failed to save source: {}", e))?;

    let target_path = images_dir.join(&target_filename);
    fs::write(&target_path, &target_data).map_err(|e| format!("Failed to save target: {}", e))?;

    let settings_json = serde_json::json!({
        "blend_edges": blend_edges,
        "seed": seed,
        "face_restore_visibility": face_restore_visibility,
        "codeformer_weight": codeformer_weight,
        "input_faces_index": input_faces_index,
    });

    let history_id = state.db.add_entry(
        "face_swap",
        Some(source_path.to_str().unwrap_or("")),
        Some(target_path.to_str().unwrap_or("")),
        Some(result_path.to_str().unwrap_or("")),
        &settings_json,
        Some(&prompt_id),
        Some("face_hair"),
    )?;

    state.db.cleanup_old(50)?;

    let result_base64 = BASE64.encode(&image_data);

    app.emit("progress", ProgressUpdate {
        stage: "complete".to_string(),
        value: 100,
        max: 100,
        percent: 100.0,
    }).ok();

    Ok(RunResult {
        history_id,
        result_image_base64: result_base64,
        prompt_id,
    })
}

#[tauri::command]
fn get_history(state: State<'_, AppState>, limit: usize) -> Result<Vec<HistoryEntry>, String> {
    state.db.get_recent(limit)
}

#[tauri::command]
fn get_history_entry(state: State<'_, AppState>, id: i64) -> Result<Option<HistoryEntry>, String> {
    state.db.get_entry(id)
}

#[tauri::command]
fn read_image_file(path: String) -> Result<String, String> {
    let data = fs::read(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    Ok(BASE64.encode(&data))
}

#[tauri::command]
fn save_image_to_path(image_base64: String, path: String) -> Result<(), String> {
    let data = BASE64
        .decode(&image_base64)
        .map_err(|e| format!("Invalid image data: {}", e))?;
    fs::write(&path, &data).map_err(|e| format!("Failed to save file: {}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = Database::new().expect("Failed to initialize database");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState { db })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            test_connection,
            get_fix_instructions,
            get_recipes,
            run_face_swap,
            run_face_swap_hair,
            get_history,
            get_history_entry,
            read_image_file,
            save_image_to_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
