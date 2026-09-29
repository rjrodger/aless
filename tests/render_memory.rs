//! Drawing a long value costs a screenful, not the value. The width
//! measures in `render` walk grapheme clusters lazily and stop once they
//! have their answer, so a string of millions of characters, well within
//! the input limit, is measured and clipped without holding anything per
//! character. A counting allocator reads the most memory each measure
//! holds at once beyond what was live when it began; this binary holds
//! this one test, so nothing else allocates while it runs.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

use aless::render::{clip, cols, Cells};
use ratatui::style::Style;
use ratatui::text::Line;

struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call goes straight to the system allocator; the counters
// only observe the sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let live = LIVE.fetch_add(layout.size(), SeqCst) + layout.size();
            PEAK.fetch_max(live, SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), SeqCst);
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// What `f` returns, and the most memory it held at once beyond what was
/// live when it began.
fn held<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let base = LIVE.load(SeqCst);
    PEAK.store(base, SeqCst);
    let out = f();
    (out, PEAK.load(SeqCst) - base)
}

/// Far above a screenful of text, far below a value's length: collecting
/// one entry per character of these values took tens of megabytes.
const SCREENFUL: usize = 64 << 10;

#[test]
fn a_long_value_is_measured_and_clipped_in_a_screenful() {
    let chars = 1 << 20;
    // ASCII, one column a character, and CJK, two.
    for (unit, per) in [("a", 1), ("日", 2)] {
        let big = unit.repeat(chars);
        let (width, bytes) = held(|| cols(&big));
        assert_eq!(width, chars * per);
        assert!(bytes < SCREENFUL, "measuring {unit:?}s held {bytes} bytes");

        let edge = unit.repeat(80 / per - 1);
        let middle = unit.repeat(78 / per);
        for (xoff, want) in [
            (0, format!("{edge}…")),
            (chars / 2, format!("…{middle}…")),
            (usize::MAX / 4, format!("…{edge}")),
        ] {
            let ((text, _), bytes) = held(|| clip(&big, xoff, 80));
            assert_eq!(text, want, "{unit:?}s at {xoff}");
            assert!(
                bytes < SCREENFUL,
                "clipping {unit:?}s at {xoff} held {bytes} bytes"
            );
        }

        let mut line = Line::default();
        line.put(big, Style::new());
        let (_, bytes) = held(|| line.fit_cols(80, Style::new()));
        assert_eq!(line.cols(), 80);
        assert!(bytes < SCREENFUL, "fitting {unit:?}s held {bytes} bytes");
    }
}
