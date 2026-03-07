use crate::adb_operator::AdbOperator;
use crate::logger::{info, warn, error, init as init_logger};
use crate::pages::PageDetector;
use std::io::{self, Write, BufRead};
use std::collections::HashMap;

/// Shell 命令处理结果
pub enum ShellCommandResult {
    Success(Option<String>),
    Error(String),
    NotFound,
    Exit,
}

/// Shell 命令处理器 trait
pub trait ShellCommandHandler {
    fn execute(&mut self, cmd: &str, args: &[&str], operator: &mut AdbOperator) -> ShellCommandResult;
    fn get_commands(&self) -> Vec<(&'static str, &'static str)>;
}

/// 页面命令注册表
#[derive(Default)]
pub struct PageCommandRegistry {
    commands: HashMap<&'static str, Box<dyn Fn(&mut dyn std::any::Any, &[&str], &mut AdbOperator) -> ShellCommandResult>>,
    help_text: HashMap<&'static str, &'static str>,
}

impl PageCommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn register<F>(&mut self, name: &'static str, help: &'static str, handler: F)
    where
        F: Fn(&mut dyn std::any::Any, &[&str], &mut AdbOperator) -> ShellCommandResult + 'static,
    {
        self.commands.insert(name, Box::new(handler));
        self.help_text.insert(name, help);
    }
    
    pub fn execute(&self, page: &mut dyn std::any::Any, cmd: &str, args: &[&str], operator: &mut AdbOperator) -> ShellCommandResult {
        if let Some(handler) = self.commands.get(cmd) {
            handler(page, args, operator)
        } else {
            ShellCommandResult::NotFound
        }
    }
    
    pub fn get_commands(&self) -> Vec<(&'static str, &'static str)> {
        self.help_text.iter().map(|(&k, &v)| (k, v)).collect()
    }
}

/// 为页面自动生成 shell 命令注册的宏
/// 使用函数指针方式，避免宏卫生性问题
#[macro_export]
macro_rules! define_page_commands {
    (
        $page_type:ident {
            $($cmd_name:ident : $help:literal => $handler:path),*
        }
    ) => {
        impl $page_type {
            pub fn get_shell_registry() -> PageCommandRegistry {
                let mut registry = PageCommandRegistry::new();
                $(
                    registry.register(stringify!($cmd_name), $help, |page_any, args, operator| {
                        let page = page_any.downcast_mut::<$page_type>().unwrap();
                        $handler(page, args, operator)
                    });
                )*
                registry
            }
        }
    };
}

pub fn get_page_names() -> Vec<&'static str> {
    PAGES.iter().map(|p| p.name).collect()
}

/// 页面信息结构体
pub struct PageDefinition {
    pub name: &'static str,
    pub keywords: &'static [&'static str],
}

/// 所有页面的定义
pub const PAGES: &[PageDefinition] = &[
    PageDefinition { name: "StartPage", keywords: &["开始", "货币战争"] },
    PageDefinition { name: "GameModePage", keywords: &["常规演算", "周期演算"] },
    PageDefinition { name: "DifficultyPage", keywords: &["难度", "选择难度"] },
    PageDefinition { name: "BossAffixPage", keywords: &["首领", "词缀"] },
    PageDefinition { name: "PlaneSelectPage", keywords: &["位面", "选择位面"] },
    PageDefinition { name: "InvestEnvironmentPage", keywords: &["环境", "投资环境"] },
    PageDefinition { name: "PreparationPage", keywords: &["准备", "准备阶段"] },
    PageDefinition { name: "ShopPage", keywords: &["商店", "星际和平商店"] },
    PageDefinition { name: "InvestStrategyPage", keywords: &["策略", "投资策略"] },
    PageDefinition { name: "ExitConfirmDialog", keywords: &["确认", "退出"] },
    PageDefinition { name: "ExitChallengeFailPage", keywords: &["挑战失败", "失败"] },
    PageDefinition { name: "ExitStatsPage", keywords: &["统计", "数据"] },
    PageDefinition { name: "ExitReturnPage", keywords: &["返回", "退出"] },
    PageDefinition { name: "BattleSettlementPage", keywords: &["战斗", "结算"] },
    PageDefinition { name: "SpecialEventPage", keywords: &["盛会之星", "命运卜者"] },
];

