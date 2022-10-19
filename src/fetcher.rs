// we initial fetcher with two possible backend: file, url

use async_trait::async_trait;
use kuchiki::traits::TendrilSink;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[async_trait]
pub(crate) trait ContentDriller<W>
where
    W: AsyncWrite + Send,
{
    async fn sink_to(&self, writer: &mut W) -> anyhow::Result<usize>;
}

pub(crate) struct FileDriller {
    filename: String,
}

impl FileDriller {
    pub fn new(file: &str) -> Self {
        Self {
            filename: file.to_string(),
        }
    }
}

#[async_trait]
impl<W: AsyncWrite + Send + Unpin> ContentDriller<W> for FileDriller {
    async fn sink_to(&self, writer: &mut W) -> anyhow::Result<usize> {
        let mut file = tokio::fs::File::open(&self.filename).await?;

        let mut length = 0;
        let mut buffer = [0; 1024 * 8];
        loop {
            let n = file.read(&mut buffer).await?;
            length += n;
            if n > 0 {
                writer.write_all(&buffer[0..n]).await?;
            } else {
                // end of file
                break;
            }
        }

        Ok(length)
    }
}

pub(crate) enum UrlDriller {
    RMUrlDriller(String),
    CoolUrlDriller(String),
    MMirrorDriller(String),
    UnknownDriller,
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

fn parse_content_multi(data: String, sel: &str) -> anyhow::Result<String> {
    let parsed_html = kuchiki::parse_html().one(data);
    let selected_html = parsed_html
        .select(sel)
        .map_err(|_| anyhow::anyhow!("no selector found"))?;

    let mut output = String::new();

    for sh in selected_html {
        // TODO: how to convert br to newline ?
        let pre_text = sh.as_node().text_contents();
        output.push_str(&pre_text);
        output.push('\n');
    }

    Ok(output)
}

impl UrlDriller {
    async fn fetch_page_content(&self, client: &reqwest::Client) -> anyhow::Result<String> {
        let page_content = match self {
            UrlDriller::RMUrlDriller(_) => unreachable!(),
            UrlDriller::CoolUrlDriller(url) => {
                let resp = client.get(url).send().await?;
                let content = resp.text_with_charset("utf-8").await?;
                content
            }
            UrlDriller::MMirrorDriller(url) => {
                if !url.contains("show=") {
                    return Err(anyhow::anyhow!("please select 'show this one only' button, otherwise result would be polluted"));
                }

                let resp = client
                    .get(url)
                    .header("Referer", "https://mirror.chromaso.net/forum/")
                    .send()
                    .await?;
                let content = resp.text_with_charset("utf-8").await?;
                content.replace("<br>", "<br> MMirrorDrillerERT")
            }
            UrlDriller::UnknownDriller => {
                return Err(anyhow::anyhow!("unknown url"));
            }
        };

        Ok(page_content)
    }

    pub async fn fetch_and_parse(&self, client: &reqwest::Client) -> anyhow::Result<String> {
        let html = self.fetch_page_content(client).await?;
        match self {
            UrlDriller::RMUrlDriller(_) => unreachable!(),
            UrlDriller::CoolUrlDriller(_) => {
                parse_content(html, ".show_content").map(|f| f.replace(" cool18.com", "\n"))
            }
            UrlDriller::MMirrorDriller(_) => {
                parse_content_multi(html, "div.card.mm-post > div.card-body")
                    .map(|f| f.replace("MMirrorDrillerERT", "\n"))
                // Ok(ret.replace(" cool18.com", "\n"))
            }
            UrlDriller::UnknownDriller => unreachable!(),
        }
        // Ok(String::new())
    }

    pub fn new(url: String) -> Self {
        if url.contains("cool18.com") {
            Self::CoolUrlDriller(url)
        } else if url.contains("s80m.com") {
            Self::RMUrlDriller(url)
        } else if url.contains("chromaso.net") {
            Self::MMirrorDriller(url)
        } else {
            Self::UnknownDriller
        }
    }
}

#[cfg(test)]
mod fetcher_test {
    use super::{ContentDriller, FileDriller, UrlDriller};

    #[tokio::test]
    async fn test_file_driller() {
        let mut source = tokio::fs::File::create(
            "/Users/zhouplus/projects/rust/cool_novel_fetcher/tests/o2.txt",
        )
        .await
        .unwrap();

        let fd = FileDriller::new("/Users/zhouplus/projects/rust/cool_novel_fetcher/tests/s1.txt");
        let r = fd.sink_to(&mut source).await.unwrap();
        println!("r: {}", r);
        ()
    }

    #[tokio::test]
    async fn test_url_driller() -> anyhow::Result<()> {
        let builder = reqwest::Client::builder();
        let mut header = reqwest::header::HeaderMap::new();
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

        let cool_driller = UrlDriller::new(String::from(
            "https://mirror.chromaso.net/thread/52498?show=154344",
        ));

        let cot = cool_driller.fetch_and_parse(&client).await?;
        println!("{}", cot);
        // let novel_content = cot.parse();
        // then save
        Ok(())
    }

    #[test]
    fn test_split() {
        let s = "file://werwerwerdsfdsf";
        let f = s.split("file://");
        for ff in f {
            println!("{}", ff);
        }
    }
}
