use crate::config::Config;
use crate::consts::*;
use crate::trae::{NewTraeTask, TraeTask, TraeTaskStatus, types::*};
use crate::utils::{normalize_executable_path_for_cdp, wait_for_selector};
use anyhow::{Error, Result};
use chromiumoxide::{Browser, Page, cdp::browser_protocol::target::TargetInfo};
use tokio::sync::RwLock;
use tokio::sync::watch::Receiver;
use tokio::time::{self, Duration, sleep};

#[derive(Debug)]
pub struct TraeEditor {
    pub(crate) main_page: Page,
    pub(crate) target: TargetInfo,
    pub(crate) prebuilt_agent: TraeEditorPrebuiltSoloAgent,
    pub(crate) mode: TraeEditorMode,
    pub(crate) tasks: RwLock<Vec<TraeTask>>,
}

pub async fn get_current_editor_mode(page: &Page) -> Result<TraeEditorMode, Error> {
    let trae_mode_badge_element = wait_for_selector(page, "div.fixed-titlebar-container div.icube-mode-tab > div.icube-tooltip-container > div.icube-tooltip-text.icube-simple-style", Duration::from_millis(DEFAULT_SELECTOR_TIMEOUT)).await.expect("Cannot locate Trae editor mode badge.");

    let mode_description = trae_mode_badge_element
        .inner_html()
        .await
        .expect("Cannot get the Trae mode badge text node")
        .expect("Cannot get Trae mode text description.");

    if mode_description.eq(TRAE_SOLO_MODE_TEXT_LABEL) {
        Ok(TraeEditorMode::IDE)
    } else if mode_description.eq(TRAE_IDE_MODE_TEXT_LABEL) {
        Ok(TraeEditorMode::SOLO)
    } else {
        Err(Error::msg(format!(
            "Cannot get the current editor mode, description: {}",
            mode_description
        )))
    }
}

pub struct TraeEditorBuilder;

impl TraeEditorBuilder {
    pub async fn build(&self, browser: &mut Browser) -> TraeEditor {
        let targets = browser.fetch_targets().await.expect("Fetch targets error.");

        sleep(Duration::from_millis(2000)).await;

        let config = Config::load().expect("Cannot load config from TraeEditorBuilder::build, make sure you write config.jsonc properly.");

        let normalized_path =
            normalize_executable_path_for_cdp(&config.trae_executable_path).unwrap();

        let mut filtered_target: Vec<TargetInfo> = targets
            .into_iter()
            .filter(|t| t.url.contains(&format!("vscode-file://vscode-app/{}/resources/app/out/vs/code/electron-browser/workbench/workbench.html",normalized_path)))
            .collect();

        let main_target = filtered_target
            .pop()
            .expect("Cannot get the main target of Trae.");

        let pages = browser
            .pages()
            .await
            .expect("Cannot get pages from browser instance.");

        let main_page = browser
            .get_page(main_target.target_id.clone())
            .await
            .expect(&format!(
                "Cannot get the main page of Trae. filtered targets: {:#?}, main_target: {:#?}, pages: {:#?}",
                filtered_target, main_target, pages
            ));

        // get the current mode
        let current_mode = get_current_editor_mode(&main_page)
            .await
            .expect("Cannot get current mode when initializing.");

        return TraeEditor {
            target: main_target,
            main_page: main_page,
            prebuilt_agent: TraeEditorPrebuiltSoloAgent::Coder,
            mode: current_mode,
            tasks: RwLock::new(Vec::new()),
        };
    }
}

impl TraeEditor {
    pub fn new() -> TraeEditorBuilder {
        TraeEditorBuilder {}
    }

    pub fn get_current_mode(&self) -> &TraeEditorMode {
        &self.mode
    }

    pub async fn get_main_page(&self) -> &Page {
        return &self.main_page;
    }

    pub async fn switch_editor_mode(&mut self, mode: TraeEditorMode) -> Result<(), Error> {
        if self.mode == mode {
            return Ok(());
        }

        let trae_mode_tab_switch = self.main_page.find_element("div.fixed-titlebar-container div.icube-mode-tab > div.icube-mode-tab-container > div.icube-mode-tab-switch").await.expect("Cannot locate Trae editor mode switch tab.");
        trae_mode_tab_switch.click().await?;

        // update current mode

        match self.mode {
            TraeEditorMode::IDE => {
                self.mode = TraeEditorMode::SOLO;
            }
            TraeEditorMode::SOLO => {
                self.mode = TraeEditorMode::IDE;
            }
        }

        Ok(())
    }

