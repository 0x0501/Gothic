#[derive(PartialEq, Debug, Clone, Copy)]
pub enum TraeEditorMode {
    SOLO,
    IDE,
}

#[derive(Debug, Clone, Copy)]
pub enum TraeEditorPrebuiltSoloAgent {
    Coder,
    Builder,
}

// state struct
#[derive(Debug)]
pub struct Interrupted;
#[derive(Debug)]
pub struct Running;
#[derive(Debug)]
pub struct WaitingForHITL;
#[derive(Debug)]
pub struct Finished;
#[derive(Debug)]
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
