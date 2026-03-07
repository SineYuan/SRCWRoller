use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use crate::logger::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpeningConfigEntry {
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub strategy: Vec<String>,
    #[serde(default)]
    pub affix: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrConfig {
    #[serde(default = "default_models_dir")]
    pub models_dir: String,
    #[serde(default = "default_det_model")]
    pub det_model: String,
    #[serde(default = "default_rec_model")]
    pub rec_model: String,
    #[serde(default = "default_keys_file")]
    pub keys_file: String,
}

fn default_models_dir() -> String { "./models".to_string() }
fn default_det_model() -> String { "PP-OCRv5_mobile_det.mnn".to_string() }
fn default_rec_model() -> String { "PP-OCRv5_mobile_rec.mnn".to_string() }
fn default_keys_file() -> String { "ppocr_keys_v5.txt".to_string() }

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            models_dir: default_models_dir(),
            det_model: default_det_model(),
            rec_model: default_rec_model(),
            keys_file: default_keys_file(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_max_retry")]
    pub max_retry: i32,
    #[serde(default = "default_prefer_env")]
    pub prefer_env: Vec<String>,
    pub openings: Vec<OpeningConfigEntry>,
    #[serde(default)]
    pub device_serial: Option<String>,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    #[serde(default = "default_click_sleep")]
    pub click_sleep: f32,
    #[serde(default = "default_page_timeout")]
    pub page_timeout: i32,
    #[serde(default)]
    pub debug: bool,
    #[serde(default)]
    pub ocr: OcrConfig,
    #[serde(default)]
    pub settings: HashMap<String, serde_json::Value>,
}

fn default_max_retry() -> i32 { 500 }
fn default_prefer_env() -> Vec<String> {
    vec![
        "彩虹时代".to_string(),
        "头彩".to_string(),
        "蓝海".to_string(),
        "特权阶级".to_string(),
        "银河学者邀请".to_string(),
        "佩佩".to_string(),
        "夜之半神邀请".to_string(),
    ]
}
fn default_confidence() -> f32 { 0.7 }
fn default_click_sleep() -> f32 { 0.5 }
fn default_page_timeout() -> i32 { 10 }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            max_retry: default_max_retry(),
            prefer_env: default_prefer_env(),
            openings: vec![],
            device_serial: None,
            confidence: default_confidence(),
            click_sleep: default_click_sleep(),
            page_timeout: default_page_timeout(),
            debug: false,
            ocr: OcrConfig::default(),
            settings: HashMap::new(),
        }
    }
}

impl AppConfig {
    pub fn from_json_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: AppConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let config: AppConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "json" => Self::from_json_file(path),
            "yaml" | "yml" => Self::from_yaml_file(path),
            _ => Err("不支持的配置文件格式，请使用 .json 或 .yaml/.yml".into()),
        }
    }

    pub fn create_example() -> Self {
        Self {
            max_retry: 500,
            prefer_env: vec![
                "彩虹时代".to_string(),
                "头彩".to_string(),
                "蓝海".to_string(),
                "特权阶级".to_string(),
                "银河学者邀请".to_string(),
                "佩佩".to_string(),
                "夜之半神邀请".to_string(),
            ],
            openings: vec![
                OpeningConfigEntry {
                    env: vec!["专家研讨会".to_string(), "特邀专家".to_string()],
                    strategy: vec!["快请专家".to_string()],
                    affix: vec![],
                },
                OpeningConfigEntry {
                    env: vec![],
                    strategy: vec!["轮回不止".to_string()],
                    affix: vec!["变宝为废".to_string()],
                },
            ],
            device_serial: None,
            confidence: 0.7,
            click_sleep: 0.5,
            page_timeout: 10,
            debug: false,
            ocr: OcrConfig::default(),
            settings: HashMap::new(),
        }
    }

    pub fn save_to_json<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn save_to_yaml<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

pub fn find_config_file() -> Option<std::path::PathBuf> {
    let possible_names = [
        "config.json",
        "config.yaml",
        "config.yml",
        "srcwroller.json",
        "srcwroller.yaml",
        "srcwroller.yml",
    ];

    for name in &possible_names {
        let path = std::path::PathBuf::from(name);
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for name in &possible_names {
                let path = exe_dir.join(name);
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }

    None
}

pub fn load_config() -> AppConfig {
    if let Some(config_path) = find_config_file() {
        info!("找到配置文件: {:?}", config_path);
        match AppConfig::from_file(&config_path) {
            Ok(config) => {
                info!("配置加载成功");
                return config;
            }
            Err(e) => {
                warn!("配置文件加载失败: {}，使用默认配置", e);
            }
        }
    } else {
        info!("未找到配置文件，使用默认配置");
    }

    AppConfig::create_example()
}

pub fn create_example_config() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::create_example();

    config.save_to_json("config.example.json")?;
    println!("已创建示例配置文件: config.example.json");

    config.save_to_yaml("config.example.yaml")?;
    println!("已创建示例配置文件: config.example.yaml");

    Ok(())
}
