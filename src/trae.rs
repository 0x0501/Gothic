use crate::{
    strings::{
        TRAE_IDE_MODE_TEXT_LABEL, TRAE_MAIN_PAGE_URL, TRAE_SOLO_MODE_TEXT_LABEL,
        TRAE_SOLO_TASK_INTERRUPTED_LABEL, TRAE_SOLO_TASK_RUNNING_LABEL,
    },
    utils::wait_for_selector,
};
use anyhow::{Error, Result};
use chromiumoxide::{Browser, Page, cdp::browser_protocol::target::TargetInfo};
use std::marker::PhantomData;
use tokio::time::{Duration, sleep};

const DEFAULT_SELECTOR_TIMEOUT: u64 = 10000; // 10 secs

#[derive(PartialEq, Debug)]
pub enum TraeEditorMode {
    SOLO,
    IDE,
}

#[derive(Debug, Clone, Copy)]
pub enum TraeEditorPrebuiltSoloAgent {
    Coder,
    Builder,
}

// state struct 0-size
pub struct Interrupted;
pub struct Running;
pub struct WaitingForHITL; // human-in-the-loop
pub struct Finished;
pub struct Idle;

pub trait TaskState {}

impl TaskState for Interrupted {}
impl TaskState for Running {}
impl TaskState for WaitingForHITL {}
impl TaskState for Finished {}
impl TaskState for Idle {}

pub trait Action {}
impl Action for Interrupted {}
impl Action for Finished {}

pub enum TraeSoloTaskFeedback {
    Good,
    Bad,
}

#[derive(Debug)]
pub struct TraeSoloTaskInner<'a, S: TaskState> {
    _state: std::marker::PhantomData<S>,
    editor: &'a TraeEditor,
    // elapse: Option<Duration>,
    prompt: Option<String>,
    title: String,
}

