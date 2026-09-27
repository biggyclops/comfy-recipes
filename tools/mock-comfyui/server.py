#!/usr/bin/env python3
"""
Mock ComfyUI server for testing Comfy Recipes without a real ComfyUI instance.

Usage:
    python server.py [--port 8188]
    python server.py [--port 8188] --missing-face-hair  # Simulate Face + hair nodes missing

This implements the ComfyUI API endpoints that Comfy Recipes uses:
- GET /system_stats - Server/GPU info
- GET /object_info - Node definitions
- POST /upload/image - Image upload
- POST /prompt - Queue workflow
- GET /history/{prompt_id} - Get results
- GET /view - Get image
- WebSocket /ws - Progress updates
"""

import argparse
import asyncio
import json
import os
import uuid
from datetime import datetime
from pathlib import Path

from aiohttp import web, WSMsgType

UPLOADED_IMAGES: dict[str, bytes] = {}
PROMPTS: dict[str, dict] = {}
RESULTS: dict[str, dict] = {}

MOCK_GPU_NAME = "NVIDIA GeForce RTX 3060"
MOCK_VRAM_TOTAL = 12 * 1024 * 1024 * 1024  # 12 GB
MOCK_VRAM_FREE = 10 * 1024 * 1024 * 1024   # 10 GB free

FAKE_RESULT_IMAGE = None
MISSING_FACE_HAIR = False

def load_fake_result():
    """Load or generate a fake result image."""
    global FAKE_RESULT_IMAGE
    
    # Try to load a sample image
    sample_paths = [
        Path(__file__).parent / "sample_result.png",
        Path(__file__).parent / "sample_result.jpg",
    ]
    
    for path in sample_paths:
        if path.exists():
            FAKE_RESULT_IMAGE = path.read_bytes()
            print(f"Loaded sample result from {path}")
            return
    
    # Generate a simple gradient PNG if no sample exists
    try:
        from PIL import Image
        import io
        
        img = Image.new('RGB', (512, 512))
        pixels = img.load()
        for i in range(512):
            for j in range(512):
                pixels[i, j] = (
                    int(255 * i / 512),
                    int(255 * j / 512),
                    128
                )
        
        buf = io.BytesIO()
        img.save(buf, format='PNG')
        FAKE_RESULT_IMAGE = buf.getvalue()
        print("Generated gradient result image")
    except ImportError:
        # Create a minimal valid PNG (1x1 red pixel)
        FAKE_RESULT_IMAGE = bytes([
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
            0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41,
            0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
            0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE,
            0xD4, 0xEF, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
            0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82
        ])
        print("Using minimal PNG result image (install Pillow for better output)")


async def handle_system_stats(request):
    """Return mock system stats."""
    return web.json_response({
        "system": {
            "os": "posix",
            "ram_total": 32 * 1024 * 1024 * 1024,
            "ram_free": 24 * 1024 * 1024 * 1024,
            "comfyui_version": "0.2.0",
            "python_version": "3.10.0"
        },
        "devices": [
            {
                "name": MOCK_GPU_NAME,
                "type": "cuda",
                "vram_total": MOCK_VRAM_TOTAL,
                "vram_free": MOCK_VRAM_FREE
            }
        ]
    })


