use crate::trae::types::*; // 引入刚才定义的类型
use crate::utils::wait_for_selector;
use crate::{consts::DEFAULT_SELECTOR_TIMEOUT, trae::editor::TraeEditor}; // 需要引用 Editor
use anyhow::{Error, Result};
use chromiumoxide::cdp::browser_protocol::input::InsertTextParams;
use std::marker::PhantomData;
use tokio::time::{Duration, sleep};

#[derive(Debug)]
pub struct TraeSoloTaskInner<'a, S: TaskState> {
    pub(crate) _state: PhantomData<S>,
    pub(crate) editor: &'a TraeEditor,
    pub(crate) prompt: Option<String>,
    pub(crate) title: String,
}

#[derive(Debug)]
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

        // fill out chat input

        let chat_input_element = wait_for_selector(
            &self.editor.main_page,
            "#agent-chat-view div.chat-input-wrapper div.chat-input-v2-input-box-editable",
            Duration::from_millis(1000 * 60),
        )
        .await
        .expect("Cannot find chat input component.");

        // activate content editable
        chat_input_element.click().await?;

        // type prompt into chat input
        self.editor
            .main_page
            .execute(InsertTextParams::new(self.prompt.as_ref().unwrap()))
            .await?; // press enter

        // wait 1 sec
        sleep(Duration::from_millis(1000)).await;

        // press enter to submit the task
        chat_input_element.press_key("Enter").await?;
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