pub enum TraeSoloTask<'a> {
    Idle(TraeSoloTaskInner<'a, Idle>),
    Running(TraeSoloTaskInner<'a, Running>),
    Interrupted(TraeSoloTaskInner<'a, Interrupted>),
    WaitingForHITL(TraeSoloTaskInner<'a, WaitingForHITL>),
    Finished(TraeSoloTaskInner<'a, Finished>),
}

impl<'a> TraeSoloTask<'a> {
    pub fn title(&self) -> &str {
        match self {
            TraeSoloTask::Idle(t) => &t.title,
            TraeSoloTask::Running(t) => &t.title,
            TraeSoloTask::Interrupted(t) => &t.title,
            TraeSoloTask::WaitingForHITL(t) => &t.title,
            TraeSoloTask::Finished(t) => &t.title,
        }
    }

    pub async fn execute(&self) -> Result<(), Error> {
        match self {
            TraeSoloTask::Idle(t) => t.execute().await,
            _ => Err(Error::msg(
                "`execute can only be invoked when state is `idle`.`",
            )),
        }
    }

    pub async fn optimize_prompt(&self) -> Result<(), Error> {
        match self {
            TraeSoloTask::Idle(t) => t.execute().await,
            _ => Err(Error::msg(
                "`optimize_prompt` can only be invoked when state is idle",
            )),
        }
    }

    pub async fn copy_task_summary(&self) -> Result<(), Error> {
        match self {
            TraeSoloTask::Interrupted(t) => t.copy_task_summary().await,
            TraeSoloTask::Finished(t) => t.copy_task_summary().await,
            _ => Err(Error::msg(
                "`copy_task_summary` can only be invoked when state is interrupted or finished.",
            )),
        }
    }
}

//
impl<'a> TraeSoloTaskInner<'a, Idle> {
    pub fn new(prompt: String, editor: &'a TraeEditor) -> Self {
        Self {
            _state: PhantomData,
            prompt: Some(prompt),
            editor,
            title: String::new(),
        }
    }

    pub async fn optimize_prompt(&self) {
        todo!()
    }

    async fn is_task_created(&self) -> Result<(), Error> {
        let _ = wait_for_selector(
            &self.editor.main_page,
            "div.welcome-page-solo-agent-title",
            Duration::from_millis(DEFAULT_SELECTOR_TIMEOUT), // 10 secs timeout
        )
        .await
        .expect("Failed to create task, no welcome page was founded.");

        // let _ = self
        //     .editor
        //     .main_page
        //     .find_element("div.welcome-page-solo-agent-title")
        //     .await
        //     .expect("Failed to create task, no welcome page was founded.");

        Ok(())
    }

    // Execute task, type enter or click send button
    pub async fn execute(&self) -> Result<(), Error> {
        // wait util the chat panel was displayed

        let _ = wait_for_selector(
            &self.editor.main_page,
            "div.chat-content-container",
            Duration::from_millis(1000 * 60), // wait up to 1 min
        )
        .await?;

        // click create task button
        let create_task_button = self
            .editor
            .main_page
            .find_element("#solo-ai-sidebar-content div[class*=new-task-button]")
            .await
            .expect("Cannot find task creation button.");

        // click create task button
        create_task_button.click().await?;

        // wait for a while
        sleep(Duration::from_millis(2000)).await;

        // check task creation state
        self.is_task_created().await?;
        Ok(())
    }
}

impl<'a, S: Action + TaskState> TraeSoloTaskInner<'a, S> {
    pub async fn copy_task_summary(&self) -> Result<(), Error> {
        todo!()
    }

    pub async fn feedback_task(&self, feedback: TraeSoloTaskFeedback) {
        todo!()
    }

    pub async fn retry(self) -> TraeSoloTaskInner<'a, Running> {
        TraeSoloTaskInner {
            _state: PhantomData,
            prompt: self.prompt,
            editor: self.editor,
            title: String::new(),
        }
    }
}

#[derive(Debug)]
pub struct TraeEditor {
    main_page: Page,
    target: TargetInfo,
    prebuilt_agent: TraeEditorPrebuiltSoloAgent,
}

pub struct TraeEditorBuilder {}

impl TraeEditorBuilder {
    pub async fn build(&self, browser: &mut Browser) -> TraeEditor {
        let targets = browser.fetch_targets().await.expect("Fetch targets error.");

        sleep(Duration::from_millis(2000)).await;

        let mut filtered_target: Vec<TargetInfo> = targets
            .into_iter()
            .filter(|t| t.url == TRAE_MAIN_PAGE_URL)
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

        return TraeEditor {
            target: main_target,
            main_page: main_page,
            prebuilt_agent: TraeEditorPrebuiltSoloAgent::Coder,
        };
    }
}

impl TraeEditor {
    pub fn new() -> TraeEditorBuilder {
        TraeEditorBuilder {}
    }

    pub async fn get_main_page(&self) -> &Page {
        return &self.main_page;
    }

    pub async fn get_current_editor_mode(&self) -> Result<TraeEditorMode, Error> {
        let trae_mode_badge_element = self.main_page.find_element("div.fixed-titlebar-container div.icube-mode-tab > div.icube-tooltip-container > div.icube-tooltip-text.icube-simple-style").await.expect("Cannot locate Trae editor mode badge.");

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
            Err(Error::msg("Cannot get the current editor mode"))
        }
    }

    pub async fn switch_editor_mode(&self, mode: TraeEditorMode) -> Result<(), Error> {
        let current_mode = self.get_current_editor_mode().await?;

        if current_mode == mode {
            return Ok(());
        }

        let trae_mode_tab_switch = self.main_page.find_element("div.fixed-titlebar-container div.icube-mode-tab > div.icube-mode-tab-container > div.icube-mode-tab-switch").await.expect("Cannot locate Trae editor mode switch tab.");
        trae_mode_tab_switch.click().await?;

        Ok(())
    }

    pub async fn create_new_task<'a>(&'a self, prompt: String) -> TraeSoloTask<'a> {
        TraeSoloTask::Idle(TraeSoloTaskInner::<Idle>::new(prompt, self))
    }

    pub fn set_default_prebuilt_solo_agent(&mut self, agent: TraeEditorPrebuiltSoloAgent) {
        self.prebuilt_agent = agent;
    }

    pub fn get_default_prebuilt_solo_agent(&self) -> TraeEditorPrebuiltSoloAgent {
        self.prebuilt_agent
    }

    // private methods

    // get tasks from sidebar
    pub async fn get_tasks(&'_ self) -> Result<Vec<TraeSoloTask<'_>>, Error> {
        let current_mode = self.get_current_editor_mode().await?;

        if current_mode != TraeEditorMode::SOLO {
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
            .find_elements("div[class*=task-item]")
            .await
            .expect("Cannot get task items from container.");

        let mut tasks: Vec<TraeSoloTask> = Vec::new();
        // TODO
        // 1. WaitingForHITL
        // 2. Finished
        for t in task_items.iter() {
            let raw_task_state = t
                .find_element("div[class*=task-type-wrap")
                .await
                .expect(&format!("Cannot get task type: {:#?}", t))
                .inner_html()
                .await
                .unwrap_or_default()
                .unwrap_or_else(|| {
                    println!("Trying to get task type label failed, the value is None");
                    return "".to_string();
                });

            let task_title = t
                .find_element("span[class*=task-title]")
                .await
                .expect(&format!("Cannot get task title: {:#?}", t))
                .inner_html()
                .await
                .unwrap_or_default()
                .unwrap_or_else(|| {
                    println!("Trying to get task title label failed, the value is None");
                    return "".to_string();
                });

            let task = match raw_task_state.as_str() {
                TRAE_SOLO_TASK_INTERRUPTED_LABEL => TraeSoloTask::Interrupted(TraeSoloTaskInner {
                    _state: PhantomData,
                    editor: self,
                    prompt: None,
                    title: task_title,
                }),
                TRAE_SOLO_TASK_RUNNING_LABEL => TraeSoloTask::Running(TraeSoloTaskInner {
                    _state: PhantomData,
                    editor: self,
                    prompt: None,
                    title: task_title,
                }),
                _ => TraeSoloTask::Idle(TraeSoloTaskInner {
                    _state: PhantomData,
                    editor: self,
                    prompt: None,
                    title: task_title,
                }),
            };

            tasks.push(task);
        }

        Ok(tasks)
    }
}
