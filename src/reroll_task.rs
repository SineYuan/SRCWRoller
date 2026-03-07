use crate::adb_operator::AdbOperator;
use crate::pages::{
    BattleSettlementPage, BossAffixPage, DifficultyPage, ExitChallengeFailPage,
    ExitConfirmDialog, ExitReturnPage, ExitStatsPage, GameModePage, InvestEnvironmentPage,
    InvestStrategyPage, PageDetector, PlaneSelectPage, PreparationPage, ShopPage,
};
use crate::selection_manager::{OpeningConfig, SelectionManager};
use crate::logger::{info, debug, warn, error};
use crate::{log_section, log_success, log_retry, log_step};

pub struct AndroidRerollStart<'a> {
    operator: &'a AdbOperator,
    selector: SelectionManager,
    max_retry: i32,
    tries: i32,
    detector: PageDetector<'a>,
    save_opening: bool,
    save_opening_dir: String,
    save_opening_count: i32,
}

const IN_HAND_AREA: [(f32, f32); 9] = [
    (0.229, 0.844), (0.297, 0.844), (0.358, 0.844), (0.426, 0.844), (0.488, 0.844),
    (0.556, 0.844), (0.618, 0.844), (0.684, 0.844), (0.749, 0.844),
];

const ON_FIELD_AREA: [(f32, f32); 4] = [
    (0.386, 0.365), (0.464, 0.365), (0.536, 0.365), (0.611, 0.365),
];

impl<'a> AndroidRerollStart<'a> {
    pub fn new(
        operator: &'a AdbOperator,
        openings: Vec<OpeningConfig>,
        max_retry: i32,
        prefer_invest_env: Vec<String>,
    ) -> Self {
        let selector = SelectionManager::new(openings, prefer_invest_env);
        let detector = PageDetector::new(operator);

        debug!("AndroidRerollStart 初始化, 分辨率: {}x{}", operator.width, operator.height);
        debug!("开局配置: {:?}", selector.active_openings.len());

        Self {
            operator,
            selector,
            max_retry,
            tries: 0,
            detector,
            save_opening: false,
            save_opening_dir: "opening_screens".to_string(),
            save_opening_count: 0,
        }
    }

    pub fn enable_save_opening(&mut self, output_dir: &str) {
        self.save_opening = true;
        self.save_opening_dir = output_dir.to_string();
        std::fs::create_dir_all(output_dir).ok();
        info!("保存开局界面已启用，输出目录: {}", output_dir);
    }

    pub fn save_opening_screenshot(&mut self, page_name: &str, suffix: &str) {
        if !self.save_opening {
            return;
        }

        let screenshot = match self.operator.screenshot() {
            Ok(img) => img,
            Err(e) => {
                warn!("获取截图失败: {}", e);
                return;
            }
        };

        self.save_opening_count += 1;
        let count = self.save_opening_count;

        let img_cv = match AdbOperator::image_to_mat(&screenshot) {
            Ok(mat) => mat,
            Err(_) => return,
        };

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = if suffix.is_empty() {
            format!("{}_{}_{:03}.png", timestamp, page_name, count)
        } else {
            format!("{}_{}_{}_{:03}.png", timestamp, page_name, suffix, count)
        };
        let filepath = std::path::Path::new(&self.save_opening_dir).join(&filename);

        if let Err(e) = opencv::imgcodecs::imwrite(
            filepath.to_str().unwrap(),
            &img_cv,
            &opencv::core::Vector::new(),
        ) {
            warn!("保存开局截图失败: {}", e);
        } else {
            info!("开局截图已保存: {:?}", filepath);
        }
    }

    pub fn run(&mut self) {
        log_section!("Android 货币战争刷开局开始");
        info!("开局配置数量: {:?}", self.selector.active_openings.len());
        info!("偏好投资环境: {:?}", self.selector.prefer_env);
        info!("最大尝试次数: {}", self.max_retry);

        while self.tries < self.max_retry {
            self.tries += 1;
            log_retry!(self.tries, self.max_retry);

            match self.run_once() {
                Ok("success") => {
                    log_success!("刷开局成功!");
                    return;
                }
                Ok("retry") => continue,
                Ok("error") => {
                    AdbOperator::sleep(2.0);
                    continue;
                }
                Err(e) => {
                    error!("执行异常: {}", e);
                    AdbOperator::sleep(2.0);
                }
                _ => {}
            }
        }

        info!("达到最大尝试次数 {}，结束", self.max_retry);
    }