/// 截图模式下的页面检测器
struct ScreenshotPageDetector {
    screenshot: image::DynamicImage,
    ocr_engine: Option<ocr_rs::OcrEngine>,
}

impl ScreenshotPageDetector {
    fn new(screenshot_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let screenshot = image::open(screenshot_path)?;
        let ocr_engine = Self::init_ocr_engine().ok();
        Ok(Self { screenshot, ocr_engine })
    }
    
    fn init_ocr_engine() -> Result<ocr_rs::OcrEngine, Box<dyn std::error::Error>> {
        let search_paths = ["models", "./models"];
        
        for models_dir in search_paths {
            let path = std::path::PathBuf::from(models_dir);
            let det_model = path.join("PP-OCRv5_mobile_det.mnn");
            let rec_model = path.join("PP-OCRv5_mobile_rec.mnn");
            let keys_file = path.join("ppocr_keys_v5.txt");

            if det_model.exists() && rec_model.exists() && keys_file.exists() {
                info!("找到 OCR 模型文件目录: {:?}", path);
                return Ok(ocr_rs::OcrEngine::new(
                    det_model.to_str().unwrap(),
                    rec_model.to_str().unwrap(),
                    keys_file.to_str().unwrap(),
                    None,
                )?);
            }
        }
        Err("OCR 模型文件未找到".into())
    }
    
    fn perform_ocr(&self, region: Option<(i32, i32, i32, i32)>) -> Vec<(String, f32, f32, f32)> {
        let mut results = Vec::new();
        
        if let Some(ref engine) = self.ocr_engine {
            let (x, y, w, h) = region.unwrap_or((0, 0, self.screenshot.width() as i32, self.screenshot.height() as i32));
            
            let cropped = if x == 0 && y == 0 && w as u32 == self.screenshot.width() && h as u32 == self.screenshot.height() {
                self.screenshot.clone()
            } else {
                let rgba = self.screenshot.to_rgba8();
                let buffer = image::imageops::crop_imm(&rgba, x as u32, y as u32, w as u32, h as u32).to_image();
                image::DynamicImage::ImageRgba8(buffer)
            };
            
            if let Ok(ocr_results) = engine.recognize(&cropped) {
                let sw = self.screenshot.width() as f32;
                let sh = self.screenshot.height() as f32;
                
                for r in ocr_results {
                    let abs_x = x as f32 + r.bbox.rect.left() as f32;
                    let abs_y = y as f32 + r.bbox.rect.top() as f32;
                    results.push((r.text, abs_x / sw, abs_y / sh, r.confidence));
                }
            }
        }
        
        results
    }
    
    fn detect_page(&self, keywords: &[&str]) -> bool {
        let elements = self.perform_ocr(None);
        keywords.iter().any(|kw| elements.iter().any(|(text, _, _, _)| text.contains(kw)))
    }
    
    fn detect_all_pages(&self) -> Vec<(&'static str, bool)> {
        PAGES.iter().map(|p| (p.name, self.detect_page(p.keywords))).collect()
    }
}

pub fn test_with_screenshot(screenshot_path: &str, page_name: Option<&str>, _interactive: bool) {
    init_logger(true);
    
    info!("==================================================");
    info!("测试页面检测 (截图模式)");
    info!("截图路径: {}", screenshot_path);
    info!("==================================================");
    
    if !std::path::Path::new(screenshot_path).exists() {
        error!("截图文件不存在: {}", screenshot_path);
        return;
    }
    
    let detector = match ScreenshotPageDetector::new(screenshot_path) {
        Ok(d) => {
            info!("截图加载成功: {}x{}", d.screenshot.width(), d.screenshot.height());
            if d.ocr_engine.is_some() { info!("OCR 引擎初始化成功"); }
            else { warn!("OCR 引擎初始化失败"); }
            d
        }
        Err(e) => { error!("无法加载截图: {}", e); return; }
    };
    
    if let Some(name) = page_name {
        let page_def = PAGES.iter().find(|p| p.name == name);
        if let Some(def) = page_def {
            info!("\n测试页面: {}", name);
            let matched = detector.detect_page(def.keywords);
            info!("{}", if matched { format!("✓ 成功匹配到页面: {}", name) } else { format!("✗ 未能匹配到页面: {}", name) });
        } else {
            error!("未知页面类: {}", name);
            error!("可用的页面类: {:?}", get_page_names());
            return;
        }
    } else {
        info!("\n依次检测所有页面类:");
        for (name, matched) in detector.detect_all_pages() {
            info!("  {} - {}", if matched { "✓ 匹配" } else { "✗ 不匹配" }, name);
        }
    }
    
    info!("\n检测到的文本元素:");
    for (i, (text, rel_x, rel_y, conf)) in detector.perform_ocr(None).iter().enumerate() {
        info!("  [{}] '{}' @ ({:.3}, {:.3}) 置信度: {:.2}%", i, text, rel_x, rel_y, conf * 100.0);
    }
    
    info!("\n==================================================");
    info!("截图模式页面检测完成");
    info!("==================================================");
}

