# Mock ComfyUI Server

A lightweight mock server that implements the ComfyUI API endpoints used by Comfy Recipes. Use this for testing when you don't have access to a real ComfyUI instance.

## Usage

```bash
# Install dependencies
pip install -r requirements.txt

# Run the server
python server.py

# Or specify a different port
python server.py --port 9000
```

The server will start on `http://0.0.0.0:8188` by default.

## Endpoints

- `GET /system_stats` - Returns mock GPU info (RTX 3060, 12GB VRAM)
- `GET /object_info` - Returns mock node definitions including ReActorFaceSwap
- `POST /upload/image` - Accepts image uploads
- `POST /prompt` - Queues a workflow, returns prompt ID
- `GET /history/{prompt_id}` - Returns mock results
- `GET /view` - Returns a fake result image
- `WS /ws` - WebSocket for progress updates

## Custom Result Image

To use your own result image, place a file named `sample_result.png` or `sample_result.jpg` in the same directory as `server.py`.

## How It Works

1. When you upload images, they're stored in memory
2. When you queue a prompt, the server simulates execution:
   - Sends progress updates over WebSocket (0-100%)
   - Waits ~2 seconds to simulate processing
   - Stores a fake result
3. When you fetch history/results, it returns the mock output
4. The `/view` endpoint returns either your custom image or a generated gradient
