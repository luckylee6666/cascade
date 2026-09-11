use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: Option<String>,
    pub model_type: ModelType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ModelType {
    Chat,
    Embedding,
    Image,
    Audio,
    Other,
}

impl ModelInfo {
    pub fn detect_type(&self) -> ModelType {
        let id = self.id.to_lowercase();
        let name = self.name.to_lowercase();

        // Non-chat filters
        if id.contains("embed") || name.contains("embed") {
            return ModelType::Embedding;
        }
        if id.contains("image") || name.contains("image") || id.contains("wan") {
            return ModelType::Image;
        }
        if id.contains("audio") || name.contains("audio") || id.contains("asr") || id.contains("tts") {
            return ModelType::Audio;
        }
        if id.contains("realtime") || id.contains("happyhorse") {
            return ModelType::Other;
        }

        ModelType::Chat
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftDiff {
    pub to_add: Vec<ModelInfo>,
    pub to_keep: Vec<String>,
    pub to_skip_non_chat: Vec<ModelInfo>,
}

pub fn compute_model_diff(
    remote: &[ModelInfo],
    local: &[String],
) -> DriftDiff {
    let mut to_add = Vec::new();
    let mut to_keep = Vec::new();
    let mut to_skip_non_chat = Vec::new();

    for model in remote {
        let model_type = model.detect_type();

        if model_type != ModelType::Chat {
            to_skip_non_chat.push(model.clone());
            continue;
        }

        if local.contains(&model.id) {
            to_keep.push(model.id.clone());
        } else {
            to_add.push(model.clone());
        }
    }

    DriftDiff {
        to_add,
        to_keep,
        to_skip_non_chat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_detection() {
        let model = ModelInfo {
            id: "gpt-4".into(),
            name: "GPT-4".into(),
            provider: None,
            model_type: ModelType::Other,
        };
        assert_eq!(model.detect_type(), ModelType::Chat);

        let embed_model = ModelInfo {
            id: "text-embedding-ada".into(),
            name: "Embedding".into(),
            provider: None,
            model_type: ModelType::Other,
        };
        assert_eq!(embed_model.detect_type(), ModelType::Embedding);
    }

    #[test]
    fn test_compute_diff() {
        let remote = vec![
            ModelInfo { id: "gpt-4".into(), name: "GPT-4".into(), provider: None, model_type: ModelType::Other },
            ModelInfo { id: "claude-3".into(), name: "Claude 3".into(), provider: None, model_type: ModelType::Other },
            ModelInfo { id: "embed-v1".into(), name: "Embed".into(), provider: None, model_type: ModelType::Other },
        ];
        let local = vec!["gpt-4".into()];

        let diff = compute_model_diff(&remote, &local);
        assert_eq!(diff.to_add.len(), 1);
        assert_eq!(diff.to_add[0].id, "claude-3");
        assert_eq!(diff.to_keep.len(), 1);
        assert_eq!(diff.to_skip_non_chat.len(), 1);
    }
}