pub fn test_with_adb(
    page_name: Option<&str>,
    device_serial: Option<&str>,
    interactive: bool,
    debug_ops: bool,
    debug_ops_dir: String,
    _save_opening: bool,
    _opening_dir: String
) {
    init_logger(true);

    info!("==================================================");
    info!("测试页面检测 (ADB模式)");
    info!("==================================================");

    info!("连接 ADB 设备...");
    let mut operator = match AdbOperator::new(device_serial) {
        Ok(mut op) => {
            info!("设备连接成功");
            info!("分辨率: {}x{}", op.width, op.height);
            if debug_ops { op.enable_debug_ops(&debug_ops_dir); }
            op
        }
        Err(e) => { info!("ADB 连接失败: {}", e); return; }
    };

    if let Some(name) = page_name {
        test_single_page(&mut operator, name, interactive);
    } else {
        test_all_pages(&mut operator);
    }
}

fn test_all_pages(operator: &mut AdbOperator) {
    info!("\n依次检测所有页面类:");

    let mut detector = PageDetector::new(operator);
    let _ = detector.refresh();

    let results: Vec<(&str, bool)> = vec![
        ("StartPage", detector.detect_start_page().is_some()),
        ("GameModePage", detector.detect_game_mode_page().is_some()),
        ("DifficultyPage", detector.detect_difficulty_page().is_some()),
        ("BossAffixPage", detector.detect_boss_affix_page().is_some()),
        ("PlaneSelectPage", detector.detect_plane_select_page().is_some()),
        ("InvestEnvironmentPage", detector.detect_invest_environment_page().is_some()),
        ("PreparationPage", detector.detect_preparation_page().is_some()),
        ("ShopPage", detector.detect_shop_page().is_some()),
        ("InvestStrategyPage", detector.detect_invest_strategy_page().is_some()),
        ("ExitConfirmDialog", detector.detect_exit_confirm_dialog().is_some()),
        ("ExitChallengeFailPage", detector.detect_exit_challenge_fail_page().is_some()),
        ("ExitStatsPage", detector.detect_exit_stats_page().is_some()),
        ("ExitReturnPage", detector.detect_exit_return_page().is_some()),
        ("BattleSettlementPage", detector.detect_battle_settlement_page().is_some()),
        ("SpecialEventPage", detector.detect_special_event_page().is_some()),
    ];

    for (name, matched) in &results {
        info!("  {} - {}", if *matched { "✓ 匹配" } else { "✗ 不匹配" }, name);
    }

    info!("\n==================================================");
}

