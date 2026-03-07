use rust_embed::RustEmbed;
use opencv::core::Mat;
use opencv::prelude::MatTraitConst;
use std::collections::HashMap;

#[derive(RustEmbed)]
#[folder = "images/"]
pub struct ImageAssets;

pub struct TemplateManager {
    templates: HashMap<String, Mat>,
}

impl TemplateManager {
    pub fn new() -> Self {
        let mut templates = HashMap::new();
        
        for file in ImageAssets::iter() {
            if let Some(content) = ImageAssets::get(&file) {
                let bytes = content.data.as_ref();
                
                if let Ok(mat) = Self::decode_image(bytes) {
                    let name = file.to_string();
                    templates.insert(name, mat);
                }
            }
        }
        
        println!("模板管理器初始化完成，加载了 {} 个模板", templates.len());
        Self { templates }
    }
    
    fn decode_image(bytes: &[u8]) -> Result<Mat, Box<dyn std::error::Error>> {
        let vec: opencv::core::Vector<u8> = opencv::core::Vector::from_iter(bytes.iter().copied());
        let mat = opencv::imgcodecs::imdecode(&vec, opencv::imgcodecs::IMREAD_COLOR)?;
        
        if mat.empty() {
            return Err("解码图片失败".into());
        }
        
        Ok(mat)
    }
    
    pub fn get(&self, name: &str) -> Option<&Mat> {
        self.templates.get(name)
    }
    
    pub fn get_with_fallback(&self, name: &str) -> Option<Mat> {
        if let Some(mat) = self.templates.get(name) {
            return Some(mat.clone());
        }
        
        let simple_name = name.trim_start_matches("images/");
        if let Some(mat) = self.templates.get(simple_name) {
            return Some(mat.clone());
        }
        
        if std::path::Path::new(name).exists() {
            if let Ok(mat) = opencv::imgcodecs::imread(name, opencv::imgcodecs::IMREAD_COLOR) {
                if !mat.empty() {
                    return Some(mat);
                }
            }
        }
        
        None
    }
    
    pub fn list(&self) -> Vec<&String> {
        self.templates.keys().collect()
    }
    
    pub fn count(&self) -> usize {
        self.templates.len()
    }
}

impl Default for TemplateManager {
    fn default() -> Self {
        Self::new()
    }
}
