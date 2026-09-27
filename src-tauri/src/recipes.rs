use serde::{Deserialize, Serialize};
use serde_json::json;

pub const SDXL_CHECKPOINT: &str = "sd_xl_base_1.0.safetensors";
pub const SDXL_VAE: &str = "sdxl_vae.safetensors";
pub const IPADAPTER_FACEID_MODEL: &str = "ip-adapter-faceid-plusv2_sdxl.bin";
pub const IPADAPTER_FACEID_LORA: &str = "ip-adapter-faceid-plusv2_sdxl_lora.safetensors";
pub const INSIGHTFACE_MODEL: &str = "buffalo_l";
pub const INSWAPPER_MODEL: &str = "inswapper_128.onnx";
pub const CODEFORMER_MODEL: &str = "codeformer-v0.1.0.pth";

pub const FACE_HAIR_REQUIRED_NODES: &[&str] = &[
    "LoadImage",
    "SaveImage",
    "ReActorFaceSwap",
    "CheckpointLoaderSimple",
    "VAELoader",
    "KSampler",
    "VAEDecode",
    "VAEEncode",
    "IPAdapterFaceID",
    "IPAdapterUnifiedLoader",
    "PersonMaskGenerator",
    "GrowMask",
    "FeatherMask",
    "ImageCompositeMasked",
    "CLIPTextEncode",
    "SetLatentNoiseMask",
];

