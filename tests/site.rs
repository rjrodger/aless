//! The documentation site, built and checked.
//!
//! The site is served at Cargo.toml's `homepage`. Its pages are Markdown
//! under `site/`, one directory for each of Diátaxis's four kinds:
//! `tutorials/`, `how-to/`, `reference/` and `explanation/`. They are
//! rendered by tabnas-markdown, the parser aless reads Markdown with, into
//! `site/layout.html`. The reference's pages are written from the binary's
//! own tables rather than by hand: the command line from the topics
//! `--help` prints (`aless::cli::reference_html`) and the keys from the
//! help F1 shows (`aless::app::help_lines`), so the site cannot disagree
//! with either.
//!
//! `cargo test --test site` builds the site in memory and checks it: its
//! links and anchors, the options each example names, the console
//! examples' output (on Unix, by running them against the files in
//! `examples/`), and the house style's rules for prose. With
//! `ALESS_SITE_OUT=DIR` it also writes the site into DIR, which is what
//! `.github/workflows/pages.yml` publishes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

const HOMEPAGE: &str = env!("CARGO_PKG_HOMEPAGE");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One of Diátaxis's four kinds: a directory of pages.
struct Section {
    dir: &'static str,
    /// The navigation's name for the section.
    label: &'static str,
    /// A page's kind, as the page heads itself.
    kind: &'static str,
}

/// The sections, in the order the navigation lists them.
const SECTIONS: [Section; 4] = [
    Section {
        dir: "tutorials",
        label: "Tutorials",
        kind: "Tutorial",
    },
    Section {
        dir: "how-to",
        label: "How-to guides",
        kind: "How-to guide",
    },
    Section {
        dir: "reference",
        label: "Reference",
        kind: "Reference",
    },
    Section {
        dir: "explanation",
        label: "Explanation",
        kind: "Explanation",
    },
];

/// Test fixtures the pages' examples read as well, published beside
/// `examples/` for a reader to download, from where they are kept.
const FIXTURES: &[&str] = &[
    "tests/fixtures/grammars/hosts.abnf",
    "tests/fixtures/grammars/hosts",
    "tests/fixtures/programs/export.alc",
    "tests/fixtures/programs/table.alc",
    "tests/fixtures/records.json",
];

const TUTORIALS: usize = 0;
const REFERENCE: usize = 2;

/// A page of the site.
struct Page {
    /// Where it is published: `how-to/convert.html`.
    path: String,
    title: String,
    /// One sentence: the page's meta description, and its line in lists.
    description: String,
    /// Its place in its section's list; 0 for a page in none.
    order: i64,
    /// Its section; `None` for the home page and the 404 page.
    section: Option<usize>,
    /// The Markdown it was written in, without its front matter; `None`
    /// for a page written from the binary.
    markdown: Option<String>,
    /// A page written from the binary, as the terminal shows it: what
    /// `aless --help` or F1 prints. Published beside the page as `.txt`.
    text: Option<String>,
    /// The file it comes from, for the footer's link to it.
    origin: String,
    /// The page's content as HTML.
    body: String,
}

impl Page {
    fn is_index(&self) -> bool {
        self.path.ends_with("/index.html")
    }
}

