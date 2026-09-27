import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
// @ts-ignore - Plugin types may not be available at build time
import { save, open } from "@tauri-apps/plugin-dialog";
// @ts-ignore - Plugin types may not be available at build time
import { readFile } from "@tauri-apps/plugin-fs";

interface Settings {
  comfyui_address: string;
}

interface HealthCheck {
  reachable: boolean;
  gpu_name: string | null;
  vram_total_gb: number | null;
  vram_free_gb: number | null;
  comfyui_version: string | null;
  missing_nodes: string[];
  missing_face_hair_nodes: string[];
  missing_face_hair_models: string[];
  face_hair_available: boolean;
  face_hair_unavailable_reason: string | null;
  error: string | null;
}

interface Recipe {
  id: string;
  name: string;
  description: string;
  required_nodes: string[];
}

interface HistoryEntry {
  id: number;
  recipe: string;
  created_at: string;
  source_image_path: string | null;
  target_image_path: string | null;
  result_image_path: string | null;
  settings_json: string;
  prompt_id: string | null;
  swap_mode: string | null;
  media_type: string | null;
  result_video_path: string | null;
  target_video_path: string | null;
}

interface RunResult {
  history_id: number;
  result_image_base64: string;
  prompt_id: string;
}

interface VideoRunResult {
  history_id: number;
  result_video_path: string;
  result_poster_base64: string;
  prompt_id: string;
}

interface VideoProbe {
  duration_secs: number;
  width: number;
  height: number;
  fps: number;
}

interface ProgressUpdate {
  stage: string;
  value: number;
  max: number;
  percent: number;
}

let _currentScreen = "home";
let settings: Settings = { comfyui_address: "http://hades:8188" };
let healthStatus: HealthCheck | null = null;
let sourceImageBase64: string | null = null;
let targetImageBase64: string | null = null;
let targetVideoPath: string | null = null;
let targetType: "photo" | "video" = "photo";
let resultImageBase64: string | null = null;
let resultVideoPath: string | null = null;
let isProcessing = false;
let currentSwapMode: "face_only" | "face_hair" = "face_only";

const screens = ["home", "connect", "face-swap", "result"];

function showScreen(screenId: string) {
  screens.forEach((id) => {
    const el = document.getElementById(`screen-${id}`);
    if (el) {
      el.classList.toggle("active", id === screenId);
    }
  });
  _currentScreen = screenId;
}

function showError(message: string) {
  const container = document.getElementById("error-container");
  if (container) {
    container.innerHTML = `<div class="error-message">${message}</div>`;
    setTimeout(() => {
      container.innerHTML = "";
    }, 5000);
  }
}

async function loadSettings() {
  try {
    settings = await invoke<Settings>("get_settings");
  } catch (e) {
    console.error("Failed to load settings:", e);
  }
}

async function saveSettings() {
  try {
    await invoke("save_settings", { settings });
  } catch (e) {
    console.error("Failed to save settings:", e);
    showError("Failed to save settings");
  }
}

async function testConnection() {
  const btn = document.getElementById("test-connection-btn") as HTMLButtonElement;
  const statusList = document.getElementById("connection-status");

  if (btn) {
    btn.disabled = true;
    btn.innerHTML = '<span class="loading-spinner"></span> Testing...';
  }

  if (statusList) {
    statusList.innerHTML = "";
  }

  try {
    const addressInput = document.getElementById("server-address") as HTMLInputElement;
    if (addressInput) {
      settings.comfyui_address = addressInput.value;
      await saveSettings();
    }

    healthStatus = await invoke<HealthCheck>("test_connection", {
      address: settings.comfyui_address,
    });

    renderConnectionStatus();
  } catch (e) {
    showError(`Connection test failed: ${e}`);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = "Test Connection";
    }
  }
}

