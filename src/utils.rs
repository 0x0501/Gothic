use anyhow::{Error, Result, bail};
use chromiumoxide::{Element, Page};
use tokio::{
    signal,
    time::{Duration, Instant, sleep},
};

pub async fn wait_for_debug_port(
    port: u16,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now(); // current time;
    while start.elapsed() < timeout {
        // try connect the port via TCP
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .is_ok()
        {
            // sleep 1 sec after the port is connectable
            sleep(Duration::from_secs(1)).await;
            return Ok(());
        }
        sleep(Duration::from_millis(300)).await;
    }
    Err(format!("Timeout listening the CDP port: {}", port).into())
}

pub async fn wait_for_shutdown() -> Result<()> {
    signal::ctrl_c().await?;
    Ok(())
}

pub async fn wait_for_selector(
    page: &Page,
    selector: &str,
    timeout: Duration,
) -> Result<Element, Error> {
    let start = Instant::now();

    loop {
        let ele = page.find_element(selector).await;
        if ele.is_ok() {
            return Ok(ele?);
        }

        if start.elapsed() >= timeout {
            bail!("Timeout locating: {}", selector);
        }

        sleep(Duration::from_millis(300)).await;
    }
}
