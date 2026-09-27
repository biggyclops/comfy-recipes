# Mock ComfyUI Server

A lightweight mock server that implements the ComfyUI API endpoints used by Comfy Recipes. Use this for testing when you don't have access to a real ComfyUI instance.

## Usage

```bash
# Install dependencies
pip install -r requirements.txt

# Run the server (all features available)
python server.py

# Or specify a different port
python server.py --port 9000

# Simulate missing Face + hair nodes (to test greyed-out state)
python server.py --missing-face-hair
```

The server will start on `http://0.0.0.0:8188` by default.

## Testing Modes

### Full Mode (default)
All nodes available including Face + hair support (IPAdapter, PersonMaskGenerator, etc.)

### Missing Face + hair Mode (`--missing-face-hair`)
Only Face only nodes available. Use this to test that the app correctly:
- Greys out the "Face + hair" option
- Shows what's missing
- Keeps "Face only" working

## Endpoints

- `GET /system_stats` - Returns mock GPU info (RTX 3060, 12GB VRAM)
- `GET /object_info` - Returns mock node definitions (varies by mode)
- `POST /upload/image` - Accepts image uploads
- `POST /prompt` - Queues a workflow, returns prompt ID
- `GET /history/{prompt_id}` - Returns mock results
- `GET /view` - Returns a fake result image
- `WS /ws` - WebSocket for progress updates

## Supported Nodes

### Face only (always available)
- LoadImage, SaveImage
- ReActorFaceSwap

### Face + hair (requires full mode)
- CheckpointLoaderSimple, VAELoader, LoraLoader
- KSampler, VAEDecode, VAEEncode
- CLIPTextEncode, SetLatentNoiseMask
- GrowMask, FeatherMask, ImageCompositeMasked
- IPAdapterFaceID, IPAdapterUnifiedLoader, IPAdapterModelLoader
- PersonMaskGenerator

## Custom Result Image

To use your own result image, place a file named `sample_result.png` or `sample_result.jpg` in the same directory as `server.py`.

## How It Works

1. When you upload images, they're stored in memory
2. When you queue a prompt, the server simulates execution:
   - Sends progress updates over WebSocket
   - Face only: ~2 seconds, 10 steps
   - Face + hair: ~4 seconds, 25 steps
   - Stores a fake result
3. When you fetch history/results, it returns the mock output
4. The `/view` endpoint returns either your custom image or a generated gradient
