use crate::adb_operator::{AdbOperator, OcrResult, Region};
use crate::logger::{info, warn, error};
use crate::test_page::{PageCommandRegistry, ShellCommandResult};
use image::DynamicImage;

#[derive(Debug, Clone)]
pub struct ClickableElement {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub rel_x: f32,
    pub rel_y: f32,
    pub confidence: f32,
}

impl ClickableElement {
    pub fn new(text: String, x: f32, y: f32, rel_x: f32, rel_y: f32, confidence: f32) -> Self {
        Self { text, x, y, rel_x, rel_y, confidence }
    }

    pub fn from_ocr_result(result: &OcrResult, screen_width: i32, screen_height: i32) -> Self {
        let rel_x = result.x as f32 / screen_width as f32;
        let rel_y = result.y as f32 / screen_height as f32;
        Self {
            text: result.text.clone(),
            x: result.x as f32,
            y: result.y as f32,
            rel_x,
            rel_y,
            confidence: result.confidence,
        }
    }
}

fn do_ocr(screenshot: &DynamicImage, operator: &AdbOperator, regions: &[(f32, f32, f32, f32)], screen_width: i32, screen_height: i32) -> Vec<ClickableElement> {
    let mut all_elements = Vec::new();

    if regions.is_empty() {
        let region = Region::new(0, 0, screen_width, screen_height);
        if let Ok(results) = operator.ocr_on_image(screenshot, &region) {
            for result in results {
                all_elements.push(ClickableElement::from_ocr_result(&result, screen_width, screen_height));
            }
        }
    } else {
        for (rel_x1, rel_y1, rel_x2, rel_y2) in regions {
            let x1 = (*rel_x1 * screen_width as f32) as i32;
            let y1 = (*rel_y1 * screen_height as f32) as i32;
            let x2 = (*rel_x2 * screen_width as f32) as i32;
            let y2 = (*rel_y2 * screen_height as f32) as i32;
            let region = Region::new(x1, y1, x2 - x1, y2 - y1);
            
            if let Ok(results) = operator.ocr_on_image(screenshot, &region) {
                for result in results {
                    all_elements.push(ClickableElement::from_ocr_result(&result, screen_width, screen_height));
                }
            }
        }
    }
    all_elements
}

pub trait BasePage {
    fn find_element(&self, elements: &[ClickableElement], keyword: &str) -> Option<ClickableElement> {
        elements.iter().find(|e| e.text.contains(keyword)).cloned()
    }

    fn find_element_in_region(&self, elements: &[ClickableElement], keyword: &str, region: (f32, f32, f32, f32)) -> Option<ClickableElement> {
        let (x1, y1, x2, y2) = region;
        elements.iter()
            .filter(|e| e.rel_x >= x1 && e.rel_x <= x2 && e.rel_y >= y1 && e.rel_y <= y2)
            .find(|e| e.text.contains(keyword))
            .cloned()
    }

    fn click_element(&self, operator: &AdbOperator, elem: &ClickableElement, after_sleep: f32) -> Result<(), Box<dyn std::error::Error>> {
        info!("点击 '{}' 位置: ({:.0}, {:.0}) 相对: ({:.2}, {:.2})", elem.text, elem.x, elem.y, elem.rel_x, elem.rel_y);
        operator.click_point(elem.rel_x, elem.rel_y, after_sleep)
    }

    fn click_by_text(&self, operator: &AdbOperator, elements: &[ClickableElement], keyword: &str, region: Option<(f32, f32, f32, f32)>, after_sleep: f32) -> bool {
        let elem = if let Some(r) = region {
            self.find_element_in_region(elements, keyword, r)
        } else {
            self.find_element(elements, keyword)
        };
        
        if let Some(e) = elem {
            if let Err(err) = self.click_element(operator, &e, after_sleep) {
                error!("点击失败: {:?}", err);
                return false;
            }
            true
        } else {
            warn!("未找到 '{}'", keyword);
            false
        }
    }
}

pub struct StartPage {
    pub elements: Vec<ClickableElement>,
}

