import { invoke } from "@tauri-apps/api/core";
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
}

interface RunResult {
  history_id: number;
  result_image_base64: string;
  prompt_id: string;
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
let resultImageBase64: string | null = null;
let isProcessing = false;

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
          <span>All required nodes found</span>
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
              return `
                <div class="history-thumb" data-id="${entry.id}">
                  <img src="data:image/png;base64,${base64}" alt="Result">
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
    
    if (entry && entry.result_image_path && entry.target_image_path) {
      resultImageBase64 = await invoke<string>("read_image_file", {
        path: entry.result_image_path,
      });
      targetImageBase64 = await invoke<string>("read_image_file", {
        path: entry.target_image_path,
      });

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

function updateGoButton() {
  const btn = document.getElementById("go-btn") as HTMLButtonElement;
  if (btn) {
    btn.disabled = !sourceImageBase64 || !targetImageBase64 || isProcessing;
  }
}

async function runFaceSwap() {
  if (!sourceImageBase64 || !targetImageBase64 || isProcessing) return;

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
    const result = await invoke<RunResult>("run_face_swap", {
      sourceImageBase64,
      targetImageBase64,
      faceRestoreVisibility: restoreVisibility,
      codeformerWeight,
      inputFacesIndex: faceIndex,
    });

    resultImageBase64 = result.result_image_base64;

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

function setupSlider(sliderId: string, valueId: string) {
  const slider = document.getElementById(sliderId) as HTMLInputElement;
  const valueEl = document.getElementById(valueId);

  if (slider && valueEl) {
    slider.addEventListener("input", () => {
      valueEl.textContent = parseFloat(slider.value).toFixed(2);
    });
  }
}

function setupEventListeners() {
  listen<ProgressUpdate>("progress", (event) => {
    const progressFill = document.getElementById("progress-fill");
    const progressText = document.getElementById("progress-text");

    if (progressFill) {
      progressFill.style.width = `${event.payload.percent}%`;
    }
    if (progressText) {
      const stages: Record<string, string> = {
        uploading: "Uploading images...",
        queued: "Queued, waiting...",
        executing: "Processing...",
        sampling: "Generating...",
        fetching: "Downloading result...",
        complete: "Complete!",
      };
      progressText.textContent = stages[event.payload.stage] || event.payload.stage;
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
    selectImage("dropzone-target", "target");
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
}

async function init() {
  await loadSettings();
  await loadRecipes();
  await loadHistory();
  setupEventListeners();
  showScreen("home");
}

window.addEventListener("DOMContentLoaded", init);
