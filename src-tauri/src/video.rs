use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_VIDEO_DURATION_SECS: f64 = 10.0;
const DEFAULT_MAX_DIMENSION: u32 = 1280;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoProbe {
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

pub fn ffmpeg_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("FFMPEG_PATH") {
        if Path::new(&path).exists() {
            return Ok(PathBuf::from(path));
        }
    }
    for candidate in ["ffmpeg", "/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg"] {
        if Command::new(candidate)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Ok(PathBuf::from(candidate));
        }
    }
    Err(
        "ffmpeg not found. Install with Homebrew (`brew install ffmpeg`) or set FFMPEG_PATH."
            .to_string(),
    )
}

pub fn ffprobe_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("FFPROBE_PATH") {
        if Path::new(&path).exists() {
            return Ok(PathBuf::from(path));
        }
    }
    let ffmpeg = ffmpeg_path().ok();
    if let Some(ref ff) = ffmpeg {
        let sibling = ff.with_file_name("ffprobe");
        if sibling.exists()
            && Command::new(&sibling)
                .arg("-version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        {
            return Ok(sibling);
        }
    }
    for candidate in ["ffprobe", "/opt/homebrew/bin/ffprobe", "/usr/local/bin/ffprobe"] {
        if Command::new(candidate)
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Ok(PathBuf::from(candidate));
        }
    }
    Err(
        "ffprobe not found. Install with Homebrew (`brew install ffmpeg`) or set FFPROBE_PATH."
            .to_string(),
    )
}

pub fn validate_duration(duration_secs: f64, max_duration_secs: f64) -> Result<(), String> {
    if duration_secs <= 0.0 {
        return Err("Video has no readable duration.".to_string());
    }
    if duration_secs > max_duration_secs {
        return Err(format!(
            "Video is {:.1}s long; maximum allowed is {:.0}s.",
            duration_secs, max_duration_secs
        ));
    }
    Ok(())
}

pub fn probe_video(path: &str) -> Result<VideoProbe, String> {
    let ffprobe = ffprobe_path()?;
    let output = Command::new(&ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
            path,
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe failed: {}", stderr.trim()));
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Invalid ffprobe output: {}", e))?;

    let duration_secs = json
        .pointer("/format/duration")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let stream = json
        .pointer("/streams/0")
        .ok_or("No video stream found in file")?;

    let width = stream
        .get("width")
        .and_then(|v| v.as_u64())
        .ok_or("Missing video width")? as u32;
    let height = stream
        .get("height")
        .and_then(|v| v.as_u64())
        .ok_or("Missing video height")? as u32;

    let fps = parse_frame_rate(
        stream
            .get("r_frame_rate")
            .and_then(|v| v.as_str())
            .unwrap_or("24/1"),
    );

    Ok(VideoProbe {
        duration_secs,
        width,
        height,
        fps,
    })
}

fn parse_frame_rate(rate: &str) -> f64 {
    if let Some((num, den)) = rate.split_once('/') {
        let n: f64 = num.parse().unwrap_or(24.0);
        let d: f64 = den.parse().unwrap_or(1.0);
        if d > 0.0 {
            n / d
        } else {
            24.0
        }
    } else {
        rate.parse().unwrap_or(24.0)
    }
}

pub fn extract_frames(
    video_path: &str,
    output_dir: &Path,
    processing_fps: f64,
    max_duration_secs: f64,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(output_dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let pattern = output_dir.join("frame_%04d.png");

    let vf = format!(
        "fps={},scale='min({},iw)':-2",
        processing_fps, DEFAULT_MAX_DIMENSION
    );

    let ffmpeg = ffmpeg_path()?;
    let output = Command::new(&ffmpeg)
        .arg("-y")
        .arg("-i")
        .arg(video_path)
        .arg("-t")
        .arg(format!("{}", max_duration_secs))
        .arg("-vf")
        .arg(vf)
        .arg(&pattern_str)
        .output()
        .map_err(|e| format!("Failed to run ffmpeg extract: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg extract failed: {}", stderr.trim()));
    }

    let mut frames: Vec<PathBuf> = std::fs::read_dir(output_dir)
        .map_err(|e| format!("Failed to read frames dir: {}", e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
        })
        .collect();
    frames.sort();

    if frames.is_empty() {
        return Err("No frames extracted from video.".to_string());
    }

    Ok(frames)
}

pub fn encode_video_from_frames(
    frame_paths: &[PathBuf],
    output_path: &Path,
    processing_fps: f64,
    audio_source_video: &str,
) -> Result<(), String> {
    if frame_paths.is_empty() {
        return Err("No frames to encode.".to_string());
    }

    let temp_dir = output_path
        .parent()
        .ok_or("Invalid output path")?
        .join(format!("encode_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    for (i, src) in frame_paths.iter().enumerate() {
        let dest = temp_dir.join(format!("frame_{:04}.png", i + 1));
        std::fs::copy(src, &dest).map_err(|e| format!("Failed to copy frame: {}", e))?;
    }

    let input_pattern = temp_dir.join("frame_%04d.png");
    let ffmpeg = ffmpeg_path()?;

    let mut cmd = Command::new(&ffmpeg);
    cmd.arg("-y")
        .arg("-framerate")
        .arg(format!("{}", processing_fps))
        .arg("-i")
        .arg(input_pattern.to_string_lossy().as_ref())
        .arg("-i")
        .arg(audio_source_video)
        .arg("-map")
        .arg("0:v:0")
        .arg("-map")
        .arg("1:a?")
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-shortest")
        .arg(output_path);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run ffmpeg encode: {}", e))?;

    let _ = std::fs::remove_dir_all(&temp_dir);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg encode failed: {}", stderr.trim()));
    }

    Ok(())
}

pub fn max_video_duration_secs() -> f64 {
    MAX_VIDEO_DURATION_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frame_rate_fraction() {
        assert!((parse_frame_rate("30000/1001") - 29.97).abs() < 0.01);
        assert!((parse_frame_rate("24/1") - 24.0).abs() < 0.001);
    }

    #[test]
    fn validate_duration_rejects_long_clips() {
        assert!(validate_duration(10.5, 10.0).is_err());
        assert!(validate_duration(9.9, 10.0).is_ok());
    }
}