impl StartPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let regions = [(0.6f32, 0.8f32, 1.0f32, 1.0f32)];
        let elements = do_ocr(screenshot, operator, &regions, operator.width, operator.height);
        
        for elem in &elements {
            if elem.text.contains("开始") && elem.text.contains("货币战争") {
                info!("识别到开始页面: '{}'", elem.text);
                return Some(Self { elements });
            }
        }
        None
    }

    pub fn click_start(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("开始") && elem.text.contains("货币战争") {
                info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'开始货币战争'按钮");
        false
    }
}

pub struct GameModePage {
    pub elements: Vec<ClickableElement>,
    pub state: i32,
}

impl GameModePage {
    pub const STATE_NEW_GAME: i32 = 1;
    pub const STATE_IN_PROGRESS: i32 = 2;

    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let regions = [(0.6f32, 0.8f32, 1.0f32, 1.0f32)];
        let elements = do_ocr(screenshot, operator, &regions, operator.width, operator.height);
        
        let all_text: String = elements.iter().map(|e| e.text.as_str()).collect();
        let has_enter = all_text.contains("进入标准博弈");
        let has_end = all_text.contains("结束并结算");
        let has_continue = all_text.contains("继续进度");

        if has_enter || (has_end && has_continue) {
            let state = if has_end || has_continue { Self::STATE_IN_PROGRESS } else { Self::STATE_NEW_GAME };
            info!("识别到游戏模式选择页面 - 状态: {}", if state == Self::STATE_IN_PROGRESS { "进行中" } else { "新游戏" });
            return Some(Self { elements, state });
        }
        None
    }

    pub fn click_enter_standard(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("进入标准博弈") {
                info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'进入标准博弈'按钮");
        false
    }

    pub fn click_end_and_settle(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("结束并结算") {
                info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'结束并结算'按钮");
        false
    }
}

pub struct DifficultyPage {
    pub elements: Vec<ClickableElement>,
}

impl DifficultyPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let regions = [(0.6f32, 0.8f32, 1.0f32, 1.0f32)];
        let elements = do_ocr(screenshot, operator, &regions, operator.width, operator.height);
        
        for elem in &elements {
            if elem.text.contains("开始对局") {
                info!("识别到难度选择页面");
                return Some(Self { elements });
            }
        }
        None
    }

    pub fn click_start_battle(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("开始对局") {
                info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'开始对局'按钮");
        false
    }
}

pub struct BossAffixPage {
    pub elements: Vec<ClickableElement>,
    pub affixes: Vec<String>,
    difficulty_ref_y: Option<f32>,
}

impl BossAffixPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let regions = [(0.0f32, 0.65f32, 1.0f32, 0.95f32)];
        let elements = do_ocr(screenshot, operator, &regions, operator.width, operator.height);
        
        let all_text: String = elements.iter().map(|e| e.text.as_str()).collect();
        let has_title = all_text.contains("本场对局首领");
        let has_button = all_text.contains("下一步");
        let has_difficulty = all_text.contains("敌人难度");

        if has_title && has_button && has_difficulty {
            info!("识别到 Boss 词条页面");
            let mut page = Self { elements: elements.clone(), affixes: Vec::new(), difficulty_ref_y: None };
            page.extract_all_boss_affixes(&elements);
            return Some(page);
        }
        None
    }

    fn extract_all_boss_affixes(&mut self, elements: &[ClickableElement]) {
        let ref_y = elements.iter()
            .find(|e| e.text.contains("敌人难度"))
            .map(|e| e.rel_y)
            .unwrap_or(0.7);

        self.affixes.clear();
        for elem in elements {
            if elem.rel_x > 0.66 { continue; }
            if (elem.rel_y - ref_y).abs() > 0.08 { continue; }
            if elem.text.contains("敌人难度") || elem.text.chars().all(|c| c.is_ascii_digit()) { continue; }
            if elem.text.chars().count() < 2 { continue; }
            self.affixes.push(elem.text.clone());
        }

        info!("共提取到 {} 个 Boss 词条: {:?}", self.affixes.len(), self.affixes);
    }

    pub fn click_next_step(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("下一步") {
                info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'下一步'按钮");
        false
    }
}

pub struct PlaneSelectPage {
    pub elements: Vec<ClickableElement>,
}

