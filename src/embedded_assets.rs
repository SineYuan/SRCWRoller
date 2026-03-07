use rust_embed::RustEmbed;
use std::io::Cursor;
use image::DynamicImage;

/// 嵌入的图片资源
#[derive(RustEmbed)]
#[folder = "images/"]
pub struct ImageAssets;

/// 获取嵌入的图片
pub fn get_image(name: &str) -> Option<DynamicImage> {
    match ImageAssets::get(name) {
        Some(file) => {
            let cursor = Cursor::new(file.data.as_ref());
            match image::load(cursor, image::ImageFormat::Png) {
                Ok(img) => Some(img),
                Err(e) => {
                    eprintln!("加载嵌入图片 {} 失败: {}", name, e);
                    None
                }
            }
        }
        None => {
            eprintln!("未找到嵌入图片: {}", name);
            None
        }
    }
}

/// 列出所有嵌入的图片
pub fn list_images() -> Vec<String> {
    ImageAssets::iter()
        .map(|f| f.to_string())
        .collect()
}

/// 检查图片是否存在
pub fn has_image(name: &str) -> bool {
    ImageAssets::get(name).is_some()
}

/// 将嵌入的图片保存到文件系统（用于调试）
pub fn extract_image(name: &str, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    match ImageAssets::get(name) {
        Some(file) => {
            std::fs::write(output_path, file.data.as_ref())?;
            println!("已提取图片: {} -> {}", name, output_path);
            Ok(())
        }
        None => Err(format!("未找到嵌入图片: {}", name).into()),
    }
}

/// 提取所有嵌入的图片到指定目录
pub fn extract_all_images(output_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(output_dir)?;
    
    for file in ImageAssets::iter() {
        let output_path = format!("{}/{}", output_dir, file);
        if let Some(content) = ImageAssets::get(&file) {
            std::fs::write(&output_path, content.data.as_ref())?;
            println!("已提取图片: {} -> {}", file, output_path);
        }
    }
    
    Ok(())
}