pub const FACE_HAIR_REQUIRED_MODELS: &[(&str, &str)] = &[
    ("checkpoints", SDXL_CHECKPOINT),
    ("vae", SDXL_VAE),
    ("ipadapter", IPADAPTER_FACEID_MODEL),
    ("loras", IPADAPTER_FACEID_LORA),
    ("insightface", INSWAPPER_MODEL),
    ("facerestore_models", CODEFORMER_MODEL),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    pub description: String,
    pub required_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceSwapParams {
    pub source_image: String,
    pub target_image: String,
    pub face_restore_visibility: f32,
    pub codeformer_weight: f32,
    pub input_faces_index: String,
}

impl Default for FaceSwapParams {
    fn default() -> Self {
        Self {
            source_image: String::new(),
            target_image: String::new(),
            face_restore_visibility: 1.0,
            codeformer_weight: 0.5,
            input_faces_index: "0".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceSwapHairParams {
    pub source_image: String,
    pub target_image: String,
    pub blend_edges: f32,
    pub seed: i64,
    pub face_restore_visibility: f32,
    pub codeformer_weight: f32,
    pub input_faces_index: String,
}

impl Default for FaceSwapHairParams {
    fn default() -> Self {
        Self {
            source_image: String::new(),
            target_image: String::new(),
            blend_edges: 0.5,
            seed: 0,
            face_restore_visibility: 1.0,
            codeformer_weight: 0.5,
            input_faces_index: "0".to_string(),
        }
    }
}

pub fn get_recipes() -> Vec<Recipe> {
    vec![Recipe {
        id: "face_swap".to_string(),
        name: "Face Swap".to_string(),
        description: "Swap a face from one photo onto another using ReActor".to_string(),
        required_nodes: vec![
            "LoadImage".to_string(),
            "ReActorFaceSwap".to_string(),
            "SaveImage".to_string(),
        ],
    }]
}

pub fn get_required_nodes() -> Vec<&'static str> {
    vec!["LoadImage", "ReActorFaceSwap", "SaveImage"]
}

pub fn build_face_swap_workflow(params: &FaceSwapParams) -> serde_json::Value {
    json!({
        "1": {
            "class_type": "LoadImage",
            "inputs": {
                "image": params.source_image
            }
        },
        "2": {
            "class_type": "LoadImage",
            "inputs": {
                "image": params.target_image
            }
        },
        "3": {
            "class_type": "ReActorFaceSwap",
            "inputs": {
                "enabled": true,
                "input_image": ["2", 0],
                "source_image": ["1", 0],
                "swap_model": INSWAPPER_MODEL,
                "facedetection": "retinaface_resnet50",
                "face_restore_model": CODEFORMER_MODEL,
                "face_restore_visibility": params.face_restore_visibility,
                "codeformer_weight": params.codeformer_weight,
                "detect_gender_input": "no",
                "detect_gender_source": "no",
                "input_faces_index": params.input_faces_index,
                "source_faces_index": "0",
                "console_log_level": 1
            }
        },
        "4": {
            "class_type": "SaveImage",
            "inputs": {
                "images": ["3", 0],
                "filename_prefix": "comfy_recipes/face_swap"
            }
        }
    })
}

pub fn build_face_swap_hair_workflow(params: &FaceSwapHairParams) -> serde_json::Value {
    let grow_amount = (params.blend_edges * 30.0) as i32 + 5;
    let feather_amount = (params.blend_edges * 40.0) as i32 + 10;
    
    json!({
        "1": {
            "class_type": "LoadImage",
            "inputs": {
                "image": params.source_image
            }
        },
        "2": {
            "class_type": "LoadImage",
            "inputs": {
                "image": params.target_image
            }
        },
        "10": {
            "class_type": "CheckpointLoaderSimple",
            "inputs": {
                "ckpt_name": SDXL_CHECKPOINT
            }
        },
        "11": {
            "class_type": "VAELoader",
            "inputs": {
                "vae_name": SDXL_VAE
            }
        },
        "20": {
            "class_type": "PersonMaskGenerator",
            "inputs": {
                "image": ["2", 0],
                "mask_type": "head"
            }
        },
        "21": {
            "class_type": "GrowMask",
            "inputs": {
                "mask": ["20", 0],
                "expand": grow_amount,
                "tapered_corners": true
            }
        },
        "22": {
            "class_type": "FeatherMask",
            "inputs": {
                "mask": ["21", 0],
                "left": feather_amount,
                "right": feather_amount,
                "top": feather_amount,
                "bottom": feather_amount
            }
        },
        "30": {
            "class_type": "IPAdapterUnifiedLoader",
            "inputs": {
                "model": ["10", 0],
                "preset": "FACEID PLUS V2"
            }
        },
        "31": {
            "class_type": "IPAdapterFaceID",
            "inputs": {
                "model": ["30", 0],
                "ipadapter": ["30", 1],
                "image": ["1", 0],
                "weight": 0.85,
                "weight_faceidv2": 0.85,
                "weight_type": "linear",
                "combine_embeds": "average",
                "start_at": 0.0,
                "end_at": 1.0,
                "embeds_scaling": "V only"
            }
        },
        "40": {
            "class_type": "CLIPTextEncode",
            "inputs": {
                "text": "photo of a person, natural hair, high quality, detailed skin, matching lighting, 8k",
                "clip": ["10", 1]
            }
        },
        "41": {
            "class_type": "CLIPTextEncode",
            "inputs": {
                "text": "deformed, blurry, bad anatomy, bad hands, extra limbs, disfigured",
                "clip": ["10", 1]
            }
        },
        "50": {
            "class_type": "VAEEncode",
            "inputs": {
                "pixels": ["2", 0],
                "vae": ["11", 0]
            }
        },
        "51": {
            "class_type": "SetLatentNoiseMask",
            "inputs": {
                "samples": ["50", 0],
                "mask": ["22", 0]
            }
        },
        "60": {
            "class_type": "KSampler",
            "inputs": {
                "model": ["31", 0],
                "positive": ["40", 0],
                "negative": ["41", 0],
                "latent_image": ["51", 0],
                "seed": params.seed,
                "steps": 25,
                "cfg": 7.0,
                "sampler_name": "euler_ancestral",
                "scheduler": "normal",
                "denoise": 0.85
            }
        },
        "61": {
            "class_type": "VAEDecode",
            "inputs": {
                "samples": ["60", 0],
                "vae": ["11", 0]
            }
        },
        "70": {
            "class_type": "ReActorFaceSwap",
            "inputs": {
                "enabled": true,
                "input_image": ["61", 0],
                "source_image": ["1", 0],
                "swap_model": INSWAPPER_MODEL,
                "facedetection": "retinaface_resnet50",
                "face_restore_model": CODEFORMER_MODEL,
                "face_restore_visibility": params.face_restore_visibility,
                "codeformer_weight": params.codeformer_weight,
                "detect_gender_input": "no",
                "detect_gender_source": "no",
                "input_faces_index": params.input_faces_index,
                "source_faces_index": "0",
                "console_log_level": 1
            }
        },
        "80": {
            "class_type": "ImageCompositeMasked",
            "inputs": {
                "destination": ["2", 0],
                "source": ["70", 0],
                "mask": ["22", 0],
                "x": 0,
                "y": 0,
                "resize_source": false
            }
        },
        "90": {
            "class_type": "SaveImage",
            "inputs": {
                "images": ["80", 0],
                "filename_prefix": "comfy_recipes/face_hair"
            }
        }
    })
}

pub fn get_fix_instructions(missing_nodes: &[String]) -> Vec<String> {
    let mut instructions = Vec::new();

    for node in missing_nodes {
        match node.as_str() {
            "ReActorFaceSwap" => {
                instructions.push(
                    "Install the ReActor custom node on Hades:\n\
                     1. cd ComfyUI/custom_nodes\n\
                     2. git clone https://github.com/Gourieff/ComfyUI-ReActor\n\
                     3. cd ComfyUI-ReActor && pip install -r requirements.txt\n\
                     4. Restart ComfyUI".to_string()
                );
                instructions.push(
                    "Download the inswapper_128.onnx model:\n\
                     Place it in ComfyUI/models/insightface/".to_string()
                );
                instructions.push(
                    "Download a face restore model (codeformer-v0.1.0.pth or GFPGANv1.4.pth):\n\
                     Place it in ComfyUI/models/facerestore_models/".to_string()
                );
            }
            "IPAdapterFaceID" | "IPAdapterUnifiedLoader" => {
                instructions.push(
                    "Install ComfyUI_IPAdapter_plus on Hades:\n\
                     1. cd ComfyUI/custom_nodes\n\
                     2. git clone https://github.com/cubiq/ComfyUI_IPAdapter_plus\n\
                     3. Restart ComfyUI".to_string()
                );
                instructions.push(
                    "Download IP-Adapter FaceID Plus V2 models:\n\
                     - ip-adapter-faceid-plusv2_sdxl.bin -> ComfyUI/models/ipadapter/\n\
                     - ip-adapter-faceid-plusv2_sdxl_lora.safetensors -> ComfyUI/models/loras/".to_string()
                );
            }
            "PersonMaskGenerator" => {
                instructions.push(
                    "Install ComfyUI-Impact-Pack on Hades:\n\
                     1. cd ComfyUI/custom_nodes\n\
                     2. git clone https://github.com/ltdrdata/ComfyUI-Impact-Pack\n\
                     3. cd ComfyUI-Impact-Pack && pip install -r requirements.txt\n\
                     4. Restart ComfyUI".to_string()
                );
            }
            "GrowMask" | "FeatherMask" | "ImageCompositeMasked" => {
                instructions.push(format!(
                    "{} is a core ComfyUI node. Make sure ComfyUI is up to date.",
                    node
                ));
            }
            "CheckpointLoaderSimple" | "VAELoader" | "KSampler" | "VAEDecode" | "VAEEncode" 
            | "CLIPTextEncode" | "SetLatentNoiseMask" => {
                instructions.push(format!(
                    "{} is a core ComfyUI node. Make sure ComfyUI is properly installed.",
                    node
                ));
            }
            "LoadImage" | "SaveImage" => {
                instructions.push(format!(
                    "{} is a core ComfyUI node. Make sure ComfyUI is properly installed.",
                    node
                ));
            }
            _ => {
                instructions.push(format!("Missing node: {}. Check ComfyUI custom nodes.", node));
            }
        }
    }

    if instructions.is_empty() && missing_nodes.is_empty() {
        return instructions;
    }

    instructions.push(
        "Make sure ComfyUI is started with --listen so other machines can reach it:\n\
         python main.py --listen".to_string()
    );

    instructions
}

pub fn get_face_hair_fix_instructions(missing_nodes: &[String], missing_models: &[String]) -> Vec<String> {
    let mut instructions = get_fix_instructions(missing_nodes);

    for model in missing_models {
        match model.as_str() {
            m if m.contains("sd_xl_base") => {
                instructions.push(
                    "Download SDXL Base checkpoint:\n\
                     - sd_xl_base_1.0.safetensors from https://huggingface.co/stabilityai/stable-diffusion-xl-base-1.0\n\
                     - Place in ComfyUI/models/checkpoints/".to_string()
                );
            }
            m if m.contains("sdxl_vae") => {
                instructions.push(
                    "Download SDXL VAE:\n\
                     - sdxl_vae.safetensors from https://huggingface.co/stabilityai/sdxl-vae\n\
                     - Place in ComfyUI/models/vae/".to_string()
                );
            }
            m if m.contains("ip-adapter-faceid") && m.contains(".bin") => {
                instructions.push(
                    "Download IP-Adapter FaceID Plus V2 model:\n\
                     - ip-adapter-faceid-plusv2_sdxl.bin from https://huggingface.co/h94/IP-Adapter-FaceID\n\
                     - Place in ComfyUI/models/ipadapter/".to_string()
                );
            }
            m if m.contains("ip-adapter-faceid") && m.contains("lora") => {
                instructions.push(
                    "Download IP-Adapter FaceID Plus V2 LoRA:\n\
                     - ip-adapter-faceid-plusv2_sdxl_lora.safetensors from https://huggingface.co/h94/IP-Adapter-FaceID\n\
                     - Place in ComfyUI/models/loras/".to_string()
                );
            }
            _ => {
                instructions.push(format!("Missing model: {}", model));
            }
        }
    }

    instructions
}