impl PlaneSelectPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let regions = [(0.4f32, 0.85f32, 0.6f32, 0.95f32)];
        let elements = do_ocr(screenshot, operator, &regions, operator.width, operator.height);
        
        for elem in &elements {
            if elem.text.contains("点击空白处继续") {
                info!("识别到位面选择页面");
                return Some(Self { elements });
            }
        }
        None
    }

    pub fn click_blank_continue(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("点击空白处继续") {
                info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'点击空白处继续'");
        false
    }
}

pub struct InvestEnvironmentPage {
    pub elements: Vec<ClickableElement>,
    pub env_names: Vec<String>,
    pub env_positions: Vec<(f32, f32)>,
}

impl InvestEnvironmentPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let elements = do_ocr(screenshot, operator, &[], operator.width, operator.height);
        
        let all_text: String = elements.iter().map(|e| e.text.as_str()).collect();
        let has_title = all_text.contains("投资环境");
        let has_confirm = all_text.contains("确认");

        if has_title && has_confirm {
            info!("识别到投资环境页面");
            let mut page = Self {
                elements: elements.clone(),
                env_names: Vec::new(),
                env_positions: Vec::new(),
            };
            page.extract_env_names(&elements);
            return Some(page);
        }
        None
    }

    fn extract_env_names(&mut self, elements: &[ClickableElement]) {
        let mut env_elements: Vec<&ClickableElement> = elements.iter()
            .filter(|e| {
                if ["投资环境", "确认", "剩余次数", "攻略", "装备"].contains(&e.text.as_str()) { return false; }
                if e.text.chars().count() > 10 { return false; }
                e.rel_y >= 0.32 && e.rel_y <= 0.38
            })
            .collect();

        env_elements.sort_by(|a, b| a.rel_x.partial_cmp(&b.rel_x).unwrap());

        for elem in env_elements {
            if elem.text.chars().count() >= 2 {
                self.env_names.push(elem.text.clone());
                self.env_positions.push((elem.rel_x, elem.rel_y));
            }
        }

        info!("检测到 {} 个投资环境: {:?}", self.env_names.len(), self.env_names);
    }

    pub fn get_envs(&self) -> Vec<(String, (f32, f32))> {
        self.env_names.clone().into_iter()
            .zip(self.env_positions.clone().into_iter())
            .collect()
    }

    pub fn select_by_index(&self, operator: &AdbOperator, index: usize) -> bool {
        if index < self.env_positions.len() {
            let (x, y) = self.env_positions[index];
            let name = &self.env_names[index];
            info!("选择投资环境 [{}]: '{}'，点击位置: ({:.2}, {:.2})", index, name, x, y);
            operator.click_point(x, y, 1.0).is_ok()
        } else {
            warn!("索引 {} 超出范围，当前有 {} 个环境", index, self.env_positions.len());
            false
        }
    }

    pub fn click_refresh(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("剩余次数") {
                let refresh_x = (elem.rel_x - 0.05).max(0.05);
                let refresh_y = elem.rel_y;
                info!("点击刷新按钮 (剩余次数左边): ({:.2}, {:.2})", refresh_x, refresh_y);
                return operator.click_point(refresh_x, refresh_y, 1.5).is_ok();
            }
        }
        error!("未找到'剩余次数'，无法点击刷新");
        false
    }

    pub fn click_confirm(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("确认") && elem.rel_y >= 0.8 && elem.rel_y <= 1.0 {
                info!("点击 '{}' 位置：({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 1.0).is_ok();
            }
        }
        error!("未找到'确认'按钮");
        false
    }
}

pub struct PreparationPage {
    pub elements: Vec<ClickableElement>,
}

impl PreparationPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let elements = do_ocr(screenshot, operator, &[], operator.width, operator.height);
        
        let has_preparation = elements.iter().any(|e| {
            e.text.contains("备战阶段") && e.rel_x < 0.3 && e.rel_y < 0.2
        });
        let has_shop = elements.iter().any(|e| {
            e.text.contains("商店") && e.rel_x > 0.7 && e.rel_y > 0.6
        });

        if has_preparation && has_shop {
            info!("识别到准备阶段页面");
            return Some(Self { elements });
        }
        None
    }

    pub fn click_battle(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("出战") && elem.rel_x > 0.75 && elem.rel_y > 0.55 && elem.rel_y < 0.8 {
                info!("点击 '{}' 位置：({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'出战'按钮");
        false
    }

    pub fn click_exit(&self, operator: &AdbOperator) -> bool {
        info!("点击返回按钮 (左上角)");
        operator.click_point(0.05, 0.05, 1.0).is_ok()
    }
}

