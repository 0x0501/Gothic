use crate::config::Config;
use crate::trae::{TraeEditor, TraeEditorMode};
use crate::utils::{wait_for_debug_port, wait_for_shutdown};
use anyhow::Result;
use chromiumoxide::Browser;
use futures::StreamExt;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::Duration;

pub mod config;
pub mod consts;
pub mod trae;
pub mod utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load()?;

    let mut trae_main = Command::new(config.trae_executable_path)
        .arg("--remote-debugging-port=9222")
        .arg("--no-sandbox")
        .stdout(Stdio::null()) // inherit current stream
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;

    // let trae_pid = trae_main.id().expect("Cannot get Trae PID.");

    println!("Hello, world!");

    wait_for_debug_port(9222, Duration::from_secs(30)).await?;

    // connect to CDP
    let (mut browser, mut handler) = Browser::connect("http://127.0.0.1:9222").await?;
    println!("Successfully connect to Trae via CDP: 127.0.0.1:9222");

    // spawn a new task that continuously polls the handler
    let handle = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            match event {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Handler error: {e}");
                    break;
                }
            }
        }
    });

    // navigate to home page
    // let infos = browser.fetch_targets().await?;
    // println!("Discover: {} targets.", infos.len());
    // println!("{:?}", infos.first().unwrap());

    // // sleep a little while
    // sleep(Duration::from_millis(300)).await;

    // let pages = browser.pages().await?;

    // println!("Discover: {} pages.", pages.len());

    // // get the first page, generally there will be only one page available

    // let page = pages
    //     .into_iter()
    //     .next()
    //     .ok_or("Cannot get the main page of Trae.")?;

    // // get the current MODE = IDE or SOLO
    // let trae_mode_badge_element = page.find_element("div.fixed-titlebar-container div.icube-mode-tab > div.icube-tooltip-container > div.icube-tooltip-text.icube-simple-style").await.expect("Cannot locate Trae editor mode badge.");

    // let mode_description = trae_mode_badge_element
    //     .inner_html()
    //     .await
    //     .expect("Cannot get the Trae mode badge text node")
    //     .expect("Cannot get Trae mode text description.");

    // 执行其他自动化操作
    // ...

    let trae_editor_builder = TraeEditor::new();

    let trae_editor = trae_editor_builder.build(&mut browser).await;

    println!(
        "Current Trae Mode: {:?}",
        trae_editor.get_current_editor_mode().await
    );
    // switch mode
    trae_editor.switch_editor_mode(TraeEditorMode::SOLO).await?;

    // create a new task
    let task = trae_editor
        .create_new_task("创建一个个人简历网站".to_string())
        .await;

    // execute task
    match task.execute().await {
        Ok(_) => println!("✅️ Task executed successfully."),
        Err(e) => eprintln!("Task execution failed: {e}"),
    }

    // receive ctrl+c signal
    wait_for_shutdown().await?;

    // close browser
    browser.close().await?;

    // join await
    let _ = handle.await?;

    let _ = trae_main.wait().await?;

    Ok(())
}