function renderConnectionStatus() {
  const statusList = document.getElementById("connection-status");
  const fixInstructions = document.getElementById("fix-instructions");

  if (!statusList || !healthStatus) return;

  let html = "";

  if (healthStatus.reachable) {
    html += `
      <li class="status-item">
        <span class="status-icon status-ok">✓</span>
        <span>Connected to ComfyUI</span>
      </li>
    `;

    if (healthStatus.gpu_name) {
      const vramInfo = healthStatus.vram_total_gb
        ? ` (${healthStatus.vram_free_gb?.toFixed(1)}/${healthStatus.vram_total_gb?.toFixed(1)} GB free)`
        : "";
      html += `
        <li class="status-item">
          <span class="status-icon status-ok">✓</span>
          <span>GPU: ${healthStatus.gpu_name}${vramInfo}</span>
        </li>
      `;
    }

    if (healthStatus.missing_nodes.length === 0) {
      html += `
        <li class="status-item">
          <span class="status-icon status-ok">✓</span>
          <span>Face only: Ready</span>
        </li>
      `;
    } else {
      html += `
        <li class="status-item">
          <span class="status-icon status-error">✗</span>
          <span>Missing nodes: ${healthStatus.missing_nodes.join(", ")}</span>
        </li>
      `;
    }

    if (healthStatus.face_hair_available) {
      html += `
        <li class="status-item">
          <span class="status-icon status-ok">✓</span>
          <span>Face + hair: Ready</span>
        </li>
      `;
    } else {
      html += `
        <li class="status-item">
          <span class="status-icon status-error">!</span>
          <span>Face + hair: Not available</span>
        </li>
      `;
    }
  } else {
    html += `
      <li class="status-item">
        <span class="status-icon status-error">✗</span>
        <span>${healthStatus.error || "Cannot connect to server"}</span>
      </li>
    `;
  }

  statusList.innerHTML = html;

  if (fixInstructions) {
    if (healthStatus.missing_nodes.length > 0 || !healthStatus.reachable) {
      invoke<string[]>("get_fix_instructions", {
        missingNodes: healthStatus.missing_nodes,
      }).then((instructions) => {
        if (instructions.length > 0) {
          fixInstructions.classList.remove("hidden");
          fixInstructions.innerHTML = `
            <h4>How to fix:</h4>
            ${instructions.map((i) => `<pre>${i}</pre>`).join("")}
          `;
        }
      });
    } else {
      fixInstructions.classList.add("hidden");
    }
  }

  updateFaceHairAvailability();
  updateTargetTypeVisibility();
}

function updateFaceHairAvailability() {
  const faceHairRadio = document.getElementById("mode-face-hair") as HTMLInputElement;
  const faceHairLabel = document.getElementById("mode-face-hair-label");
  const unavailableDiv = document.getElementById("face-hair-unavailable");

  if (!faceHairRadio || !faceHairLabel || !unavailableDiv) return;

  if (healthStatus && healthStatus.face_hair_available) {
    faceHairRadio.disabled = false;
    faceHairLabel.classList.remove("disabled");
    unavailableDiv.classList.add("hidden");
  } else {
    faceHairRadio.disabled = true;
    faceHairLabel.classList.add("disabled");
    if (currentSwapMode === "face_hair") {
      currentSwapMode = "face_only";
      const faceOnlyRadio = document.querySelector(
        'input[name="swap-mode"][value="face_only"]'
      ) as HTMLInputElement;
      if (faceOnlyRadio) faceOnlyRadio.checked = true;
      updateFaceHairOptionsVisibility();
    }
    if (healthStatus && healthStatus.face_hair_unavailable_reason) {
      unavailableDiv.textContent = healthStatus.face_hair_unavailable_reason;
      unavailableDiv.classList.remove("hidden");
    } else {
      unavailableDiv.textContent = "Check server connection to enable Face + hair mode";
      unavailableDiv.classList.remove("hidden");
    }
  }
}

function updateFaceHairOptionsVisibility() {
  const options = document.getElementById("face-hair-options");
  if (options) {
    if (currentSwapMode === "face_hair") {
      options.classList.remove("hidden");
    } else {
      options.classList.add("hidden");
    }
  }
}