/// 通用交互 shell - 支持页面注册的命令
fn interactive_shell(
    operator: &mut AdbOperator,
    page_name: &str,
    elements: &[(String, f32, f32)],
    page_registry: Option<&PageCommandRegistry>,
) {
    println!("\n==================================================");
    println!("交互模式 - {}", page_name);
    println!("==================================================");
    println!("通用命令:");
    println!("  elements, e              - 显示检测到的所有元素");
    println!("  click <索引>             - 点击指定索引的元素");
    println!("  click_text <文字>        - 点击包含指定文字的元素 (模糊匹配)");
    println!("  screenshot, s            - 保存当前截图");
    println!("  help, h                  - 显示帮助");
    println!("  quit, q                  - 退出交互模式");
    
    // 显示页面特定命令
    if let Some(registry) = page_registry {
        let commands = registry.get_commands();
        if !commands.is_empty() {
            println!("\n页面特定命令:");
            for (cmd, help) in commands {
                println!("  {:24} - {}", cmd, help);
            }
        }
    }
    
    println!("==================================================");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("\n[{}]> ", page_name);
        stdout.flush().unwrap();

        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() { break; }

        let input = input.trim();
        if input.is_empty() { continue; }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "quit" | "exit" | "q" => { println!("退出交互模式"); break; }
            "elements" | "e" | "list" | "ls" => {
                if elements.is_empty() {
                    println!("未检测到任何元素");
                } else {
                    println!("检测到的元素 ({} 个):", elements.len());
                    for (i, (text, x, y)) in elements.iter().enumerate() {
                        println!("  [{}] '{}' @ ({:.3}, {:.3})", i, text, x, y);
                    }
                }
            }
            "click" | "c" => {
                if args.is_empty() { println!("用法: click <索引>"); continue; }
                if let Ok(idx) = args[0].parse::<usize>() {
                    if idx < elements.len() {
                        let (text, x, y) = &elements[idx];
                        println!("点击元素 [{}]: '{}' @ ({:.3}, {:.3})", idx, text, x, y);
                        let _ = operator.click_point(*x, *y, 1.0);
                        AdbOperator::sleep(2.0);
                    } else {
                        println!("索引 {} 超出范围 (0-{})", idx, elements.len().saturating_sub(1));
                    }
                } else {
                    println!("无效的索引: {}", args[0]);
                }
            }
            "click_text" | "ct" => {
                if args.is_empty() { println!("用法: click_text <文字>"); continue; }
                let search_text = args.join(" ");
                if let Some((i, (text, x, y))) = elements.iter().enumerate().find(|(_, (t, _, _))| t.contains(&search_text)) {
                    println!("点击元素 [{}]: '{}' @ ({:.3}, {:.3})", i, text, x, y);
                    let _ = operator.click_point(*x, *y, 1.0);
                    AdbOperator::sleep(2.0);
                } else {
                    println!("未找到包含 '{}' 的元素", search_text);
                }
            }
            "screenshot" | "s" | "ss" => {
                match operator.screenshot() {
                    Ok(img) => {
                        let filename = format!("screenshot_{}.png", chrono::Local::now().format("%Y%m%d_%H%M%S"));
                        if let Err(e) = img.save(&filename) { println!("保存截图失败: {}", e); }
                        else { println!("截图已保存: {}", filename); }
                    }
                    Err(e) => println!("截图失败: {}", e),
                }
            }
            "help" | "h" | "?" => {
                println!("\n通用命令:");
                println!("  elements, e              - 显示检测到的所有元素");
                println!("  click <索引>             - 点击指定索引的元素");
                println!("  click_text <文字>        - 点击包含指定文字的元素");
                println!("  screenshot, s            - 保存当前截图");
                println!("  help, h                  - 显示帮助");
                println!("  quit, q                  - 退出交互模式");
                
                if let Some(registry) = page_registry {
                    let commands = registry.get_commands();
                    if !commands.is_empty() {
                        println!("\n页面特定命令:");
                        for (cmd, help) in commands {
                            println!("  {:24} - {}", cmd, help);
                        }
                    }
                }
            }
            _ => {
                println!("未知命令: {}", cmd);
                println!("输入 'help' 查看帮助，'quit' 退出");
            }
        }
    }
}