async def handle_object_info(request):
    """Return mock node definitions including ReActor and Face + hair nodes."""
    nodes = {
        "LoadImage": {
            "input": {
                "required": {
                    "image": ["STRING", {}]
                }
            },
            "output": ["IMAGE", "MASK"],
            "output_name": ["IMAGE", "MASK"],
            "name": "LoadImage",
            "display_name": "Load Image",
            "category": "image"
        },
        "SaveImage": {
            "input": {
                "required": {
                    "images": ["IMAGE", {}],
                    "filename_prefix": ["STRING", {"default": "ComfyUI"}]
                }
            },
            "output": [],
            "name": "SaveImage",
            "display_name": "Save Image",
            "category": "image"
        },
        "ReActorFaceSwap": {
            "input": {
                "required": {
                    "enabled": ["BOOLEAN", {"default": True}],
                    "input_image": ["IMAGE", {}],
                    "swap_model": [["inswapper_128.onnx"], {}],
                    "facedetection": [["retinaface_resnet50", "retinaface_mobile0.25", "YOLOv5l", "YOLOv5n"], {}],
                    "face_restore_model": [["none", "codeformer-v0.1.0.pth", "GFPGANv1.4.pth"], {}],
                    "face_restore_visibility": ["FLOAT", {"default": 1, "min": 0.1, "max": 1}],
                    "codeformer_weight": ["FLOAT", {"default": 0.5, "min": 0, "max": 1}],
                    "detect_gender_input": [["no", "female", "male"], {"default": "no"}],
                    "detect_gender_source": [["no", "female", "male"], {"default": "no"}],
                    "input_faces_index": ["STRING", {"default": "0"}],
                    "source_faces_index": ["STRING", {"default": "0"}],
                    "console_log_level": [[0, 1, 2], {"default": 1}]
                },
                "optional": {
                    "source_image": ["IMAGE", {}],
                    "face_model": ["FACE_MODEL", {}]
                }
            },
            "output": ["IMAGE", "FACE_MODEL", "IMAGE"],
            "output_name": ["SWAPPED_IMAGE", "FACE_MODEL", "ORIGINAL_IMAGE"],
            "name": "ReActorFaceSwap",
            "display_name": "ReActor Face Swap",
            "category": "ReActor"
        },
        "CheckpointLoaderSimple": {
            "input": {
                "required": {
                    "ckpt_name": [["sd_xl_base_1.0.safetensors", "v1-5-pruned.safetensors"], {}]
                }
            },
            "output": ["MODEL", "CLIP", "VAE"],
            "name": "CheckpointLoaderSimple",
            "category": "loaders"
        },
        "VAELoader": {
            "input": {
                "required": {
                    "vae_name": [["sdxl_vae.safetensors", "vae-ft-mse-840000-ema-pruned.safetensors"], {}]
                }
            },
            "output": ["VAE"],
            "name": "VAELoader",
            "category": "loaders"
        },
        "LoraLoader": {
            "input": {
                "required": {
                    "model": ["MODEL", {}],
                    "clip": ["CLIP", {}],
                    "lora_name": [["ip-adapter-faceid-plusv2_sdxl_lora.safetensors"], {}],
                    "strength_model": ["FLOAT", {"default": 1.0}],
                    "strength_clip": ["FLOAT", {"default": 1.0}]
                }
            },
            "output": ["MODEL", "CLIP"],
            "name": "LoraLoader",
            "category": "loaders"
        },
        "KSampler": {
            "input": {
                "required": {
                    "model": ["MODEL", {}],
                    "positive": ["CONDITIONING", {}],
                    "negative": ["CONDITIONING", {}],
                    "latent_image": ["LATENT", {}],
                    "seed": ["INT", {"default": 0}],
                    "steps": ["INT", {"default": 20}],
                    "cfg": ["FLOAT", {"default": 7.0}],
                    "sampler_name": [["euler", "euler_ancestral", "dpmpp_2m"], {}],
                    "scheduler": [["normal", "karras"], {}],
                    "denoise": ["FLOAT", {"default": 1.0}]
                }
            },
            "output": ["LATENT"],
            "name": "KSampler",
            "category": "sampling"
        },
        "VAEDecode": {
            "input": {
                "required": {
                    "samples": ["LATENT", {}],
                    "vae": ["VAE", {}]
                }
            },
            "output": ["IMAGE"],
            "name": "VAEDecode",
            "category": "latent"
        },
        "VAEEncode": {
            "input": {
                "required": {
                    "pixels": ["IMAGE", {}],
                    "vae": ["VAE", {}]
                }
            },
            "output": ["LATENT"],
            "name": "VAEEncode",
            "category": "latent"
        },
        "CLIPTextEncode": {
            "input": {
                "required": {
                    "text": ["STRING", {"multiline": True}],
                    "clip": ["CLIP", {}]
                }
            },
            "output": ["CONDITIONING"],
            "name": "CLIPTextEncode",
            "category": "conditioning"
        },
        "SetLatentNoiseMask": {
            "input": {
                "required": {
                    "samples": ["LATENT", {}],
                    "mask": ["MASK", {}]
                }
            },
            "output": ["LATENT"],
            "name": "SetLatentNoiseMask",
            "category": "latent"
        },
        "GrowMask": {
            "input": {
                "required": {
                    "mask": ["MASK", {}],
                    "expand": ["INT", {"default": 0}],
                    "tapered_corners": ["BOOLEAN", {"default": True}]
                }
            },
            "output": ["MASK"],
            "name": "GrowMask",
            "category": "mask"
        },
        "FeatherMask": {
            "input": {
                "required": {
                    "mask": ["MASK", {}],
                    "left": ["INT", {"default": 0}],
                    "right": ["INT", {"default": 0}],
                    "top": ["INT", {"default": 0}],
                    "bottom": ["INT", {"default": 0}]
                }
            },
            "output": ["MASK"],
            "name": "FeatherMask",
            "category": "mask"
        },
        "ImageCompositeMasked": {
            "input": {
                "required": {
                    "destination": ["IMAGE", {}],
                    "source": ["IMAGE", {}],
                    "mask": ["MASK", {}],
                    "x": ["INT", {"default": 0}],
                    "y": ["INT", {"default": 0}],
                    "resize_source": ["BOOLEAN", {"default": False}]
                }
            },
            "output": ["IMAGE"],
            "name": "ImageCompositeMasked",
            "category": "image"
        }
    }

    nodes.update({
        "CLIPVisionLoader": {
            "input": {
                "required": {
                    "clip_name": [["CLIP-ViT-bigG-14-laion2B-39B-b160k.safetensors", "CLIP-ViT-H-14-laion2B-s32B-b79K.safetensors"], {}]
                }
            },
            "output": ["CLIP_VISION"],
            "name": "CLIPVisionLoader",
            "category": "loaders"
        }
    })

    if not MISSING_FACE_HAIR:
        nodes.update({
            "IPAdapterFaceID": {
                "input": {
                    "required": {
                        "model": ["MODEL", {}],
                        "ipadapter": ["IPADAPTER", {}],
                        "image": ["IMAGE", {}],
                        "weight": ["FLOAT", {"default": 0.85, "min": -1, "max": 3, "step": 0.05}],
                        "weight_faceidv2": ["FLOAT", {"default": 0.85, "min": -1, "max": 5, "step": 0.05}],
                        "weight_type": [["linear", "ease in", "ease out", "ease in-out", "reverse in-out", "weak input", "weak output", "weak middle", "strong middle", "style transfer", "composition", "strong style transfer", "style and target"], {}],
                        "combine_embeds": [["average", "concat", "norm average"], {}],
                        "start_at": ["FLOAT", {"default": 0.0, "min": 0, "max": 1, "step": 0.001}],
                        "end_at": ["FLOAT", {"default": 1.0, "min": 0, "max": 1, "step": 0.001}],
                        "embeds_scaling": [["V only", "K+V", "K+V w/ C penalty", "K+mean(V) w/ C penalty"], {}]
                    },
                    "optional": {
                        "clip_vision": ["CLIP_VISION", {}],
                        "insightface": ["INSIGHTFACE", {}],
                        "attn_mask": ["MASK", {}]
                    }
                },
                "output": ["MODEL"],
                "name": "IPAdapterFaceID",
                "display_name": "IPAdapter FaceID",
                "category": "ipadapter/faceid"
            },
            "IPAdapterUnifiedLoaderFaceID": {
                "input": {
                    "required": {
                        "model": ["MODEL", {}],
                        "preset": [["FACEID", "FACEID PLUS - SD1.5 only", "FACEID PLUS V2", "FACEID PORTRAIT (style transfer)", "FACEID PORTRAIT UNNORM - SDXL only (strong)"], {}],
                        "lora_strength": ["FLOAT", {"default": 0.6, "min": 0, "max": 1, "step": 0.01}],
                        "provider": [["CPU", "CUDA", "ROCM", "DirectML", "OpenVINO", "CoreML"], {}]
                    }
                },
                "output": ["MODEL", "IPADAPTER", "INSIGHTFACE"],
                "output_name": ["MODEL", "ipadapter", "insightface"],
                "name": "IPAdapterUnifiedLoaderFaceID",
                "display_name": "IPAdapter Unified Loader FaceID",
                "category": "ipadapter/loaders"
            },
            "IPAdapterModelLoader": {
                "input": {
                    "required": {
                        "ipadapter_file": [["ip-adapter-faceid-plusv2_sdxl.bin", "ip-adapter_sdxl.safetensors"], {}]
                    }
                },
                "output": ["IPADAPTER"],
                "name": "IPAdapterModelLoader",
                "category": "ipadapter"
            },
            "APersonMaskGenerator": {
                "input": {
                    "required": {
                        "images": ["IMAGE", {}]
                    },
                    "optional": {
                        "face_mask": ["BOOLEAN", {"default": True}],
                        "background_mask": ["BOOLEAN", {"default": False}],
                        "hair_mask": ["BOOLEAN", {"default": False}],
                        "body_mask": ["BOOLEAN", {"default": False}],
                        "clothes_mask": ["BOOLEAN", {"default": False}],
                        "confidence": ["FLOAT", {"default": 0.40, "min": 0.01, "max": 1.0, "step": 0.01}],
                        "refine_mask": ["BOOLEAN", {"default": True}]
                    }
                },
                "output": ["MASK"],
                "output_name": ["masks"],
                "name": "APersonMaskGenerator",
                "display_name": "A Person Mask Generator",
                "category": "A Person Mask Generator"
            }
        })

    return web.json_response(nodes)


