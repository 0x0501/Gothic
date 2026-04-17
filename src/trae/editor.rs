use crate::config::Config;
use crate::consts::*;
use crate::trae::{NewTraeTask, TraeTask, TraeTaskStatus, types::*};
use crate::utils::{normalize_executable_path_for_cdp, wait_for_selector};
use anyhow::{Error, Result};
use chromiumoxide::cdp::browser_protocol::input::InsertTextParams;
use chromiumoxide::{Browser, Page, cdp::browser_protocol::target::TargetInfo};
use serde::Deserialize;
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

#[derive(Debug, Deserialize)]
struct TaskSnapshotFromUi {
    title: String,
    raw_status: String,
    selected: bool,
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

        // let task_container = wait_for_selector(
        //     &self.main_page,
        //     "#solo-ai-sidebar-content div[class*=task-items-list]",
        //     Duration::from_millis(DEFAULT_SELECTOR_TIMEOUT),
        // )
        // .await
        // .expect("Cannot get task container.");

        let _ = wait_for_selector(
            &self.main_page,
            r#"div[class*=index-module__task-item___]"#,
            Duration::from_millis(DEFAULT_SELECTOR_TIMEOUT),
        )
        .await
        .expect("Cannot get task items from page.");

        // Read the list in a single browser-side snapshot to avoid stale
        // Element handles when the sidebar re-renders between awaits.
        let task_snapshots: Vec<TaskSnapshotFromUi> = self
            .main_page
            .evaluate(
                r#"
                Array.from(document.querySelectorAll('div[class*="index-module__task-item___"]'))
                  .map((item) => {
                    const titleElement = item.querySelector('span[class*="task-title"]');
                    const statusElement = item.querySelector('div[class*="task-type-wrap__status"]');

                    return {
                      title: (titleElement?.textContent ?? '').trim(),
                      raw_status: (statusElement?.innerText ?? '').trim(),
                      selected: item.className.includes('selected'),
                    };
                  })
                "#,
            )
            .await?
            .into_value()?;

        let mut tasks: Vec<TraeTask> = Vec::with_capacity(task_snapshots.len());

        for (index, snapshot) in task_snapshots.into_iter().enumerate() {
            let status = match snapshot.raw_status.as_str() {
                TRAE_SOLO_TASK_RUNNING_LABEL => TraeTaskStatus::Running,
                TRAE_SOLO_TASK_INTERRUPTED_LABEL => TraeTaskStatus::Interrupted,
                TRAE_SOLO_TASK_FINISHED_LABEL => TraeTaskStatus::Finished,
                TRAE_SOLO_TASK_WAITING_FOR_HITL_LABEL => TraeTaskStatus::WaitingForHITL,
                _ => TraeTaskStatus::Idle,
            };

            tasks.push(TraeTask {
                title: snapshot.title,
                status,
                selected: snapshot.selected,
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

    async fn get_task_by_index(&self, index: usize) -> Result<TraeTask, Error> {
        let latest_tasks = self.get_tasks().await?;

        let target = latest_tasks
            .get(index)
            .cloned()
            .ok_or_else(|| Error::msg(format!("Cannot find task by index: {}", index)))?;

        Ok(target)
    }

    pub async fn get_task_handle_by_index(
        &self,
        index: usize,
    ) -> Result<TraeTaskHandler<'_>, Error> {
        let task = self.get_task_by_index(index).await?;
        Ok(TraeTaskHandler::new(self, task))
    }

    /// Operations
    pub async fn select_task_by_index(&self, index: usize) -> Result<(), Error> {
        let _ = wait_for_selector(
            &self.main_page,
            "#solo-ai-sidebar-content div[class*=task-items-list]",
            Duration::from_millis(DEFAULT_SELECTOR_TIMEOUT),
        )
        .await
        .expect("Cannot get task container.");

        let clicked: bool = self
            .main_page
            .evaluate(format!(
                r#"
                (() => {{
                    const items = Array.from(document.querySelectorAll(
                      '#solo-ai-sidebar-content div[class*="task-items-list"] div[class*="index-module__task-item___"]'
                    ));
                    const target = items[{index}];

                    if (!target) {{
                      return false;
                    }}

                    target.click();
                    return true;
                }})()
                "#
            ))
            .await?
            .into_value()?;

        if !clicked {
            return Err(Error::msg(format!(
                "Cannot find task element by index: {}",
                index
            )));
        }

        Ok(())
    }

    pub async fn type_content_to_chat_input(&self, content: &str) -> Result<(), Error> {
        let chat_input_element = wait_for_selector(
            &self.main_page,
            "#agent-chat-view div.chat-input-wrapper div.chat-input-v2-input-box-editable",
            Duration::from_millis(1000 * 60),
        )
        .await?;

        chat_input_element.click().await?;

        self.main_page
            .execute(InsertTextParams::new(content))
            .await?;

        sleep(Duration::from_millis(100)).await;

        Ok(())
    }

    async fn copy_task_summary_by_index(&self) -> Result<(), Error> {
        todo!()
    }

    async fn feedback_task_by_index(&self, feedback: TraeSoloTaskFeedback) {
        todo!()
    }

    async fn retry_task_by_index(&self) {
        todo!()
    }

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
