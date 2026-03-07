use crate::logger::{info, debug};

#[derive(Debug, Clone)]
pub struct OpeningConfig {
    pub env: Vec<String>,
    pub strategy: Vec<String>,
    pub affix: Vec<String>,
}

impl OpeningConfig {
    pub fn new(env: Vec<String>, strategy: Vec<String>, affix: Vec<String>) -> Self {
        Self { env, strategy, affix }
    }

    pub fn has_env_requirement(&self) -> bool { !self.env.is_empty() }
    pub fn has_affix_requirement(&self) -> bool { !self.affix.is_empty() }
    pub fn has_strategy_requirement(&self) -> bool { !self.strategy.is_empty() }

    pub fn check_env(&self, current_env: &str) -> bool {
        if !self.has_env_requirement() { return true; }
        self.env.iter().any(|e| current_env.contains(e))
    }

    pub fn check_affix(&self, current_affixes: &[String]) -> bool {
        if !self.has_affix_requirement() { return true; }
        self.affix.iter().any(|a| {
            current_affixes.iter().any(|affix| affix.contains(a))
        })
    }

    pub fn check_strategy(&self, current_strategy: &str) -> bool {
        if !self.has_strategy_requirement() { return true; }
        self.strategy.iter().any(|s| current_strategy.contains(s))
    }
}

pub struct SelectionManager {
    openings: Vec<OpeningConfig>,
    pub prefer_env: Vec<String>,
    pub active_openings: Vec<OpeningConfig>,
    pub current_env: String,
    pub current_strategy: String,
    pub current_affixes: Vec<String>,
}