async def handle_upload_image(request):
    """Handle image upload."""
    reader = await request.multipart()
    
    filename = f"upload_{uuid.uuid4().hex[:8]}.png"
    
    async for part in reader:
        if part.name == 'image':
            data = await part.read()
            UPLOADED_IMAGES[filename] = data
            filename = part.filename or filename
            break
    
    return web.json_response({
        "name": filename,
        "subfolder": "",
        "type": "input"
    })


def get_available_nodes():
    """Get the current set of available nodes (same logic as handle_object_info)."""
    nodes = {
        "LoadImage", "SaveImage", "ReActorFaceSwap", "CheckpointLoaderSimple",
        "VAELoader", "LoraLoader", "KSampler", "VAEDecode", "VAEEncode",
        "CLIPTextEncode", "SetLatentNoiseMask", "GrowMask", "FeatherMask",
        "ImageCompositeMasked", "CLIPVisionLoader"
    }
    if not MISSING_FACE_HAIR:
        nodes.update({
            "IPAdapterFaceID", "IPAdapterUnifiedLoaderFaceID", "IPAdapterModelLoader",
            "APersonMaskGenerator"
        })
    return nodes


def validate_prompt(prompt: dict) -> tuple[bool, dict]:
    """
    Validate a prompt against available nodes.
    Returns (is_valid, node_errors dict).
    """
    available_nodes = get_available_nodes()
    node_errors = {}
    
    for node_id, node_data in prompt.items():
        if not isinstance(node_data, dict):
            continue
        
        class_type = node_data.get("class_type")
        if not class_type:
            node_errors[node_id] = {
                "type": "missing_class_type",
                "message": f"Node {node_id} is missing class_type",
                "details": "",
                "extra_info": {}
            }
            continue
        
        if class_type not in available_nodes:
            node_errors[node_id] = {
                "type": "invalid_class_type",
                "message": f"Unknown node type: {class_type}",
                "details": f"Node type '{class_type}' is not installed or does not exist",
                "extra_info": {"class_type": class_type}
            }
    
    return len(node_errors) == 0, node_errors


