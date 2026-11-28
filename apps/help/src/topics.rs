//! Help topics, read from the repository's Markdown at build time, so Help
//! works offline and shows exactly the documentation of the build it ships
//! with. The parser keeps the structure a reader needs (headings, lists,
//! code blocks, paragraphs) and drops the rest of the Markdown syntax.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading(String),
    Text(String),
    Item(String),
    Code(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Topic {
    /// A short name for deep links, such as `architecture`.
    pub id: &'static str,
    pub title: String,
    pub blocks: Vec<Block>,
}

/// The topics this build carries.
pub fn bundled() -> Vec<Topic> {
    vec![
        parse("readme", include_str!("../../../README.md")),
        parse(
            "architecture",
            include_str!("../../../docs/architecture.md"),
        ),
        parse("development", include_str!("../../../docs/development.md")),
    ]
}

/// Parse one Markdown document. The title is its first `#` heading, or the id.
pub fn parse(id: &'static str, markdown: &str) -> Topic {
    let mut title = None;
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut in_code = false;
    for raw in markdown.lines() {
        if in_code {
            if raw.trim_start().starts_with("```") {
                in_code = false;
            } else {
                blocks.push(Block::Code(raw.trim_end().to_owned()));
            }
            continue;
        }
        let line = raw.trim_end();
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            flush(&mut paragraph, &mut blocks);
            in_code = true;
        } else if trimmed.is_empty() {
            flush(&mut paragraph, &mut blocks);
        } else if let Some(text) = trimmed.strip_prefix("# ") {
            flush(&mut paragraph, &mut blocks);
            let text = plain(text);
            title.get_or_insert_with(|| text.clone());
            blocks.push(Block::Heading(text));
        } else if let Some(text) = trimmed
            .strip_prefix("## ")
            .or_else(|| trimmed.strip_prefix("### "))
        {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Heading(plain(text)));
        } else if let Some(text) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::Item(plain(text)));
        } else {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(&plain(trimmed));
        }
    }
    flush(&mut paragraph, &mut blocks);
    Topic {
        id,
        title: title.unwrap_or_else(|| id.to_owned()),
        blocks,
    }
}

fn flush(paragraph: &mut String, blocks: &mut Vec<Block>) {
    if !paragraph.is_empty() {
        blocks.push(Block::Text(std::mem::take(paragraph)));
    }
}

/// Removes the inline Markdown marks a plain-text window does not draw.
fn plain(text: &str) -> String {
    text.replace("**", "").replace('`', "")
}

impl Topic {
    /// Whether `query` (already lower case) appears in the title or the text.
    fn matches(&self, query: &str) -> bool {
        if self.title.to_lowercase().contains(query) {
            return true;
        }
        self.blocks.iter().any(|block| {
            let text = match block {
                Block::Heading(text)
                | Block::Text(text)
                | Block::Item(text)
                | Block::Code(text) => text,
            };
            text.to_lowercase().contains(query)
        })
    }
}

/// Indices of the topics matching `query`, in order. A blank query matches all.
pub fn search(topics: &[Topic], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    topics
        .iter()
        .enumerate()
        .filter(|(_, topic)| query.is_empty() || topic.matches(&query))
        .map(|(index, _)| index)
        .collect()
}

/// The index of the topic with this id.
pub fn find(topics: &[Topic], id: &str) -> Option<usize> {
    topics.iter().position(|topic| topic.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Sample guide\n\nFirst line of text\ncontinues here.\n\n## Install\n- Run `install.sh`\n* Restart the session\n\n```\ncargo build --locked\n```\n\nClosing **words**.\n";

    #[test]
    fn headings_items_code_and_paragraphs_are_kept_apart() {
        let topic = parse("sample", SAMPLE);
        assert_eq!(topic.title, "Sample guide");
        assert_eq!(
            topic.blocks,
            vec![
                Block::Heading("Sample guide".to_owned()),
                Block::Text("First line of text continues here.".to_owned()),
                Block::Heading("Install".to_owned()),
                Block::Item("Run install.sh".to_owned()),
                Block::Item("Restart the session".to_owned()),
                Block::Code("cargo build --locked".to_owned()),
                Block::Text("Closing words.".to_owned()),
            ]
        );
    }

    #[test]
    fn a_document_without_a_heading_is_named_by_its_id() {
        let topic = parse("plain", "just words\n");
        assert_eq!(topic.title, "plain");
    }

    #[test]
    fn search_matches_titles_and_text_ignoring_case() {
        let topics = vec![parse("a", SAMPLE), parse("b", "# Other\n\nNothing here.\n")];
        assert_eq!(search(&topics, ""), vec![0, 1]);
        assert_eq!(search(&topics, "  RESTART "), vec![0]);
        assert_eq!(search(&topics, "other"), vec![1]);
        assert!(search(&topics, "absent words").is_empty());
    }

    #[test]
    fn deep_links_find_a_topic_by_id() {
        let topics = vec![parse("a", SAMPLE), parse("b", "# B\n")];
        assert_eq!(find(&topics, "b"), Some(1));
        assert_eq!(find(&topics, "missing"), None);
    }

    #[test]
    fn the_bundled_topics_come_from_the_repository_docs() {
        let topics = bundled();
        assert_eq!(topics.len(), 3);
        assert!(find(&topics, "architecture").is_some());
        for topic in &topics {
            assert!(!topic.title.is_empty(), "{}", topic.id);
            assert!(topic.blocks.len() > 3, "{} has too little text", topic.id);
        }
    }
}
