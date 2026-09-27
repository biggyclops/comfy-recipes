mod comfyui;
mod database;
mod recipes;
mod settings;
mod video;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use comfyui::{ComfyClient, HealthCheck, ProgressUpdate};
use database::{Database, HistoryEntry, NewHistoryEntry};
use recipes::{FaceSwapHairParams, FaceSwapParams, Recipe, FACE_HAIR_REQUIRED_MODELS, FACE_HAIR_REQUIRED_NODES};
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;
use video::{encode_video_from_frames, extract_frames, probe_video, validate_duration, VideoProbe};

struct AppState {
    db: Database,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunResult {
    history_id: i64,
    result_image_base64: String,
    prompt_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VideoRunResult {
    history_id: i64,
    result_video_path: String,
    result_poster_base64: String,
    prompt_id: String,
}

async fn swap_single_frame(
    client: &ComfyClient,
    app: Option<&AppHandle>,
    source_comfy_name: &str,
    target_data: &[u8],
    target_filename: &str,
    face_restore_visibility: f32,
    codeformer_weight: f32,
    input_faces_index: &str,
    relay_comfy_progress: bool,
) -> Result<(Vec<u8>, String), String> {
    let target_upload = client
        .upload_image(target_data, target_filename)
        .await
        .map_err(|e| format!("Failed to upload target frame: {}", e))?;

    let params = FaceSwapParams {
        source_image: source_comfy_name.to_string(),
        target_image: target_upload.name.clone(),
        face_restore_visibility,
        codeformer_weight,
        input_faces_index: input_faces_index.to_string(),
    };

    let workflow = recipes::build_face_swap_workflow(&params);
    let prompt_response = client
        .queue_prompt(workflow)
        .await
        .map_err(|e| format!("Failed to queue prompt: {}", e))?;

    let prompt_id = prompt_response.prompt_id.clone();

    if relay_comfy_progress {
        if let Some(app) = app {
            let app_clone = app.clone();
            client
                .watch_progress(&prompt_id, move |progress| {
                    app_clone.emit("progress", progress).ok();
                })
                .await
                .map_err(|e| format!("Error during execution: {}", e))?;
        }
    } else {
        client
            .watch_progress(&prompt_id, |_| {})
            .await
            .map_err(|e| format!("Error during execution: {}", e))?;
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

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
        .get_image(
            &output_image.filename,
            &output_image.subfolder,
            &output_image.r#type,
        )
        .await
        .map_err(|e| format!("Failed to download result: {}", e))?;

    Ok((image_data, prompt_id))
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
    Ok(client
        .health_check_extended(
            &required_nodes,
            FACE_HAIR_REQUIRED_NODES,
            FACE_HAIR_REQUIRED_MODELS,
        )
        .await)
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
fn probe_video_file(path: String, max_duration_secs: Option<f64>) -> Result<VideoProbe, String> {
    let probe = probe_video(&path)?;
    let max = max_duration_secs.unwrap_or(video::max_video_duration_secs());
    validate_duration(probe.duration_secs, max)?;
    Ok(probe)
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

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "uploading".to_string(),
            value: 0,
            max: 100,
            percent: 0.0,
        },
    )
    .ok();

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

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "uploading".to_string(),
            value: 50,
            max: 100,
            percent: 50.0,
        },
    )
    .ok();

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "queued".to_string(),
            value: 0,
            max: 100,
            percent: 0.0,
        },
    )
    .ok();

    let (image_data, prompt_id) = swap_single_frame(
        &client,
        Some(&app),
        &source_upload.name,
        &target_data,
        &target_filename,
        face_restore_visibility,
        codeformer_weight,
        &input_faces_index,
        true,
    )
    .await?;

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "fetching".to_string(),
            value: 0,
            max: 100,
            percent: 0.0,
        },
    )
    .ok();

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

    let history_id = state.db.add_entry(NewHistoryEntry {
        recipe: "face_swap".to_string(),
        source_image_path: Some(source_path.to_string_lossy().into_owned()),
        target_image_path: Some(target_path.to_string_lossy().into_owned()),
        target_video_path: None,
        result_image_path: Some(result_path.to_string_lossy().into_owned()),
        result_video_path: None,
        settings: settings_json,
        prompt_id: Some(prompt_id.clone()),
        swap_mode: Some("face_only".to_string()),
        media_type: "image".to_string(),
    })?;

    state.db.cleanup_old(50)?;

    let result_base64 = BASE64.encode(&image_data);

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "complete".to_string(),
            value: 100,
            max: 100,
            percent: 100.0,
        },
    )
    .ok();

    Ok(RunResult {
        history_id,
        result_image_base64: result_base64,
        prompt_id,
    })
}