/// The site: its pages, and every file it publishes, by path.
struct Site {
    pages: Vec<Page>,
    files: BTreeMap<String, Vec<u8>>,
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sources() -> PathBuf {
    root().join("site")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The site, built once for every test here.
fn site() -> &'static Site {
    static SITE: OnceLock<Site> = OnceLock::new();
    SITE.get_or_init(build)
}

fn build() -> Site {
    let mut pages = markdown_pages();
    pages.extend(reference_pages());
    list_sections(&mut pages);
    let layout = read(&sources().join("layout.html"));
    let mut files = BTreeMap::new();
    for page in &pages {
        files.insert(
            page.path.clone(),
            render(&layout, page, &pages).into_bytes(),
        );
        files.insert(
            page.path.replace(".html", ".md"),
            twin(page, &pages).into_bytes(),
        );
        if let Some(text) = &page.text {
            files.insert(
                page.path.replace(".html", ".txt"),
                text.clone().into_bytes(),
            );
        }
    }
    for name in ["style.css", "favicon.svg", "CNAME"] {
        let bytes = std::fs::read(sources().join(name)).unwrap();
        files.insert(name.to_string(), bytes);
    }
    // The files the pages' examples read, for a reader to download.
    let mut downloads: Vec<PathBuf> = std::fs::read_dir(root().join("examples"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().is_some_and(|n| n != "README.md"))
        .collect();
    downloads.extend(FIXTURES.iter().map(|f| root().join(f)));
    for path in downloads {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let previous = files.insert(format!("examples/{name}"), bytes);
        assert!(previous.is_none(), "two downloads are named {name}");
    }
    files.insert("SKILL.md".into(), aless::cli::SKILL.as_bytes().to_vec());
    files.insert("llms.txt".into(), llms(&pages).into_bytes());
    files.insert("sitemap.xml".into(), sitemap(&pages).into_bytes());
    Site { pages, files }
}

/// Every page written in Markdown: `site/index.md`, `site/404.md`, and
/// each section's directory.
fn markdown_pages() -> Vec<Page> {
    let mut dirs: Vec<(&str, Option<usize>)> = vec![("", None)];
    dirs.extend(SECTIONS.iter().enumerate().map(|(i, s)| (s.dir, Some(i))));
    let mut pages = Vec::new();
    for (dir, section) in dirs {
        let mut names: Vec<String> = std::fs::read_dir(sources().join(dir))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".md"))
            .collect();
        names.sort();
        for name in names {
            let origin = match dir {
                "" => format!("site/{name}"),
                dir => format!("site/{dir}/{name}"),
            };
            let text = read(&root().join(&origin));
            let (meta, markdown) = front_matter(&text, &origin);
            let stem = name.trim_end_matches(".md");
            let path = match dir {
                "" => format!("{stem}.html"),
                dir => format!("{dir}/{stem}.html"),
            };
            let field = |key: &str| {
                meta.get(key)
                    .cloned()
                    .unwrap_or_else(|| panic!("{origin}: no {key} in its front matter"))
            };
            // `order` places a page in its section's list, which every
            // page of a section but its index is in, and no other page.
            let listed = section.is_some() && stem != "index";
            let order = match (listed, meta.get("order")) {
                (true, Some(order)) => order
                    .parse()
                    .unwrap_or_else(|_| panic!("{origin}: order {order:?} is not a number")),
                (true, None) => panic!("{origin}: no order in its front matter"),
                (false, Some(_)) => panic!(
                    "{origin}: order places a page in its section's list, and an index \
                     page, the home page and the 404 page are in none"
                ),
                (false, None) => 0,
            };
            pages.push(Page {
                path,
                title: field("title"),
                description: field("description"),
                order,
                section,
                body: markdown_html(&markdown),
                markdown: Some(markdown),
                text: None,
                origin,
            });
        }
    }
    for (i, section) in SECTIONS.iter().enumerate() {
        let index = format!("{}/index.html", section.dir);
        assert!(
            pages
                .iter()
                .any(|p| p.path == index && p.section == Some(i)),
            "site/{}/index.md is missing",
            section.dir
        );
    }
    pages
}

/// A page's front matter, `key: value` lines between `---` lines, and
/// the Markdown after it.
fn front_matter(text: &str, origin: &str) -> (BTreeMap<String, String>, String) {
    let text = text.replace("\r\n", "\n");
    let rest = text
        .strip_prefix("---\n")
        .unwrap_or_else(|| panic!("{origin} does not start with front matter"));
    let (head, body) = rest
        .split_once("\n---\n")
        .unwrap_or_else(|| panic!("{origin}: its front matter does not end"));
    let mut meta = BTreeMap::new();
    for line in head.lines() {
        let (key, value) = line
            .split_once(':')
            .unwrap_or_else(|| panic!("{origin}: {line:?} is not key: value"));
        assert!(
            ["title", "description", "order"].contains(&key),
            "{origin}: unknown front matter key {key}"
        );
        meta.insert(key.to_string(), value.trim().to_string());
    }
    (meta, body.trim_start().to_string())
}

/// Markdown as HTML, after the values a page may name are filled in.
fn markdown_html(markdown: &str) -> String {
    let markdown = fill(markdown);
    screens(&tabnas_markdown::to_html(
        &markdown,
        &tabnas_markdown::DEFAULT_OPTIONS,
    ))
}