function updateTargetTypeVisibility() {
  const targetTypeGroup = document.getElementById("target-type-group");
  const videoOptions = document.getElementById("video-options");
  const isFaceOnly = currentSwapMode === "face_only";

  if (targetTypeGroup) {
    targetTypeGroup.classList.toggle("hidden", !isFaceOnly);
  }
  if (videoOptions) {
    videoOptions.classList.toggle("hidden", !isFaceOnly || targetType !== "video");
  }
  if (!isFaceOnly && targetType === "video") {
    targetType = "photo";
    targetVideoPath = null;
    targetImageBase64 = null;
    const photoRadio = document.querySelector(
      'input[name="target-type"][value="photo"]'
    ) as HTMLInputElement;
    if (photoRadio) photoRadio.checked = true;
    resetTargetDropzone();
  }
  updateGoButton();
}

function resetTargetDropzone() {
  const dropzone = document.getElementById("dropzone-target");
  const hint = document.getElementById("dropzone-target-hint");
  const probeInfo = document.getElementById("video-probe-info");
  if (dropzone) {
    dropzone.classList.remove("has-image");
    dropzone.innerHTML = `
      <div class="dropzone-label">
        <div class="icon">${targetType === "video" ? "🎬" : "🖼️"}</div>
        <div id="dropzone-target-hint">Click to select ${targetType === "video" ? "video" : "image"}</div>
      </div>
    `;
  }
  if (hint && !dropzone) {
    hint.textContent = `Click to select ${targetType === "video" ? "video" : "image"}`;
  }
  if (probeInfo) {
    probeInfo.classList.add("hidden");
    probeInfo.textContent = "";
  }
}

async function loadRecipes() {
  try {
    const recipes = await invoke<Recipe[]>("get_recipes");
    const grid = document.getElementById("recipe-grid");

    if (grid) {
      grid.innerHTML = recipes
        .map(
          (r) => `
          <div class="recipe-card" data-recipe="${r.id}">
            <h3>${r.name}</h3>
            <p>${r.description}</p>
          </div>
        `
        )
        .join("");

      grid.querySelectorAll(".recipe-card").forEach((card) => {
        card.addEventListener("click", () => {
          const recipeId = card.getAttribute("data-recipe");
          if (recipeId === "face_swap") {
            showScreen("face-swap");
          }
        });
      });
    }
  } catch (e) {
    console.error("Failed to load recipes:", e);
  }
}

function showResultView(isVideo: boolean) {
  const imageSection = document.getElementById("result-image-section");
  const videoSection = document.getElementById("result-video-section");
  if (imageSection) imageSection.classList.toggle("hidden", isVideo);
  if (videoSection) videoSection.classList.toggle("hidden", !isVideo);
}

async function loadHistory() {
  try {
    const history = await invoke<HistoryEntry[]>("get_history", { limit: 10 });
    const grid = document.getElementById("history-grid");

    if (grid && history.length > 0) {
      const thumbs = await Promise.all(
        history.map(async (entry) => {
          if (entry.result_image_path) {
            try {
              const base64 = await invoke<string>("read_image_file", {
                path: entry.result_image_path,
              });
              const isVideo = entry.media_type === "video";
              const modeBadge = isVideo
                ? "VID"
                : entry.swap_mode === "face_hair"
                  ? "F+H"
                  : "F";
              return `
                <div class="history-thumb" data-id="${entry.id}">
                  <img src="data:image/png;base64,${base64}" alt="Result">
                  <span class="mode-badge">${modeBadge}</span>
                </div>
              `;
            } catch {
              return "";
            }
          }
          return "";
        })
      );

      grid.innerHTML = thumbs.filter((t) => t).join("");

      grid.querySelectorAll(".history-thumb").forEach((thumb) => {
        thumb.addEventListener("click", async () => {
          const id = parseInt(thumb.getAttribute("data-id") || "0");
          await loadHistoryEntry(id);
        });
      });
    }
  } catch (e) {
    console.error("Failed to load history:", e);
  }
}