    fn run_once(&mut self) -> Result<&'static str, Box<dyn std::error::Error>> {
        self.selector.reset();
        info!("当前活跃开局配置: {}", self.selector.get_active_count());

        self.detector.refresh();

        log_step!(1, "检测开始页面");
        
        let prep_page = self.detector.detect_preparation_page();
        let start_page = self.detector.detect_start_page();

        if prep_page.is_none() && start_page.is_none() {
            error!("页面识别失败");
            return Ok("error");
        }

        if prep_page.is_some() {
            info!("处于准备阶段，结算返回");
            self.abort_and_return(true)?;
            return Ok("retry");
        }

        let start_page = match start_page {
            Some(page) => page,
            None => {
                warn!("未在开始页面，尝试返回");
                self.try_go_back()?;
                return Ok("retry");
            }
        };

        log_step!(2, "点击开始按钮");
        if !start_page.click_start(self.operator) {
            return Ok("error");
        }

        log_step!(3, "等待游戏模式选择页面");
        let game_mode_page = self.wait_for_game_mode_page(10)?;

        if game_mode_page.state == GameModePage::STATE_IN_PROGRESS {
            info!("检测到游戏进行中，点击结束并结算");
            if !game_mode_page.click_end_and_settle(self.operator) {
                return Ok("error");
            }
            if !self.exit_in_progress_game()? {
                return Ok("error");
            }
            return Ok("retry");
        } else {
            if !game_mode_page.click_enter_standard(self.operator) {
                return Ok("error");
            }
        }

        log_step!(4, "等待难度选择页面");
        let difficulty_page = self.wait_for_difficulty_page(10)?;
        if !difficulty_page.click_start_battle(self.operator) {
            return Ok("error");
        }

        log_step!(5, "等待 Boss 词条页面");
        let boss_affix_page = self.wait_for_boss_affix_page(10)?;

        self.selector.current_affixes = boss_affix_page.affixes.clone();
        info!("当前 Boss 词条: {:?}", self.selector.current_affixes);

        self.filter_openings_by_affix();

        if !boss_affix_page.click_next_step(self.operator) {
            return Ok("error");
        }

        info!("等待 Boss 词条页面动画完成...");
        AdbOperator::sleep(3.0);

        log_step!(6, "等待位面选择页面");
        let plane_select_page = self.wait_for_plane_select_page(10)?;
        if !plane_select_page.click_blank_continue(self.operator) {
            return Ok("error");
        }

        log_step!(7, "处理投资环境");
        if !self.handle_invest_environment_loop()? {
            return Ok("retry");
        }

