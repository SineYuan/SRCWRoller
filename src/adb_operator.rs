use image::DynamicImage;
use ocr_rs::OcrEngine;
use radb::AdbClient;
use radb::protocols::AdbProtocol;
use opencv::prelude::MatTraitConst;
use std::time::Duration;
use std::{thread, time::Instant};
use crate::config::OcrConfig;
use crate::logger::{debug, info, warn};
use crate::template_manager::TemplateManager;

#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl Region {
    pub fn new(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self { left, top, width, height }
    }

    pub fn sub_region(&self, from_x: f32, from_y: f32, to_x: f32, to_y: f32) -> Self {
        let x1 = (self.left as f32 + self.width as f32 * from_x) as i32;
        let y1 = (self.top as f32 + self.height as f32 * from_y) as i32;
        let x2 = (self.left as f32 + self.width as f32 * to_x) as i32;
        let y2 = (self.top as f32 + self.height as f32 * to_y) as i32;

        Self { left: x1, top: y1, width: x2 - x1, height: y2 - y1 }
    }
}

#[derive(Debug, Clone)]
pub struct MatchBox {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub source: String,
}

impl MatchBox {
    pub fn center(&self) -> (i32, i32) {
        (self.left + self.width / 2, self.top + self.height / 2)
    }
}

#[derive(Debug, Clone)]
pub struct OcrResult {
    pub text: String,
    pub confidence: f32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub struct AdbOperator {
    device_serial: Option<String>,
    pub width: i32,
    pub height: i32,
    pub left: i32,
    pub top: i32,
    pub confidence: f32,
    ocr_engine: Option<OcrEngine>,
    template_manager: TemplateManager,
    pub debug_ops: bool,
    pub debug_ops_dir: String,
    debug_ops_count: std::cell::Cell<i32>,
}

impl AdbOperator {
    pub fn new(device_id: Option<&str>) -> Result<Self, Box<dyn std::error::Error>> {
        Self::new_with_ocr_config(device_id, None)
    }
    
    pub fn new_with_ocr_config(device_id: Option<&str>, ocr_config: Option<&OcrConfig>) -> Result<Self, Box<dyn std::error::Error>> {
        let device_serial = Self::connect_and_find_device(device_id)?;

        if device_serial.is_none() {
            return Err("未找到 ADB 设备，请确保：\n1. 设备已连接并开启 USB 调试\n2. 已授权 ADB 调试\n3. ADB 服务器已启动".into());
        }

        let (width, height) = Self::get_screen_size(&device_serial)?;
        
        let ocr_engine = match Self::init_ocr_engine(ocr_config) {
            Ok(engine) => Some(engine),
            Err(e) => {
                eprintln!("警告: OCR 引擎初始化失败: {}", e);
                None
            }
        };

        Ok(Self {
            device_serial,
            width,
            height,
            left: 0,
            top: 0,
            confidence: 0.72,
            ocr_engine,
            template_manager: TemplateManager::new(),
            debug_ops: false,
            debug_ops_dir: "debug_ops".to_string(),
            debug_ops_count: std::cell::Cell::new(0),
        })
    }

    /// 连接 ADB 并查找设备，如果失败则尝试启动 ADB 服务器
    fn connect_and_find_device(device_id: Option<&str>) -> Result<Option<String>, Box<dyn std::error::Error>> {
        // 首先尝试连接
        match Self::try_find_device(device_id) {
            Ok(Some(serial)) => return Ok(Some(serial)),
            Ok(None) => return Ok(None),
            Err(e) => {
                warn!("ADB 连接失败: {}", e);
                warn!("尝试启动 ADB 服务器...");
            }
        }
        
        // 尝试启动 ADB 服务器并重试
        Self::start_adb_server()?;
        thread::sleep(Duration::from_millis(1500));
        
        Self::try_find_device(device_id)
    }
    