#[tauri::command]
async fn run_video_face_swap(
    app: AppHandle,
    state: State<'_, AppState>,
    source_image_base64: String,
    target_video_path: String,
    face_restore_visibility: f32,
    codeformer_weight: f32,
    input_faces_index: String,
    max_duration_secs: Option<f64>,
    processing_fps: Option<f64>,
) -> Result<VideoRunResult, String> {
    let max_duration = max_duration_secs.unwrap_or(video::max_video_duration_secs());
    let fps = processing_fps.unwrap_or(12.0).clamp(4.0, 24.0);

    let probe = probe_video(&target_video_path)?;
    validate_duration(probe.duration_secs, max_duration)?;

    let settings = settings::load_settings();
    let client = ComfyClient::new(&settings.comfyui_address);
    let images_dir = database::get_images_dir()?;
    let job_id = Uuid::new_v4();
    let work_dir = std::env::temp_dir().join(format!("comfy_recipes_video_{}", job_id));
    let frames_in = work_dir.join("in");
    let frames_out = work_dir.join("out");

    fs::create_dir_all(&frames_out).map_err(|e| e.to_string())?;

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "extracting".to_string(),
            value: 0,
            max: 100,
            percent: 0.0,
        },
    )
    .ok();

    let frame_paths = extract_frames(
        &target_video_path,
        &frames_in,
        fps,
        max_duration,
    )?;
    let frame_count = frame_paths.len();

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "uploading".to_string(),
            value: 0,
            max: frame_count as u32,
            percent: 0.0,
        },
    )
    .ok();

    let source_data = BASE64
        .decode(&source_image_base64)
        .map_err(|e| format!("Invalid source image: {}", e))?;
    let source_filename = format!("source_{}.png", job_id);
    let source_upload = client
        .upload_image(&source_data, &source_filename)
        .await
        .map_err(|e| format!("Failed to upload source image: {}", e))?;

    let source_path = images_dir.join(&source_filename);
    fs::write(&source_path, &source_data).map_err(|e| format!("Failed to save source: {}", e))?;

    let target_video_copy = images_dir.join(format!("target_{}.mp4", job_id));
    fs::copy(&target_video_path, &target_video_copy)
        .map_err(|e| format!("Failed to copy target video: {}", e))?;

    let mut swapped_paths: Vec<PathBuf> = Vec::with_capacity(frame_count);
    let mut last_prompt_id = String::new();

    for (index, frame_path) in frame_paths.iter().enumerate() {
        let frame_num = index + 1;
        app.emit(
            "progress",
            ProgressUpdate {
                stage: "frame".to_string(),
                value: frame_num as u32,
                max: frame_count as u32,
                percent: (frame_num as f32 / frame_count as f32) * 100.0,
            },
        )
        .ok();

        let target_data =
            fs::read(frame_path).map_err(|e| format!("Failed to read frame: {}", e))?;
        let target_filename = format!("frame_{:04}.png", frame_num);

        let (swapped_data, prompt_id) = swap_single_frame(
            &client,
            None,
            &source_upload.name,
            &target_data,
            &target_filename,
            face_restore_visibility,
            codeformer_weight,
            &input_faces_index,
            false,
        )
        .await?;
        last_prompt_id = prompt_id;

        let out_path = frames_out.join(format!("frame_{:04}.png", frame_num));
        fs::write(&out_path, &swapped_data)
            .map_err(|e| format!("Failed to write swapped frame: {}", e))?;
        swapped_paths.push(out_path);
    }

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "encoding".to_string(),
            value: 0,
            max: 100,
            percent: 0.0,
        },
    )
    .ok();

    let result_video_path = images_dir.join(format!("result_{}.mp4", job_id));
    encode_video_from_frames(
        &swapped_paths,
        &result_video_path,
        fps,
        &target_video_path,
    )?;

    let poster_path = images_dir.join(format!("poster_{}.png", job_id));
    if let Some(first) = swapped_paths.first() {
        fs::copy(first, &poster_path).map_err(|e| format!("Failed to save poster: {}", e))?;
    }

    let _ = fs::remove_dir_all(&work_dir);

    let settings_json = serde_json::json!({
        "face_restore_visibility": face_restore_visibility,
        "codeformer_weight": codeformer_weight,
        "input_faces_index": input_faces_index,
        "processing_fps": fps,
        "frame_count": frame_count,
        "max_duration_secs": max_duration,
    });

    let poster_base64 = fs::read(&poster_path)
        .map(|b| BASE64.encode(b))
        .unwrap_or_default();

    let history_id = state.db.add_entry(NewHistoryEntry {
        recipe: "face_swap".to_string(),
        source_image_path: Some(source_path.to_string_lossy().into_owned()),
        target_image_path: None,
        target_video_path: Some(target_video_copy.to_string_lossy().into_owned()),
        result_image_path: Some(poster_path.to_string_lossy().into_owned()),
        result_video_path: Some(result_video_path.to_string_lossy().into_owned()),
        settings: settings_json,
        prompt_id: Some(last_prompt_id.clone()),
        swap_mode: Some("face_only".to_string()),
        media_type: "video".to_string(),
    })?;

    state.db.cleanup_old(50)?;

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "complete".to_string(),
            value: 100,
            max: 100,
            percent: 100.0,
        },
    )
    .ok();

    Ok(VideoRunResult {
        history_id,
        result_video_path: result_video_path.to_string_lossy().into_owned(),
        result_poster_base64: poster_base64,
        prompt_id: last_prompt_id,
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

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "uploading".to_string(),
            value: 0,
            max: 100,
            percent: 0.0,
        },
    )
    .ok();

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

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "uploading".to_string(),
            value: 50,
            max: 100,
            percent: 50.0,
        },
    )
    .ok();

    let target_upload = client
        .upload_image(&target_data, &target_filename)
        .await
        .map_err(|e| format!("Failed to upload target image: {}", e))?;

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "queued".to_string(),
            value: 0,
            max: 100,
            percent: 0.0,
        },
    )
    .ok();

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

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "fetching".to_string(),
            value: 0,
            max: 100,
            percent: 0.0,
        },
    )
    .ok();

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
        .get_image(
            &output_image.filename,
            &output_image.subfolder,
            &output_image.r#type,
        )
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

    let history_id = state.db.add_entry(NewHistoryEntry {
        recipe: "face_swap".to_string(),
        source_image_path: Some(source_path.to_string_lossy().into_owned()),
        target_image_path: Some(target_path.to_string_lossy().into_owned()),
        target_video_path: None,
        result_image_path: Some(result_path.to_string_lossy().into_owned()),
        result_video_path: None,
        settings: settings_json,
        prompt_id: Some(prompt_id.clone()),
        swap_mode: Some("face_hair".to_string()),
        media_type: "image".to_string(),
    })?;

    state.db.cleanup_old(50)?;

    let result_base64 = BASE64.encode(&image_data);

    app.emit(
        "progress",
        ProgressUpdate {
            stage: "complete".to_string(),
            value: 100,
            max: 100,
            percent: 100.0,
        },
    )
    .ok();

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

#[tauri::command]
fn copy_file_to_path(source_path: String, dest_path: String) -> Result<(), String> {
    if let Some(parent) = Path::new(&dest_path).parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    fs::copy(&source_path, &dest_path).map_err(|e| format!("Failed to copy file: {}", e))?;
    Ok(())
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
            probe_video_file,
            run_face_swap,
            run_face_swap_hair,
            run_video_face_swap,
            get_history,
            get_history_entry,
            read_image_file,
            save_image_to_path,
            copy_file_to_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