        log_step!(8, "继续游戏到 1-2 关卡检测投资策略");
        match self.handle_invest_strategy()? {
            "success" => return Ok("success"),
            "retry" => return Ok("retry"),
            _ => return Ok("error"),
        }
    }

    fn wait_for_game_mode_page(&mut self, timeout: i32) -> Result<GameModePage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_game_mode_page() {
                return Ok(page);
            }
            AdbOperator::sleep(1.5);
        }
        Err("等待游戏模式选择页面超时".into())
    }

    fn wait_for_difficulty_page(&mut self, timeout: i32) -> Result<DifficultyPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_difficulty_page() {
                return Ok(page);
            }
            AdbOperator::sleep(1.5);
        }
        Err("等待难度选择页面超时".into())
    }

    fn wait_for_boss_affix_page(&mut self, timeout: i32) -> Result<BossAffixPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_boss_affix_page() {
                self.save_opening_screenshot("BossAffixPage", "detected");
                return Ok(page);
            }
            AdbOperator::sleep(1.5);
        }
        Err("等待 Boss 词条页面超时".into())
    }

    fn wait_for_plane_select_page(&mut self, timeout: i32) -> Result<PlaneSelectPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_plane_select_page() {
                return Ok(page);
            }
            AdbOperator::sleep(1.5);
        }
        Err("等待位面选择页面超时".into())
    }

    fn wait_for_preparation_page(&mut self, timeout: i32) -> Result<PreparationPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_preparation_page() {
                return Ok(page);
            }
            AdbOperator::sleep(0.5);
        }
        Err("等待准备阶段页面超时".into())
    }

    fn wait_for_invest_environment_page(&mut self, timeout: i32) -> Result<InvestEnvironmentPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_invest_environment_page() {
                return Ok(page);
            }
            AdbOperator::sleep(0.5);
        }
        Err("等待投资环境页面超时".into())
    }

    fn wait_for_shop_page(&mut self, timeout: i32) -> Result<ShopPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_shop_page() {
                return Ok(page);
            }
            AdbOperator::sleep(0.5);
        }
        Err("等待商店页面超时".into())
    }

    fn wait_for_invest_strategy_page(&mut self, timeout: i32) -> Result<InvestStrategyPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_invest_strategy_page() {
                return Ok(page);
            }
            AdbOperator::sleep(0.5);
        }
        Err("等待投资策略选择页面超时".into())
    }

    fn wait_for_battle_settlement_page(&mut self, timeout: i32) -> Result<BattleSettlementPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_battle_settlement_page() {
                return Ok(page);
            }
            AdbOperator::sleep(2.0);
        }
        Err("等待战斗结算页面超时".into())
    }

    fn wait_for_exit_confirm_dialog(&mut self, timeout: i32) -> Result<ExitConfirmDialog, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_exit_confirm_dialog() {
                return Ok(page);
            }
            AdbOperator::sleep(0.5);
        }
        Err("等待退出确认对话框超时".into())
    }

    fn wait_for_exit_challenge_fail_page(&mut self, timeout: i32) -> Result<ExitChallengeFailPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_exit_challenge_fail_page() {
                return Ok(page);
            }
            AdbOperator::sleep(0.5);
        }
        Err("等待挑战失败页面超时".into())
    }

    fn wait_for_exit_stats_page(&mut self, timeout: i32) -> Result<ExitStatsPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_exit_stats_page() {
                return Ok(page);
            }
            AdbOperator::sleep(0.5);
        }
        Err("等待统计页面超时".into())
    }

    fn wait_for_exit_return_page(&mut self, timeout: i32) -> Result<ExitReturnPage, Box<dyn std::error::Error>> {
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < timeout as u64 {
            self.detector.refresh();
            if let Some(page) = self.detector.detect_exit_return_page() {
                return Ok(page);
            }
            AdbOperator::sleep(0.5);
        }
        Err("等待返回页面超时".into())
    }

    fn handle_invest_environment_loop(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        let max_loops = 5;
        let mut loop_count = 0;

        while loop_count < max_loops {
            loop_count += 1;
            info!("投资环境选择循环 {}/{}", loop_count, max_loops);

            info!("等待准备阶段页面...");
            if let Ok(_page) = self.wait_for_preparation_page(10) {
                info!("已进入准备阶段，投资环境选择完成");
                return Ok(true);
            }

            info!("等待投资环境页面...");
            if let Ok(page) = self.wait_for_invest_environment_page(10) {
                if !self.handle_single_invest_env(&page)? {
                    return Ok(false);
                }
                AdbOperator::sleep(2.0);
                continue;
            }

            warn!("等待投资环境页面或准备阶段页面超时");
            return Ok(false);
        }

        warn!("投资环境选择循环次数超过限制 ({})", max_loops);
        Ok(false)
    }

    fn handle_single_invest_env(&mut self, page: &InvestEnvironmentPage) -> Result<bool, Box<dyn std::error::Error>> {
        let mut envs = page.get_envs();
        let mut env_names: Vec<String> = envs.iter().map(|(name, _)| name.clone()).collect();

        // 如果检测到 0 个环境，等待并重新检测
        if env_names.is_empty() {
            warn!("检测到 0 个投资环境，等待动画结束后重新检测...");
            AdbOperator::sleep(1.0);
            self.detector.refresh();
            if let Some(new_page) = self.detector.detect_invest_environment_page() {
                envs = new_page.get_envs();
                env_names = envs.iter().map(|(name, _)| name.clone()).collect();
            }
        }

        let (env_index, env_name, reason) = self.selector.select_env(&env_names, true, false);

        if reason == "random" && self.selector.has_wanted_envs() {
            info!("未匹配到目标环境，尝试刷新...");
            self.save_opening_screenshot("InvestEnvironmentPage", "before_refresh");
            page.click_refresh(self.operator);
            AdbOperator::sleep(2.0);

            info!("刷新后重新检测...");
            self.detector.refresh();
            let new_page = self.wait_for_invest_environment_page(10)?;

            let envs = new_page.get_envs();
            let env_names: Vec<String> = envs.iter().map(|(name, _)| name.clone()).collect();
            let (index, name, _reason) = self.selector.select_env(&env_names, true, false);

            if index == -1 {
                error!("无法选择投资环境");
                return Ok(false);
            }

            self.selector.filter_by_env(&name);

            new_page.select_by_index(self.operator, index as usize);
            self.save_opening_screenshot("InvestEnvironmentPage", "before_confirm");
            new_page.click_confirm(self.operator);
            AdbOperator::sleep(2.0);
            return Ok(true);
        }

        if env_index == -1 {
            error!("无法选择投资环境");
            return Ok(false);
        }

        self.selector.filter_by_env(&env_name);

        page.select_by_index(self.operator, env_index as usize);
        self.save_opening_screenshot("InvestEnvironmentPage", "before_confirm");
        page.click_confirm(self.operator);
        AdbOperator::sleep(2.0);
        Ok(true)
    }

    fn filter_openings_by_affix(&mut self) {
        let affixes = self.selector.current_affixes.clone();
        self.selector.filter_by_affix(&affixes);
    }

    fn handle_invest_strategy(&mut self) -> Result<&'static str, Box<dyn std::error::Error>> {
        info!("等待准备阶段页面...");
        let prep_page = self.wait_for_preparation_page(15)?;

        if !self.selector.has_active_openings() {
            info!("没有活跃的开局配置，退出本次尝试");
            prep_page.click_exit(self.operator);
            self.exit_in_progress_game()?;
            return Ok("retry");
        }

        info!("活跃开局配置: {}，开始游戏流程...", self.selector.get_active_count());

        info!("拖动手牌前 4 个到前台...");
        for i in 0..4.min(IN_HAND_AREA.len()) {
            let hand_pos = IN_HAND_AREA[i];
            let field_pos = ON_FIELD_AREA[i];
            debug!("拖动 手牌[{}] -> 场上[{}]", i, i);
            self.operator.drag_to(hand_pos.0, hand_pos.1, field_pos.0, field_pos.1)?;
            AdbOperator::sleep(0.5);

            debug!("检查特殊事件...");
            self.handle_special_events2()?;
        }

        info!("=== 第一场战斗 ===");
        info!("点击出战按钮...");
        if !prep_page.click_battle(self.operator) {
            error!("点击出战按钮失败");
            return Ok("error");
        }

        self.check_and_handle_no_enough_dialog()?;

        info!("等待战斗结束...");
        let settlement_page = self.wait_for_battle_settlement_page(600)?;
        info!("挑战结束，点击继续...");
        settlement_page.click_continue(self.operator);

        info!("等待商店页面...");
        let shop_page = self.wait_for_shop_page(10)?;
        info!("退出商店...");
        shop_page.exit_shop(self.operator);

        info!("=== 第二场战斗 ===");
        info!("等待准备阶段页面...");
        let prep_page = self.wait_for_preparation_page(15)?;
        info!("点击出战按钮...");
        if !prep_page.click_battle(self.operator) {
            error!("点击出战按钮失败");
            return Ok("error");
        }

        self.check_and_handle_no_enough_dialog()?;

        info!("等待战斗结束...");
        let settlement_page = self.wait_for_battle_settlement_page(600)?;
        info!("挑战结束，点击继续...");
        settlement_page.click_continue(self.operator);

        info!("等待投资策略选择页面...");
        let strategy_page = self.wait_for_invest_strategy_page(30)?;
        info!("到达投资策略选择页面");

        let result = self.do_select_invest_strategy(&strategy_page)?;
        if result != "success" {
            return Ok(result);
        }

        info!("等待检测下一步页面（商店或再次策略选择）...");
        AdbOperator::sleep(5.0);
        self.detector.refresh();

        let next_strategy_page = self.detector.detect_invest_strategy_page();
        let next_shop_page = self.detector.detect_shop_page();

        let shop_page = if next_strategy_page.is_some() {
            info!("检测到再次进入投资策略选择页面，进行第二次选择...");
            let strategy_page = next_strategy_page.unwrap();
            let result = self.do_select_invest_strategy(&strategy_page)?;
            if result != "success" {
                return Ok(result);
            }

            info!("第二次选择后等待检测页面...");
            AdbOperator::sleep(5.0);
            self.detector.refresh();

            if let Some(shop) = self.detector.detect_shop_page() {
                shop
            } else {
                error!("第二次策略选择后未进入商店页面");
                return Ok("error");
            }
        } else if let Some(shop) = next_shop_page {
            shop
        } else {
            error!("策略选择后未检测到商店页面或策略页面");
            return Ok("error");
        };

        info!("退出商店...");
        shop_page.exit_shop(self.operator);

        if !self.selector.has_active_openings() {
            info!("未匹配到目标策略，执行退出重试流程...");

            info!("等待准备阶段页面...");
            let prep_page = self.wait_for_preparation_page(15)?;

            info!("点击退出...");
            prep_page.click_exit(self.operator);

            info!("执行退出流程...");
            self.exit_in_progress_game()?;
            return Ok("retry");
        }

        Ok("success")
    }

    fn do_select_invest_strategy(&mut self, strategy_page: &InvestStrategyPage) -> Result<&'static str, Box<dyn std::error::Error>> {
        let mut strategies = strategy_page.get_strategies();
        let mut strategy_names: Vec<String> = strategies.iter().map(|(name, _)| name.clone()).collect();

        // 如果检测到 0 个策略，等待并重新检测
        if strategy_names.is_empty() {
            warn!("检测到 0 个投资策略，等待动画结束后重新检测...");
            AdbOperator::sleep(1.0);
            self.detector.refresh();
            if let Some(new_page) = self.detector.detect_invest_strategy_page() {
                strategies = new_page.get_strategies();
                strategy_names = strategies.iter().map(|(name, _)| name.clone()).collect();
            }
        }

        let (mut strategy_index, mut strategy_name, mut reason) = self.selector.select_strategy(&strategy_names, false);
        let mut current_page = strategy_page.clone();

        // 循环刷新直到匹配到目标策略或刷新次数用完
        if reason == "random" && self.selector.has_wanted_strategies() {
            loop {
                let refresh_count = current_page.get_refresh_count();
                if refresh_count <= 0 {
                    info!("未匹配到目标策略，且无刷新次数");
                    break;
                }

                info!("未匹配到目标策略，有 {} 次刷新机会，执行刷新...", refresh_count);
                self.save_opening_screenshot("InvestStrategyPage", "before_refresh");
                current_page.click_refresh(self.operator);
                AdbOperator::sleep(2.0);

                let _ = self.detector.refresh();
                let new_page = self.wait_for_invest_strategy_page(10)?;
                let strategies = new_page.get_strategies();
                let strategy_names: Vec<String> = strategies.iter().map(|(name, _)| name.clone()).collect();
                let (index, name, new_reason) = self.selector.select_strategy(&strategy_names, false);

                strategy_index = index;
                strategy_name = name;
                reason = new_reason;
                current_page = new_page;

                // 如果匹配到目标策略，退出循环
                if reason != "random" {
                    info!("刷新后匹配到目标策略: '{}'", strategy_name);
                    break;
                }

                // 如果刷新次数用完，也退出循环
                if current_page.get_refresh_count() <= 0 {
                    info!("刷新次数已用完，选择当前策略: '{}'", strategy_name);
                    break;
                }
            }
        }

        if strategy_index == -1 {
            error!("无法选择投资策略");
            return Ok("error");
        }

        self.selector.filter_by_strategy(&strategy_name);

        current_page.select_by_index(self.operator, strategy_index as usize);
        // 选择策略后等待，确保数据加载完成（Python 约3秒，Rust 用3.5秒更保险）
        AdbOperator::sleep(3.5);
        self.save_opening_screenshot("InvestStrategyPage", "before_confirm");
        current_page.click_confirm(self.operator);

        Ok("success")
    }

    fn handle_special_events(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 使用 wait_any_img 等待特殊事件出现（最多3秒，每0.5秒检测一次）
        let (event, _box_) = self.operator.wait_any_img(&[
            "ThePlanetOfFestivities.png",
            "FortuneTeller.png",
        ], 3.0, 0.5)?;

        if event == 0 {
            info!("检测到盛会之星事件，选择第一个选项...");
            self.operator.click_point(0.5, 0.35, 1.0)?;
            self.operator.click_point(0.77, 0.62, 1.0)?;
            // 处理完一个事件后，递归调用以检测是否有更多事件
            return self.handle_special_events();
        }

        if event == 1 {
            info!("检测到命运卜者事件，选择第三个选项...");
            self.operator.click_point(0.8, 0.35, 1.0)?;
            self.operator.click_point(0.77, 0.521, 1.0)?;
            // 处理完一个事件后，递归调用以检测是否有更多事件
            return self.handle_special_events();
        }

        info!("特殊事件处理完成");
        Ok(())
    }

    /// 使用 OCR + 模板匹配识别特殊事件
    /// 盛会之星: OCR 识别 "请选择1名角色成为巨星" 文字
    /// 命运卜者: 模板匹配 FortuneTeller.png
    fn handle_special_events2(&self) -> Result<(), Box<dyn std::error::Error>> {
        use crate::adb_operator::Region;
        
        // 定义 OCR 识别区域（小区域以提高速度）
        let ocr_region = Region {
            left: (0.45 * self.operator.get_width() as f32) as i32,
            top: (0.11 * self.operator.get_height() as f32) as i32,
            width: (0.20 * self.operator.get_width() as f32) as i32,
            height: (0.05 * self.operator.get_height() as f32) as i32,
        };
        
        info!("使用 OCR+模板匹配 检测特殊事件");
        
        // 最多检测3秒，每0.5秒检测一次
        let start = std::time::Instant::now();
        while start.elapsed().as_secs_f32() < 1.5 {
            // 1. 先用 OCR 检测盛会之星
            let ocr_result = self.operator.ocr_in_region(&ocr_region)?;
            let has_festivity_event = ocr_result.iter().any(|r| {
                r.text.contains("请选择") || r.text.contains("成为巨星")
            });
            
            if has_festivity_event {
                info!("OCR 检测到盛会之星事件，选择第一个选项...");
                self.operator.click_point(0.5, 0.35, 1.0)?;
                self.operator.click_point(0.77, 0.62, 1.0)?;
                // 处理完一个事件后，递归调用以检测是否有更多事件
                return self.handle_special_events2();
            }
            
            // 2. 用模板匹配检测命运卜者
            if let Some(_box) = self.operator.locate("FortuneTeller.png")? {
                info!("模板匹配检测到命运卜者事件，选择第三个选项...");
                self.operator.click_point(0.8, 0.35, 1.0)?;
                self.operator.click_point(0.77, 0.521, 1.0)?;
                // 处理完一个事件后，递归调用以检测是否有更多事件
                return self.handle_special_events2();
            }
            
            // 等待0.5秒后重试
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        
        info!("未检测到特殊事件");
        Ok(())
    }

    fn abort_and_return(&mut self, in_game: bool) -> Result<(), Box<dyn std::error::Error>> {
        info!("放弃当前进度并返回...");

        if in_game {
            self.detector.refresh();
            if let Some(prep_page) = self.detector.detect_preparation_page() {
                prep_page.click_exit(self.operator);
                AdbOperator::sleep(1.0);
                self.exit_in_progress_game()?;
            }
        }

        Ok(())
    }

    fn exit_in_progress_game(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        info!("处理退出流程...");

        self.detector.refresh();
        if let Ok(page) = self.wait_for_exit_confirm_dialog(5) {
            info!("点击放弃并结算...");
            page.click_give_up_and_settle(self.operator);
            AdbOperator::sleep(2.0);
        }

        if let Ok(page) = self.wait_for_exit_challenge_fail_page(10) {
            info!("点击下一步...");
            page.click_next_step(self.operator);
            AdbOperator::sleep(2.0);
        }

        if let Ok(page) = self.wait_for_exit_stats_page(10) {
            info!("点击下一页...");
            page.click_next_page(self.operator);
            AdbOperator::sleep(2.0);
        }

        self.detector.refresh();
        if let Ok(page) = self.wait_for_exit_return_page(5) {
            info!("点击返回货币战争...");
            page.click_return(self.operator);
            AdbOperator::sleep(2.0);
        }

        Ok(true)
    }

    fn check_and_handle_no_enough_dialog(&self) -> Result<bool, Box<dyn std::error::Error>> {
        if let Some(_box) = self.operator.wait_img("no_enough.png", 2.0, 0.3)? {
            info!("检测到'人数未达上限'提示框，点击确认...");
            self.operator.click_point(0.65, 0.65, 1.0)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn try_go_back(&self) -> Result<bool, Box<dyn std::error::Error>> {
        info!("尝试返回到开始页面...");
        for _ in 0..3 {
            self.operator.press_key("esc")?;
            AdbOperator::sleep(1.0);
        }
        Ok(false)
    }
}