/// Each `screen` block, a capture of the viewer, marked for the
/// stylesheet, with its status bar marked within it: the row that ends in
/// ` · LINE:COL`, which the viewer draws in reverse video.
fn screens(html: &str) -> String {
    let block = Regex::new(r#"(?s)<pre><code class="language-screen">(.*?)</code></pre>"#).unwrap();
    let status = Regex::new(r"^(?: \S|…).* · \d+:\d+$").unwrap();
    block
        .replace_all(html, |c: &regex::Captures| {
            let rows: Vec<String> = c[1]
                .split('\n')
                .map(|row| match status.is_match(row) {
                    true => format!("<span class=\"status\">{row}</span>"),
                    false => row.to_string(),
                })
                .collect();
            format!(
                "<pre class=\"screen\"><code class=\"language-screen\">{}</code></pre>",
                rows.join("\n")
            )
        })
        .into_owned()
}

/// `{{version}}` and `{{tagline}}` in a page, filled in.
fn fill(text: &str) -> String {
    text.replace("{{version}}", VERSION)
        .replace("{{tagline}}", aless::cli::TAGLINE)
}

/// The reference's pages, written from the binary's own tables.
fn reference_pages() -> Vec<Page> {
    let command_line = format!(
        "<p>The reference <code>aless --help</code> prints, for aless {VERSION}. \
         <code>aless -h</code> prints a summary of the options, and \
         <code>man aless</code> this reference as a manual page. The same text is \
         <a href=\"/reference/command-line.txt\">plain text</a> here, as the \
         terminal shows it.</p>\n{}",
        aless::cli::reference_html()
    );
    vec![
        Page {
            path: "reference/command-line.html".into(),
            title: "Command line".into(),
            description: "Every option, what each output prints, the errors and the exit \
                statuses: the whole reference aless --help prints."
                .into(),
            order: 1,
            section: Some(REFERENCE),
            markdown: None,
            text: Some(aless::cli::reference()),
            origin: "src/cli.rs".into(),
            body: command_line,
        },
        Page {
            path: "reference/keys.html".into(),
            title: "Keys".into(),
            description: "Every key and command of the viewer, as F1 lists them inside \
                aless."
                .into(),
            order: 2,
            section: Some(REFERENCE),
            markdown: None,
            text: Some(aless::app::help_lines().join("\n") + "\n"),
            origin: "src/app.rs".into(),
            body: keys_html(),
        },
    ]
}

/// The help F1 shows, a section for each of its headings.
fn keys_html() -> String {
    let mut out = String::from(
        "<p>The keys are jless's where jless has the key, and a count before a key \
         repeats it (<code>3j</code>). Press <kbd>F1</kbd> or type <code>:help</code> \
         inside aless for this list; the same text is \
         <a href=\"/reference/keys.txt\">plain text</a> here.</p>\n",
    );
    let mut heading: Option<(String, String)> = None;
    let mut lines: Vec<String> = Vec::new();
    let flush = |out: &mut String, heading: &Option<(String, String)>, lines: &mut Vec<String>| {
        if let Some((title, note)) = heading {
            out.push_str(&format!(
                "<h2 id=\"{}\">{}</h2>\n",
                slug(title),
                escape(title)
            ));
            if !note.is_empty() {
                out.push_str(&format!("<p class=\"note\">{}</p>\n", escape(note)));
            }
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            let text: Vec<&str> = lines
                .iter()
                .map(|l| l.strip_prefix("  ").unwrap_or(l.as_str()))
                .collect();
            out.push_str(&format!(
                "<pre><code>{}</code></pre>\n",
                escape(&text.join("\n"))
            ));
        }
        lines.clear();
    };
    // The first line is the help's title; a line at the margin in capitals
    // heads a section, with a note in parentheses after it.
    for line in aless::app::help_lines().into_iter().skip(1) {
        let first = line.split_whitespace().next().unwrap_or("");
        let is_heading =
            !line.starts_with(' ') && !first.is_empty() && first.chars().all(|c| !c.is_lowercase());
        if is_heading {
            flush(&mut out, &heading, &mut lines);
            let (title, note) = match line.split_once("  (") {
                Some((title, note)) => (title.trim(), note.trim_end_matches(')')),
                None => (line.trim(), ""),
            };
            heading = Some((sentence_case(title), note.to_string()));
        } else if heading.is_some() && !(lines.is_empty() && line.trim().is_empty()) {
            lines.push(line);
        }
    }
    flush(&mut out, &heading, &mut lines);
    out
}

/// `TABS, FILES AND WATCHING` as a heading is written.
fn sentence_case(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut chars = lower.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Each section's index page, with the list of its pages appended.
fn list_sections(pages: &mut [Page]) {
    for (i, _) in SECTIONS.iter().enumerate() {
        let mut members: Vec<(i64, String, String, String)> = pages
            .iter()
            .filter(|p| p.section == Some(i) && !p.is_index())
            .map(|p| {
                (
                    p.order,
                    p.title.clone(),
                    p.path.clone(),
                    p.description.clone(),
                )
            })
            .collect();
        members.sort();
        let mut list = String::from("<dl class=\"pages\">\n");
        for (_, title, path, description) in &members {
            list.push_str(&format!(
                "<dt><a href=\"/{path}\">{}</a></dt>\n<dd>{}</dd>\n",
                escape(title),
                escape(description)
            ));
        }
        list.push_str("</dl>\n");
        let index = pages
            .iter_mut()
            .find(|p| p.section == Some(i) && p.is_index())
            .unwrap();
        index.body.push_str(&list);
    }
}

/// The pages of a section, in their order.
fn members(pages: &[Page], section: usize) -> Vec<&Page> {
    let mut members: Vec<&Page> = pages
        .iter()
        .filter(|p| p.section == Some(section) && !p.is_index())
        .collect();
    members.sort_by(|a, b| (a.order, &a.title).cmp(&(b.order, &b.title)));
    members
}

/// The page in the layout.
fn render(layout: &str, page: &Page, pages: &[Page]) -> String {
    let depth = page.path.matches('/').count();
    // The 404 page is served for any missing path, at any depth, so its
    // links start at the site's root.
    let root = if page.path == "404.html" {
        "/".to_string()
    } else {
        "../".repeat(depth)
    };
    let home = page.path == "index.html";
    let head = match page.section {
        Some(s) => format!(
            "<p class=\"kind\">{}</p>\n<h1>{}</h1>\n<p class=\"lede\">{}</p>\n",
            SECTIONS[s].kind,
            escape(&page.title),
            escape(&page.description)
        ),
        None if home => String::new(),
        None => format!("<h1>{}</h1>\n", escape(&page.title)),
    };
    let nav = match page.section {
        Some(_) => nav(page, pages),
        None => String::new(),
    };
    let head_title = if home {
        format!("aless: {}", page.title)
    } else {
        format!("{} · aless", page.title)
    };
    let mut html = layout
        .replace("{{head_title}}", &escape(&head_title))
        .replace("{{description}}", &escape(&page.description))
        .replace("{{canonical}}", &url(&page.path))
        .replace("{{body_class}}", if home { "home" } else { "doc" })
        .replace("{{page_head}}", &head)
        .replace("{{nav}}", &nav)
        .replace("{{version}}", VERSION)
        .replace(
            "{{source}}",
            &format!("{REPOSITORY}/blob/main/{}", page.origin),
        )
        .replace("{{root}}", "/");
    assert!(
        !html.replace("{{content}}", "").contains("{{"),
        "layout.html names a value the build does not fill"
    );
    html = html.replace("{{content}}", &anchor_headings(&page.body));
    relative(&html, &root)
}

/// The sidebar: every section and its pages, this page marked.
fn nav(page: &Page, pages: &[Page]) -> String {
    let mut out = String::from("<nav class=\"sidebar\" aria-label=\"Documentation\">\n");
    for (i, section) in SECTIONS.iter().enumerate() {
        let current = |path: &str| {
            if path == page.path {
                " aria-current=\"page\""
            } else {
                ""
            }
        };
        let index = format!("{}/index.html", section.dir);
        out.push_str(&format!(
            "<p class=\"nav-group\"><a href=\"/{index}\"{}>{}</a></p>\n<ul>\n",
            current(&index),
            section.label
        ));
        for member in members(pages, i) {
            out.push_str(&format!(
                "<li><a href=\"/{}\"{}>{}</a></li>\n",
                member.path,
                current(&member.path),
                escape(&member.title)
            ));
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</nav>\n");
    out
}

/// Links from the site's root (`/how-to/convert.html`) made relative to a
/// page `root` above it (`../how-to/convert.html`), so that the site reads
/// the same from a directory on disk as from its domain.
fn relative(html: &str, root: &str) -> String {
    let link = Regex::new(r#"(href|src)="/([^"/][^"]*)?""#).unwrap();
    link.replace_all(html, |c: &regex::Captures| {
        let target = c.get(2).map_or("", |m| m.as_str());
        let (path, fragment) = match target.split_once('#') {
            Some((path, fragment)) => (path, format!("#{fragment}")),
            None => (target, String::new()),
        };
        let path = if path.is_empty() || path.ends_with('/') {
            format!("{path}index.html")
        } else {
            path.to_string()
        };
        format!("{}=\"{root}{path}{fragment}\"", &c[1])
    })
    .into_owned()
}

/// Every `<h2>` to `<h4>` given an `id` and a link to itself.
fn anchor_headings(html: &str) -> String {
    let heading = Regex::new(r#"<h([2-4])(?: id="([^"]*)")?>(.*?)</h[2-4]>"#).unwrap();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    heading
        .replace_all(html, |c: &regex::Captures| {
            let level = &c[1];
            let text = &c[3];
            let mut id = c
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| slug(text));
            let base = id.clone();
            let mut n = 2;
            while !seen.insert(id.clone()) {
                id = format!("{base}-{n}");
                n += 1;
            }
            format!(
                "<h{level} id=\"{id}\">{text} <a class=\"anchor\" href=\"#{id}\" \
                 aria-label=\"Link to this section\">#</a></h{level}>"
            )
        })
        .into_owned()
}

/// A heading's `id`: its words, lower case, joined by `-`.
fn slug(html: &str) -> String {
    let tags = Regex::new(r"<[^>]*>").unwrap();
    let text = tags
        .replace_all(html, "")
        .replace("&amp;", "and")
        .replace("&quot;", "")
        .replace("&lt;", "")
        .replace("&gt;", "")
        .replace("&#39;", "")
        .to_lowercase();
    let mut out = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if (c.is_whitespace() || c == '-' || c == '_') && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// `& < > "` escaped, for HTML.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A page's address on the web: a directory's index by the directory.
fn url(path: &str) -> String {
    let path = path.strip_suffix("index.html").unwrap_or(path);
    format!("{HOMEPAGE}/{path}")
}

/// A page as Markdown, for a reader that prefers it, at its address with
/// `.md` for `.html`: its own Markdown under its title, with the site's
/// links made absolute, or for a page written from the binary, its text as
/// the terminal shows it. The pages and `llms.txt` promise one for every
/// page.
fn twin(page: &Page, pages: &[Page]) -> String {
    let body = match (&page.markdown, &page.text) {
        (Some(markdown), _) => fill(markdown).replace("](/", &format!("]({HOMEPAGE}/")),
        (None, Some(text)) => {
            // A fence longer than any run of backticks in the text.
            let run = Regex::new("`+")
                .unwrap()
                .find_iter(text)
                .map(|m| m.len())
                .max()
                .unwrap_or(0);
            let fence = "`".repeat(run.max(2) + 1);
            format!("{fence}text\n{text}{fence}\n")
        }
        (None, None) => panic!("{}: neither Markdown nor text", page.path),
    };
    let mut out = format!("# {}\n\n{}\n\n{body}", page.title, page.description);
    if let Some(section) = page.section.filter(|_| page.is_index()) {
        out.push('\n');
        for member in members(pages, section) {
            out.push_str(&format!(
                "- [{}]({}): {}\n",
                member.title,
                url(&member.path),
                member.description
            ));
        }
    }
    out
}

/// `llms.txt`: what the site is, and every page, for a language model.
fn llms(pages: &[Page]) -> String {
    let mut out = format!(
        "# aless\n\n> {}.\n\nThe documentation of aless {VERSION}, in Diátaxis's four \
         kinds. Every page has a Markdown version at its address with .md for .html, \
         and the reference is also plain text, as aless --help and F1 print it.\n",
        aless::cli::TAGLINE
    );
    for (i, section) in SECTIONS.iter().enumerate() {
        out.push_str(&format!("\n## {}\n\n", section.label));
        for page in members(pages, i) {
            let address = url(&page.path.replace(".html", ".md"));
            out.push_str(&format!(
                "- [{}]({address}): {}\n",
                page.title, page.description
            ));
        }
    }
    out.push_str(&format!(
        "\n## Optional\n\n- [Agent Skill]({}): the skill that teaches an agent to drive \
         aless, which aless --generate skill also prints\n- [Source]({REPOSITORY}): the \
         code, the changelog and the releases\n",
        url("SKILL.md")
    ));
    out
}

/// `sitemap.xml`: the address of every page but the 404 page.
fn sitemap(pages: &[Page]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for page in pages.iter().filter(|p| p.path != "404.html") {
        out.push_str(&format!("<url><loc>{}</loc></url>\n", url(&page.path)));
    }
    out.push_str("</urlset>\n");
    out
}

/// A fenced block of a page's Markdown: its info string and its lines.
struct Fence {
    info: String,
    lines: Vec<String>,
}

/// The fenced code blocks of some Markdown, and its text outside them.
fn fences(markdown: &str) -> (Vec<Fence>, String) {
    let mut blocks = Vec::new();
    let mut prose = String::new();
    let mut open: Option<(String, Fence)> = None;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if let Some((marker, fence)) = open.as_mut() {
            let closes = trimmed.starts_with(marker.as_str())
                && trimmed.trim_start_matches('`').trim().is_empty();
            if !closes {
                fence.lines.push(line.to_string());
                continue;
            }
            if let Some((_, fence)) = open.take() {
                blocks.push(fence);
            }
            prose.push('\n');
        } else if trimmed.starts_with("```") {
            let ticks = trimmed.len() - trimmed.trim_start_matches('`').len();
            let info = trimmed[ticks..].trim().to_string();
            let fence = Fence {
                info,
                lines: Vec::new(),
            };
            open = Some(("`".repeat(ticks), fence));
            prose.push('\n');
        } else {
            prose.push_str(line);
            prose.push('\n');
        }
    }
    assert!(open.is_none(), "a code block does not close");
    (blocks, prose)
}

/// The site builds; every link in it reaches a file the site publishes,
/// and every `#fragment` an `id` on its page; and the domain in `CNAME` is
/// the homepage's. With `ALESS_SITE_OUT` set, the site is written there.
#[test]
fn the_site_builds_and_its_links_hold() {
    let site = site();
    let href = Regex::new(r#"(?:href|src)="([^"]*)""#).unwrap();
    let mut broken = Vec::new();
    for page in &site.pages {
        let html = String::from_utf8_lossy(&site.files[&page.path]).to_string();
        assert!(
            !html.contains("{{"),
            "{}: a value is not filled in",
            page.path
        );
        let dir = match page.path.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/"),
            None => String::new(),
        };
        for c in href.captures_iter(&html) {
            let link = c[1].replace("&amp;", "&");
            if ["https://", "http://", "mailto:"]
                .iter()
                .any(|s| link.starts_with(s))
            {
                continue;
            }
            let (path, fragment) = link.split_once('#').unwrap_or((&link, ""));
            let target = if path.is_empty() {
                page.path.clone()
            } else if let Some(absolute) = path.strip_prefix('/') {
                absolute.to_string()
            } else {
                resolve(&dir, path)
            };
            let Some(file) = site.files.get(&target) else {
                broken.push(format!("{}: {link} (no {target})", page.path));
                continue;
            };
            if !fragment.is_empty()
                && !String::from_utf8_lossy(file).contains(&format!("id=\"{fragment}\""))
            {
                broken.push(format!(
                    "{}: {link} (no #{fragment} in {target})",
                    page.path
                ));
            }
        }
        let twin = page.path.replace(".html", ".md");
        assert!(site.files.contains_key(&twin), "{}: no {twin}", page.path);
        if page.markdown.is_some() && page.path != "index.html" {
            assert!(
                !page.body.contains("<h1"),
                "{}: the layout writes the page's one h1; start at ##",
                page.origin
            );
        }
    }
    assert!(
        broken.is_empty(),
        "links that reach nothing:\n{}",
        broken.join("\n")
    );
    let cname = String::from_utf8_lossy(&site.files["CNAME"])
        .trim()
        .to_string();
    assert_eq!(
        format!("https://{cname}"),
        HOMEPAGE,
        "site/CNAME names the domain Cargo.toml's homepage is served at"
    );
    if let Some(out) = std::env::var_os("ALESS_SITE_OUT") {
        let out = PathBuf::from(out);
        for (path, bytes) in &site.files {
            let file = out.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, bytes).unwrap();
        }
        println!(
            "site: wrote {} files to {}",
            site.files.len(),
            out.display()
        );
    }
}

/// `dir` + `path`, with `..` and `.` taken out.
fn resolve(dir: &str, path: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for part in path.split('/') {
        match part {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            part => parts.push(part),
        }
    }
    parts.join("/")
}

/// The reference's pages hold every option and every line of the help F1
/// shows: they are written from the binary, not by hand.
#[test]
fn the_reference_is_the_binarys() {
    let site = site();
    let page = |path: &str| String::from_utf8_lossy(&site.files[path]).to_string();
    let command_line = page("reference/command-line.html");
    for opt in aless::cli::OPTIONS {
        let id = format!("id=\"{}\"", aless::cli::option_id(opt));
        assert!(command_line.contains(&id), "{}", opt.long);
    }
    let keys = page("reference/keys.html");
    for line in aless::app::help_lines().into_iter().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let shown = escape(line);
        let heading = sentence_case(line.split("  (").next().unwrap_or(line));
        assert!(
            keys.contains(&shown) || keys.contains(&escape(&heading)),
            "the keys page leaves out {line:?}"
        );
    }
}

/// Every command line on the pages names options aless takes.
#[test]
fn every_example_names_options_aless_takes() {
    let quoted = Regex::new(r#"'[^']*'|"[^"]*""#).unwrap();
    let mut unknown = Vec::new();
    for page in &site().pages {
        let Some(markdown) = &page.markdown else {
            continue;
        };
        let (blocks, _) = fences(markdown);
        for block in blocks {
            if !["", "console", "bash", "sh", "shell", "powershell"].contains(&block.info.as_str())
            {
                continue;
            }
            for line in &block.lines {
                let line = quoted.replace_all(line.trim_start_matches("$ "), "''");
                let mut words = line.split_whitespace().skip_while(|w| *w != "aless");
                if words.next().is_none() {
                    continue;
                }
                for word in words.take_while(|w| !["|", ">", "<", ";", "&&", "||", "#"].contains(w))
                {
                    let name = word.split('=').next().unwrap_or(word);
                    if name.starts_with('-')
                        && name.len() > 1
                        && name != "--"
                        && aless::cli::lookup(name).is_none()
                    {
                        unknown.push(format!("{}: {name} in {line}", page.origin));
                    }
                }
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "options aless does not take:\n{}",
        unknown.join("\n")
    );
}

/// Every `console` example prints what its page says it prints: each
/// `$ ` line is run by `sh` in a directory holding the files the site
/// publishes under `examples/`, as a reader who downloaded them would run
/// it, with the aless this test was built with first on the PATH; what it
/// writes to standard output and standard error together is held to the
/// lines after it.
#[cfg(unix)]
#[test]
fn the_console_examples_print_what_the_pages_say() {
    let bin = Path::new(env!("CARGO_BIN_EXE_aless")).parent().unwrap();
    let work = std::env::temp_dir().join(format!("aless-site-examples-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    for (path, bytes) in &site().files {
        if let Some(name) = path.strip_prefix("examples/") {
            std::fs::write(work.join(name), bytes).unwrap();
        }
    }
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut wrong = Vec::new();
    let mut ran = 0;
    for page in &site().pages {
        let Some(markdown) = &page.markdown else {
            continue;
        };
        let (blocks, _) = fences(&fill(markdown));
        for block in blocks.iter().filter(|b| b.info == "console") {
            let mut runs: Vec<(String, Vec<String>)> = Vec::new();
            for line in &block.lines {
                match line.strip_prefix("$ ") {
                    Some(command) => runs.push((command.to_string(), Vec::new())),
                    None => runs
                        .last_mut()
                        .unwrap_or_else(|| {
                            panic!("{}: a console block starts with output", page.origin)
                        })
                        .1
                        .push(line.clone()),
                }
            }
            let mut script = String::new();
            for (i, (command, _)) in runs.iter().enumerate() {
                script.push_str(&format!(
                    "printf '%s\\n' '@@example {i}@@'\n{{ {command}\n}} 2>&1\n"
                ));
            }
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(&script)
                .current_dir(&work)
                .env("PATH", &path)
                .env_remove("NO_COLOR")
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&out.stdout);
            for (i, (command, expected)) in runs.iter().enumerate() {
                ran += 1;
                let start = format!("@@example {i}@@\n");
                let end = format!("@@example {}@@\n", i + 1);
                let got = text
                    .split_once(&start)
                    .map(|(_, rest)| rest.split(&end).next().unwrap_or(""))
                    .unwrap_or("");
                let tidy = |s: &str| {
                    s.lines()
                        .map(str::trim_end)
                        .collect::<Vec<_>>()
                        .join("\n")
                        .trim_end()
                        .to_string()
                };
                if tidy(got) != tidy(&expected.join("\n")) {
                    wrong.push(format!(
                        "{}: $ {command}\n--- the page says:\n{}\n--- aless printed:\n{}",
                        page.origin,
                        tidy(&expected.join("\n")),
                        tidy(got)
                    ));
                }
            }
        }
    }
    assert!(ran > 0, "no console examples ran");
    println!(
        "site: ran {ran} console examples, {} printed something else",
        wrong.len()
    );
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n\n"));
}

/// The house style's rules for prose, on every page written by hand: no
/// phrase `site/reject.txt` bans, no em dash, no emoji; `we` in tutorials
/// alone, `I` nowhere; and an exclamation mark in a tutorial, once at most.
/// Code, quoted output, link targets and markup are not prose.
/// `site/reject.txt` is a copy of the tabnas house style's list, which
/// tabnas/parser keeps for Vale in
/// `.vale/styles/config/vocabularies/Tabnas/reject.txt`.
#[test]
fn the_pages_keep_the_house_style() {
    let patterns: Vec<(Regex, String)> = read(&sources().join("reject.txt"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|src| {
            assert!(
                !src.starts_with('#'),
                "reject.txt holds patterns, not comments"
            );
            let re = Regex::new(&format!(r"(?i)\b(?:{src})\b")).unwrap();
            (re, src.to_string())
        })
        .collect();
    assert!(patterns.len() > 50, "reject.txt loaded too few patterns");
    let code = Regex::new(r"`[^`]*`").unwrap();
    let markup = Regex::new(r"<[^>]*>|\]\([^)]*\)").unwrap();
    let emoji = Regex::new(r"\p{Emoji_Presentation}").unwrap();
    let i = Regex::new(r"\bI\b").unwrap();
    let we = Regex::new(r"(?i)\b(?:we|we['’](?:ll|re|ve|d))\b").unwrap();
    let mut faults = Vec::new();
    for page in &site().pages {
        let Some(markdown) = &page.markdown else {
            continue;
        };
        let (_, prose) = fences(markdown);
        let prose = markup
            .replace_all(&code.replace_all(&prose, "code"), " ")
            .to_string();
        // A paragraph is matched whole, so a phrase split across lines is found.
        let paragraphs: Vec<String> = prose
            .split("\n\n")
            .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect();
        let tutorial = page.section == Some(TUTORIALS);
        let kind = page.section.is_some();
        for paragraph in &paragraphs {
            for (re, src) in &patterns {
                if let Some(m) = re.find(paragraph) {
                    faults.push(format!("{}: {:?} (banned: {src})", page.origin, m.as_str()));
                }
            }
            if paragraph.contains('—') {
                faults.push(format!("{}: an em dash in {paragraph:?}", page.origin));
            }
            if let Some(m) = emoji.find(paragraph) {
                faults.push(format!("{}: emoji {:?}", page.origin, m.as_str()));
            }
            if i.is_match(paragraph) {
                faults.push(format!("{}: \"I\" in {paragraph:?}", page.origin));
            }
            if kind && !tutorial && we.is_match(paragraph) {
                faults.push(format!(
                    "{}: \"we\" outside a tutorial: {paragraph:?}",
                    page.origin
                ));
            }
        }
        let bangs = paragraphs
            .iter()
            .map(|p| p.matches('!').count())
            .sum::<usize>();
        if kind && (bangs > 1 || (bangs == 1 && !tutorial)) {
            faults.push(format!("{}: {bangs} exclamation marks", page.origin));
        }
    }
    assert!(faults.is_empty(), "\n{}", faults.join("\n"));
}