/// 带页面对象的交互 shell - 支持执行页面特定命令
fn interactive_shell_with_registry(
    operator: &mut AdbOperator,
    page_name: &str,
    elements: &[(String, f32, f32)],
    registry: &PageCommandRegistry,
    mut page: Box<dyn std::any::Any>,
) {
    println!("\n==================================================");
    println!("交互模式 - {}", page_name);
    println!("==================================================");
    println!("通用命令:");
    println!("  elements, e              - 显示检测到的所有元素");
    println!("  click <索引>             - 点击指定索引的元素");
    println!("  click_text <文字>        - 点击包含指定文字的元素 (模糊匹配)");
    println!("  screenshot, s            - 保存当前截图");
    println!("  help, h                  - 显示帮助");
    println!("  quit, q                  - 退出交互模式");
    
    // 显示页面特定命令
    let commands = registry.get_commands();
    if !commands.is_empty() {
        println!("\n页面特定命令:");
        for (cmd, help) in &commands {
            println!("  {:24} - {}", cmd, help);
        }
    }
    
    println!("==================================================");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("\n[{}]> ", page_name);
        stdout.flush().unwrap();

        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() { break; }

        let input = input.trim();
        if input.is_empty() { continue; }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "quit" | "exit" | "q" => { println!("退出交互模式"); break; }
            "elements" | "e" | "list" | "ls" => {
                if elements.is_empty() {
                    println!("未检测到任何元素");
                } else {
                    println!("检测到的元素 ({} 个):", elements.len());
                    for (i, (text, x, y)) in elements.iter().enumerate() {
                        println!("  [{}] '{}' @ ({:.3}, {:.3})", i, text, x, y);
                    }
                }
            }
            "click" | "c" => {
                if args.is_empty() { println!("用法: click <索引>"); continue; }
                if let Ok(idx) = args[0].parse::<usize>() {
                    if idx < elements.len() {
                        let (text, x, y) = &elements[idx];
                        println!("点击元素 [{}]: '{}' @ ({:.3}, {:.3})", idx, text, x, y);
                        let _ = operator.click_point(*x, *y, 1.0);
                        AdbOperator::sleep(2.0);
                    } else {
                        println!("索引 {} 超出范围 (0-{})", idx, elements.len().saturating_sub(1));
                    }
                } else {
                    println!("无效的索引: {}", args[0]);
                }
            }
            "click_text" | "ct" => {
                if args.is_empty() { println!("用法: click_text <文字>"); continue; }
                let search_text = args.join(" ");
                if let Some((i, (text, x, y))) = elements.iter().enumerate().find(|(_, (t, _, _))| t.contains(&search_text)) {
                    println!("点击元素 [{}]: '{}' @ ({:.3}, {:.3})", i, text, x, y);
                    let _ = operator.click_point(*x, *y, 1.0);
                    AdbOperator::sleep(2.0);
                } else {
                    println!("未找到包含 '{}' 的元素", search_text);
                }
            }
            "screenshot" | "s" | "ss" => {
                match operator.screenshot() {
                    Ok(img) => {
                        let filename = format!("screenshot_{}.png", chrono::Local::now().format("%Y%m%d_%H%M%S"));
                        if let Err(e) = img.save(&filename) { println!("保存截图失败: {}", e); }
                        else { println!("截图已保存: {}", filename); }
                    }
                    Err(e) => println!("截图失败: {}", e),
                }
            }
            "help" | "h" | "?" => {
                println!("\n通用命令:");
                println!("  elements, e              - 显示检测到的所有元素");
                println!("  click <索引>             - 点击指定索引的元素");
                println!("  click_text <文字>        - 点击包含指定文字的元素");
                println!("  screenshot, s            - 保存当前截图");
                println!("  help, h                  - 显示帮助");
                println!("  quit, q                  - 退出交互模式");
                
                let commands = registry.get_commands();
                if !commands.is_empty() {
                    println!("\n页面特定命令:");
                    for (cmd, help) in &commands {
                        println!("  {:24} - {}", cmd, help);
                    }
                }
            }
            _ => {
                // 执行页面特定命令
                match registry.execute(page.as_mut(), cmd, args, operator) {
                    ShellCommandResult::Success(Some(msg)) => println!("{}", msg),
                    ShellCommandResult::Success(None) => {},
                    ShellCommandResult::Error(e) => println!("错误: {}", e),
                    ShellCommandResult::NotFound => {
                        println!("未知命令: {}", cmd);
                        println!("输入 'help' 查看帮助，'quit' 退出");
                    }
                    ShellCommandResult::Exit => { println!("退出交互模式"); break; }
                }
            }
        }
    }
}

/// 从页面提取元素信息的 trait
trait ExtractElements {
    fn extract_elements(&self) -> Vec<(String, f32, f32)>;
}

// 为所有页面类型实现 ExtractElements
macro_rules! impl_extract_elements {
    ($page_type:ty) => {
        impl ExtractElements for $page_type {
            fn extract_elements(&self) -> Vec<(String, f32, f32)> {
                self.elements.iter().map(|e| (e.text.clone(), e.rel_x, e.rel_y)).collect()
            }
        }
    };
}