    pub async fn create_new_task(&self, prompt: impl Into<String>) -> NewTraeTask<'_> {
        NewTraeTask::new(self, prompt.into())
    }

    pub fn set_default_prebuilt_solo_agent(&mut self, agent: TraeEditorPrebuiltSoloAgent) {
        self.prebuilt_agent = agent;
    }

    pub fn get_default_prebuilt_solo_agent(&self) -> TraeEditorPrebuiltSoloAgent {
        self.prebuilt_agent
    }

    // private methods
    // get tasks from sidebar
    pub async fn fetch_tasks_from_ui(&self) -> Result<Vec<TraeTask>, Error> {
        if self.mode != TraeEditorMode::SOLO {
            return Err(Error::msg(
                "Cannot get tasks under IDE mode, please switch to SOLO mode.",
            ));
        }

        let task_container = self
            .main_page
            .find_element("#solo-ai-sidebar-content div[class*=task-items-list]")
            .await
            .expect("Cannot get task container.");

        let task_items = task_container
            .find_elements(r#"div[class*="index-module__task-item___"#)
            .await
            .expect("Cannot get task items from container.");

        let mut tasks: Vec<TraeTask> = Vec::with_capacity(task_items.len());

        for (index, item) in task_items.iter().enumerate() {
            let class_name = item.attribute("class").await?.unwrap_or_default();

            let selected = class_name.contains("selected");

            let raw_task_state = item
                .find_element("div[class*=task-type-wrap__status]")
                .await
                .expect(&format!("Cannot get task type: {:#?}", item))
                .inner_text()
                .await
                .unwrap_or_default()
                .unwrap_or_else(|| {
                    println!("Trying to get task type label failed, the value is None");
                    return "".to_string();
                });

            let title = item
                .find_element("span[class*=task-title]")
                .await
                .expect(&format!("Cannot get task title: {:#?}", item))
                .inner_html()
                .await
                .unwrap_or_default()
                .unwrap_or_else(|| {
                    println!("Trying to get task title label failed, the value is None");
                    return "".to_string();
                });

            let status = match raw_task_state.as_str() {
                TRAE_SOLO_TASK_RUNNING_LABEL => TraeTaskStatus::Running,
                TRAE_SOLO_TASK_INTERRUPTED_LABEL => TraeTaskStatus::Interrupted,
                TRAE_SOLO_TASK_FINISHED_LABEL => TraeTaskStatus::Finished,
                TRAE_SOLO_TASK_WAITING_FOR_HITL_LABEL => TraeTaskStatus::WaitingForHITL,
                _ => TraeTaskStatus::Idle,
            };

            tasks.push(TraeTask {
                title,
                status,
                selected,
                index,
            });
        }

        Ok(tasks)
    }

    pub async fn refresh_tasks(&self) -> Result<Vec<TraeTask>, Error> {
        let latest = self.fetch_tasks_from_ui().await?;

        let mut guard = self.tasks.write().await;
        *guard = latest.clone();

        Ok(latest)
    }

    /// Get latest Trae tasks from UI.
    pub async fn get_tasks(&self) -> Result<Vec<TraeTask>, Error> {
        let latest = self.fetch_tasks_from_ui().await?;
        let mut guard = self.tasks.write().await;
        *guard = latest.clone();

        Ok(latest)
    }

    pub async fn click_task_item_by_index(&self, index: usize) -> Result<(), Error> {
        let task_container = self
            .main_page
            .find_element("#solo-ai-sidebar-content div[class*=task-items-list]")
            .await
            .expect("Cannot get task container.");

        let task_items = task_container
            .find_elements(r#"div[class*="index-module__task-item___"#)
            .await
            .expect("Cannot get task items from container.");

        let target_task_item = task_items.get(index as usize);

        match target_task_item {
            Some(element) => {
                element.click().await?;
            }
            None => {
                println!("Cannot find element by index: {}", index);
            }
        }

        Ok(())
    }

    // pub async fn find_task_by(&self, title: &str, status: Option<TraeTaskStatus>) -> Option<TraeTask> {
    //     let guard = self.tasks.read().await;
    //     guard.iter().find(|t| {
    //         match status {

    //         }
    //     }).cloned()
    // }

    pub async fn cached_tasks(&self) -> Vec<TraeTask> {
        self.tasks.read().await.clone()
    }

    pub async fn run_task_sync_loop(&self, interval: Duration, mut shutdown_rx: Receiver<bool>) {
        let _ = self.refresh_tasks().await;

        let mut ticker = time::interval(interval);
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        ticker.tick().await;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(err) = self.refresh_tasks().await {
                        eprintln!("refresh_tasks failed: {err:?}");
                    }
                }
                changed = shutdown_rx.changed() => {
                    match changed {
                        Ok(_) => {
                            if *shutdown_rx.borrow() {
                                break;
                            }
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }
            }
        }
    }
}
