mod adb_operator;
mod pages;
mod reroll_task;
mod selection_manager;
mod config;
mod embedded_assets;
mod template_manager;
mod logger;
mod test_page;

use adb_operator::AdbOperator;
use reroll_task::AndroidRerollStart;
use selection_manager::OpeningConfig;
use config::{load_config, create_example_config};
use logger::{info, error, debug, init as init_logger};
use test_page::{print_available_pages, test_with_screenshot, test_with_adb};
use std::env;

fn print_usage() {
    println!("SRCWRoller - 星穹铁道货币战争自动刷开局工具");
    println!();
    println!("用法:");
    println!("  srcwroller <command> [options]");
    println!();
    println!("Commands:");
    println!("  run                    运行主程序 (默认命令)");
    println!("  test-page              测试页面检测");
    println!();
    println!("Global Options:");
    println!("  --help, -h             显示帮助信息");
    println!("  --config <path>        指定配置文件路径");
    println!("  --device <serial>      指定 ADB 设备序列号");
    println!("  --debug_ops [dir]     保存操作截图 (点击/拖动/OCR可视化)");
    println!("  --save_opening [dir]  保存开局界面图片 (词条/环境/策略页面)");
    println!();
    println!("Run Options:");
    println!("  --example              创建示例配置文件");
    println!();
    println!("Test-Page Options:");
    println!("  --page <name>          指定要测试的页面名 (不指定则测试所有)");
    println!("  --screenshot <path>    使用截图文件测试 (不连接 ADB)");
    println!("  --shell                进入交互模式");
    println!("  --list-pages           列出所有可用的页面类");
    println!("  --list-images          列出所有嵌入的图片资源");
    println!("  --extract-images       提取所有嵌入的图片到当前目录");
    println!();
    println!("配置文件搜索顺序:");
    println!("  1. 当前目录: config.json, config.yaml, srcwroller.json, srcwroller.yaml");
    println!("  2. 可执行文件所在目录");
    println!();
    println!("Examples:");
    println!("  srcwroller                                         # 运行主程序");
    println!("  srcwroller run --debug_ops                    # 运行并保存操作截图");
    println!("  srcwroller run --save_opening                # 运行并保存开局界面图片");
    println!("  srcwroller test-page                          # ADB 测试所有页面");
    println!("  srcwroller test-page --page StartPage         # ADB 测试单个页面");
    println!("  srcwroller test-page --page StartPage --shell # 测试并进入交互模式");
    println!("  srcwroller test-page --screenshot test.png    # 截图测试所有页面");
    println!("  srcwroller test-page --list-pages             # 列出所有页面");
}

