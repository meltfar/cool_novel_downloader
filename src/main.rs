use std::{io::Write, sync::Arc};

use async_trait::async_trait;
use kuchiki::traits::TendrilSink;
use reqwest::header::HeaderMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[async_trait]
trait FileSink {
    async fn dump_to_file(&self) -> anyhow::Result<()>;
}

#[async_trait]
impl FileSink for String {
    async fn dump_to_file(&self) -> anyhow::Result<()> {
        let self = self.trim();
        let first_line = self
            .split("\n")
            .take(1)
            .next()
            .ok_or(anyhow::anyhow!("this novel has no content"))?;

        let first_line = first_line.trim();
        log::info!("dumping: {}", first_line);
        if first_line.len() <= 0 {
            return Err(anyhow::anyhow!("this novel doesn't have a title"));
        }

        let mut file = tokio::fs::File::create(format!("./{}.txt", first_line)).await?;
        file.write_all(self.as_bytes()).await?;
        Ok(())
    }
}

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

    Ok(pre_text)
}

async fn get_novel(client: &reqwest::Client, url: &str) -> anyhow::Result<String> {
    let resp = fetch_page_content(client, url).await?;

    let ret = tokio::task::spawn_blocking(move || parse_content(resp, ".show_content")).await??;
    // let ret = parse_content(&resp, ".show_content").await;
    let ret = ret.replace(" cool18.com", "\n");
    // log::info!("{}", ret);

    Ok(ret)
}

async fn dest_novel_list() -> anyhow::Result<Vec<String>> {
    let mut list = tokio::fs::File::open("./novel_list.txt").await?;
    let mut buffer = String::new();
    list.read_to_string(&mut buffer).await?;
    Ok(buffer
        .split("\n")
        .filter_map(|f| {
            if f.trim().len() <= 0 || f.starts_with("//") {
                None
            } else {
                Some(f.trim().to_string())
            }
        })
        .collect::<Vec<String>>())
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
        .filter_level(log::LevelFilter::Info)
        .init();

    let builder = reqwest::Client::builder();
    let header = HeaderMap::new();
    let client = builder
        .default_headers(header)
        .proxy(reqwest::Proxy::http("http://127.0.0.1:7890")?)
        .proxy(reqwest::Proxy::https("http://127.0.0.1:7890")?)
        .build()?;

    let client = Arc::new(client);

    // initialize over, let's start download now.
    let list = dest_novel_list().await?;
    for novel_url in list {
        log::info!("handling: {}", novel_url);
        let novel_content = get_novel(&client, &novel_url).await?;

        novel_content.dump_to_file().await?;
    }

    Ok(())
}
