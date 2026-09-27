# Comfy Recipes

A Mac desktop app for running ComfyUI face-swap workflows without touching nodes. Pick a recipe, drop in photos, press Go, and get the result back.

## Features

- **Face Swap**: Swap a face from one photo onto another using ReActor
- **Simple UI**: No node graphs - just pick photos and adjust a few sliders
- **History**: Last 50 results saved locally with settings, so you can repeat good results
- **Privacy**: Everything stays on your home network - no accounts, telemetry, or cloud services

## Requirements

### On Your Mac
- macOS 10.15 (Catalina) or later

### On Hades (or your ComfyUI server)
- ComfyUI running with `--listen` flag so other machines can connect
- RTX 3060 12GB or similar GPU with 12GB+ VRAM
- ReActor custom node installed

## Setting Up Hades

### 1. Start ComfyUI with --listen

```bash
cd ComfyUI
python main.py --listen
```

This allows connections from other machines on your network.

### 2. Install ReActor Custom Node

```bash
cd ComfyUI/custom_nodes
git clone https://github.com/Gourieff/ComfyUI-ReActor
cd ComfyUI-ReActor
pip install -r requirements.txt
```

Restart ComfyUI after installation.

### 3. Download Required Models

**Face swap model** (`inswapper_128.onnx`):
- Download from [InsightFace](https://github.com/deepinsight/insightface) or HuggingFace
- Place in `ComfyUI/models/insightface/`

**Face restore model** (choose one):
- `codeformer-v0.1.0.pth` - Better quality, slower
- `GFPGANv1.4.pth` - Faster, good quality

Download from [ReActor releases](https://github.com/Gourieff/ComfyUI-ReActor) or HuggingFace.
Place in `ComfyUI/models/facerestore_models/`

## Installation

### macOS Gatekeeper Warning

This app is unsigned, so macOS will show a security warning on first launch.

**To open the app:**

1. Right-click (or Control-click) on Comfy Recipes
2. Select "Open" from the context menu
3. Click "Open" in the dialog that appears

**Alternative method:**

1. Try to open the app normally (it will be blocked)
2. Go to **System Settings → Privacy & Security**
3. Scroll down and click **"Open Anyway"** next to the Comfy Recipes message

## Development

### Prerequisites

- Node.js 20+
- Rust (latest stable)
- Tauri CLI: `cargo install tauri-cli`

### Setup

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

### Building a Universal Binary

To build a .dmg that works on both Apple Silicon and Intel Macs:

```bash
# Add the targets
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# Build universal binary
npm run tauri build -- --target universal-apple-darwin
```

The .dmg will be in `src-tauri/target/universal-apple-darwin/release/bundle/dmg/`.

## Configuration

Settings are stored in:
- macOS: `~/Library/Application Support/com.comfyrecipes.Comfy Recipes/settings.json`

History database:
- macOS: `~/Library/Application Support/com.comfyrecipes.Comfy Recipes/history.db`

Result images:
- macOS: `~/Library/Application Support/com.comfyrecipes.Comfy Recipes/images/`

## Troubleshooting

### "Cannot connect to server"
- Make sure ComfyUI is running with `--listen`
- Try using the IP address instead of hostname (e.g., `http://192.168.1.100:8188`)
- Check that both machines are on the same network

### "Missing nodes: ReActorFaceSwap"
- Follow the ReActor installation steps above
- Restart ComfyUI after installing

### Face swap results look bad
- Try adjusting the "Face Detail" slider (higher = more enhancement)
- Adjust "CodeFormer Weight" (0 = natural, 1 = more enhanced)
- Use higher quality source photos with clear, front-facing faces

## ReActor Node Assumptions

This app uses the ReActor node with these default settings:
- `swap_model`: `inswapper_128.onnx`
- `facedetection`: `retinaface_resnet50`
- `face_restore_model`: `codeformer-v0.1.0.pth`
- Face ordering: largest to smallest

If your ReActor installation has different model files available, you may need to update the workflow template in `src-tauri/src/recipes.rs`.

## License

MIT
