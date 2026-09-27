use serde::{Deserialize, Serialize};
use serde_json::json;

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
                "swap_model": "inswapper_128.onnx",
                "facedetection": "retinaface_resnet50",
                "face_restore_model": "codeformer-v0.1.0.pth",
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