async function loadHistoryEntry(id: number) {
  try {
    const entry = await invoke<HistoryEntry | null>("get_history_entry", { id });

    if (!entry) return;

    if (entry.media_type === "video" && entry.result_video_path) {
      resultVideoPath = entry.result_video_path;
      resultImageBase64 = entry.result_image_path
        ? await invoke<string>("read_image_file", { path: entry.result_image_path })
        : null;

      showResultView(true);
      const videoEl = document.getElementById("result-video") as HTMLVideoElement;
      if (videoEl) {
        videoEl.src = convertFileSrc(resultVideoPath);
      }
      showScreen("result");
      return;
    }

    if (entry.result_image_path && entry.target_image_path) {
      resultVideoPath = null;
      resultImageBase64 = await invoke<string>("read_image_file", {
        path: entry.result_image_path,
      });
      targetImageBase64 = await invoke<string>("read_image_file", {
        path: entry.target_image_path,
      });

      showResultView(false);
      const resultImg = document.getElementById("result-after") as HTMLImageElement;
      const beforeImg = document.getElementById("result-before") as HTMLImageElement;

      if (resultImg) resultImg.src = `data:image/png;base64,${resultImageBase64}`;
      if (beforeImg) beforeImg.src = `data:image/png;base64,${targetImageBase64}`;

      showScreen("result");
    }
  } catch (e) {
    showError(`Failed to load history entry: ${e}`);
  }
}

async function selectImage(dropzoneId: string, type: "source" | "target") {
  try {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }],
    });

    if (selected) {
      const data = await readFile(selected);
      const base64 = btoa(String.fromCharCode(...data));

      if (type === "source") {
        sourceImageBase64 = base64;
      } else {
        targetImageBase64 = base64;
        targetVideoPath = null;
      }

      const dropzone = document.getElementById(dropzoneId);
      if (dropzone) {
        dropzone.innerHTML = `<img src="data:image/png;base64,${base64}" alt="${type}">`;
        dropzone.classList.add("has-image");
      }

      updateGoButton();
    }
  } catch (e) {
    console.error("Failed to select image:", e);
  }
}

async function selectTargetVideo() {
  try {
    const selected = await open({
      multiple: false,
      filters: [
        { name: "Video", extensions: ["mp4", "mov", "m4v"] },
      ],
    });

    if (!selected) return;

    const probe = await invoke<VideoProbe>("probe_video_file", {
      path: selected,
      maxDurationSecs: 10,
    });

    targetVideoPath = selected;
    targetImageBase64 = null;

    const dropzone = document.getElementById("dropzone-target");
    const probeInfo = document.getElementById("video-probe-info");
    if (dropzone) {
      dropzone.classList.add("has-image");
      dropzone.innerHTML = `
        <div class="dropzone-label">
          <div class="icon">🎬</div>
          <div>${selected.split(/[/\\]/).pop()}</div>
        </div>
      `;
    }
    if (probeInfo) {
      probeInfo.classList.remove("hidden");
      probeInfo.textContent = `${probe.duration_secs.toFixed(1)}s · ${probe.width}×${probe.height} · ${probe.fps.toFixed(1)} fps native`;
    }

    updateGoButton();
  } catch (e) {
    showError(`${e}`);
    targetVideoPath = null;
    updateGoButton();
  }
}

function updateGoButton() {
  const btn = document.getElementById("go-btn") as HTMLButtonElement;
  if (btn) {
    const hasTarget =
      targetType === "video" ? !!targetVideoPath : !!targetImageBase64;
    btn.disabled = !sourceImageBase64 || !hasTarget || isProcessing;
  }
}

