use std::{io::Write, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use clap::{arg, command, value_parser};
use reqwest::header::HeaderMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use crate::fetcher::{ContentDriller, FileDriller, UrlDriller};

mod fetcher;

#[async_trait]
trait FileSink {
    async fn dump(&self) -> anyhow::Result<()>;
    async fn dump_to_file(&self, file: &mut tokio::fs::File) -> anyhow::Result<()>;
}

#[async_trait]
impl FileSink for String {
    // dump to file, parse filename from html content
    async fn dump(&self) -> anyhow::Result<()> {
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

    // dump to file, use given file descriptor
    async fn dump_to_file(&self, file: &mut tokio::fs::File) -> anyhow::Result<()> {
        file.write_all(self.as_bytes()).await.map_err(|e| e.into())
    }
}

// async fn fetch_page_content(client: &reqwest::Client, url: &str) -> anyhow::Result<String> {
//     let resp = client.get(url).send().await?;
//     let ret = resp.text().await?;
//     Ok(ret)
// }

// fn parse_content(data: String, sel: &str) -> anyhow::Result<String> {
//     let parsed_html = kuchiki::parse_html().one(data);
//     let selected_html = parsed_html
//         .select_first(sel)
//         .map_err(|_| anyhow::anyhow!("no selector found"))?;
//     let pre = selected_html.as_node();
//     let pre_text = pre.text_contents();

//     Ok(pre_text)
// }

// async fn get_novel(client: &reqwest::Client, url: &str) -> anyhow::Result<String> {
//     let resp = fetch_page_content(client, url).await?;

//     let ret = tokio::task::spawn_blocking(move || parse_content(resp, ".show_content")).await??;
//     // let ret = parse_content(&resp, ".show_content").await;
//     let ret = ret.replace(" cool18.com", "\n");
//     // log::info!("{}", ret);

//     Ok(ret)
// }

struct NovelReader {
    reader: tokio::io::Lines<tokio::io::BufReader<tokio::fs::File>>,
}

impl NovelReader {
    fn new(file: tokio::fs::File) -> Self {
        let reader = tokio::io::BufReader::new(file);
        NovelReader {
            reader: reader.lines(),
        }
    }

    async fn next_novel(&mut self) -> anyhow::Result<Option<Vec<String>>> {
        let mut output: Vec<String> = Vec::new();
        let reader = &mut self.reader;

        let mut novel_name = chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S.%3f")
            .to_string();

        while let Some(line) = reader.next_line().await? {
            let trimmed_line = line.trim();
            // ignore empty lines and comments
            if trimmed_line.starts_with("//") || trimmed_line.len() <= 0 {
                continue;
            }

            // found marker
            if trimmed_line.starts_with("---") {
                // previous novel list had fulfilled
                if output.len() > 0 {
                    let nn = trimmed_line.replace("---", "").trim().to_string();
                    if nn.len() > 0 {
                        novel_name = nn;
                    }
                    break;
                } else {
                    // start a new novel list
                    continue;
                }
            }

            if trimmed_line.starts_with("http") || trimmed_line.starts_with("file://") {
                output.push(trimmed_line.to_string());
            }
        }

        if output.len() > 0 {
            // append novel name to the last of array, then we pop it to use.
            output.push(novel_name);
            Ok(Some(output))
        } else {
            Ok(None)
        }
    }
}

async fn handle_one_novel(
    client: &reqwest::Client,
    mut novel_list: Vec<String>,
    semap: Arc<tokio::sync::Semaphore>,
) -> anyhow::Result<()> {
    let novel_name = novel_list
        .pop()
        .ok_or(anyhow::anyhow!("invalid novel name"))?;

    let mut novel_file = tokio::fs::File::create(format!("./{}.txt", novel_name)).await?;

    for novel_url in novel_list {
        let sema = semap.acquire().await?;
        log::info!("downloading: {}", novel_url);
        // NOTE: here we use semaphore to limit the amount we send http requests at the same time.
        // Beside, we also use sleep to "hold on" for a while.
        novel_file.write_all("\n\n".as_bytes()).await?;

        if novel_url.starts_with("file") {
            // file://
            let file_name_split = novel_url.split("file://");
            let file_name = file_name_split
                .skip(1)
                .next()
                .ok_or(anyhow::anyhow!("no file found"))?;
            let fd = FileDriller::new(file_name);
            fd.sink_to(&mut novel_file).await?;
        } else {
            // http
            let ud = UrlDriller::new(novel_url);
            let novel_content = ud.fetch_and_parse(client).await?;
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            drop(sema);

            novel_content.dump_to_file(&mut novel_file).await?;
        }
        // let novel_content = get_novel(client, &novel_url).await?;
    }
    log::info!("novel: {} has done", novel_name);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let matchs = command!()
        .arg(
            arg!(-n --novel_list <FILE> "set the path of novel list")
                .required(true)
                .value_parser(value_parser!(PathBuf)),
        )
        .get_matches();

    let novel_path = matchs.get_one::<PathBuf>("novel_list").ok_or(anyhow::anyhow!("no novel list sepeficied"))?;

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
    let mut header = HeaderMap::new();
    header.append(
        "User-Agent",
        reqwest::header::HeaderValue::from_static(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:103.0) Gecko/20100101 Firefox/103.0",
        ),
    );
    let client = builder
        .default_headers(header)
        .proxy(reqwest::Proxy::http("http://127.0.0.1:7890")?)
        .proxy(reqwest::Proxy::https("http://127.0.0.1:7890")?)
        .build()?;

    let client = Arc::new(client);

    // initialize over, let's start download now.
    let file = tokio::fs::File::open(novel_path).await?;
    let mut reader = NovelReader::new(file);

    let semaphone = Arc::new(tokio::sync::Semaphore::new(3));

    let mut all_task = vec![];

    while let Some(novel_list) = reader.next_novel().await? {
        // let sema = Arc::clone(&semaphone).acquire_owned().await?;
        let semap = Arc::clone(&semaphone);
        let in_client = Arc::clone(&client);

        log::info!("start to handle a novel: {:#?}", novel_list.last());
        let handle = tokio::spawn(async move {
            // let _sema = sema;
            handle_one_novel(&in_client, novel_list, semap).await
        });
        all_task.push(handle);
    }

    // let _sema = semaphone.acquire_many(3).await?;
    for handle in all_task {
        handle.await??;
    }

    log::info!("all done");
    Ok(())
}