    /// 尝试查找设备（不启动 ADB 服务器）
    fn try_find_device(device_id: Option<&str>) -> Result<Option<String>, Box<dyn std::error::Error>> {
        // 使用 catch_unwind 捕获 radb 库可能产生的 panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            AdbClient::default().iter_devices()
        }));
        
        match result {
            Ok(Ok(devices)) => {
                for d in devices {
                    if let Some(id) = device_id {
                        if d.serial.as_ref().map(|s: &String| s.as_str()) == Some(id) {
                            return Ok(d.serial.clone());
                        }
                    } else {
                        return Ok(d.serial.clone());
                    }
                }
                Ok(None)
            }
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err("ADB 连接失败（服务器可能未运行）".into()),
        }
    }

    /// 尝试启动 ADB 服务器
    /// 1. 首先尝试系统 PATH 中的 adb 命令
    /// 2. 然后尝试查找可执行文件同目录下的 adb 可执行文件
    fn start_adb_server() -> Result<(), Box<dyn std::error::Error>> {
        use std::process::Command;
        
        info!("正在尝试启动 ADB 服务器...");
        
        // 首先尝试系统 PATH 中的 adb
        let result = Command::new("adb")
            .args(&["start-server"])
            .output();
            
        match result {
            Ok(output) => {
                if output.status.success() {
                    info!("成功通过系统 adb 命令启动 ADB 服务器");
                    return Ok(());
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!("系统 adb 命令执行失败: {}", stderr);
                }
            }
            Err(e) => {
                warn!("系统 adb 命令不可用: {}", e);
            }
        }
        
        // 尝试查找可执行文件同目录下的 adb 可执行文件
        // 根据平台选择正确的可执行文件名
        #[cfg(target_os = "windows")]
        let adb_filename = "adb.exe";
        #[cfg(not(target_os = "windows"))]
        let adb_filename = "adb";
        
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let adb_path = exe_dir.join(adb_filename);
                if adb_path.exists() {
                    info!("找到同目录下的 {}: {:?}", adb_filename, adb_path);
                    let result = Command::new(&adb_path)
                        .args(&["start-server"])
                        .output();
                        
                    match result {
                        Ok(output) => {
                            if output.status.success() {
                                info!("成功通过同目录 {} 启动 ADB 服务器", adb_filename);
                                return Ok(());
                            } else {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                warn!("同目录 {} 执行失败: {}", adb_filename, stderr);
                            }
                        }
                        Err(e) => {
                            warn!("同目录 {} 执行失败: {}", adb_filename, e);
                        }
                    }
                } else {
                    warn!("同目录下未找到 {}: {:?}", adb_filename, adb_path);
                }
            }
        }
        
        // 尝试常见的 ADB 安装路径（根据平台）
        #[cfg(target_os = "windows")]
        let common_paths: Vec<String> = vec![
            format!(r"C:\Users\{}\AppData\Local\Android\Sdk\platform-tools\adb.exe", 
                std::env::var("USERNAME").unwrap_or_default()),
            r"C:\Program Files (x86)\Android\android-sdk\platform-tools\adb.exe".to_string(),
            r"D:\Android\Sdk\platform-tools\adb.exe".to_string(),
        ];
        
        #[cfg(target_os = "macos")]
        let common_paths: Vec<String> = vec![
            "~/Library/Android/sdk/platform-tools/adb".to_string(),
            "/usr/local/bin/adb".to_string(),
            "/opt/homebrew/bin/adb".to_string(),
            "/Applications/Android Studio.app/Contents/platform-tools/adb".to_string(),
        ];
        
        #[cfg(target_os = "linux")]
        let common_paths: Vec<String> = vec![
            "~/Android/Sdk/platform-tools/adb".to_string(),
            "/usr/bin/adb".to_string(),
            "/usr/local/bin/adb".to_string(),
            "/opt/android-sdk/platform-tools/adb".to_string(),
        ];
        
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        let common_paths: Vec<String> = vec![];
        
        for path in &common_paths {
            // 展开 ~ 为用户主目录
            let expanded_path = if path.starts_with("~") {
                if let Ok(home) = std::env::var("HOME") {
                    path.replacen("~", &home, 1)
                } else {
                    continue;
                }
            } else {
                path.clone()
            };
            
            let adb_path = std::path::PathBuf::from(&expanded_path);
            if adb_path.exists() {
                info!("找到常见路径下的 {}: {:?}", adb_filename, adb_path);
                let result = Command::new(&adb_path)
                    .args(&["start-server"])
                    .output();
                    
                match result {
                    Ok(output) => {
                        if output.status.success() {
                            info!("成功通过常见路径 {} 启动 ADB 服务器", adb_filename);
                            return Ok(());
                        }
                    }
                    Err(_) => continue,
                }
            }
        }
        
        Err("无法找到或启动 ADB 服务器。请确保：\n1. ADB 已安装并在 PATH 中\n2. 或将 adb 可执行文件放在程序同目录下".into())
    }

    fn init_ocr_engine(ocr_config: Option<&OcrConfig>) -> Result<OcrEngine, Box<dyn std::error::Error>> {
        let search_paths = Self::get_ocr_search_paths(ocr_config);
        let (det_name, rec_name, keys_name) = if let Some(config) = ocr_config {
            (config.det_model.as_str(), config.rec_model.as_str(), config.keys_file.as_str())
        } else {
            //("PP-OCRv5_mobile_det.mnn", "PP-OCRv5_mobile_rec.mnn", "ppocr_keys_v5.txt")
            ("ch_PP-OCRv4_det_infer.mnn", "ch_PP-OCRv4_rec_infer.mnn", "ppocr_keys_v4.txt")
        };
        
        for models_dir in search_paths {
            let det_model = models_dir.join(det_name);
            let rec_model = models_dir.join(rec_name);
            let keys_file = models_dir.join(keys_name);

            if det_model.exists() && rec_model.exists() && keys_file.exists() {
                println!("找到 OCR 模型文件目录: {:?}", models_dir);
                println!("  - det_model: {:?}", det_model);
                println!("  - rec_model: {:?}", rec_model);
                println!("  - keys_file: {:?}", keys_file);
                
                let engine = OcrEngine::new(
                    det_model.to_str().unwrap(),
                    rec_model.to_str().unwrap(),
                    keys_file.to_str().unwrap(),
                    None,
                )?;
                
                return Ok(engine);
            }
        }
        
        Err("OCR 模型文件未找到，请在配置文件中设置 ocr.models_dir 或确保 models/ 目录存在".into())
    }
    
    fn get_ocr_search_paths(ocr_config: Option<&OcrConfig>) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        
        if let Some(config) = ocr_config {
            paths.push(std::path::PathBuf::from(&config.models_dir));
        }
        
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                paths.push(exe_dir.join("models"));
            }
        }
        
        if let Ok(cwd) = std::env::current_dir() {
            paths.push(cwd.join("models"));
        }
        
        paths
    }

    fn get_screen_size(device_serial: &Option<String>) -> Result<(i32, i32), Box<dyn std::error::Error>> {
        let mut client = AdbClient::default();
        let mut target_device = None;
        
        for mut d in client.iter_devices()? {
            if device_serial.is_none() || d.serial == *device_serial {
                target_device = Some(d);
                break;
            }
        }
        
        let mut device = target_device.ok_or("未找到设备")?;
        let output = device.shell("wm size")?;

        for line in output.lines() {
            if line.contains("Physical size") {
                let size_str = line.split(':').nth(1).unwrap_or("").trim();
                let parts: Vec<&str> = size_str.split('x').collect();
                if parts.len() == 2 {
                    let w: i32 = parts[0].parse()?;
                    let h: i32 = parts[1].parse()?;
                    if w < h {
                        return Ok((h, w));
                    }
                    return Ok((w, h));
                }
            }
        }

        Ok((1920, 1080))
    }

    pub fn get_win_region(&self) -> Region {
        Region::new(self.left, self.top, self.width, self.height)
    }

    pub fn get_width(&self) -> i32 {
        self.width
    }

    pub fn get_height(&self) -> i32 {
        self.height
    }

    pub fn screenshot(&self) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        let mut client = AdbClient::default();
        for mut d in client.iter_devices()? {
            if self.device_serial.is_none() || d.serial == self.device_serial {
                let mut stream = d.shell_stream("screencap -p")?;
                let mut raw_data = Vec::new();
                loop {
                    let bytes = stream.recv(4096)?;
                    if bytes.is_empty() {
                        break;
                    }
                    raw_data.extend(bytes);
                }
                
                let png_start = raw_data.windows(4)
                    .position(|w| w == b"\x89PNG")
                    .unwrap_or(0);
                
                if png_start > 0 {
                    debug!("跳过 {} 字节非PNG数据", png_start);
                }
                
                let img = image::load_from_memory(&raw_data[png_start..])?;
                return Ok(img);
            }
        }
        Err("设备未找到".into())
    }

    pub fn screenshot_in_region(&self, region: &Region) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        // 如果 region 是全屏，直接返回截图
        if region.left == 0 && region.top == 0 
            && region.width == self.width && region.height == self.height {
            return self.screenshot();
        }
        
        let full_img = self.screenshot()?;
        let cropped = full_img.crop_imm(
            region.left as u32,
            region.top as u32,
            region.width as u32,
            region.height as u32,
        );
        Ok(DynamicImage::ImageRgb8(cropped.to_rgb8()))
    }

    pub fn screenshot_in_tuple(&self, from_x: f32, from_y: f32, to_x: f32, to_y: f32) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        let region = self.get_win_region().sub_region(from_x, from_y, to_x, to_y);
        self.screenshot_in_region(&region)
    }

    pub fn click_point(&self, x: f32, y: f32, after_sleep: f32) -> Result<(), Box<dyn std::error::Error>> {
        let (real_x, real_y) = if x <= 1.0 && y <= 1.0 {
            ((x * self.width as f32) as i32, (y * self.height as f32) as i32)
        } else {
            (x as i32, y as i32)
        };

        let (rel_x, rel_y) = if x <= 1.0 && y <= 1.0 {
            (x, y)
        } else {
            (x / self.width as f32, y / self.height as f32)
        };

        debug!("点击: ({}, {})", real_x, real_y);

        if self.debug_ops {
            match self.screenshot() {
                Ok(screenshot) => {
                    self.draw_debug_click(&screenshot, real_x, real_y, rel_x, rel_y, "");
                }
                Err(e) => {
                    warn!("Debug 截图失败: {}", e);
                }
            }
        }

        let mut client = AdbClient::default();
        for mut d in client.iter_devices()? {
            if self.device_serial.is_none() || d.serial == self.device_serial {
                let cmd = format!("input tap {} {}", real_x, real_y);
                d.shell(cmd.as_str())?;
                break;
            }
        }

        if after_sleep > 0.0 {
            thread::sleep(Duration::from_secs_f32(after_sleep));
        }

        Ok(())
    }

    pub fn click_box(&self, box_: &MatchBox, after_sleep: f32) -> Result<(), Box<dyn std::error::Error>> {
        let (x, y) = box_.center();
        self.click_point(x as f32, y as f32, after_sleep)
    }

    pub fn drag_to(&self, from_x: f32, from_y: f32, to_x: f32, to_y: f32) -> Result<(), Box<dyn std::error::Error>> {
        let (real_from_x, real_from_y) = if from_x <= 1.0 && from_y <= 1.0 {
            ((from_x * self.width as f32) as i32, (from_y * self.height as f32) as i32)
        } else {
            (from_x as i32, from_y as i32)
        };

        let (real_to_x, real_to_y) = if to_x <= 1.0 && to_y <= 1.0 {
            ((to_x * self.width as f32) as i32, (to_y * self.height as f32) as i32)
        } else {
            (to_x as i32, to_y as i32)
        };

        let (rel_from_x, rel_from_y) = if from_x <= 1.0 && from_y <= 1.0 {
            (from_x, from_y)
        } else {
            (from_x / self.width as f32, from_y / self.height as f32)
        };

        let (rel_to_x, rel_to_y) = if to_x <= 1.0 && to_y <= 1.0 {
            (to_x, to_y)
        } else {
            (to_x / self.width as f32, to_y / self.height as f32)
        };

        debug!("拖动: ({}, {}) -> ({}, {})", real_from_x, real_from_y, real_to_x, real_to_y);

        if self.debug_ops {
            match self.screenshot() {
                Ok(screenshot) => {
                    self.draw_debug_drag(&screenshot, real_from_x, real_from_y, real_to_x, real_to_y,
                                         rel_from_x, rel_from_y, rel_to_x, rel_to_y);
                }
                Err(e) => {
                    warn!("Debug 截图失败: {}", e);
                }
            }
        }

        let duration_ms = 500;
        let mut client = AdbClient::default();
        for mut d in client.iter_devices()? {
            if self.device_serial.is_none() || d.serial == self.device_serial {
                let cmd = format!(
                    "input swipe {} {} {} {} {}",
                    real_from_x, real_from_y, real_to_x, real_to_y, duration_ms
                );
                d.shell(cmd.as_str())?;
                break;
            }
        }

        Ok(())
    }

    pub fn press_key(&self, key: &str) -> Result<(), Box<dyn std::error::Error>> {
        let keycode = match key.to_lowercase().as_str() {
            "esc" | "escape" => "KEYCODE_ESCAPE",
            "enter" => "KEYCODE_ENTER",
            "back" => "KEYCODE_BACK",
            "home" => "KEYCODE_HOME",
            "menu" => "KEYCODE_MENU",
            "power" => "KEYCODE_POWER",
            "volume_up" => "KEYCODE_VOLUME_UP",
            "volume_down" => "KEYCODE_VOLUME_DOWN",
            "tab" => "KEYCODE_TAB",
            "space" => "KEYCODE_SPACE",
            "del" => "KEYCODE_DEL",
            "delete" => "KEYCODE_FORWARD_DEL",
            "up" => "KEYCODE_DPAD_UP",
            "down" => "KEYCODE_DPAD_DOWN",
            "left" => "KEYCODE_DPAD_LEFT",
            "right" => "KEYCODE_DPAD_RIGHT",
            "center" => "KEYCODE_DPAD_CENTER",
            "f1" => "KEYCODE_F1", "f2" => "KEYCODE_F2", "f3" => "KEYCODE_F3", "f4" => "KEYCODE_F4",
            "f5" => "KEYCODE_F5", "f6" => "KEYCODE_F6", "f7" => "KEYCODE_F7", "f8" => "KEYCODE_F8",
            "f9" => "KEYCODE_F9", "f10" => "KEYCODE_F10", "f11" => "KEYCODE_F11", "f12" => "KEYCODE_F12",
            "a" => "KEYCODE_A", "b" => "KEYCODE_B", "c" => "KEYCODE_C", "d" => "KEYCODE_D",
            "e" => "KEYCODE_E", "f" => "KEYCODE_F", "g" => "KEYCODE_G", "h" => "KEYCODE_H",
            "i" => "KEYCODE_I", "j" => "KEYCODE_J", "k" => "KEYCODE_K", "l" => "KEYCODE_L",
            "m" => "KEYCODE_M", "n" => "KEYCODE_N", "o" => "KEYCODE_O", "p" => "KEYCODE_P",
            "q" => "KEYCODE_Q", "r" => "KEYCODE_R", "s" => "KEYCODE_S", "t" => "KEYCODE_T",
            "u" => "KEYCODE_U", "v" => "KEYCODE_V", "w" => "KEYCODE_W", "x" => "KEYCODE_X",
            "y" => "KEYCODE_Y", "z" => "KEYCODE_Z",
            "0" => "KEYCODE_0", "1" => "KEYCODE_1", "2" => "KEYCODE_2", "3" => "KEYCODE_3",
            "4" => "KEYCODE_4", "5" => "KEYCODE_5", "6" => "KEYCODE_6", "7" => "KEYCODE_7",
            "8" => "KEYCODE_8", "9" => "KEYCODE_9",
            _ => key,
        };

        debug!("按键: {}", keycode);
        let mut client = AdbClient::default();
        for mut d in client.iter_devices()? {
            if self.device_serial.is_none() || d.serial == self.device_serial {
                let cmd = format!("input keyevent {}", keycode);
                d.shell(cmd.as_str())?;
                break;
            }
        }
        Ok(())
    }

    pub fn sleep(seconds: f32) {
        thread::sleep(Duration::from_secs_f32(seconds));
    }

    pub fn locate_in_region(&self, template_path: &str, region: Option<&Region>) -> Result<Option<MatchBox>, Box<dyn std::error::Error>> {
        let region = region.map(|r| r.clone()).unwrap_or_else(|| self.get_win_region());

        let template_mat = if let Some(mat) = self.template_manager.get(template_path) {
            mat.clone()
        } else if let Some(mat) = self.template_manager.get_with_fallback(template_path) {
            mat
        } else {
            return Ok(None);
        };

        let template_width = template_mat.cols();
        let template_height = template_mat.rows();

        let screenshot = self.screenshot_in_region(&region)?;
        let screenshot_mat = Self::image_to_mat(&screenshot)?;

        let mut result = opencv::core::Mat::default();
        opencv::imgproc::match_template(
            &screenshot_mat,
            &template_mat,
            &mut result,
            opencv::imgproc::TM_CCOEFF_NORMED,
            &opencv::core::no_array(),
        )?;

        let mut min_val = 0.0;
        let mut max_val = 0.0;
        let mut min_loc = opencv::core::Point::default();
        let mut max_loc = opencv::core::Point::default();
        opencv::core::min_max_loc(
            &result,
            Some(&mut min_val),
            Some(&mut max_val),
            Some(&mut min_loc),
            Some(&mut max_loc),
            &opencv::core::no_array(),
        )?;

        debug!("模板匹配: {} 最大相似度: {:.3} (阈值: {})", template_path, max_val, self.confidence);
        
        if max_val >= self.confidence as f64 {
            Ok(Some(MatchBox {
                left: region.left + max_loc.x,
                top: region.top + max_loc.y,
                width: template_width,
                height: template_height,
                source: template_path.to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn locate(&self, template_path: &str) -> Result<Option<MatchBox>, Box<dyn std::error::Error>> {
        self.locate_in_region(template_path, None)
    }

    pub fn locate_any(&self, templates: &[&str]) -> Result<(i32, Option<MatchBox>), Box<dyn std::error::Error>> {
        for (index, template) in templates.iter().enumerate() {
            if let Some(box_) = self.locate(template)? {
                return Ok((index as i32, Some(box_)));
            }
        }
        Ok((-1, None))
    }

    pub fn wait_img(&self, template: &str, timeout: f32, interval: f32) -> Result<Option<MatchBox>, Box<dyn std::error::Error>> {
        let start = Instant::now();
        while start.elapsed().as_secs_f32() < timeout {
            if let Some(box_) = self.locate(template)? {
                return Ok(Some(box_));
            }
            Self::sleep(interval);
        }
        Ok(None)
    }

    pub fn wait_any_img(&self, templates: &[&str], timeout: f32, interval: f32) -> Result<(i32, Option<MatchBox>), Box<dyn std::error::Error>> {
        let start = Instant::now();
        while start.elapsed().as_secs_f32() < timeout {
            let (index, box_) = self.locate_any(templates)?;
            if index != -1 {
                return Ok((index, box_));
            }
            Self::sleep(interval);
        }
        Ok((-1, None))
    }

    pub fn click_img(&self, template: &str, after_sleep: f32) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(box_) = self.locate(template)? {
            self.click_box(&box_, after_sleep)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn ocr_in_region(&self, region: &Region) -> Result<Vec<OcrResult>, Box<dyn std::error::Error>> {
        let screenshot_start = std::time::Instant::now();
        let img = self.screenshot_in_region(region)?;
        let screenshot_time = screenshot_start.elapsed();

        if let Some(ref engine) = self.ocr_engine {
            let ocr_start = std::time::Instant::now();
            let results = engine.recognize(&img)?;
            let ocr_time = ocr_start.elapsed();

            debug!("OCR 推理耗时: {:.2}ms (截图: {:.2}ms, 区域: {}x{} at {},{})",
                ocr_time.as_secs_f64() * 1000.0,
                screenshot_time.as_secs_f64() * 1000.0,
                region.width, region.height, region.left, region.top);

            let mut ocr_results = Vec::new();
            for result in &results {
                let left = result.bbox.rect.left() as i32;
                let top = result.bbox.rect.top() as i32;
                let right = result.bbox.rect.right() as i32;
                let bottom = result.bbox.rect.bottom() as i32;
                let center_x = (left + right) / 2 + region.left;
                let center_y = (top + bottom) / 2 + region.top;

                let ocr_result = OcrResult {
                    text: result.text.clone(),
                    confidence: result.confidence,
                    x: center_x,
                    y: center_y,
                    width: result.bbox.rect.width() as i32,
                    height: result.bbox.rect.height() as i32,
                };
                debug!("OCR: '{}' (conf: {:.2}) at center ({}, {})",
                    result.text, result.confidence, ocr_result.x, ocr_result.y);
                ocr_results.push(ocr_result);
            }

            if self.debug_ops && !ocr_results.is_empty() {
                match self.screenshot() {
                    Ok(full_screenshot) => {
                        self.draw_debug_ocr_results(&full_screenshot, region, &ocr_results);
                    }
                    Err(e) => {
                        warn!("Debug 截图失败: {}", e);
                    }
                }
            }

            Ok(ocr_results)
        } else {
            warn!("OCR 引擎未初始化，返回空结果");
            Ok(vec![])
        }
    }

    pub fn ocr_in_tuple(&self, from_x: f32, from_y: f32, to_x: f32, to_y: f32) -> Result<Vec<OcrResult>, Box<dyn std::error::Error>> {
        let region = self.get_win_region().sub_region(from_x, from_y, to_x, to_y);
        self.ocr_in_region(&region)
    }

    pub fn ocr_on_image(&self, screenshot: &DynamicImage, region: &Region) -> Result<Vec<OcrResult>, Box<dyn std::error::Error>> {
        let ocr_start = std::time::Instant::now();
        
        let cropped = screenshot.crop_imm(
            region.left as u32,
            region.top as u32,
            region.width as u32,
            region.height as u32,
        );

        if let Some(ref engine) = self.ocr_engine {
            let results = engine.recognize(&DynamicImage::ImageRgb8(cropped.to_rgb8()))?;
            let ocr_time = ocr_start.elapsed();

            debug!("OCR 推理耗时: {:.2}ms (区域: {}x{} at {},{}), 无截图耗时",
                ocr_time.as_secs_f64() * 1000.0,
                region.width, region.height, region.left, region.top);

            let mut ocr_results = Vec::new();
            for result in &results {
                let left = result.bbox.rect.left() as i32;
                let top = result.bbox.rect.top() as i32;
                let right = result.bbox.rect.right() as i32;
                let bottom = result.bbox.rect.bottom() as i32;
                let center_x = (left + right) / 2 + region.left;
                let center_y = (top + bottom) / 2 + region.top;

                let ocr_result = OcrResult {
                    text: result.text.clone(),
                    confidence: result.confidence,
                    x: center_x,
                    y: center_y,
                    width: result.bbox.rect.width() as i32,
                    height: result.bbox.rect.height() as i32,
                };
                debug!("OCR: '{}' (conf: {:.2}) at center ({}, {})",
                    result.text, result.confidence, ocr_result.x, ocr_result.y);
                ocr_results.push(ocr_result);
            }

            if self.debug_ops && !ocr_results.is_empty() {
                self.draw_debug_ocr_results(screenshot, region, &ocr_results);
            }

            Ok(ocr_results)
        } else {
            warn!("OCR 引擎未初始化，返回空结果");
            Ok(vec![])
        }
    }

    pub fn ocr_on_image_full(&self, screenshot: &DynamicImage) -> Result<Vec<OcrResult>, Box<dyn std::error::Error>> {
        let region = Region::new(0, 0, screenshot.width() as i32, screenshot.height() as i32);
        self.ocr_on_image(screenshot, &region)
    }

    pub fn image_to_mat(img: &DynamicImage) -> Result<opencv::core::Mat, Box<dyn std::error::Error>> {
        let rgb_img = img.to_rgb8();
        let (width, height) = rgb_img.dimensions();
        let raw_data = rgb_img.into_raw();

        let mat = unsafe {
            opencv::core::Mat::new_rows_cols_with_data_unsafe(
                height as i32,
                width as i32,
                opencv::core::CV_8UC3,
                raw_data.as_ptr() as *mut _,
                opencv::core::Mat_AUTO_STEP,
            )?
        };

        let mut bgr_mat = opencv::core::Mat::default();
        opencv::imgproc::cvt_color(&mat, &mut bgr_mat, opencv::imgproc::COLOR_RGB2BGR, 0, opencv::core::AlgorithmHint::ALGO_HINT_DEFAULT)?;

        Ok(bgr_mat)
    }

    pub fn enable_debug_ops(&mut self, output_dir: &str) {
        self.debug_ops = true;
        self.debug_ops_dir = output_dir.to_string();
        std::fs::create_dir_all(output_dir).ok();
        crate::logger::info!("Debug 操作截图已启用，输出目录: {}", output_dir);
    }

    fn save_debug_ops_image(&self, img: &opencv::core::Mat, count: i32, suffix: &str) {
        if !self.debug_ops {
            return;
        }

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("{}_{:03}_{}.png", timestamp, count, suffix);
        let filepath = std::path::Path::new(&self.debug_ops_dir).join(&filename);

        if let Err(e) = opencv::imgcodecs::imwrite(
            filepath.to_str().unwrap(),
            img,
            &opencv::core::Vector::new(),
        ) {
            crate::logger::warn!("保存 Debug 图片失败: {}", e);
        } else {
            crate::logger::debug!("Debug 图片已保存: {:?}", filepath);
        }
    }

    pub fn draw_debug_click(&self, screenshot: &DynamicImage, x: i32, y: i32, rel_x: f32, rel_y: f32, tag: &str) {
        if !self.debug_ops {
            return;
        }

        self.debug_ops_count.set(self.debug_ops_count.get() + 1);
        let count = self.debug_ops_count.get();

        let mut img_cv = match Self::image_to_mat(screenshot) {
            Ok(mat) => mat,
            Err(_) => return,
        };

        let center = opencv::core::Point::new(x, y);
        let _ = opencv::imgproc::draw_marker(
            &mut img_cv,
            center,
            opencv::core::Scalar::new(0.0, 255.0, 0.0, 0.0),
            opencv::imgproc::MARKER_CROSS,
            30,
            3,
            opencv::imgproc::LINE_8,
        );
        let _ = opencv::imgproc::circle(
            &mut img_cv,
            center,
            15,
            opencv::core::Scalar::new(0.0, 255.0, 0.0, 0.0),
            2,
            opencv::imgproc::LINE_8,
            0,
        );

        let text = format!("#{} Click ({:.3}, {:.3})", count, rel_x, rel_y);
        let text_pos = opencv::core::Point::new(x + 20, y - 10);
        let _ = opencv::imgproc::put_text(
            &mut img_cv,
            &text,
            text_pos,
            opencv::imgproc::FONT_HERSHEY_SIMPLEX,
            0.6,
            opencv::core::Scalar::new(0.0, 0.0, 255.0, 0.0),
            2,
            opencv::imgproc::LINE_8,
            false,
        );

        if !tag.is_empty() {
            let tag_text = format!("[{}]", tag);
            let tag_pos = opencv::core::Point::new(x + 20, y - 35);
            let _ = opencv::imgproc::put_text(
                &mut img_cv,
                &tag_text,
                tag_pos,
                opencv::imgproc::FONT_HERSHEY_SIMPLEX,
                0.5,
                opencv::core::Scalar::new(255.0, 0.0, 0.0, 0.0),
                1,
                opencv::imgproc::LINE_8,
                false,
            );
        }

        self.save_debug_ops_image(&img_cv, count, "click");
    }

    pub fn draw_debug_drag(&self, screenshot: &DynamicImage, from_x: i32, from_y: i32, to_x: i32, to_y: i32,
                           rel_from_x: f32, rel_from_y: f32, rel_to_x: f32, rel_to_y: f32) {
        if !self.debug_ops {
            return;
        }

        self.debug_ops_count.set(self.debug_ops_count.get() + 1);
        let count = self.debug_ops_count.get();

        let mut img_cv = match Self::image_to_mat(screenshot) {
            Ok(mat) => mat,
            Err(_) => return,
        };

        let from_point = opencv::core::Point::new(from_x, from_y);
        let to_point = opencv::core::Point::new(to_x, to_y);

        let _ = opencv::imgproc::circle(
            &mut img_cv,
            from_point,
            10,
            opencv::core::Scalar::new(255.0, 0.0, 0.0, 0.0),
            -1,
            opencv::imgproc::LINE_8,
            0,
        );

        let _ = opencv::imgproc::circle(
            &mut img_cv,
            to_point,
            10,
            opencv::core::Scalar::new(0.0, 165.0, 255.0, 0.0),
            -1,
            opencv::imgproc::LINE_8,
            0,
        );

        let _ = opencv::imgproc::arrowed_line(
            &mut img_cv,
            from_point,
            to_point,
            opencv::core::Scalar::new(255.0, 0.0, 0.0, 0.0),
            3,
            opencv::imgproc::LINE_8,
            0,
            0.2,
        );

        let text = format!("#{} Drag ({:.3}, {:.3}) -> ({:.3}, {:.3})",
                           count, rel_from_x, rel_from_y, rel_to_x, rel_to_y);
        let text_pos = opencv::core::Point::new(from_x + 20, from_y - 10);
        let _ = opencv::imgproc::put_text(
            &mut img_cv,
            &text,
            text_pos,
            opencv::imgproc::FONT_HERSHEY_SIMPLEX,
            0.5,
            opencv::core::Scalar::new(0.0, 0.0, 255.0, 0.0),
            1,
            opencv::imgproc::LINE_8,
            false,
        );

        self.save_debug_ops_image(&img_cv, count, "drag");
    }

    pub fn draw_debug_ocr_results(&self, screenshot: &DynamicImage, region: &Region, results: &[OcrResult]) {
        if !self.debug_ops || results.is_empty() {
            return;
        }

        self.debug_ops_count.set(self.debug_ops_count.get() + 1);
        let count = self.debug_ops_count.get();

        let mut img_cv = match Self::image_to_mat(screenshot) {
            Ok(mat) => mat,
            Err(_) => return,
        };

        // 画出 OCR 区域框（黄色）
        let _ = opencv::imgproc::rectangle(
            &mut img_cv,
            opencv::core::Rect::new(region.left, region.top, region.width, region.height),
            opencv::core::Scalar::new(0.0, 255.0, 255.0, 0.0),
            2,
            opencv::imgproc::LINE_8,
            0,
        );

        // 画出每个 OCR 结果
        for (i, result) in results.iter().enumerate() {
            let x1 = result.x - result.width / 2;
            let y1 = result.y - result.height / 2;

            let color = if result.confidence >= 0.9 {
                opencv::core::Scalar::new(0.0, 255.0, 0.0, 0.0)
            } else if result.confidence >= 0.7 {
                opencv::core::Scalar::new(0.0, 255.0, 255.0, 0.0)
            } else {
                opencv::core::Scalar::new(0.0, 0.0, 255.0, 0.0)
            };

            let _ = opencv::imgproc::rectangle(
                &mut img_cv,
                opencv::core::Rect::new(x1, y1, result.width, result.height),
                color,
                2,
                opencv::imgproc::LINE_8,
                0,
            );

            let rel_x = result.x as f32 / self.width as f32;
            let rel_y = result.y as f32 / self.height as f32;

            let text = format!("#{} '{}' ({:.2})", i, result.text, result.confidence);
            let text_pos = opencv::core::Point::new(x1, y1 - 5);
            let _ = opencv::imgproc::put_text(
                &mut img_cv,
                &text,
                text_pos,
                opencv::imgproc::FONT_HERSHEY_SIMPLEX,
                0.5,
                color,
                1,
                opencv::imgproc::LINE_8,
                false,
            );

            let text2 = format!("Rel: ({:.3}, {:.3})", rel_x, rel_y);
            let text2_pos = opencv::core::Point::new(x1, y1 - 20);
            let _ = opencv::imgproc::put_text(
                &mut img_cv,
                &text2,
                text2_pos,
                opencv::imgproc::FONT_HERSHEY_SIMPLEX,
                0.4,
                color,
                1,
                opencv::imgproc::LINE_8,
                false,
            );
        }

        self.save_debug_ops_image(&img_cv, count, "ocr");
    }
}