async function runFaceSwap() {
  const hasTarget =
    targetType === "video" ? !!targetVideoPath : !!targetImageBase64;
  if (!sourceImageBase64 || !hasTarget || isProcessing) return;

  isProcessing = true;
  updateGoButton();

  const progressContainer = document.getElementById("progress-container");
  const progressFill = document.getElementById("progress-fill");
  const progressText = document.getElementById("progress-text");

  if (progressContainer) progressContainer.classList.remove("hidden");
  if (progressFill) progressFill.style.width = "0%";
  if (progressText) progressText.textContent = "Starting...";

  const restoreVisibility = parseFloat(
    (document.getElementById("slider-restore") as HTMLInputElement)?.value || "1"
  );
  const codeformerWeight = parseFloat(
    (document.getElementById("slider-codeformer") as HTMLInputElement)?.value || "0.5"
  );
  const faceIndex = (document.getElementById("select-face") as HTMLSelectElement)?.value || "0";

  try {
    if (targetType === "video" && targetVideoPath) {
      const processingFps = parseInt(
        (document.getElementById("slider-video-fps") as HTMLInputElement)?.value || "12"
      );

      const result = await invoke<VideoRunResult>("run_video_face_swap", {
        sourceImageBase64,
        targetVideoPath,
        faceRestoreVisibility: restoreVisibility,
        codeformerWeight,
        inputFacesIndex: faceIndex,
        maxDurationSecs: 10,
        processingFps,
      });

      resultVideoPath = result.result_video_path;
      resultImageBase64 = result.result_poster_base64;

      showResultView(true);
      const videoEl = document.getElementById("result-video") as HTMLVideoElement;
      if (videoEl) {
        videoEl.src = convertFileSrc(result.result_video_path);
      }

      showScreen("result");
      await loadHistory();
      return;
    }

    let result: RunResult;

    if (currentSwapMode === "face_hair") {
      const blendEdges = parseFloat(
        (document.getElementById("slider-blend") as HTMLInputElement)?.value || "0.5"
      );
      const seed = parseInt(
        (document.getElementById("input-seed") as HTMLInputElement)?.value || "0"
      );

      result = await invoke<RunResult>("run_face_swap_hair", {
        sourceImageBase64,
        targetImageBase64,
        blendEdges,
        seed,
        faceRestoreVisibility: restoreVisibility,
        codeformerWeight,
        inputFacesIndex: faceIndex,
      });
    } else {
      result = await invoke<RunResult>("run_face_swap", {
        sourceImageBase64,
        targetImageBase64,
        faceRestoreVisibility: restoreVisibility,
        codeformerWeight,
        inputFacesIndex: faceIndex,
      });
    }

    resultVideoPath = null;
    resultImageBase64 = result.result_image_base64;

    showResultView(false);
    const resultImg = document.getElementById("result-after") as HTMLImageElement;
    const beforeImg = document.getElementById("result-before") as HTMLImageElement;

    if (resultImg) resultImg.src = `data:image/png;base64,${resultImageBase64}`;
    if (beforeImg) beforeImg.src = `data:image/png;base64,${targetImageBase64}`;

    showScreen("result");
    await loadHistory();
  } catch (e) {
    showError(`Face swap failed: ${e}`);
  } finally {
    isProcessing = false;
    updateGoButton();
    if (progressContainer) progressContainer.classList.add("hidden");
  }
}

async function saveResult() {
  if (resultVideoPath) {
    try {
      const path = await save({
        filters: [{ name: "MP4 Video", extensions: ["mp4"] }],
        defaultPath: "face_swap_result.mp4",
      });

      if (path) {
        await invoke("copy_file_to_path", {
          sourcePath: resultVideoPath,
          destPath: path,
        });
      }
    } catch (e) {
      showError(`Failed to save: ${e}`);
    }
    return;
  }

  if (!resultImageBase64) return;

  try {
    const path = await save({
      filters: [{ name: "PNG Image", extensions: ["png"] }],
      defaultPath: "face_swap_result.png",
    });

    if (path) {
      await invoke("save_image_to_path", {
        imageBase64: resultImageBase64,
        path,
      });
    }
  } catch (e) {
    showError(`Failed to save: ${e}`);
  }
}

