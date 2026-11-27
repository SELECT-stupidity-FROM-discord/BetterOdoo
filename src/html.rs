#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Text(Vec<Run>),
    Item(String, Vec<Run>),
    Attachment { label: String, url: String },
}

fn decode(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}="))? + name.len() + 1;
    let rest = &tag[at..];
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let inner = &rest[1..];
    Some(decode(&inner[..inner.find(quote)?]))
}

fn absolute(base: &str, src: &str) -> String {
    if src.starts_with("http") {
        src.to_string()
    } else {
        format!(
            "{}/{}",
            base.trim_end_matches('/'),
            src.trim_start_matches('/')
        )
    }
}

fn is_file(url: &str) -> bool {
    url.contains("/web/content") || url.contains("/web/image") || url.contains("/web/binary")
}

#[derive(Default)]
struct Parser {
    out: Vec<Block>,
    runs: Vec<Run>,
    bold: usize,
    lists: Vec<Option<usize>>,
    marker: Option<String>,
    link: Option<(String, usize)>,
}

impl Parser {
    fn text(&mut self, raw: &str) {
        for c in decode(raw).chars() {
            let last_space = self.runs.last().is_none_or(|r| r.text.ends_with(' '));
            if c.is_whitespace() && last_space {
                continue;
            }
            let c = if c.is_whitespace() { ' ' } else { c };
            match self.runs.last_mut() {
                Some(r) if r.bold == (self.bold > 0) => r.text.push(c),
                _ => self.runs.push(Run {
                    text: c.to_string(),
                    bold: self.bold > 0,
                }),
            }
        }
    }

    fn flush(&mut self) {
        if let Some(last) = self.runs.last_mut() {
            last.text = last.text.trim_end().to_string();
        }
        self.runs.retain(|r| !r.text.is_empty());
        if self.runs.is_empty() {
            return;
        }
        let runs = std::mem::take(&mut self.runs);
        self.out.push(match self.marker.take() {
            Some(m) => Block::Item(m, runs),
            None => Block::Text(runs),
        });
    }

    fn attachment(&mut self, label: String, url: String) {
        self.flush();
        self.out.push(Block::Attachment { label, url });
    }

    fn tag(&mut self, tag: &str, base: &str) {
        let tag = tag.trim();
        let lower = tag.to_lowercase();
        let closing = lower.starts_with('/');
        let name = lower
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        match (name, closing) {
            ("b" | "strong", false) => self.bold += 1,
            ("b" | "strong", true) => self.bold = self.bold.saturating_sub(1),
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                self.flush();
                self.bold += 1;
            }
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => {
                self.flush();
                self.bold = self.bold.saturating_sub(1);
            }
            ("p" | "div" | "tr" | "br" | "table", _) => self.flush(),
            ("td" | "th", true) => self.text(" "),
            ("ul", false) => {
                self.flush();
                self.lists.push(None);
            }
            ("ol", false) => {
                self.flush();
                self.lists.push(Some(0));
            }
            ("ul" | "ol", true) => {
                self.flush();
                self.lists.pop();
            }
            ("li", false) => {
                self.flush();
                self.marker = Some(match self.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{n}.")
                    }
                    _ => "•".into(),
                });
            }
            ("li", true) => self.flush(),
            ("img", false) => {
                if let Some(src) = attr(tag, "src").filter(|s| !s.starts_with("data:")) {
                    self.attachment("Image attachment".into(), absolute(base, &src));
                }
            }
            ("iframe", false) => {
                if let Some(src) = attr(tag, "src") {
                    let host = src.split('/').nth(2).unwrap_or("link");
                    self.attachment(format!("Embedded content · {host}"), absolute(base, &src));
                }
            }
            ("a", false) => {
                self.link = attr(tag, "href")
                    .map(|h| absolute(base, &h))
                    .filter(|u| is_file(u))
                    .map(|u| (u, self.runs.len()));
            }
            ("a", true) => {
                if let Some((url, start)) = self.link.take() {
                    let label: String = self.runs.drain(start..).map(|r| r.text).collect();
                    let label = label.trim();
                    let label = if label.is_empty() {
                        "Attachment"
                    } else {
                        label
                    };
                    self.attachment(label.to_string(), url);
                }
            }
            _ => {}
        }
    }
}

pub fn parse(html: &str, base: &str) -> Vec<Block> {
    let mut p = Parser::default();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        p.text(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else { break };
        p.tag(&after[..close], base);
        rest = &after[close + 1..];
    }
    p.text(rest);
    p.flush();
    p.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, bold: bool) -> Run {
        Run {
            text: text.into(),
            bold,
        }
    }

    #[test]
    fn paragraphs_lists_and_bold() {
        let html = "<p>Fix the <b>ETA</b> card &amp; push</p><ul><li>one</li><li>two</li></ul><p><br></p><ol><li>a</li><li>b</li></ol>";
        assert_eq!(
            parse(html, "https://x.io"),
            vec![
                Block::Text(vec![
                    run("Fix the ", false),
                    run("ETA", true),
                    run(" card & push", false)
                ]),
                Block::Item("•".into(), vec![run("one", false)]),
                Block::Item("•".into(), vec![run("two", false)]),
                Block::Item("1.".into(), vec![run("a", false)]),
                Block::Item("2.".into(), vec![run("b", false)]),
            ]
        );
    }

    #[test]
    fn images_and_file_links_become_attachments() {
        let html = "<p>See</p><img src=\"/web/image/7?access_token=t\"><a href=\"/web/content/9\">spec.pdf</a>";
        assert_eq!(
            parse(html, "https://x.io/"),
            vec![
                Block::Text(vec![run("See", false)]),
                Block::Attachment {
                    label: "Image attachment".into(),
                    url: "https://x.io/web/image/7?access_token=t".into()
                },
                Block::Attachment {
                    label: "spec.pdf".into(),
                    url: "https://x.io/web/content/9".into()
                },
            ]
        );
    }
}