enum Command {
    Run,
    TestPage,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        run_main(None, None, false, "debug_ops".to_string(), false, "opening_screens".to_string());
        return;
    }

    let first_arg = args[1].as_str();

    if first_arg == "--help" || first_arg == "-h" {
        print_usage();
        return;
    }

    // --list-pages 现在需要在 test-page 子命令下使用
    // 例如: srcwroller test-page --list-pages

    if first_arg == "--list-images" {
        println!("嵌入的图片资源:");
        for img in embedded_assets::list_images() {
            println!("  - {}", img);
        }
        return;
    }

    if first_arg == "--extract-images" {
        if let Err(e) = embedded_assets::extract_all_images("./extracted_images") {
            eprintln!("提取图片失败: {}", e);
        }
        return;
    }

    if first_arg == "--example" {
        if let Err(e) = create_example_config() {
            eprintln!("创建示例配置文件失败: {}", e);
        }
        return;
    }

    let command = match first_arg {
        "run" => Command::Run,
        "test-page" => Command::TestPage,
        _ => {
            if first_arg.starts_with("--") {
                Command::Run
            } else {
                eprintln!("未知命令: {}", first_arg);
                print_usage();
                return;
            }
        }
    };

    let start_idx = match command {
        Command::Run => if first_arg == "run" { 2 } else { 1 },
        Command::TestPage => 2,
    };

    match command {
        Command::Run => {
            let mut config_path: Option<String> = None;
            let mut device_serial: Option<String> = None;
            let mut debug_ops = false;
            let mut debug_ops_dir = "debug_ops".to_string();
            let mut save_opening = false;
            let mut opening_dir = "opening_screens".to_string();
            let mut i = start_idx;

            while i < args.len() {
                match args[i].as_str() {
                    "--help" | "-h" => {
                        print_usage();
                        return;
                    }
                    "--config" | "-c" => {
                        if i + 1 < args.len() {
                            config_path = Some(args[i + 1].clone());
                            i += 1;
                        } else {
                            eprintln!("错误: --config 需要指定配置文件路径");
                            return;
                        }
                    }
                    "--device" | "-d" => {
                        if i + 1 < args.len() {
                            device_serial = Some(args[i + 1].clone());
                            i += 1;
                        } else {
                            eprintln!("错误: --device 需要指定设备序列号");
                            return;
                        }
                    }
                    "--debug_ops" => {
                        debug_ops = true;
                        if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                            debug_ops_dir = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--save_opening" => {
                        save_opening = true;
                        if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                            opening_dir = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--example" => {
                        if let Err(e) = create_example_config() {
                            eprintln!("创建示例配置文件失败: {}", e);
                        }
                        return;
                    }
                    _ => {
                        eprintln!("未知选项: {}", args[i]);
                        print_usage();
                        return;
                    }
                }
                i += 1;
            }

            run_main(config_path.as_deref(), device_serial.as_deref(), debug_ops, debug_ops_dir, save_opening, opening_dir);
        }
        Command::TestPage => {
            let mut page_name: Option<String> = None;
            let mut device_serial: Option<String> = None;
            let mut screenshot_path: Option<String> = None;
            let mut interactive = false;
            let mut debug_ops = false;
            let mut debug_ops_dir = "debug_ops".to_string();
            let mut save_opening = false;
            let mut opening_dir = "opening_screens".to_string();
            let mut list_pages = false;
            let mut i = start_idx;

            while i < args.len() {
                match args[i].as_str() {
                    "--help" | "-h" => {
                        print_usage();
                        return;
                    }
                    "--page" | "-p" => {
                        if i + 1 < args.len() {
                            page_name = Some(args[i + 1].clone());
                            i += 1;
                        } else {
                            eprintln!("错误: --page 需要指定页面名称");
                            return;
                        }
                    }
                    "--screenshot" | "-s" => {
                        if i + 1 < args.len() {
                            screenshot_path = Some(args[i + 1].clone());
                            i += 1;
                        } else {
                            eprintln!("错误: --screenshot 需要指定截图文件路径");
                            return;
                        }
                    }
                    "--device" | "-d" => {
                        if i + 1 < args.len() {
                            device_serial = Some(args[i + 1].clone());
                            i += 1;
                        } else {
                            eprintln!("错误: --device 需要指定设备序列号");
                            return;
                        }
                    }
                    "--shell" => {
                        interactive = true;
                    }
                    "--list-pages" | "-l" => {
                        list_pages = true;
                    }
                    "--debug_ops" => {
                        debug_ops = true;
                        if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                            debug_ops_dir = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    "--save_opening" => {
                        save_opening = true;
                        if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                            opening_dir = args[i + 1].clone();
                            i += 1;
                        }
                    }
                    _ => {
                        if !args[i].starts_with("--") && page_name.is_none() {
                            page_name = Some(args[i].clone());
                        } else {
                            eprintln!("未知选项: {}", args[i]);
                            print_usage();
                            return;
                        }
                    }
                }
                i += 1;
            }

            if list_pages {
                init_logger(false);
                print_available_pages();
                return;
            }

            if let Some(screenshot) = screenshot_path {
                test_with_screenshot(&screenshot, page_name.as_deref(), interactive);
            } else {
                test_with_adb(page_name.as_deref(), device_serial.as_deref(), interactive, debug_ops, debug_ops_dir, save_opening, opening_dir);
            }
        }
    }
}

fn run_main(
    config_path: Option<&str>,
    device_serial: Option<&str>,
    debug_ops: bool,
    debug_ops_dir: String,
    save_opening: bool,
    opening_dir: String
) {
    init_logger(false);

    let config = load_config();

    debug!("配置加载完成: {:?}", config);

    info!("连接 ADB 设备...");
    let operator = match AdbOperator::new_with_ocr_config(
        device_serial,
        Some(&config.ocr),
    ) {
        Ok(mut op) => {
            info!("设备连接成功");
            info!("分辨率: {}x{}", op.width, op.height);
            if debug_ops {
                op.enable_debug_ops(&debug_ops_dir);
            }
            op
        }
        Err(e) => {
            error!("ADB 连接失败: {}", e);
            error!("请确保:");
            error!("  1. 手机已开启 USB 调试并连接电脑");
            error!("  2. ADB 服务器正在运行 (执行: adb start-server)");
            error!("  3. 已安装 Android Platform Tools");
            return;
        }
    };

    let openings: Vec<OpeningConfig> = config.openings.iter().map(|o| {
        OpeningConfig::new(o.env.clone(), o.strategy.clone(), o.affix.clone())
    }).collect();

    let prefer_env = config.prefer_env.clone();
    let max_retry = config.max_retry;

    let mut reroll = AndroidRerollStart::new(&operator, openings, max_retry, prefer_env);
    if save_opening {
        reroll.enable_save_opening(&opening_dir);
    }

    info!("开始运行货币战争重开任务...");
    reroll.run();
}