function setupSlider(sliderId: string, valueId: string, decimals = 2) {
  const slider = document.getElementById(sliderId) as HTMLInputElement;
  const valueEl = document.getElementById(valueId);

  if (slider && valueEl) {
    slider.addEventListener("input", () => {
      valueEl.textContent =
        decimals === 0
          ? parseInt(slider.value, 10).toString()
          : parseFloat(slider.value).toFixed(decimals);
    });
  }
}

function setupEventListeners() {
  listen<ProgressUpdate>("progress", (event) => {
    const progressFill = document.getElementById("progress-fill");
    const progressText = document.getElementById("progress-text");
    const { stage, value, max, percent } = event.payload;

    if (progressFill) {
      progressFill.style.width = `${percent}%`;
    }
    if (progressText) {
      if (stage === "frame") {
        progressText.textContent = `Frame ${value} / ${max}`;
      } else {
        const stages: Record<string, string> = {
          extracting: "Extracting frames...",
          uploading: "Uploading source face...",
          encoding: "Encoding video...",
          queued: "Queued, waiting...",
          executing: "Processing...",
          sampling: "Generating...",
          fetching: "Downloading result...",
          complete: "Complete!",
        };
        progressText.textContent = stages[stage] || stage;
      }
    }
  });

  document.getElementById("settings-btn")?.addEventListener("click", () => {
    const input = document.getElementById("server-address") as HTMLInputElement;
    if (input) input.value = settings.comfyui_address;
    showScreen("connect");
  });

  document.getElementById("back-to-home")?.addEventListener("click", () => {
    showScreen("home");
  });

  document.getElementById("test-connection-btn")?.addEventListener("click", testConnection);

  document.getElementById("dropzone-source")?.addEventListener("click", () => {
    selectImage("dropzone-source", "source");
  });

  document.getElementById("dropzone-target")?.addEventListener("click", () => {
    if (targetType === "video") {
      selectTargetVideo();
    } else {
      selectImage("dropzone-target", "target");
    }
  });

  document.getElementById("go-btn")?.addEventListener("click", runFaceSwap);

  document.getElementById("save-result-btn")?.addEventListener("click", saveResult);

  document.getElementById("try-again-btn")?.addEventListener("click", () => {
    showScreen("face-swap");
  });

  document.getElementById("tweak-btn")?.addEventListener("click", () => {
    showScreen("face-swap");
  });

  document.getElementById("back-from-swap")?.addEventListener("click", () => {
    showScreen("home");
  });

  document.getElementById("back-from-result")?.addEventListener("click", () => {
    showScreen("home");
  });

  setupSlider("slider-restore", "value-restore");
  setupSlider("slider-codeformer", "value-codeformer");
  setupSlider("slider-blend", "value-blend");
  setupSlider("slider-video-fps", "value-video-fps", 0);

  document.querySelectorAll('input[name="swap-mode"]').forEach((radio) => {
    radio.addEventListener("change", (e) => {
      const target = e.target as HTMLInputElement;
      currentSwapMode = target.value as "face_only" | "face_hair";
      updateFaceHairOptionsVisibility();
      updateTargetTypeVisibility();
    });
  });

  document.querySelectorAll('input[name="target-type"]').forEach((radio) => {
    radio.addEventListener("change", (e) => {
      const target = e.target as HTMLInputElement;
      targetType = target.value as "photo" | "video";
      targetImageBase64 = null;
      targetVideoPath = null;
      resetTargetDropzone();
      updateTargetTypeVisibility();
    });
  });

  document.getElementById("random-seed-btn")?.addEventListener("click", () => {
    const seedInput = document.getElementById("input-seed") as HTMLInputElement;
    if (seedInput) {
      seedInput.value = Math.floor(Math.random() * 2147483647).toString();
    }
  });
}

async function init() {
  await loadSettings();
  await loadRecipes();
  await loadHistory();
  setupEventListeners();
  updateTargetTypeVisibility();
  showScreen("home");
}

window.addEventListener("DOMContentLoaded", init);
