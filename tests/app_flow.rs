//! A headless run of the application against real files: open, move,
//! fold, search, then change the file on disk and check the reload keeps
//! the reader's place.

use std::path::PathBuf;
use std::time::Instant;

use aless::app::{App, Input, Key, Options, RELOAD_DEBOUNCE};
use aless::fmt;
use aless::render;

fn temp(name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aless-flow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, contents).unwrap();
    p
}

fn keys(app: &mut App, s: &str) {
    for c in s.chars() {
        app.handle(Input::Key(Key::ch(c)));
    }
}

fn focused_path(app: &mut App) -> String {
    let tab = app.tab();
    let node = tab.focused_node();
    fmt::path_dot(&tab.doc.path(node))
}

#[test]
fn edit_reload_keeps_the_place_and_the_screen_row() {
    let mut src = String::from("{\n");
    for i in 0..40 {
        src.push_str(&format!(
            "  \"k{i}\": {{\"v\": {i}, \"tags\": [\"t{i}\"]}},\n"
        ));
    }
    src.push_str("  \"end\": true\n}\n");
    let path = temp("place.json", &src);
    let mut app = App::new(Options::default(), 60, 12);
    app.open_path(&path, None);
    // Fold everything to one level, walk down, open one entry.
    app.run_command("depth 1");
    keys(&mut app, "20j");
    assert_eq!(focused_path(&mut app), ".k19");
    keys(&mut app, "ll"); // expand, then step into the first child
    assert_eq!(focused_path(&mut app), ".k19.v");
    let screen_row = app.tab().focus - app.tab().scroll;
    let before = render::render(&mut app).text();
    assert!(
        before.contains("▽ k19: {"),
        "k19 is expanded, its child focused:\n{before}"
    );

    // Insert a key at the top and change k19's value: the path survives,
    // so the focus stays on .k19.v, on the same screen row.
    let edited = src
        .replacen("{\n", "{\n  \"inserted\": 0,\n", 1)
        .replace("\"v\": 19,", "\"v\": 1900,");
    std::fs::write(&path, &edited).unwrap();
    app.handle(Input::FileChanged(path.clone()));
    app.handle(Input::Tick(Instant::now() + RELOAD_DEBOUNCE * 2));
    assert_eq!(focused_path(&mut app), ".k19.v");
    assert_eq!(app.tab().focus - app.tab().scroll, screen_row);
    let after = render::render(&mut app).text();
    assert!(after.contains("v: 1900"), "{after}");
    assert!(after.contains("Reloaded"), "{after}");
    // The folds survived: k18 is still a collapsed preview.
    assert!(
        after.contains("▷ k18: (2) {v: 18, tags: […]}") || after.contains("k18: (2)"),
        "{after}"
    );

    // Remove k19 entirely: fall back to the nearest surviving line, which
    // is a neighbour entry at the same place in the file.
    let removed: String = edited
        .lines()
        .filter(|l| !l.contains("\"k19\""))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(&path, &removed).unwrap();
    app.handle(Input::FileChanged(path.clone()));
    app.handle(Input::Tick(Instant::now() + RELOAD_DEBOUNCE * 2));
    let p = focused_path(&mut app);
    assert!(
        p.starts_with(".k20") || p.starts_with(".k18"),
        "landed on {p}"
    );

    // A broken write keeps the document and reports the error; fixing it
    // recovers.
    std::fs::write(&path, "{ \"broken\": ").unwrap();
    app.handle(Input::FileChanged(path.clone()));
    app.handle(Input::Tick(Instant::now() + RELOAD_DEBOUNCE * 2));
    assert!(app.tab().error.is_some());
    assert!(app.tab().doc.len() > 10);
    let screen = render::render(&mut app).text();
    assert!(screen.contains('!'), "{screen}");
    std::fs::write(&path, &removed).unwrap();
    app.handle(Input::FileChanged(path.clone()));
    app.handle(Input::Tick(Instant::now() + RELOAD_DEBOUNCE * 2));
    assert!(app.tab().error.is_none());
}

#[test]
fn search_and_yank_through_the_key_map() {
    let path = temp(
        "search.yaml",
        "name: aless\nitems:\n  - alpha\n  - beta\nnested:\n  deep:\n    value: 42\n",
    );
    let mut app = App::new(Options::default(), 60, 12);
    app.open_path(&path, None);
    keys(&mut app, "/beta");
    app.handle(Input::Key(Key::code(aless::app::KeyCode::Enter)));
    assert_eq!(focused_path(&mut app), ".items[1]");
    keys(&mut app, "/42");
    app.handle(Input::Key(Key::code(aless::app::KeyCode::Enter)));
    assert_eq!(focused_path(&mut app), ".nested.deep.value");
    let line = app.tab().focused_line().unwrap().0;
    assert_eq!(line, 7);
    keys(&mut app, "yq");
    let copied = app.effects.iter().find_map(|e| match e {
        aless::app::Effect::Copy { text, .. } => Some(text.clone()),
        _ => None,
    });
    assert_eq!(copied.as_deref(), Some(".nested.deep.value"));
    keys(&mut app, "s");
    let screen = render::render(&mut app).text();
    assert!(screen.contains("(source)"), "{screen}");
    assert!(screen.lines().any(|l| l.contains("value: 42")), "{screen}");
}