pub struct ShopPage {
    pub elements: Vec<ClickableElement>,
}

impl ShopPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let elements = do_ocr(screenshot, operator, &[], operator.width, operator.height);
        
        let has_preparation = elements.iter().any(|e| e.text.contains("备战阶段"));
        let has_refresh = elements.iter().any(|e| e.text.contains("刷新"));

        if has_preparation && has_refresh {
            info!("识别到商店页面");
            return Some(Self { elements });
        }
        None
    }

    pub fn exit_shop(&self, operator: &AdbOperator) -> bool {
        if let Err(e) = operator.press_key("back") {
            error!("退出商店失败: {:?}", e);
            false
        } else {
            AdbOperator::sleep(1.0);
            true
        }
    }
}

#[derive(Clone)]
pub struct InvestStrategyPage {
    pub elements: Vec<ClickableElement>,
    pub strategy_names: Vec<String>,
    pub strategy_positions: Vec<(f32, f32)>,
}

impl InvestStrategyPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let elements = do_ocr(screenshot, operator, &[], operator.width, operator.height);
        
        let has_title = elements.iter().any(|e| e.text.contains("请选择投资策略"));
        let has_confirm = elements.iter().any(|e| e.text.contains("确认") && e.rel_y > 0.8);

        if has_title && has_confirm {
            info!("识别到投资策略选择页面");
            let mut page = Self {
                elements: elements.clone(),
                strategy_names: Vec::new(),
                strategy_positions: Vec::new(),
            };
            page.extract_strategy_names(&elements);
            return Some(page);
        }
        None
    }

    fn extract_strategy_names(&mut self, elements: &[ClickableElement]) {
        let mut strategy_elements: Vec<&ClickableElement> = elements.iter()
            .filter(|e| {
                if ["请选择投资策略", "确认", "刷新次数", "图例", "攻略", "返回备战界面"].contains(&e.text.as_str()) { return false; }
                if e.text.chars().count() > 10 { return false; }
                e.rel_y >= 0.46 && e.rel_y <= 0.51
            })
            .collect();

        strategy_elements.sort_by(|a, b| a.rel_x.partial_cmp(&b.rel_x).unwrap());

        for elem in strategy_elements {
            if elem.text.chars().count() >= 2 {
                self.strategy_names.push(elem.text.clone());
                self.strategy_positions.push((elem.rel_x, elem.rel_y));
            }
        }

        info!("检测到 {} 个投资策略: {:?}", self.strategy_names.len(), self.strategy_names);
    }

    pub fn get_strategies(&self) -> Vec<(String, (f32, f32))> {
        self.strategy_names.clone().into_iter()
            .zip(self.strategy_positions.clone().into_iter())
            .collect()
    }

    pub fn select_by_index(&self, operator: &AdbOperator, index: usize) -> bool {
        if index < self.strategy_positions.len() {
            let (x, y) = self.strategy_positions[index];
            let name = &self.strategy_names[index];
            info!("选择投资策略 [{}]: '{}'，点击位置: ({:.2}, {:.2})", index, name, x, y);
            operator.click_point(x, y, 1.0).is_ok()
        } else {
            warn!("索引 {} 超出范围，当前有 {} 个策略", index, self.strategy_positions.len());
            false
        }
    }

    pub fn get_refresh_count(&self) -> i32 {
        for elem in &self.elements {
            if elem.text.contains("刷新次数") {
                let digits: String = elem.text.chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(count) = digits.parse::<i32>() {
                    return count;
                }
            }
        }
        0
    }

    pub fn click_refresh(&self, operator: &AdbOperator) -> bool {
        // 收集所有包含"刷新次数"的元素
        let mut refresh_elements: Vec<&ClickableElement> = self.elements
            .iter()
            .filter(|e| e.text.contains("刷新次数"))
            .collect();

        if refresh_elements.is_empty() {
            error!("未找到'刷新次数'，无法点击刷新");
            return false;
        }

        // 按 x 坐标排序（从左到右）
        refresh_elements.sort_by(|a, b| a.rel_x.partial_cmp(&b.rel_x).unwrap());

        info!("找到 {} 个刷新按钮", refresh_elements.len());

        // 依次点击每个刷新按钮
        for (i, elem) in refresh_elements.iter().enumerate() {
            let refresh_x = (elem.rel_x - 0.05).max(0.05);
            let refresh_y = elem.rel_y;
            info!("点击第 {} 个刷新按钮: ({:.2}, {:.2})", i + 1, refresh_x, refresh_y);
            if let Err(e) = operator.click_point(refresh_x, refresh_y, 1.0) {
                error!("点击刷新按钮失败: {:?}", e);
                return false;
            }
        }

        true
    }

    pub fn click_confirm(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("确认") && elem.rel_y >= 0.8 && elem.rel_y <= 1.0 {
                info!("点击 '{}' 位置：({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'确认'按钮");
        false
    }
}

