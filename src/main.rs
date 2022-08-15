use std::{io::Write, sync::Arc};

use kuchiki::traits::TendrilSink;
use reqwest::header::HeaderMap;

async fn fetch_page_content(client: &reqwest::Client, url: &str) -> anyhow::Result<String> {
    let resp = client.get(url).send().await?;
    let ret = resp.text().await?;
    Ok(ret)
}

fn parse_content(data: String, sel: &str) -> anyhow::Result<String> {
    let parsed_html = kuchiki::parse_html().one(data);
    let selected_html = parsed_html
        .select_first(sel)
        .map_err(|_| anyhow::anyhow!("no selector found"))?;
    let pre = selected_html.as_node();
    let pre_text = pre.text_contents();
    log::info!("{}", &pre_text);

    Ok(pre_text)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] {}:{} - {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S.%3f"),
                record.level(),
                record.file().unwrap_or("0"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .filter_level(log::LevelFilter::Debug)
        .init();

    println!("Hello, world!");
    log::info!("{}", "wochao");

    let builder = reqwest::Client::builder();
    let header = HeaderMap::new();
    let client = builder
        .default_headers(header)
        .proxy(reqwest::Proxy::http("http://127.0.0.1:7890")?)
        .proxy(reqwest::Proxy::https("http://127.0.0.1:7890")?)
        .build()?;

    let client = Arc::new(client);

    let resp = fetch_page_content(
        &Arc::clone(&client),
        "https://cool18.com/bbs4/index.php?app=forum&act=threadview&tid=14269613",
    )
    .await?;

    let ret = tokio::task::spawn_blocking(move || parse_content(resp, ".show_content")).await??;
    // let ret = parse_content(&resp, ".show_content").await;
    log::info!("{}", ret);

    Ok(())
}
