//! The documentation site, built from `docs/` and checked: links and
//! anchors, the options each example names, and each console example's
//! output. `ALESS_SITE_OUT=DIR` writes it, which `.github/workflows/pages.yml`
//! publishes; `docs/contributing/site.md` says how the build works.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

const HOMEPAGE: &str = env!("CARGO_PKG_HOMEPAGE");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A directory of pages: a Diátaxis kind, or a person's own writing.
struct Section {
    dir: &'static str,
    /// The navigation's name for the section.
    label: &'static str,
    /// The groups the section lists its pages under, in order; none for
    /// a section listed flat. A page of a grouped section names its group
    /// in its front matter.
    groups: &'static [Group],
    /// Written by a person, in their own words: each page names its
    /// `author`, and the style guide's gates leave the section out
    /// (`the_gates_read_every_page_but_the_human_ones`).
    human: bool,
}

/// A heading in a section's list: the pages under it share a situation.
struct Group {
    name: &'static str,
    /// One sentence under the heading, on the list page and in llms.txt.
    blurb: &'static str,
}

/// The sections, in the order the navigation lists them.
const SECTIONS: [Section; 5] = [
    Section {
        dir: "tutorials",
        label: "Tutorials",
        groups: &[],
        human: false,
    },
    Section {
        dir: "how-to",
        label: "How-to guides",
        groups: &[
            Group {
                name: "Setting up",
                blurb: "aless on your machine, with its completions and its manual page, and \
                        what each platform needs of a terminal.",
            },
            Group {
                name: "Reading a document",
                blurb: "A document open in the viewer: finding things in it, copying from it, \
                        how it is drawn, and writing it out as JSON.",
            },
            Group {
                name: "Files and formats",
                blurb: "What is open, and how it is read: a file that changes, several files, \
                        a directory, the format aless reads, a file that does not parse, and \
                        a format of your own.",
            },
        ],
        human: false,
    },
    Section {
        dir: "reference",
        label: "Reference",
        groups: &[],
        human: false,
    },
    Section {
        dir: "explanation",
        label: "Explanation",
        groups: &[],
        human: false,
    },
    Section {
        dir: "why",
        label: "Why",
        groups: &[],
        human: true,
    },
];

/// Test fixtures the pages' examples read as well, published beside
/// `examples/` for a reader to download, from where they are kept.
const FIXTURES: &[&str] = &[
    "tests/fixtures/grammars/hosts.abnf",
    "tests/fixtures/grammars/hosts",
];

const REFERENCE: usize = 2;

/// A page of the site.
struct Page {
    /// Where it is published: `how-to/search.html`.
    path: String,
    title: String,
    /// One sentence: the page's meta description, and its line in lists.
    description: String,
    /// Its place in its section's list; 0 for a page in none.
    order: i64,
    /// Its section; `None` for the home page and the 404 page.
    section: Option<usize>,
    /// Its group in its section's list, an index into the section's
    /// `groups`; `None` in a section listed flat, and for a page in no
    /// list.
    group: Option<usize>,
    /// Who wrote it, for a page of a human-written section; `None` for
    /// every other page.
    author: Option<String>,
    /// The Markdown it was written in, without its front matter; `None`
    /// for a page written from the binary.
    markdown: Option<String>,
    /// A page written from the binary, as the terminal shows it: what
    /// `aless --help` or F1 prints. Published beside the page as `.txt`.
    text: Option<String>,
    /// The file it comes from, for the footer's link to it.
    origin: String,
    /// The page's content as HTML, every heading given an id.
    body: String,
    /// The page's own h2 and h3 headings, for the list of its contents.
    headings: Vec<Heading>,
}

/// A heading of a page: its level, its id, and its text without markup.
struct Heading {
    level: u8,
    id: String,
    text: String,
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
    root().join("docs")
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
    pages.push(releases_page());
    list_sections(&mut pages);
    for page in &mut pages {
        let (body, headings) = anchor_headings(&page.body);
        page.body = wrap_tables(&body);
        page.headings = headings;
    }
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
    for name in ["style.css", "favicon.svg", "logo.svg", "CNAME"] {
        let bytes = std::fs::read(sources().join(name)).unwrap();
        files.insert(name.to_string(), bytes);
    }
    // The logo for each theme, the stylesheet showing one: an SVG shown
    // through <img> may follow the system's colour scheme rather than the
    // header's toggle, so neither leaves its wordmark to a media query.
    // logo.svg keeps the query, for a page that shows it on its own.
    let logo = read(&sources().join("logo.svg"));
    let rule = "@media (prefers-color-scheme: dark) { #aless-wordmark { fill: #f1e8cd } }";
    assert!(
        logo.contains(rule),
        "docs/logo.svg lacks the wordmark's dark rule the build rewrites"
    );
    files.insert("logo-light.svg".into(), logo.replace(rule, "").into_bytes());
    files.insert(
        "logo-dark.svg".into(),
        logo.replace(rule, "#aless-wordmark { fill: #f1e8cd }")
            .into_bytes(),
    );
    // The faces, served from the site itself: a page requests nothing
    // anywhere else.
    for entry in std::fs::read_dir(sources().join("fonts")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        files.insert(format!("fonts/{name}"), std::fs::read(&path).unwrap());
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
    files.insert("llms.txt".into(), llms(&pages).into_bytes());
    files.insert("sitemap.xml".into(), sitemap(&pages).into_bytes());
    Site { pages, files }
}

/// Every page written in Markdown: `docs/index.md`, `docs/404.md`, and
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
                "" => format!("docs/{name}"),
                dir => format!("docs/{dir}/{name}"),
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
            // `group` places a listed page under one of its section's
            // groups, which a grouped section requires and a flat one
            // refuses.
            let group = match (listed, meta.get("group")) {
                (true, Some(name)) => {
                    let section = &SECTIONS[section.unwrap()];
                    Some(
                        section
                            .groups
                            .iter()
                            .position(|g| g.name == name)
                            .unwrap_or_else(|| {
                                panic!(
                                    "{origin}: group {name:?} is not a group of {}",
                                    section.label
                                )
                            }),
                    )
                }
                (true, None) => {
                    let section = &SECTIONS[section.unwrap()];
                    assert!(
                        section.groups.is_empty(),
                        "{origin}: no group in its front matter, and {} lists its pages by \
                         group",
                        section.label
                    );
                    None
                }
                (false, Some(_)) => panic!(
                    "{origin}: group places a page under a heading of its section's list, \
                     and this page is in no list"
                ),
                (false, None) => None,
            };
            let author = meta.get("author").cloned();
            let human = section.is_some_and(|s| SECTIONS[s].human);
            assert_eq!(
                author.is_some(),
                human,
                "{origin}: a page of a human-written section names its author, and no \
                 other page does"
            );
            pages.push(Page {
                path,
                title: field("title"),
                description: field("description"),
                order,
                section,
                group,
                author,
                body: markdown_html(&markdown),
                markdown: Some(markdown),
                text: None,
                origin,
                headings: Vec::new(),
            });
        }
    }
    for (i, section) in SECTIONS.iter().enumerate() {
        let index = format!("{}/index.html", section.dir);
        assert!(
            pages
                .iter()
                .any(|p| p.path == index && p.section == Some(i)),
            "docs/{}/index.md is missing",
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
            ["title", "description", "order", "group", "author"].contains(&key),
            "{origin}: unknown front matter key {key}"
        );
        meta.insert(key.to_string(), value.trim().to_string());
    }
    (meta, body.trim_start().to_string())
}