pub struct ExitConfirmDialog {
    pub elements: Vec<ClickableElement>,
}

impl ExitConfirmDialog {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let regions = [(0.2f32, 0.2f32, 0.8f32, 0.8f32)];
        let elements = do_ocr(screenshot, operator, &regions, operator.width, operator.height);
        
        let all_text: String = elements.iter().map(|e| e.text.as_str()).collect();
        let has_tip = all_text.contains("提示");
        let has_give_up = all_text.contains("放弃并结算");

        if has_tip && has_give_up {
            info!("识别到退出确认对话框");
            return Some(Self { elements });
        }
        None
    }

    pub fn click_give_up_and_settle(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("放弃并结算") {
                if elem.rel_x >= 0.2 && elem.rel_x <= 0.8 && elem.rel_y >= 0.6 && elem.rel_y <= 1.0 {
                    info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                    return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
                }
            }
        }
        error!("未找到'放弃并结算'按钮");
        false
    }
}

pub struct ExitChallengeFailPage {
    pub elements: Vec<ClickableElement>,
}

impl ExitChallengeFailPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let regions = [(0.35f32, 0.05f32, 0.65f32, 0.95f32)];
        let elements = do_ocr(screenshot, operator, &regions, operator.width, operator.height);
        
        let all_text: String = elements.iter().map(|e| e.text.as_str()).collect();
        let has_fail = all_text.contains("挑战失败");
        let has_next = all_text.contains("下一步");

        if has_fail && has_next {
            info!("识别到挑战失败页面");
            return Some(Self { elements });
        }
        None
    }

    pub fn click_next_step(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("下一步") {
                info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'下一步'按钮");
        false
    }
}

pub struct ExitStatsPage {
    pub elements: Vec<ClickableElement>,
}

impl ExitStatsPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let regions = [(0.15f32, 0.1f32, 0.6f32, 1.0f32)];
        let elements = do_ocr(screenshot, operator, &regions, operator.width, operator.height);
        
        let all_text: String = elements.iter().map(|e| e.text.as_str()).collect();
        let has_unfinished = all_text.contains("对局未完成");
        let has_next_page = all_text.contains("下一页");

        if has_unfinished && has_next_page {
            info!("识别到统计页面");
            return Some(Self { elements });
        }
        None
    }

    pub fn click_next_page(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("下一页") {
                info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'下一页'按钮");
        false
    }
}

pub struct ExitReturnPage {
    pub elements: Vec<ClickableElement>,
}

impl ExitReturnPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let regions = [(0.3f32, 0.8f32, 0.7f32, 1.0f32)];
        let elements = do_ocr(screenshot, operator, &regions, operator.width, operator.height);
        
        for elem in &elements {
            if elem.text.contains("返回货币战争") {
                info!("识别到返回页面");
                return Some(Self { elements });
            }
        }
        None
    }

    pub fn click_return(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("返回货币战争") {
                info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'返回货币战争'按钮");
        false
    }
}

pub struct BattleSettlementPage {
    pub elements: Vec<ClickableElement>,
}

impl BattleSettlementPage {
    pub fn detect(screenshot: &DynamicImage, operator: &AdbOperator) -> Option<Self> {
        let regions = [(0.2f32, 0.05f32, 0.8f32, 0.25f32), (0.3f32, 0.8f32, 0.7f32, 0.95f32)];
        let elements = do_ocr(screenshot, operator, &regions, operator.width, operator.height);
        
        let has_success = elements.iter().any(|e| e.text.contains("挑战成功"));
        let has_continue = elements.iter().any(|e| e.text.contains("继续挑战"));

        if has_success && has_continue {
            info!("识别到战斗结算页面");
            return Some(Self { elements });
        }
        None
    }