impl SelectionManager {
    pub fn new(openings: Vec<OpeningConfig>, prefer_env: Vec<String>) -> Self {
        let active_openings = openings.clone();
        Self {
            openings,
            prefer_env,
            active_openings,
            current_env: String::new(),
            current_strategy: String::new(),
            current_affixes: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.active_openings = self.openings.clone();
        self.current_env.clear();
        self.current_strategy.clear();
        self.current_affixes.clear();
    }

    pub fn get_all_wanted_envs(&self) -> Vec<String> {
        let mut envs: Vec<String> = self.active_openings.iter()
            .flat_map(|o| o.env.clone())
            .collect();
        envs.sort();
        envs.dedup();
        envs
    }

    pub fn get_all_wanted_strategies(&self) -> Vec<String> {
        let mut strategies: Vec<String> = self.active_openings.iter()
            .flat_map(|o| o.strategy.clone())
            .collect();
        strategies.sort();
        strategies.dedup();
        strategies
    }

    pub fn filter_by_env(&mut self, selected_env: &str) -> Vec<OpeningConfig> {
        self.current_env = selected_env.to_string();
        self.active_openings = self.active_openings.iter()
            .filter(|o| o.check_env(selected_env))
            .cloned()
            .collect();
        debug!("环境检测后活跃开局配置: {:?}", self.active_openings.len());
        self.active_openings.clone()
    }

    pub fn filter_by_affix(&mut self, current_affixes: &[String]) -> Vec<OpeningConfig> {
        self.current_affixes = current_affixes.to_vec();
        self.active_openings = self.active_openings.iter()
            .filter(|o| o.check_affix(current_affixes))
            .cloned()
            .collect();
        debug!("词条检测后活跃开局配置: {:?}", self.active_openings.len());
        self.active_openings.clone()
    }

    pub fn filter_by_strategy(&mut self, selected_strategy: &str) -> Vec<OpeningConfig> {
        self.current_strategy = selected_strategy.to_string();
        self.active_openings = self.active_openings.iter()
            .filter(|o| o.check_strategy(selected_strategy))
            .cloned()
            .collect();
        debug!("策略检测后活跃开局配置: {:?}", self.active_openings.len());
        self.active_openings.clone()
    }

    pub fn has_active_openings(&self) -> bool { !self.active_openings.is_empty() }
    pub fn get_active_count(&self) -> usize { self.active_openings.len() }
    pub fn has_wanted_envs(&self) -> bool { self.openings.iter().any(|o| o.has_env_requirement()) }
    pub fn has_wanted_strategies(&self) -> bool { self.openings.iter().any(|o| o.has_strategy_requirement()) }

    pub fn select_env(&mut self, env_names: &[String], use_prefer: bool, update_state: bool) -> (i32, String, String) {
        let wanted_envs = self.get_all_wanted_envs();
        if !wanted_envs.is_empty() {
            for (i, name) in env_names.iter().enumerate() {
                for wanted_env in &wanted_envs {
                    if name.contains(wanted_env) {
                        info!("[选择环境] 匹配目标环境: {} -> {} (位置 {})", wanted_env, name, i);
                        self.current_env = name.clone();
                        if update_state { self.filter_by_env(name); }
                        return (i as i32, name.clone(), "target".to_string());
                    }
                }
            }
            debug!("[选择环境] 未匹配到目标环境");
        }

        if use_prefer && !self.prefer_env.is_empty() {
            for (i, name) in env_names.iter().enumerate() {
                for prefer in &self.prefer_env {
                    if name.contains(prefer) {
                        info!("[选择环境] 匹配偏好环境: {} -> {} (位置 {})", prefer, name, i);
                        self.current_env = name.clone();
                        if update_state { self.filter_by_env(name); }
                        return (i as i32, name.clone(), "prefer".to_string());
                    }
                }
            }
            debug!("[选择环境] 未匹配到偏好环境");
        }

        if !env_names.is_empty() {
            use rand::Rng;
            let idx = rand::thread_rng().gen_range(0..env_names.len());
            let name = env_names[idx].clone();
            info!("[选择环境] 随机选择: {} (位置 {})", name, idx);
            self.current_env = name.clone();
            if update_state { self.filter_by_env(&name); }
            return (idx as i32, name, "random".to_string());
        }

        debug!("[选择环境] 无环境可选择");
        (-1, String::new(), String::new())
    }

    pub fn select_strategy(&mut self, strategy_names: &[String], update_state: bool) -> (i32, String, String) {
        let wanted_strategies = self.get_all_wanted_strategies();
        if !wanted_strategies.is_empty() {
            for (i, name) in strategy_names.iter().enumerate() {
                for wanted_strategy in &wanted_strategies {
                    if name.contains(wanted_strategy) {
                        info!("[选择策略] 匹配目标策略: {} -> {} (位置 {})", wanted_strategy, name, i);
                        self.current_strategy = name.clone();
                        if update_state { self.filter_by_strategy(name); }
                        return (i as i32, name.clone(), "target".to_string());
                    }
                }
            }
            debug!("[选择策略] 未匹配到目标策略");
        }

        if !strategy_names.is_empty() {
            use rand::Rng;
            let idx = rand::thread_rng().gen_range(0..strategy_names.len());
            let name = strategy_names[idx].clone();
            info!("[选择策略] 随机选择: {} (位置 {})", name, idx);
            self.current_strategy = name.clone();
            if update_state { self.filter_by_strategy(&name); }
            return (idx as i32, name, "random".to_string());
        }

        debug!("[选择策略] 无策略可选择");
        (-1, String::new(), String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opening_config() {
        let config1 = OpeningConfig::new(
            vec!["专家研讨会".to_string(), "特邀专家".to_string()],
            vec!["快请专家".to_string()],
            vec!["变宝为废".to_string()],
        );
        
        let config2 = OpeningConfig::new(
            vec![],
            vec!["轮回不止".to_string()],
            vec![],
        );
        
        let config3 = OpeningConfig::new(
            vec![],
            vec![],
            vec![],
        );
        
        assert!(config1.has_env_requirement());
        assert!(config1.has_strategy_requirement());
        assert!(config1.has_affix_requirement());
        
        assert!(!config2.has_env_requirement());
        assert!(config2.has_strategy_requirement());
        assert!(!config2.has_affix_requirement());
        
        assert!(!config3.has_env_requirement());
        assert!(!config3.has_strategy_requirement());
        assert!(!config3.has_affix_requirement());
        
        assert!(config1.check_env("专家研讨会"));
        assert!(config1.check_env("特邀专家"));
        assert!(!config1.check_env("彩虹时代"));
        
        assert!(config2.check_env("任意环境"));
        assert!(config3.check_env("任意环境"));
        
        assert!(config1.check_affix(&["变宝为废".to_string(), "其他词条".to_string()]));
        assert!(!config1.check_affix(&["其他词条".to_string()]));
        assert!(config2.check_affix(&["任意词条".to_string()]));
        
        assert!(config1.check_strategy("快请专家"));
        assert!(!config1.check_strategy("轮回不止"));
        assert!(config2.check_strategy("轮回不止"));
        assert!(config3.check_strategy("任意策略"));
    }

    #[test]
    fn test_selection_manager_basic() {
        let openings = vec![
            OpeningConfig::new(
                vec!["专家研讨会".to_string()],
                vec!["快请专家".to_string()],
                vec![],
            ),
            OpeningConfig::new(
                vec![],
                vec!["轮回不止".to_string()],
                vec!["变宝为废".to_string()],
            ),
        ];
        
        let prefer_env = vec!["彩虹时代".to_string(), "头彩".to_string()];
        
        let manager = SelectionManager::new(openings, prefer_env);
        
        assert_eq!(manager.get_active_count(), 2);
        assert!(manager.has_active_openings());
        assert!(manager.has_wanted_envs());
        assert!(manager.has_wanted_strategies());
        
        let wanted_envs = manager.get_all_wanted_envs();
        assert!(wanted_envs.contains(&"专家研讨会".to_string()));
        
        let wanted_strategies = manager.get_all_wanted_strategies();
        assert!(wanted_strategies.contains(&"快请专家".to_string()));
        assert!(wanted_strategies.contains(&"轮回不止".to_string()));
    }

    #[test]
    fn test_selection_manager_filter() {
        let openings = vec![
            OpeningConfig::new(
                vec!["专家研讨会".to_string()],
                vec!["快请专家".to_string()],
                vec!["变宝为废".to_string()],
            ),
            OpeningConfig::new(
                vec!["彩虹时代".to_string()],
                vec!["轮回不止".to_string()],
                vec!["变宝为废".to_string()],
            ),
            OpeningConfig::new(
                vec![],
                vec!["轮回不止".to_string()],
                vec![],
            ),
        ];
        
        let mut manager = SelectionManager::new(openings, vec![]);
        
        assert_eq!(manager.get_active_count(), 3);
        
        manager.filter_by_env("专家研讨会");
        assert_eq!(manager.get_active_count(), 2);
        
        manager.reset();
        assert_eq!(manager.get_active_count(), 3);
        
        manager.filter_by_affix(&["变宝为废".to_string()]);
        assert_eq!(manager.get_active_count(), 3);
        
        let openings_with_affix = vec![
            OpeningConfig::new(
                vec![],
                vec!["快请专家".to_string()],
                vec!["变宝为废".to_string()],
            ),
            OpeningConfig::new(
                vec![],
                vec!["轮回不止".to_string()],
                vec!["其他词条".to_string()],
            ),
            OpeningConfig::new(
                vec![],
                vec!["其他策略".to_string()],
                vec![],
            ),
        ];
        
        let mut manager2 = SelectionManager::new(openings_with_affix, vec![]);
        
        manager2.filter_by_affix(&["变宝为废".to_string()]);
        assert_eq!(manager2.get_active_count(), 2);
        
        manager2.reset();
        manager2.filter_by_strategy("轮回不止");
        assert_eq!(manager2.get_active_count(), 1);
    }

    #[test]
    fn test_selection_manager_select_env() {
        let openings = vec![
            OpeningConfig::new(
                vec!["专家研讨会".to_string()],
                vec!["快请专家".to_string()],
                vec![],
            ),
            OpeningConfig::new(
                vec![],
                vec!["轮回不止".to_string()],
                vec![],
            ),
        ];
        
        let prefer_env = vec!["彩虹时代".to_string(), "头彩".to_string()];
        let mut manager = SelectionManager::new(openings, prefer_env);
        
        let env_names = vec![
            "专家研讨会".to_string(),
            "彩虹时代".to_string(),
            "其他环境".to_string(),
        ];
        
        let (_, name, reason) = manager.select_env(&env_names, false, false);
        assert_eq!(reason, "target");
        assert_eq!(name, "专家研讨会");
        
        manager.reset();
        let (_, _, reason) = manager.select_env(&env_names, true, false);
        assert_eq!(reason, "target");
        
        let env_names_no_match = vec![
            "未知环境1".to_string(),
            "未知环境2".to_string(),
        ];
        
        manager.reset();
        let (_, _, reason) = manager.select_env(&env_names_no_match, true, false);
        assert_eq!(reason, "random");
        
        let empty: Vec<String> = vec![];
        manager.reset();
        let (idx, _, _) = manager.select_env(&empty, false, false);
        assert_eq!(idx, -1);
    }

    #[test]
    fn test_selection_manager_select_strategy() {
        let openings = vec![
            OpeningConfig::new(
                vec!["专家研讨会".to_string()],
                vec!["快请专家".to_string()],
                vec![],
            ),
            OpeningConfig::new(
                vec![],
                vec!["轮回不止".to_string()],
                vec![],
            ),
        ];
        
        let mut manager = SelectionManager::new(openings, vec![]);
        
        let strategy_names = vec![
            "快请专家".to_string(),
            "轮回不止".to_string(),
            "其他策略".to_string(),
        ];
        
        manager.reset();
        let (_, _, reason) = manager.select_strategy(&strategy_names, false);
        assert_eq!(reason, "target");
        
        let strategy_names_no_match = vec![
            "未知策略1".to_string(),
            "未知策略2".to_string(),
        ];
        
        manager.reset();
        let (_, _, reason) = manager.select_strategy(&strategy_names_no_match, false);
        assert_eq!(reason, "random");
    }
}