/// Markdown as HTML, after the values a page may name are filled in and
/// the capture comments are taken out.
fn markdown_html(markdown: &str) -> String {
    let markdown = strip_captures(&fill(markdown));
    screens(&tabnas_markdown::to_html(
        &markdown,
        &tabnas_markdown::DEFAULT_OPTIONS,
    ))
}

/// Some Markdown without the comments the tests read: `<!-- screen: ...
/// -->` before a `screen` block, which names the capture it is drawn from,
/// and `<!-- test: skip REASON -->` before an example no test runs. Neither
/// is any part of the page.
fn strip_captures(markdown: &str) -> String {
    let comment = Regex::new(r"(?ms)^<!-- (?:screen|test):.*?-->\n").unwrap();
    comment.replace_all(markdown, "").into_owned()
}

/// A line of Markdown as HTML, without the paragraph around it.
fn inline_html(markdown: &str) -> String {
    let html = tabnas_markdown::to_html(markdown, &tabnas_markdown::DEFAULT_OPTIONS);
    let html = html.trim();
    html.strip_prefix("<p>")
        .and_then(|h| h.strip_suffix("</p>"))
        .unwrap_or(html)
        .to_string()
}

/// Each table in a box of its own that scrolls sideways when the table is
/// wider than the page, so that the table itself stays a table.
fn wrap_tables(html: &str) -> String {
    let table = Regex::new(r"(?s)<table[ >].*?</table>").unwrap();
    table
        .replace_all(html, |c: &regex::Captures| {
            format!("<div class=\"table-wrap\">{}</div>", &c[0])
        })
        .into_owned()
}