async def handle_prompt(request):
    """Queue a prompt and return prompt ID."""
    data = await request.json()
    prompt = data.get("prompt", {})
    
    # Validate the prompt
    is_valid, node_errors = validate_prompt(prompt)
    
    if not is_valid:
        # Return error response like real ComfyUI does
        return web.json_response({
            "error": {
                "type": "prompt_invalid",
                "message": "Prompt validation failed",
                "details": "",
                "extra_info": {}
            },
            "node_errors": node_errors
        }, status=400)
    
    prompt_id = str(uuid.uuid4())
    
    PROMPTS[prompt_id] = {
        "prompt": prompt,
        "client_id": data.get("client_id", ""),
        "queued_at": datetime.now().isoformat()
    }
    
    # Schedule fake execution
    asyncio.create_task(execute_prompt(prompt_id, data.get("client_id", "")))
    
    return web.json_response({
        "prompt_id": prompt_id,
        "number": len(PROMPTS),
        "node_errors": {}
    })


async def execute_prompt(prompt_id: str, client_id: str):
    """Simulate prompt execution with progress updates."""
    prompt_data = PROMPTS.get(prompt_id, {})
    prompt = prompt_data.get("prompt", {})
    
    is_face_hair = any("IPAdapterFaceID" in str(node) or "APersonMaskGenerator" in str(node) 
                       for node in prompt.values())
    
    steps = 25 if is_face_hair else 10
    delay = 0.15 if is_face_hair else 0.2
    
    await asyncio.sleep(0.5)
    
    for ws in list(WS_CONNECTIONS):
        try:
            await ws.send_json({
                "type": "executing",
                "data": {
                    "prompt_id": prompt_id,
                    "node": "1"
                }
            })
            
            for i in range(steps):
                await asyncio.sleep(delay)
                await ws.send_json({
                    "type": "progress",
                    "data": {
                        "value": i + 1,
                        "max": steps
                    }
                })
            
            output_node = "90" if is_face_hair else "4"
            subfolder = "comfy_recipes/face_hair" if is_face_hair else "comfy_recipes"
            
            await ws.send_json({
                "type": "executed",
                "data": {
                    "prompt_id": prompt_id,
                    "node": output_node,
                    "output": {
                        "images": [{
                            "filename": f"result_{prompt_id[:8]}.png",
                            "subfolder": subfolder,
                            "type": "output"
                        }]
                    }
                }
            })
        except Exception as e:
            print(f"WebSocket error: {e}")
    
    output_node = "90" if is_face_hair else "4"
    subfolder = "comfy_recipes/face_hair" if is_face_hair else "comfy_recipes"
    
    RESULTS[prompt_id] = {
        "outputs": {
            output_node: {
                "images": [{
                    "filename": f"result_{prompt_id[:8]}.png",
                    "subfolder": subfolder,
                    "type": "output"
                }]
            }
        },
        "status": {
            "status_str": "success",
            "completed": True,
            "messages": []
        }
    }