// 导入所有页面类型并为其生成实现
use crate::pages::{
    StartPage, GameModePage, DifficultyPage, BossAffixPage,
    PlaneSelectPage, InvestEnvironmentPage, PreparationPage, ShopPage,
    InvestStrategyPage, ExitConfirmDialog, ExitChallengeFailPage,
    ExitStatsPage, ExitReturnPage, BattleSettlementPage
};

impl_extract_elements!(StartPage);
impl_extract_elements!(GameModePage);
impl_extract_elements!(DifficultyPage);
impl_extract_elements!(BossAffixPage);
impl_extract_elements!(PlaneSelectPage);
impl_extract_elements!(InvestEnvironmentPage);
impl_extract_elements!(PreparationPage);
impl_extract_elements!(ShopPage);
impl_extract_elements!(InvestStrategyPage);
impl_extract_elements!(ExitConfirmDialog);
impl_extract_elements!(ExitChallengeFailPage);
impl_extract_elements!(ExitStatsPage);
impl_extract_elements!(ExitReturnPage);
impl_extract_elements!(BattleSettlementPage);

fn test_single_page(operator: &mut AdbOperator, page_name: &str, interactive: bool) {
    let mut detector = PageDetector::new(operator);

    macro_rules! test_page {
        ($detect_fn:ident) => {{
            let _ = detector.refresh();
            if let Some(page) = detector.$detect_fn() {
                info!("✓ 成功识别到页面: {}", page_name);
                if interactive {
                    let elements = page.extract_elements();
                    interactive_shell(operator, page_name, &elements, None);
                }
            } else {
                info!("✗ 未能识别到页面: {}", page_name);
            }
        }};
    }

    match page_name {
        "StartPage" => test_page!(detect_start_page),
        "GameModePage" => test_page!(detect_game_mode_page),
        "DifficultyPage" => test_page!(detect_difficulty_page),
        "BossAffixPage" => {
            let _ = detector.refresh();
            if let Some(page) = detector.detect_boss_affix_page() {
                info!("✓ 成功识别到页面: {}", page_name);
                info!("Boss 词条: {:?}", page.affixes);
                if interactive {
                    let elements = page.extract_elements();
                    interactive_shell(operator, page_name, &elements, None);
                }
            } else {
                info!("✗ 未能识别到页面: {}", page_name);
            }
        }
        "PlaneSelectPage" => test_page!(detect_plane_select_page),
        "InvestEnvironmentPage" => {
            let _ = detector.refresh();
            if let Some(page) = detector.detect_invest_environment_page() {
                info!("✓ 成功识别到页面: {}", page_name);
                info!("投资环境: {:?}", page.env_names);
                if interactive {
                    let elements = page.extract_elements();
                    let registry = InvestEnvironmentPage::get_shell_registry();
                    interactive_shell_with_registry(operator, page_name, &elements, &registry, Box::new(page));
                }
            } else {
                info!("✗ 未能识别到页面: {}", page_name);
            }
        }
        "PreparationPage" => test_page!(detect_preparation_page),
        "ShopPage" => test_page!(detect_shop_page),
        "InvestStrategyPage" => {
            let _ = detector.refresh();
            if let Some(page) = detector.detect_invest_strategy_page() {
                info!("✓ 成功识别到页面: {}", page_name);
                info!("投资策略: {:?}", page.strategy_names);
                if interactive {
                    let elements = page.extract_elements();
                    let registry = InvestStrategyPage::get_shell_registry();
                    interactive_shell_with_registry(operator, page_name, &elements, &registry, Box::new(page));
                }
            } else {
                info!("✗ 未能识别到页面: {}", page_name);
            }
        }
        "ExitConfirmDialog" => test_page!(detect_exit_confirm_dialog),
        "ExitChallengeFailPage" => test_page!(detect_exit_challenge_fail_page),
        "ExitStatsPage" => test_page!(detect_exit_stats_page),
        "ExitReturnPage" => test_page!(detect_exit_return_page),
        "BattleSettlementPage" => test_page!(detect_battle_settlement_page),
        _ => {
            info!("未知页面类: {}", page_name);
            info!("可用的页面类: {:?}", get_page_names());
            return;
        }
    }

    info!("==================================================");
}

pub fn print_available_pages() {
    info!("可用的页面类:");
    for name in get_page_names() {
        info!("  ✓ {}", name);
    }
}