/// Each `screen` block, a capture of the viewer, marked for the
/// stylesheet, with its status bar marked: the row that ends in its line
/// and column, or in ` · watching` where the focused node has none. The
/// stylesheet sizes the type to the box around it, and a block too wide
/// to fit scrolls, and takes the focus so a keyboard can scroll it.
fn screens(html: &str) -> String {
    let block = Regex::new(r#"(?s)<pre><code class="language-screen">(.*?)</code></pre>"#).unwrap();
    let status = Regex::new(r"^(?: \S|…).* · (?:\d+:\d+|watching)$").unwrap();
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
                "<div class=\"screen-box\"><pre class=\"screen\" tabindex=\"0\">\
                 <code class=\"language-screen\">{}</code></pre></div>",
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

/// The reference's pages written from the binary's own tables.
fn reference_pages() -> Vec<Page> {
    let command_line = format!(
        "<p>The reference <code>aless --help</code> prints, for aless {VERSION}. \
         <code>aless -h</code> prints a summary of the options, and \
         <code>man aless</code> this reference as a manual page. The same text is \
         <a href=\"/reference/command-line.txt\">plain text</a> here, as the \
         terminal shows it.</p>\n{}",
        command_line_html(&aless::cli::reference_html())
    );
    vec![
        Page {
            path: "reference/command-line.html".into(),
            title: "Command line".into(),
            description: "Every option, the formats, the keys used most, the limits and the \
                exit statuses: the whole reference aless --help prints."
                .into(),
            order: 1,
            section: Some(REFERENCE),
            group: None,
            author: None,
            markdown: None,
            text: Some(aless::cli::reference()),
            origin: "src/cli.rs".into(),
            body: command_line,
            headings: Vec::new(),
        },
        Page {
            path: "reference/keys.html".into(),
            title: "Keys".into(),
            description: "Every key, command and setting of the viewer, as F1 lists them \
                inside aless."
                .into(),
            order: 2,
            section: Some(REFERENCE),
            group: None,
            author: None,
            body: keys_html(),
            markdown: Some(keys_markdown()),
            text: Some(aless::app::help_lines().join("\n") + "\n"),
            origin: "src/app.rs".into(),
            headings: Vec::new(),
        },
    ]
}

/// The releases, written from `CHANGELOG.md`: every release and what
/// changed in it, from the first release heading on. The file's own
/// introduction, which describes the repository, stays in the repository.
/// A release's heading is its version, linked to its release, and its
/// date, and its `id` is the version after a `v`.
fn releases_page() -> Page {
    let changelog = read(&root().join("CHANGELOG.md")).replace("\r\n", "\n");
    let start = changelog
        .find("\n## ")
        .expect("CHANGELOG.md has no release heading");
    // The changelog's own links for its versions, where it gives one; a
    // release's tag on GitHub where it does not.
    let definition = Regex::new(r"(?m)^\[(\d+\.\d+\.\d+)\]: (\S+)$").unwrap();
    let links: BTreeMap<String, String> = definition
        .captures_iter(&changelog)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();
    let release = Regex::new(r"(?m)^## \[(\d+\.\d+\.\d+)\] - (\S+)$").unwrap();
    let body = release.replace_all(&changelog[start + 1..], |c: &regex::Captures| {
        let link = links
            .get(&c[1])
            .cloned()
            .unwrap_or_else(|| format!("{REPOSITORY}/releases/tag/v{}", &c[1]));
        format!("## [{}]({link}), {}", &c[1], &c[2])
    });
    let body = body.replace("\n## [Unreleased]\n", "\n## Unreleased, on main\n");
    let body = match body.strip_prefix("## [Unreleased]\n") {
        Some(rest) => format!("## Unreleased, on main\n{rest}"),
        None => body,
    };
    let markdown = format!(
        "The releases of aless, newest first, each with what it added, changed and \
         fixed. Every release is on [GitHub](https://github.com/rjrodger/aless/releases), \
         with its archives and installers; [Install aless](/how-to/install.html) says how \
         to get the newest.\n\n{body}"
    );
    let heading = Regex::new(r#"<h2><a href="[^"]*">(\d+\.\d+\.\d+)</a>"#).unwrap();
    let html = heading
        .replace_all(&markdown_html(&markdown), |c: &regex::Captures| {
            c[0].replacen("<h2>", &format!("<h2 id=\"v{}\">", &c[1]), 1)
        })
        .replace(
            "<h2>Unreleased, on main</h2>",
            "<h2 id=\"unreleased\">Unreleased, on main</h2>",
        );
    Page {
        path: "reference/releases.html".into(),
        title: "Releases".into(),
        description: "Every release of aless and what changed in it, newest first, from \
            the repository's changelog."
            .into(),
        order: 4,
        section: Some(REFERENCE),
        group: None,
        author: None,
        body: html,
        markdown: Some(markdown),
        text: None,
        origin: "CHANGELOG.md".into(),
        headings: Vec::new(),
    }
}

/// The command line's reference as `aless::cli::reference_html` writes
/// it, made to read as a page: the examples as a table of commands and
/// what each does, a see-also entry that is a phrase rather than a name in
/// plain text, and each option a link to itself.
fn command_line_html(html: &str) -> String {
    // The examples are a command and what it does, a run of spaces apart;
    // a command too long for its column has what it does on the next line.
    let examples =
        Regex::new(r#"(?s)(<h2 id="examples">.*?</h2>\n)<pre><code>(.*?)</code></pre>"#).unwrap();
    let html = examples.replace(html, |c: &regex::Captures| {
        let mut rows: Vec<(String, String)> = Vec::new();
        for line in c[2].lines() {
            match (line.starts_with(' '), rows.last_mut()) {
                (true, Some((_, does))) => {
                    if !does.is_empty() {
                        does.push(' ');
                    }
                    does.push_str(line.trim());
                }
                _ => {
                    let (command, does) = line.split_once("  ").unwrap_or((line, ""));
                    rows.push((command.trim().to_string(), does.trim().to_string()));
                }
            }
        }
        let rows: String = rows
            .iter()
            .map(|(command, does)| {
                format!("<tr><td><code>{command}</code></td><td>{does}</td></tr>\n")
            })
            .collect();
        format!(
            "{}<table class=\"examples\">\n<thead><tr><th>Command</th><th>Does</th></tr></thead>\n\
             <tbody>\n{rows}</tbody>\n</table>",
            &c[1]
        )
    });
    // A see-also entry named in words, "the documentation", is not code.
    let html = match html.split_once("<h2 id=\"see-also\">") {
        Some((head, tail)) => {
            let phrase = Regex::new(r"<dt><code>([a-z]+(?: [a-z]+)+)</code></dt>").unwrap();
            format!(
                "{head}<h2 id=\"see-also\">{}",
                phrase.replace_all(tail, "<dt>$1</dt>")
            )
        }
        None => html.into_owned(),
    };
    // Every option's term links to itself, as a heading does.
    let term = Regex::new(r#"<dt id="(opt-[^"]+)">(.*?)</dt>"#).unwrap();
    let tags = Regex::new(r"<[^>]*>").unwrap();
    term.replace_all(&html, |c: &regex::Captures| {
        format!(
            "<dt id=\"{id}\">{term} <a class=\"heading-anchor\" href=\"#{id}\" \
             aria-label=\"Link to {label}\">#</a></dt>",
            id = &c[1],
            term = &c[2],
            label = tags.replace_all(&c[2], "").replace('"', "&quot;")
        )
    })
    .into_owned()
}

/// The keys page: [`aless::app::KEY_MAP`], a section a group, its
/// bindings in a table with a row a binding, each row's `id` named for its
/// first key (`key-j`, `key-slash`), command (`cmd-open`) or setting
/// (`set-number`). A key is drawn as a key, and a command or a setting,
/// which is typed, as code.
fn keys_html() -> String {
    let mut out = String::from(
        "<p>Press <kbd>F1</kbd> or type <code>:help</code> inside aless for this list. The \
         keys are jless's wherever jless has the key, and the same list is \
         <a href=\"/reference/keys.txt\">plain text</a> here, as the terminal shows it.</p>\n",
    );
    let mut ids = BTreeSet::new();
    for group in aless::app::KEY_MAP {
        out.push_str(&format!("<h2>{}</h2>\n", escape(group.title)));
        if !group.note.is_empty() {
            out.push_str(&format!("<p>{}</p>\n", inline_html(group.note)));
        }
        out.push_str(
            "<table class=\"keys\">\n<thead><tr><th>Keys</th><th>Does</th></tr></thead>\n<tbody>\n",
        );
        for (keys, does) in group.keys {
            let shown: Vec<String> = keys
                .split("  ")
                .map(|alt| match group.typed {
                    true => format!("<code>{}</code>", escape(alt)),
                    false => alt
                        .split(' ')
                        .map(|k| format!("<kbd>{}</kbd>", escape(k)))
                        .collect::<Vec<_>>()
                        .join(" "),
                })
                .collect();
            let base = binding_id(keys, group.typed);
            let (mut id, mut n) = (base.clone(), 2);
            while !ids.insert(id.clone()) {
                id = format!("{base}-{n}");
                n += 1;
            }
            out.push_str(&format!(
                "<tr id=\"{id}\"><td>{}</td><td>{}</td></tr>\n",
                shown.join(", "),
                inline_html(does)
            ));
        }
        out.push_str("</tbody>\n</table>\n");
    }
    out
}

/// A binding's `id` on the keys page, from its first key or word, a symbol
/// spelled out: `key-j`, `key-C-d`, `key-slash`, `cmd-open`, `cmd-w-bang`,
/// `set-so`.
fn binding_id(keys: &str, typed: bool) -> String {
    let first = keys.split_whitespace().next().unwrap_or("");
    let prefix = match (typed, first.starts_with(':')) {
        (false, _) => "key",
        (true, true) => "cmd",
        (true, false) => "set",
    };
    let first = match typed {
        true => first
            .trim_start_matches(':')
            .split('=')
            .next()
            .unwrap_or(""),
        false => first,
    };
    let mut name = String::new();
    for c in first.chars() {
        let word = match c {
            c if c.is_ascii_alphanumeric() => {
                name.push(c);
                continue;
            }
            '-' if !name.is_empty() => {
                name.push('-');
                continue;
            }
            '/' => "slash",
            '?' => "question",
            '*' => "star",
            '#' => "hash",
            '.' => "dot",
            ',' => "comma",
            ';' => "semicolon",
            ':' => "colon",
            '<' => "lt",
            '>' => "gt",
            '%' => "percent",
            '!' => "bang",
            '$' => "dollar",
            '^' => "caret",
            '-' => "minus",
            '↓' => "down",
            '↑' => "up",
            '←' => "left",
            '→' => "right",
            _ => continue,
        };
        if !name.is_empty() && !name.ends_with('-') {
            name.push('-');
        }
        name.push_str(word);
    }
    format!("{prefix}-{name}")
}

/// The keys page, as Markdown: [`aless::app::KEY_MAP`], a section a
/// group, its bindings in a table.
fn keys_markdown() -> String {
    let mut out = String::from(
        "Press `F1` or type `:help` inside aless for this list. The keys are jless's \
         wherever jless has the key, and the same list is \
         [plain text](/reference/keys.txt) here, as the terminal shows it.\n",
    );
    for group in aless::app::KEY_MAP {
        out.push_str(&format!("\n## {}\n\n", group.title));
        if !group.note.is_empty() {
            out.push_str(&format!("{}\n\n", group.note));
        }
        out.push_str("| Keys | Does |\n|---|---|\n");
        for (keys, does) in group.keys {
            let shown: Vec<String> = keys
                .split("  ")
                .map(|alt| match group.typed {
                    true => format!("`{alt}`"),
                    false => alt
                        .split(' ')
                        .map(|k| format!("`{k}`"))
                        .collect::<Vec<_>>()
                        .join(" "),
                })
                .collect();
            out.push_str(&format!(
                "| {} | {} |\n",
                shown.join(", ").replace('|', "\\|"),
                does.replace('|', "\\|")
            ));
        }
    }
    out
}

/// Each section's index page, with the list of its pages appended: one
/// row a page, under the section's group headings where it has them.
fn list_sections(pages: &mut [Page]) {
    for i in 0..SECTIONS.len() {
        let list = list_html(pages, i);
        let index = pages
            .iter_mut()
            .find(|p| p.section == Some(i) && p.is_index())
            .unwrap();
        index.body.push_str(&list);
    }
}

/// A section's pages as a list of rows, grouped where the section is;
/// nothing for a section whose one page is its index.
fn list_html(pages: &[Page], section: usize) -> String {
    let groups = SECTIONS[section].groups;
    let members = members(pages, section);
    let mut out = String::new();
    if members.is_empty() {
        return out;
    }
    if groups.is_empty() {
        out.push_str("<ul class=\"list\">\n");
        for &page in &members {
            out.push_str(&row(page));
        }
        out.push_str("</ul>\n");
    }
    for (g, group) in groups.iter().enumerate() {
        out.push_str(&format!(
            "<h2>{}</h2>\n<p class=\"group-blurb\">{}</p>\n<ul class=\"list\">\n",
            escape(group.name),
            escape(group.blurb)
        ));
        for &page in members.iter().filter(|p| p.group == Some(g)) {
            out.push_str(&row(page));
        }
        out.push_str("</ul>\n");
    }
    out
}

/// A page's row in its section's list: its title and its description.
fn row(page: &Page) -> String {
    format!(
        "<li><a class=\"row\" href=\"/{}\"><span class=\"row-title\">{}</span>\
         <span class=\"row-desc\">{}</span></a></li>\n",
        page.path,
        escape(&page.title),
        escape(&page.description)
    )
}

/// The pages of a section, in their order: by group, then by `order`.
fn members(pages: &[Page], section: usize) -> Vec<&Page> {
    let mut members: Vec<&Page> = pages
        .iter()
        .filter(|p| p.section == Some(section) && !p.is_index())
        .collect();
    members.sort_by(|a, b| (a.group, a.order, &a.title).cmp(&(b.group, b.order, &b.title)));
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
    // A person's own page names its author under its title, where any
    // other page has its description.
    let head = match (page.section, &page.author) {
        (Some(_), Some(author)) => format!(
            "<h1>{}</h1>\n<p class=\"lede\">By {}</p>\n",
            escape(&page.title),
            escape(author)
        ),
        (Some(_), None) => format!(
            "<h1>{}</h1>\n<p class=\"lede\">{}</p>\n",
            escape(&page.title),
            escape(&page.description)
        ),
        (None, _) if home => String::new(),
        (None, _) => format!("<h1>{}</h1>\n", escape(&page.title)),
    };
    let source = format!("{REPOSITORY}/blob/main/{}", page.origin);
    let (nav, toc, toc_inline, pager, describes) = match page.section {
        Some(_) => {
            let items = toc_items(page);
            // A page written in Markdown here can be edited; one written
            // from the binary or the changelog names what it comes from.
            let edit = match page.origin.starts_with("docs/") {
                true => format!("<a href=\"{source}\">Edit this page</a>"),
                false => format!("Written from <a href=\"{source}\">{}</a>", page.origin),
            };
            (
                nav(page, pages),
                items
                    .as_ref()
                    .map(|items| {
                        format!(
                            "<nav class=\"toc\" aria-label=\"On this page\">\n\
                             <p class=\"toc-head\">On this page</p>\n{items}</nav>\n"
                        )
                    })
                    .unwrap_or_default(),
                items
                    .map(|items| {
                        format!(
                            "<nav class=\"toc-inline\" aria-label=\"On this page\">\n<details>\n\
                             <summary>On this page</summary>\n{items}</details>\n</nav>\n"
                        )
                    })
                    .unwrap_or_default(),
                pager(page, pages),
                format!(
                    "<p class=\"describes\">{}{edit} · \
                     <a href=\"/{}\">This page as Markdown</a></p>\n",
                    match page.author {
                        Some(_) => String::new(),
                        None => format!("Describes aless {VERSION}. "),
                    },
                    twin_path(page)
                ),
            )
        }
        None => Default::default(),
    };
    // On the home page, the capture after the introduction, with the
    // sentence that introduces it, stands beside it where the screen is
    // wide enough.
    let body = match home {
        true => Regex::new(
            r#"(?s)(<section class="intro">.*?</section>)\s*(<p>.*?</p>\s*<div class="screen-box">.*?</div>)"#,
        )
        .unwrap()
        .replacen(
            &page.body,
            1,
            "<div class=\"hero\">\n$1\n<div class=\"hero-shot\">\n$2\n</div>\n</div>",
        )
        .into_owned(),
        false => page.body.clone(),
    };
    let head_title = if home {
        format!("aless: {}", page.title)
    } else {
        format!("{} · aless", page.title)
    };
    let body_class = match page.section {
        Some(s) => format!("doc section-{}", SECTIONS[s].dir),
        None if home => "home".to_string(),
        None => "plain".to_string(),
    };
    let mut html = layout
        .replace("{{head_title}}", &escape(&head_title))
        .replace("{{description}}", &escape(&page.description))
        .replace("{{canonical}}", &url(&page.path))
        .replace("{{twin}}", &format!("/{}", twin_path(page)))
        .replace("{{body_class}}", &body_class)
        .replace("{{sections}}", &sections(page, pages))
        .replace("{{page_head}}", &head)
        .replace("{{toc_inline}}", &toc_inline)
        .replace("{{nav}}", &nav)
        .replace("{{toc}}", &toc)
        .replace("{{pager}}", &pager)
        .replace("{{describes}}", &describes)
        .replace("{{version}}", VERSION)
        .replace("{{tagline}}", &escape(aless::cli::TAGLINE))
        .replace("{{source}}", &source)
        .replace("{{root}}", "/");
    assert!(
        !html.replace("{{content}}", "").contains("{{"),
        "layout.html names a value the build does not fill"
    );
    html = html.replace("{{content}}", &body);
    relative(&html, &root)
}

/// The header's sections, the page's own marked; under it, on a narrow
/// screen where the menu holds them, the section's pages.
fn sections(page: &Page, pages: &[Page]) -> String {
    let mut out = String::new();
    for (i, section) in SECTIONS.iter().enumerate() {
        let index = format!("{}/index.html", section.dir);
        let current = match (page.section == Some(i), page.path == index) {
            (true, true) => " aria-current=\"page\"",
            (true, false) => " aria-current=\"true\"",
            (false, _) => "",
        };
        out.push_str(&format!(
            "<a class=\"nav-link\" href=\"/{index}\"{current}>{}</a>\n",
            section.label
        ));
        if page.section != Some(i) || members(pages, i).is_empty() {
            continue;
        }
        out.push_str(&format!(
            "<ul class=\"menu-pages\" aria-label=\"{}\">\n",
            section.label
        ));
        for member in members(pages, i) {
            out.push_str(&format!(
                "<li><a href=\"/{}\"{}>{}</a></li>\n",
                member.path,
                match member.path == page.path {
                    true => " aria-current=\"page\"",
                    false => "",
                },
                escape(&member.title)
            ));
        }
        out.push_str("<li class=\"menu-all\"><a href=\"#site-pages\">Every page</a></li>\n</ul>\n");
    }
    out
}

/// The sidebar: every section and its pages, grouped as the section is,
/// this page marked.
fn nav(page: &Page, pages: &[Page]) -> String {
    let current = |path: &str| {
        if path == page.path {
            " aria-current=\"page\""
        } else {
            ""
        }
    };
    let link = |member: &Page| {
        format!(
            "<li><a class=\"side-link\" href=\"/{}\"{}>{}</a></li>\n",
            member.path,
            current(&member.path),
            escape(&member.title)
        )
    };
    let mut out =
        String::from("<nav class=\"sidebar\" id=\"site-pages\" aria-label=\"Documentation\">\n");
    for (i, section) in SECTIONS.iter().enumerate() {
        let index = format!("{}/index.html", section.dir);
        out.push_str(&format!(
            "<div class=\"group\">\n<p class=\"group-head\"><a href=\"/{index}\"{}>{}</a></p>\n",
            current(&index),
            section.label
        ));
        let members = members(pages, i);
        if section.groups.is_empty() && !members.is_empty() {
            out.push_str("<ul>\n");
            for &member in &members {
                out.push_str(&link(member));
            }
            out.push_str("</ul>\n");
        }
        for (g, group) in section.groups.iter().enumerate() {
            out.push_str(&format!(
                "<p class=\"subgroup\">{}</p>\n<ul>\n",
                escape(group.name)
            ));
            for &member in members.iter().filter(|p| p.group == Some(g)) {
                out.push_str(&link(member));
            }
            out.push_str("</ul>\n");
        }
        out.push_str("</div>\n");
    }
    out.push_str("</nav>\n");
    out
}

/// The page's own contents, when it has more than one heading: its h2 and
/// h3 headings as a list of links, or its h2 headings alone where both
/// would make a list longer than a screen.
fn toc_items(page: &Page) -> Option<String> {
    if page.headings.len() < 2 {
        return None;
    }
    let deep = page.headings.len() <= 24;
    let mut out = String::from("<ul>\n");
    for heading in page.headings.iter().filter(|h| deep || h.level == 2) {
        out.push_str(&format!(
            "<li{}><a href=\"#{}\">{}</a></li>\n",
            if heading.level == 3 {
                " class=\"sub\""
            } else {
                ""
            },
            heading.id,
            heading.text
        ));
    }
    out.push_str("</ul>\n");
    Some(out)
}

/// The previous and the next page of the section: its index first, then
/// its pages in their order.
fn pager(page: &Page, pages: &[Page]) -> String {
    let Some(section) = page.section else {
        return String::new();
    };
    let index = pages
        .iter()
        .find(|p| p.section == Some(section) && p.is_index())
        .unwrap();
    let mut sequence = vec![index];
    sequence.extend(members(pages, section));
    if sequence.len() == 1 {
        return String::new();
    }
    let at = sequence.iter().position(|p| p.path == page.path).unwrap();
    let link = |p: &Page, class: &str, label: &str| {
        format!(
            "<a class=\"pg {class}\" href=\"/{}\"><span>{label}</span>{}</a>",
            p.path,
            escape(&p.title)
        )
    };
    let prev = match at {
        0 => "<span></span>".to_string(),
        at => link(sequence[at - 1], "prev", "← Previous"),
    };
    let next = match sequence.get(at + 1) {
        Some(p) => link(p, "next", "Next →"),
        None => "<span></span>".to_string(),
    };
    format!("<nav class=\"pager\" aria-label=\"Previous and next\">\n{prev}\n{next}\n</nav>\n")
}

/// Links from the site's root (`/how-to/search.html`) made relative to a
/// page `root` above it (`../how-to/search.html`), so that the site reads
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

/// Every `<h2>` to `<h4>` given an `id` and a link to itself, and the
/// h2 and h3 headings listed, for the page's contents. The heading is
/// named by its own text, not the link's, and a heading inside a link (a
/// card on the home page) is given no link of its own, since a link holds
/// no other.
fn anchor_headings(html: &str) -> (String, Vec<Heading>) {
    let heading = Regex::new(r#"<h([2-4])(?: id="([^"]*)")?>(.*?)</h[2-4]>"#).unwrap();
    let tags = Regex::new(r"<[^>]*>").unwrap();
    let links = Regex::new(r"<a[\s>]|</a>").unwrap();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut headings = Vec::new();
    let body = heading
        .replace_all(html, |c: &regex::Captures| {
            let start = c.get(0).unwrap().start();
            let in_link = links
                .find_iter(&html[..start])
                .fold(0i32, |open, m| match m.as_str() {
                    "</a>" => open - 1,
                    _ => open + 1,
                })
                > 0;
            let level: u8 = c[1].parse().unwrap();
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
            let plain = tags.replace_all(text, "").into_owned();
            if level < 4 {
                headings.push(Heading {
                    level,
                    id: id.clone(),
                    text: plain.clone(),
                });
            }
            match in_link {
                true => format!("<h{level} id=\"{id}\">{text}</h{level}>"),
                false => format!(
                    "<h{level} id=\"{id}\" aria-labelledby=\"{id}--text\">\
                     <span id=\"{id}--text\">{text}</span> <a class=\"heading-anchor\" \
                     href=\"#{id}\" aria-label=\"Link to {}\">#</a></h{level}>",
                    plain.replace('"', "&quot;")
                ),
            }
        })
        .into_owned();
    (body, headings)
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

/// Where a page's Markdown twin is published: its path with `.md`.
fn twin_path(page: &Page) -> String {
    page.path.replace(".html", ".md")
}

/// A page as Markdown, for a reader that prefers it, at its address with
/// `.md` for `.html`: its own Markdown under its title, with the site's
/// links and images made absolute, or for a page written from the binary,
/// its text as the terminal shows it. The pages and `llms.txt` promise one
/// for every page.
fn twin(page: &Page, pages: &[Page]) -> String {
    let body = match (&page.markdown, &page.text) {
        (Some(markdown), _) => strip_captures(&fill(markdown))
            .replace("](/", &format!("]({HOMEPAGE}/"))
            .replace("src=\"/", &format!("src=\"{HOMEPAGE}/")),
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
    let mut out = format!("# {}\n\n{}\n\n", page.title, page.description);
    if let Some(author) = &page.author {
        out.push_str(&format!("By {author}.\n\n"));
    }
    out.push_str(&body);
    if let Some(section) = page.section.filter(|_| page.is_index()) {
        out.push_str(&listing(pages, section, "## "));
    }
    out
}

/// A section's pages as a Markdown list, under its group headings where
/// it has them, at the heading level `heading` gives.
fn listing(pages: &[Page], section: usize, heading: &str) -> String {
    let groups = SECTIONS[section].groups;
    let members = members(pages, section);
    let line = |page: &Page| {
        format!(
            "- [{}]({}): {}\n",
            page.title,
            url(&page.path),
            page.description
        )
    };
    let mut out = String::new();
    if members.is_empty() {
        return out;
    }
    if groups.is_empty() {
        out.push('\n');
        for &page in &members {
            out.push_str(&line(page));
        }
    }
    for (g, group) in groups.iter().enumerate() {
        out.push_str(&format!("\n{heading}{}\n\n{}\n\n", group.name, group.blurb));
        for &page in members.iter().filter(|p| p.group == Some(g)) {
            out.push_str(&line(page));
        }
    }
    out
}

/// `llms.txt`: what the site is, and every page, for a language model.
fn llms(pages: &[Page]) -> String {
    let mut out = format!(
        "# aless\n\n> {}.\n\nThe documentation of aless {VERSION}, in four kinds: \
         tutorials, how-to guides, reference and explanation, and why aless was written, \
         in its author's words. Every page has a Markdown \
         version at its address with .md for .html, \
         and the reference is also plain text, as aless --help and F1 print it.\n",
        aless::cli::TAGLINE
    );
    for (i, section) in SECTIONS.iter().enumerate() {
        out.push_str(&format!("\n## {}\n", section.label));
        let groups = SECTIONS[i].groups;
        let line = |page: &Page| {
            format!(
                "- [{}]({}): {}\n",
                page.title,
                url(&twin_path(page)),
                page.description
            )
        };
        // A section whose one page is its index lists that page.
        let mut members = members(pages, i);
        if members.is_empty() {
            members.extend(pages.iter().filter(|p| p.section == Some(i)));
        }
        if groups.is_empty() {
            out.push('\n');
            for &page in &members {
                out.push_str(&line(page));
            }
        }
        for (g, group) in groups.iter().enumerate() {
            out.push_str(&format!("\n### {}\n\n{}\n\n", group.name, group.blurb));
            for &page in members.iter().filter(|p| p.group == Some(g)) {
                out.push_str(&line(page));
            }
        }
    }
    out.push_str(&format!(
        "\n## Optional\n\n- [Source]({REPOSITORY}): the code, the changelog and the \
         releases\n"
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
    let pre = Regex::new(r"(?s)<pre[ >].*?</pre>").unwrap();
    let mut broken = Vec::new();
    let mut stray = Vec::new();
    for page in &site.pages {
        let html = String::from_utf8_lossy(&site.files[&page.path]).to_string();
        assert!(
            !html.contains("{{"),
            "{}: a value is not filled in",
            page.path
        );
        // Markdown the renderer left as text: a heading run into the raw
        // HTML before it shows its `#`s.
        for line in pre.replace_all(&html, "").lines() {
            if line.trim_start().starts_with('#') {
                stray.push(format!("{}: {}", page.origin, line.trim()));
            }
        }
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
        let twin = twin_path(page);
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
        stray.is_empty(),
        "lines of Markdown left as text, outside any code block (a heading needs a \
         blank line between it and raw HTML before it):\n{}",
        stray.join("\n")
    );
    assert!(
        broken.is_empty(),
        "links that reach nothing:\n{}",
        broken.join("\n")
    );
    // The comments the tests read are the build's, not the reader's.
    for (path, bytes) in &site.files {
        if path.ends_with(".html") || path.ends_with(".md") {
            let text = String::from_utf8_lossy(bytes);
            assert!(
                !text.contains("<!-- screen") && !text.contains("<!-- test:"),
                "{path}: a comment for the tests is published"
            );
        }
    }
    let cname = String::from_utf8_lossy(&site.files["CNAME"])
        .trim()
        .to_string();
    assert_eq!(
        format!("https://{cname}"),
        HOMEPAGE,
        "docs/CNAME names the domain Cargo.toml's homepage is served at"
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

/// The reference's pages hold every option, every line of the help F1
/// shows, and every release the changelog records: they are written from
/// the binary and the repository, not by hand.
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
    for group in aless::app::KEY_MAP {
        assert!(
            keys.contains(&escape(group.title)),
            "the keys page leaves out {}",
            group.title
        );
        for (_, does) in group.keys {
            let words = does
                .split(|c: char| !c.is_alphanumeric())
                .find(|w| w.len() > 3);
            assert!(
                words.is_none_or(|w| keys.contains(w)),
                "the keys page leaves out {does:?}"
            );
        }
    }
    // The versions the changelog records, read from the changelog itself,
    // each on the releases page's own body: the rendered page names this
    // build's version in its footer whatever the changelog says, so
    // neither it nor the page can be the witness for the other.
    let releases = &site
        .pages
        .iter()
        .find(|p| p.path == "reference/releases.html")
        .unwrap()
        .body;
    let release = Regex::new(r"(?m)^## \[(\d+\.\d+\.\d+)\]").unwrap();
    let changelog = read(&root().join("CHANGELOG.md"));
    let versions: Vec<&str> = release
        .captures_iter(&changelog)
        .map(|c| c.get(1).unwrap().as_str())
        .collect();
    assert!(!versions.is_empty(), "CHANGELOG.md records no release");
    for version in &versions {
        assert!(
            releases.contains(version),
            "the releases page leaves out {version}"
        );
    }
    assert!(
        versions.contains(&VERSION),
        "CHANGELOG.md has no section for {VERSION}, this build's version"
    );
}

/// The formats page has a row for every format aless reads, naming each
/// extension that implies it.
#[test]
fn the_formats_page_is_the_binarys() {
    let page = site()
        .pages
        .iter()
        .find(|p| p.path == "reference/formats.html")
        .expect("no reference/formats.html");
    let row = Regex::new(r"(?s)<tr>(.*?)</tr>").unwrap();
    let rows: Vec<String> = row
        .captures_iter(&page.body)
        .map(|c| c[1].trim().to_string())
        .collect();
    for format in aless::load::Format::ALL {
        let head = format!("<td><code>{}</code></td>", format.name());
        let Some(row) = rows.iter().find(|r| r.starts_with(&head)) else {
            panic!("the formats page has no row for {}", format.name());
        };
        for ext in format.extensions() {
            assert!(
                row.contains(&format!("<code>{ext}</code>")),
                "the formats page's {} row leaves out {ext}",
                format.name()
            );
        }
    }
}

/// The style guide's gates read every page of the site but the pages a
/// person wrote. `prose-gate.json` names each directory it gates rather
/// than all of `docs/`, so a section added without its directory there
/// would go unchecked, and this fails instead.
#[test]
fn the_gates_read_every_page_but_the_human_ones() {
    let config = read(&root().join("admin/scripts/prose-gate.json"));
    assert!(
        !config.contains("\"docs/**"),
        "prose-gate.json gates all of docs/, and with it the pages a person wrote"
    );
    for dir in ["docs/*.md", "docs/contributing/*.md"] {
        assert!(
            config.contains(&format!("\"{dir}\"")),
            "prose-gate.json does not gate {dir}"
        );
    }
    for section in &SECTIONS {
        let glob = format!("\"docs/{}/*.md\"", section.dir);
        assert_eq!(
            config.contains(&glob),
            !section.human,
            "prose-gate.json gates docs/{}/ when, and only when, a person did not \
             write it",
            section.dir
        );
    }
}

/// Every example in a tutorial, a how-to guide or a reference page is run
/// by a test, or says why not. A `console` block runs, and a `screen`
/// block is drawn; any other block is followed by the capture of one of
/// its commands, or carries `<!-- test: skip REASON -->` before it.
#[test]
fn every_example_is_run_or_says_why_not() {
    let block = Regex::new(
        r"(?ms)^(?:<!-- (?P<comment>(?:screen|test):.*?)-->\n)?```(?P<info>[^\n]*)\n(?P<body>.*?)^```$",
    )
    .unwrap();
    let mut unrun = Vec::new();
    for dir in ["tutorials", "how-to", "reference"] {
        for entry in std::fs::read_dir(sources().join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let text = read(&path).replace("\r\n", "\n");
            let blocks: Vec<regex::Captures> = block.captures_iter(&text).collect();
            for (i, c) in blocks.iter().enumerate() {
                let info = c["info"].trim();
                if matches!(info, "console" | "screen") {
                    continue;
                }
                let comment = c.name("comment").map_or("", |m| m.as_str()).trim();
                if let Some(reason) = comment.strip_prefix("test: skip") {
                    if reason.trim().is_empty() {
                        unrun.push(format!("{}: a skip gives no reason", path.display()));
                    }
                    continue;
                }
                // The next block, a capture of one of this block's commands.
                let captured = blocks.get(i + 1).is_some_and(|next| {
                    let command = next
                        .name("comment")
                        .and_then(|m| m.as_str().strip_prefix("screen:"))
                        .and_then(|s| s.trim().lines().next())
                        .unwrap_or("");
                    &next["info"] == "screen"
                        && c["body"].lines().any(|l| l.trim() == command.trim())
                });
                if !captured {
                    let first = c["body"].lines().next().unwrap_or("");
                    unrun.push(format!("{}: ```{info} {first}", path.display()));
                }
            }
        }
    }
    assert!(
        unrun.is_empty(),
        "examples no test runs, with no <!-- test: skip REASON --> before them:\n{}",
        unrun.join("\n")
    );
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

/// Every `console` example prints what its page says: each `$ ` line runs
/// in `sh`, among the files the site publishes under `examples/`, with this
/// build's aless first on the PATH, and its output is held to the lines
/// after it.
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

/// What a `screen` block on a page was captured from, read from the
/// comment before it: `<!-- screen: COMMAND -->`, with `keys:`, `size:`,
/// `rows:`, `edit:` and `cwd:` lines after the command where they apply
/// (`docs/contributing/site.md`).
struct Capture {
    command: String,
    keys: Vec<String>,
    size: (u16, u16),
    rows: Option<(usize, usize)>,
    edit: Option<(String, String, String)>,
    cwd: String,
}

fn capture_spec(comment: &str) -> Result<Capture, String> {
    let mut lines = comment.trim().lines();
    let command = lines.next().unwrap_or("").trim().to_string();
    let mut spec = Capture {
        command,
        keys: Vec::new(),
        size: (80, 12),
        rows: None,
        edit: None,
        cwd: ".".into(),
    };
    for line in lines {
        let (field, value) = line
            .split_once(':')
            .map(|(f, v)| (f.trim(), v.trim()))
            .ok_or_else(|| format!("not a field: {line:?}"))?;
        match field {
            "keys" => spec.keys = value.split_whitespace().map(str::to_string).collect(),
            "size" => {
                let (w, h) = value.split_once('x').ok_or("size is COLSxROWS")?;
                spec.size = (
                    w.parse().map_err(|_| "size is COLSxROWS")?,
                    h.parse().map_err(|_| "size is COLSxROWS")?,
                );
            }
            "rows" => {
                let (a, b) = value.split_once('-').unwrap_or((value, value));
                let a: usize = a.trim().parse().map_err(|_| "rows is N or N-M")?;
                let b: usize = b.trim().parse().map_err(|_| "rows is N or N-M")?;
                spec.rows = Some((a, b));
            }
            "edit" => {
                let (file, change) = value.split_once(": ").ok_or("edit is FILE: OLD => NEW")?;
                let (old, new) = change
                    .split_once(" => ")
                    .ok_or("edit is FILE: OLD => NEW")?;
                spec.edit = Some((file.into(), old.into(), new.into()));
            }
            "cwd" => spec.cwd = value.into(),
            other => return Err(format!("no field {other}")),
        }
    }
    Ok(spec)
}

/// A command's words, as `sh` splits them: single quotes hold spaces.
fn words(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in command.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut word));
                    any = false;
                }
            }
            c => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(word);
    }
    out
}

/// A key as a capture names it: `Enter`, `S-Tab`, `C-d`, `F1`; any other
/// word is typed as it is spelled.
fn named_key(name: &str) -> Option<aless::app::Key> {
    use aless::app::{Key, KeyCode};
    let code = match name {
        "Enter" => KeyCode::Enter,
        "Esc" => KeyCode::Esc,
        "Tab" => KeyCode::Tab,
        "S-Tab" => KeyCode::BackTab,
        "Backspace" => KeyCode::Backspace,
        "Space" => KeyCode::Char(' '),
        "Up" => KeyCode::Up,
        "Down" => KeyCode::Down,
        "Left" => KeyCode::Left,
        "Right" => KeyCode::Right,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PgUp" => KeyCode::PageUp,
        "PgDn" => KeyCode::PageDown,
        _ => {
            if let Some(c) = name.strip_prefix("C-").filter(|c| c.chars().count() == 1) {
                return Some(Key::ctrl(c.chars().next().unwrap()));
            }
            let n = name.strip_prefix('F')?.parse().ok()?;
            KeyCode::F(n)
        }
    };
    Some(Key::code(code))
}

/// Run a capture in `dir`, a directory of the files the site publishes
/// under `/examples/`, and return the screen aless draws.
fn capture(spec: &Capture, dir: &Path) -> Result<String, String> {
    use aless::app::{App, Input, Key, RELOAD_DEBOUNCE};
    let here = match spec.cwd.as_str() {
        "." => dir.to_path_buf(),
        cwd => dir.join(cwd),
    };
    // `printf 'TEXT' | aless ...` pipes TEXT into standard input.
    let (stdin, command) = match spec.command.split_once(" | ") {
        Some((printf, rest)) => {
            let text = words(printf);
            if text.len() != 2 || text[0] != "printf" {
                return Err(format!("only printf pipes into a capture: {printf}"));
            }
            (
                Some(text[1].replace("\\n", "\n").replace("\\t", "\t")),
                rest,
            )
        }
        None => (None, spec.command.as_str()),
    };
    let argv = words(command);
    if argv.first().map(String::as_str) != Some("aless") {
        return Err(format!("a capture runs aless: {command}"));
    }
    // A capture runs in the test's own directory, the repository's.
    if spec
        .keys
        .windows(2)
        .any(|k| k[0] == ":" && k[1].starts_with('w'))
    {
        return Err("a capture never writes a file, and :w would".into());
    }
    let mut args = match aless::cli::parse_args(argv[1..].iter().map(std::ffi::OsString::from))? {
        aless::cli::Parsed::Run(args) => *args,
        aless::cli::Parsed::Print(_) => return Err("a capture runs the viewer".into()),
    };
    // The paths the command names, as `sh` in `here` would find them.
    for def in &mut args.grammars {
        if let aless::grammar::Source::File(path) = &mut def.source {
            *path = here.join(&*path);
        }
    }
    args.prepare()?;
    let mut app = App::new(args.opts, spec.size.0, spec.size.1);
    for file in &args.files {
        app.open_path(&here.join(file), args.kind);
    }
    if let Some(text) = stdin {
        let format = args.kind.unwrap_or(aless::load::Format::Json);
        app.open_source("(stdin)", text, format);
    }
    if app.tabs.is_empty() {
        app.open_explorer(&here);
    }
    app.active = 0;
    for name in &spec.keys {
        match named_key(name) {
            Some(key) => app.handle(Input::Key(key)),
            None => {
                for c in name.chars() {
                    app.handle(Input::Key(Key::ch(c)));
                }
            }
        }
    }
    if let Some((file, old, new)) = &spec.edit {
        let path = here.join(file);
        let text = read(&path);
        if !text.contains(old.as_str()) {
            return Err(format!("{file} does not hold {old:?}"));
        }
        std::fs::write(&path, text.replacen(old.as_str(), new, 1)).map_err(|e| e.to_string())?;
        app.handle(Input::FileChanged(path));
        app.handle(Input::Tick(std::time::Instant::now() + RELOAD_DEBOUNCE * 2));
    }
    let text = aless::render::screen(&mut app).text();
    let mut rows: Vec<&str> = text.lines().collect();
    if let Some((a, b)) = spec.rows {
        if a == 0 || b < a || b > rows.len() {
            return Err(format!("rows {a}-{b} of a screen of {}", rows.len()));
        }
        rows = rows[a - 1..b].to_vec();
    }
    while rows.last().is_some_and(|r| r.is_empty()) {
        rows.pop();
    }
    // A file named as `sh` in `here` would find it is named so on screen.
    let mut screen = rows.join("\n");
    for file in args.files.iter().filter(|f| f.is_relative()) {
        let full = here.join(file);
        if full.is_file() {
            screen = screen.replace(&full.display().to_string(), &file.display().to_string());
        }
    }
    Ok(screen)
}

/// `from` replaced by `to` in a captured screen, the widest run of
/// spaces inside a row taking up the difference, so a status bar keeps
/// its width.
fn substitute(screen: &str, from: &str, to: &str) -> String {
    let gap = Regex::new(r" {2,}").unwrap();
    screen
        .lines()
        .map(|row| {
            let new = row.replace(from, to);
            let grow = row.chars().count() as isize - new.chars().count() as isize;
            let widest = gap
                .find_iter(&new)
                .filter(|m| m.start() > 0 && m.end() < new.len())
                .max_by_key(|m| m.len());
            match widest {
                Some(m) if grow != 0 => {
                    let len = (m.len() as isize + grow).max(2) as usize;
                    format!(
                        "{}{}{}",
                        &new[..m.start()],
                        " ".repeat(len),
                        &new[m.end()..]
                    )
                }
                _ => new,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every `screen` block on a page is what aless draws for the capture the
/// comment before it names, run in a directory of the files the site
/// publishes under `/examples/`, which the pages show as
/// `/home/user/bookshelf`. With `ALESS_SCREENS=write` set, the blocks are
/// written from their captures instead.
#[test]
fn the_screens_are_what_aless_draws() {
    let write = std::env::var_os("ALESS_SCREENS").is_some_and(|v| v == "write");
    let block =
        Regex::new(r"(?ms)^(?:<!-- screen:(?P<spec>.*?)-->\n)?```screen\n(?P<body>.*?)^```$")
            .unwrap();
    // A short directory, so that a path the viewer shows fits its status bar.
    let base = match cfg!(unix) {
        true => PathBuf::from("/tmp"),
        false => std::env::temp_dir(),
    };
    let work = base.join(format!("aless-screens-{}", std::process::id()));
    let mut pages = vec![root().join("README.md")];
    let mut dirs = vec![sources()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() && !path.ends_with("contributing") {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                pages.push(path);
            }
        }
    }
    pages.sort();
    let tidy = |s: &str| {
        let mut rows: Vec<&str> = s.lines().map(str::trim_end).collect();
        while rows.last().is_some_and(|r| r.is_empty()) {
            rows.pop();
        }
        rows.join("\n")
    };
    let (mut wrong, mut ran, mut skipped) = (Vec::new(), 0, 0);
    for source in &pages {
        let text = read(source).replace("\r\n", "\n");
        let name = source.strip_prefix(root()).unwrap().display().to_string();
        let mut written = String::new();
        let mut last = 0;
        for c in block.captures_iter(&text) {
            let body = c.name("body").unwrap();
            let Some(spec) = c.name("spec") else {
                wrong.push(format!(
                    "{name}: a screen block with no capture comment before it"
                ));
                continue;
            };
            ran += 1;
            let dir = work.join(ran.to_string());
            let shelf = dir.join("bookshelf");
            std::fs::create_dir_all(&shelf).unwrap();
            for (path, bytes) in &site().files {
                if let Some(file) = path.strip_prefix("examples/") {
                    std::fs::write(shelf.join(file), bytes).unwrap();
                }
            }
            let got = match capture_spec(spec.as_str()).and_then(|s| capture(&s, &shelf)) {
                Ok(got) => got,
                Err(e) => {
                    wrong.push(format!("{name}: {}: {e}", spec.as_str().trim()));
                    continue;
                }
            };
            // The canonical form first, since it can hold the other.
            let shown: Vec<String> = [std::fs::canonicalize(&dir).unwrap(), dir.clone()]
                .iter()
                .map(|d| d.display().to_string())
                .collect();
            // A Windows path is not the page's /home/user, and Windows may
            // spell the directory another way (`RUNNER~1`), so look for its
            // name.
            let name = work.file_name().unwrap().to_string_lossy().into_owned();
            if cfg!(windows) && got.contains(name.as_str()) {
                skipped += 1;
                continue;
            }
            let got = shown
                .iter()
                .fold(got, |s, d| substitute(&s, d, "/home/user"));
            if tidy(&got) != tidy(body.as_str()) {
                if write {
                    written.push_str(&text[last..body.start()]);
                    written.push_str(&tidy(&got));
                    written.push('\n');
                    last = body.end();
                } else {
                    wrong.push(format!(
                        "{name}: {}\n--- the page shows:\n{}\n--- aless draws:\n{}",
                        spec.as_str().trim(),
                        tidy(body.as_str()),
                        tidy(&got)
                    ));
                }
            }
        }
        if write && last > 0 {
            written.push_str(&text[last..]);
            std::fs::write(source, written).unwrap();
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    println!("site: drew {ran} screens, {skipped} skipped on this platform");
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n\n"));
}