async def handle_history(request):
    """Get prompt history."""
    prompt_id = request.match_info.get('prompt_id')
    
    if prompt_id and prompt_id in RESULTS:
        return web.json_response({
            prompt_id: {
                "prompt": PROMPTS.get(prompt_id, {}).get("prompt", {}),
                "outputs": RESULTS[prompt_id]["outputs"],
                "status": RESULTS[prompt_id]["status"]
            }
        })
    
    return web.json_response({})


async def handle_view(request):
    """Return an image."""
    filename = request.query.get('filename', '')
    
    # Return the fake result image
    if FAKE_RESULT_IMAGE:
        return web.Response(
            body=FAKE_RESULT_IMAGE,
            content_type='image/png'
        )
    
    return web.Response(status=404)


WS_CONNECTIONS: set = set()

async def handle_websocket(request):
    """Handle WebSocket connections for progress updates."""
    ws = web.WebSocketResponse()
    await ws.prepare(request)
    
    WS_CONNECTIONS.add(ws)
    print(f"WebSocket connected. Total: {len(WS_CONNECTIONS)}")
    
    try:
        async for msg in ws:
            if msg.type == WSMsgType.TEXT:
                # Echo or handle messages if needed
                pass
            elif msg.type == WSMsgType.ERROR:
                print(f'WebSocket error: {ws.exception()}')
    finally:
        WS_CONNECTIONS.discard(ws)
        print(f"WebSocket disconnected. Total: {len(WS_CONNECTIONS)}")
    
    return ws


def create_app():
    """Create the aiohttp application."""
    app = web.Application()
    
    app.router.add_get('/system_stats', handle_system_stats)
    app.router.add_get('/object_info', handle_object_info)
    app.router.add_post('/upload/image', handle_upload_image)
    app.router.add_post('/prompt', handle_prompt)
    app.router.add_get('/history/{prompt_id}', handle_history)
    app.router.add_get('/history', handle_history)
    app.router.add_get('/view', handle_view)
    app.router.add_get('/ws', handle_websocket)
    
    return app


def main():
    global MISSING_FACE_HAIR
    
    parser = argparse.ArgumentParser(description='Mock ComfyUI server')
    parser.add_argument('--port', type=int, default=8188, help='Port to listen on')
    parser.add_argument('--host', default='0.0.0.0', help='Host to bind to')
    parser.add_argument('--missing-face-hair', action='store_true', 
                        help='Simulate missing Face + hair nodes/models')
    args = parser.parse_args()
    
    MISSING_FACE_HAIR = args.missing_face_hair
    
    load_fake_result()
    
    app = create_app()
    
    print(f"Mock ComfyUI server starting on http://{args.host}:{args.port}")
    if MISSING_FACE_HAIR:
        print("  MODE: Face + hair nodes MISSING (greyed out state)")
    else:
        print("  MODE: All nodes available")
    print()
    print("Endpoints:")
    print("  GET  /system_stats - Server info")
    print("  GET  /object_info  - Node definitions")
    print("  POST /upload/image - Upload image")
    print("  POST /prompt       - Queue workflow")
    print("  GET  /history/{id} - Get results")
    print("  GET  /view         - Get image")
    print("  WS   /ws           - Progress updates")
    print()
    print("Press Ctrl+C to stop")
    
    web.run_app(app, host=args.host, port=args.port)


if __name__ == '__main__':
    main()