    pub fn click_continue(&self, operator: &AdbOperator) -> bool {
        for elem in &self.elements {
            if elem.text.contains("继续挑战") {
                info!("点击 '{}' 位置: ({:.2}, {:.2})", elem.text, elem.rel_x, elem.rel_y);
                return operator.click_point(elem.rel_x, elem.rel_y, 2.0).is_ok();
            }
        }
        error!("未找到'继续挑战'按钮");
        false
    }
}

pub struct PageDetector<'a> {
    operator: &'a AdbOperator,
    screenshot: Option<DynamicImage>,
}

impl<'a> PageDetector<'a> {
    pub fn new(operator: &'a AdbOperator) -> Self {
        Self { operator, screenshot: None }
    }

    pub fn refresh(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.screenshot = Some(self.operator.screenshot()?);
        Ok(())
    }

    pub fn detect_start_page(&self) -> Option<StartPage> {
        if let Some(ref screenshot) = self.screenshot {
            StartPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_game_mode_page(&self) -> Option<GameModePage> {
        if let Some(ref screenshot) = self.screenshot {
            GameModePage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_difficulty_page(&self) -> Option<DifficultyPage> {
        if let Some(ref screenshot) = self.screenshot {
            DifficultyPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_boss_affix_page(&self) -> Option<BossAffixPage> {
        if let Some(ref screenshot) = self.screenshot {
            BossAffixPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_plane_select_page(&self) -> Option<PlaneSelectPage> {
        if let Some(ref screenshot) = self.screenshot {
            PlaneSelectPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_invest_environment_page(&self) -> Option<InvestEnvironmentPage> {
        if let Some(ref screenshot) = self.screenshot {
            InvestEnvironmentPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_preparation_page(&self) -> Option<PreparationPage> {
        if let Some(ref screenshot) = self.screenshot {
            PreparationPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_shop_page(&self) -> Option<ShopPage> {
        if let Some(ref screenshot) = self.screenshot {
            ShopPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_invest_strategy_page(&self) -> Option<InvestStrategyPage> {
        if let Some(ref screenshot) = self.screenshot {
            InvestStrategyPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_exit_confirm_dialog(&self) -> Option<ExitConfirmDialog> {
        if let Some(ref screenshot) = self.screenshot {
            ExitConfirmDialog::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_exit_challenge_fail_page(&self) -> Option<ExitChallengeFailPage> {
        if let Some(ref screenshot) = self.screenshot {
            ExitChallengeFailPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_exit_stats_page(&self) -> Option<ExitStatsPage> {
        if let Some(ref screenshot) = self.screenshot {
            ExitStatsPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_exit_return_page(&self) -> Option<ExitReturnPage> {
        if let Some(ref screenshot) = self.screenshot {
            ExitReturnPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }

    pub fn detect_battle_settlement_page(&self) -> Option<BattleSettlementPage> {
        if let Some(ref screenshot) = self.screenshot {
            BattleSettlementPage::detect(screenshot, self.operator)
        } else {
            None
        }
    }
}

// 投资环境页面命令处理函数
mod env_commands {
    use super::*;
    use crate::test_page::ShellCommandResult;
    
    pub fn select(page: &mut InvestEnvironmentPage, args: &[&str], operator: &mut AdbOperator) -> ShellCommandResult {
        if args.is_empty() {
            return ShellCommandResult::Error("需要指定索引".to_string());
        }
        match args[0].parse::<usize>() {
            Ok(index) => {
                if page.select_by_index(operator, index) {
                    ShellCommandResult::Success(Some(format!("已选择 [{}]", index)))
                } else {
                    ShellCommandResult::Error(format!("选择失败，索引 {} 超出范围", index))
                }
            }
            Err(_) => ShellCommandResult::Error("索引必须是数字".to_string())
        }
    }
    
    pub fn refresh(page: &mut InvestEnvironmentPage, _args: &[&str], operator: &mut AdbOperator) -> ShellCommandResult {
        if page.click_refresh(operator) {
            ShellCommandResult::Success(Some("已点击刷新".to_string()))
        } else {
            ShellCommandResult::Error("未找到刷新按钮".to_string())
        }
    }
    
    pub fn confirm(page: &mut InvestEnvironmentPage, _args: &[&str], operator: &mut AdbOperator) -> ShellCommandResult {
        if page.click_confirm(operator) {
            ShellCommandResult::Success(Some("已点击确认".to_string()))
        } else {
            ShellCommandResult::Error("未找到确认按钮".to_string())
        }
    }
    
    pub fn envs(page: &mut InvestEnvironmentPage, _args: &[&str], _operator: &mut AdbOperator) -> ShellCommandResult {
        let items = page.get_envs();
        let mut result = String::new();
        for (i, (name, (x, y))) in items.iter().enumerate() {
            result.push_str(&format!("[{}] '{}' @ ({:.3}, {:.3})\n", i, name, x, y));
        }
        ShellCommandResult::Success(Some(result))
    }
}

// 投资策略页面命令处理函数
mod strategy_commands {
    use super::*;
    use crate::test_page::ShellCommandResult;
    
    pub fn select(page: &mut InvestStrategyPage, args: &[&str], operator: &mut AdbOperator) -> ShellCommandResult {
        if args.is_empty() {
            return ShellCommandResult::Error("需要指定索引".to_string());
        }
        match args[0].parse::<usize>() {
            Ok(index) => {
                if page.select_by_index(operator, index) {
                    ShellCommandResult::Success(Some(format!("已选择 [{}]", index)))
                } else {
                    ShellCommandResult::Error(format!("选择失败，索引 {} 超出范围", index))
                }
            }
            Err(_) => ShellCommandResult::Error("索引必须是数字".to_string())
        }
    }
    
    pub fn refresh(page: &mut InvestStrategyPage, _args: &[&str], operator: &mut AdbOperator) -> ShellCommandResult {
        if page.click_refresh(operator) {
            ShellCommandResult::Success(Some("已点击刷新".to_string()))
        } else {
            ShellCommandResult::Error("未找到刷新按钮".to_string())
        }
    }
    
    pub fn confirm(page: &mut InvestStrategyPage, _args: &[&str], operator: &mut AdbOperator) -> ShellCommandResult {
        if page.click_confirm(operator) {
            ShellCommandResult::Success(Some("已点击确认".to_string()))
        } else {
            ShellCommandResult::Error("未找到确认按钮".to_string())
        }
    }
    
    pub fn strategies(page: &mut InvestStrategyPage, _args: &[&str], _operator: &mut AdbOperator) -> ShellCommandResult {
        let items = page.get_strategies();
        let mut result = String::new();
        for (i, (name, (x, y))) in items.iter().enumerate() {
            result.push_str(&format!("[{}] '{}' @ ({:.3}, {:.3})\n", i, name, x, y));
        }
        result.push_str(&format!("刷新次数: {}\n", page.get_refresh_count()));
        ShellCommandResult::Success(Some(result))
    }
    
    pub fn refresh_count(page: &mut InvestStrategyPage, _args: &[&str], _operator: &mut AdbOperator) -> ShellCommandResult {
        let count = page.get_refresh_count();
        ShellCommandResult::Success(Some(format!("刷新次数: {}", count)))
    }
}

// 使用宏为页面生成 shell 命令注册
crate::define_page_commands!(InvestEnvironmentPage {
    select : "select <索引> - 选择指定索引的投资环境" => env_commands::select,
    refresh : "refresh - 点击刷新按钮" => env_commands::refresh,
    confirm : "confirm - 点击确认按钮" => env_commands::confirm,
    envs : "envs - 显示所有投资环境" => env_commands::envs
});

crate::define_page_commands!(InvestStrategyPage {
    select : "select <索引> - 选择指定索引的投资策略" => strategy_commands::select,
    refresh : "refresh - 点击刷新按钮" => strategy_commands::refresh,
    confirm : "confirm - 点击确认按钮" => strategy_commands::confirm,
    strategies : "strategies - 显示所有投资策略" => strategy_commands::strategies,
    refresh_count : "refresh_count - 显示刷新次数" => strategy_commands::refresh_count
});
