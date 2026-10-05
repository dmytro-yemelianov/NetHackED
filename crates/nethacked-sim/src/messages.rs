//! Top-line message window (frontend state) and the event-to-text policy.
//!
//! Both frontends (terminal UI and browser) show the messages of one command
//! through the same code: [`turn_messages`] turns a step's events into text and
//! [`MessageWindow`] is a port of the tty top line (`win/tty/topl.c`). The
//! window is display state only: it is never part of [`SimulationWorld`], uses
//! no RNG and emits no [`GameEvent`].

use crate::{GameEvent, SimulationWorld};
use nethacked_i18n::{t, Messages};
use std::collections::VecDeque;

/// Longest packed top line, `min(CO - 8, TBUFSZ)` for CO = 80 columns
/// (win/tty/topl.c:262-269). Eight columns stay free for `--More--`
/// (win/tty/wintty.c:182 `defmorestr`).
pub const PACK_LIMIT: usize = 72;

/// Lines kept for `^P`: `iflags.msg_history = 20` (src/options.c:7198).
pub const HISTORY_LIMIT: usize = 20;

/// Texts shown for the events of one step, in order.
///
/// `LogMessage` is shown as is. `AttackLanded` / `AttackMissed` are skipped
/// when the next event is a `LogMessage` (combat.rs emits the pair), otherwise
/// they fall back to the generic hit / miss text. `DoorToggled` prints
/// "The door opens." / "The door closes." (lock.c:906 `pline_The("door
/// opens.")`, lock.c:1040 `pline_The("door closes.")`); a broken door prints
/// nothing here (the kick pairs its own text) and a toggle directly followed
/// by a `LogMessage` prints nothing, so the line is not said twice. Level
/// changes and every other event print nothing.
///
/// The world is a parameter although only its locale is read today: later
/// items derive more text from the world after the step.
pub fn turn_messages(world: &SimulationWorld, events: &[GameEvent]) -> Vec<String> {
    let loc = world.locale;
    let mut out = Vec::new();
    for (i, ev) in events.iter().enumerate() {
        let paired = matches!(events.get(i + 1), Some(GameEvent::LogMessage { .. }));
        match ev {
            GameEvent::LogMessage { text } => out.push(text.clone()),
            GameEvent::AttackLanded { damage, lethal, .. } if !paired => {
                out.push(Messages::tui_hit(*damage, *lethal, loc))
            }
            GameEvent::AttackMissed { .. } if !paired => out.push(t("tui.miss", loc).to_string()),
            GameEvent::DoorToggled { new_state, .. } if !paired => match new_state {
                nethacked_types::DoorState::Open => out.push(Messages::door_opens(loc).to_string()),
                nethacked_types::DoorState::Closed => {
                    out.push(Messages::door_closes(loc).to_string())
                }
                _ => {}
            },
            _ => {}
        }
    }
    out
}

/// Split `text` into pages of at most [`PACK_LIMIT`] characters at spaces.
///
/// The page ends at the last space within the limit; with no such space the
/// first space after it is used, and a text without any space stays whole
/// (win/tty/topl.c:285-298 splits at the last space before the line width and
/// "tries splitting after" a huge token, and spits out a token without spaces
/// whole).
fn split_pages(text: &str) -> Vec<String> {
    let mut pages = Vec::new();
    let mut rest: Vec<char> = text.chars().collect();
    while rest.len() > PACK_LIMIT {
        let cut = (1..=PACK_LIMIT)
            .rev()
            .find(|&i| rest[i] == ' ')
            .or_else(|| (PACK_LIMIT + 1..rest.len()).find(|&i| rest[i] == ' '));
        let Some(cut) = cut else { break };
        let page: String = rest[..cut].iter().collect();
        let page = page.trim_end().to_string();
        let tail: String = rest[cut + 1..].iter().collect();
        let tail = tail.trim_start();
        if !page.is_empty() {
            pages.push(page);
        }
        rest = tail.chars().collect();
    }
    let last: String = rest.iter().collect();
    if !last.is_empty() {
        pages.push(last);
    }
    pages
}

/// Pack the texts of one command into top-line pages.
///
/// A text joins the current line with two spaces while `n0 + strlen(toplines)
/// + 3 < min(CO - 8, TBUFSZ)` and the new text does not start with "You die"
/// (win/tty/topl.c:262-269 `update_topl`); otherwise it starts a new page, which
/// is shown after a `--More--`. Lengths are character counts. A text longer than
/// [`PACK_LIMIT`] is split at spaces into extra pages.
pub fn pack_lines(texts: &[String]) -> Vec<String> {
    let mut pages: Vec<String> = Vec::new();
    // Characters of the line being built (`strlen(gt.toplines)`).
    let mut cur: Option<String> = None;
    for text in texts.iter().filter(|t| !t.is_empty()) {
        let n0 = text.chars().count();
        if let Some(line) = cur.as_mut() {
            if n0 + line.chars().count() + 3 < PACK_LIMIT && !text.starts_with("You die") {
                line.push_str("  ");
                line.push_str(text);
                continue;
            }
            pages.push(cur.take().unwrap_or_default());
        }
        let mut split = split_pages(text);
        cur = split.pop();
        pages.extend(split);
    }
    pages.extend(cur);
    pages
}

/// The message line of a frontend: what `win/tty/topl.c` keeps in `toplines`,
/// the pages waiting behind `--More--`, and the `^P` ring.
#[derive(Debug, Clone, Default)]
pub struct MessageWindow {
    /// Text on the top line now.
    line: String,
    /// Pages still to show, each after a `--More--` (topl.c:205 `more`).
    pending: VecDeque<String>,
    /// Remembered lines, oldest first (topl.c:168-190 `remember_topl`).
    history: VecDeque<String>,
    /// Index into `history` of the line `^P` showed last; `None` when no
    /// recall is in progress (`cw->maxcol = cw->maxrow` after a new line).
    prev: Option<usize>,
}

impl MessageWindow {
    pub fn new() -> Self {
        Self::default()
    }

    fn remember(&mut self, page: &str) {
        self.history.push_back(page.to_string());
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
        self.prev = None;
    }

    /// Show the messages of one command (win/tty/topl.c:251-303 `update_topl`).
    ///
    /// They are packed by [`pack_lines`]; the first page is on the line, the
    /// rest wait behind `--More--`, and every page is remembered for `^P`. An
    /// empty list clears the line (history is unchanged).
    pub fn post_turn(&mut self, texts: &[String]) {
        let mut pages: VecDeque<String> = pack_lines(texts).into();
        self.pending.clear();
        let Some(first) = pages.pop_front() else {
            self.line.clear();
            return;
        };
        for page in std::iter::once(&first).chain(pages.iter()) {
            self.remember(page);
        }
        self.line = first;
        self.pending = pages;
    }

    /// Show one message of its own (a `pline`); see [`Self::post_turn`].
    pub fn post(&mut self, text: &str) {
        self.post_turn(&[text.to_string()]);
    }

    /// Show a prompt or echo on the line without remembering it
    /// (win/tty/topl.c:421-426 `SUPPRESS_HISTORY`).
    pub fn show(&mut self, text: &str) {
        self.pending.clear();
        self.line = text.to_string();
    }

    /// Add `text` as a page of its own after what is shown: an occupied line
    /// gets a `--More--` first (win/tty/topl.c:262-269, the "You die" case).
    pub fn append_page(&mut self, text: &str) {
        for page in split_pages(text) {
            self.remember(&page);
            if self.line.is_empty() && self.pending.is_empty() {
                self.line = page;
            } else {
                self.pending.push_back(page);
            }
        }
    }

    /// True while a `--More--` waits for a key.
    pub fn more(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Any key at `--More--`: show the next page (win/tty/topl.c:205-245).
    pub fn dismiss(&mut self) {
        if let Some(next) = self.pending.pop_front() {
            self.line = next;
        }
    }

    /// Esc at `--More--`: drop the remaining pages and clear the line
    /// (win/tty/topl.c:233-236 sets `WIN_STOP`, :240-243 clears the line).
    /// History keeps every page.
    pub fn skip_rest(&mut self) {
        if self.more() {
            self.pending.clear();
            self.line.clear();
        }
    }

    /// The next command key was read: the line is cleared (win/tty/wintty.c:4100-4102: the topline has been seen once a key is read).
    /// Does nothing while a `--More--` is pending.
    pub fn clear_line(&mut self) {
        if !self.more() {
            self.line.clear();
        }
    }

    /// `^P` with `msg_window:single` (win/tty/topl.c:102-119 `tty_doprev_message`,
    /// src/options.c:7202): the first press shows the newest line, each further
    /// press an older one, wrapping to the newest. Ignored (returns `None`)
    /// at `--More--` and with no history.
    pub fn recall_prev(&mut self) -> Option<&str> {
        if self.more() || self.history.is_empty() {
            return None;
        }
        let newest = self.history.len() - 1;
        let idx = match self.prev {
            Some(i) if i > 0 && i <= newest => i - 1,
            _ => newest,
        };
        self.prev = Some(idx);
        self.line = self.history[idx].clone();
        Some(self.line.as_str())
    }

    /// Text on the line, without any label.
    pub fn text(&self) -> &str {
        &self.line
    }

    /// The line as drawn: the text followed directly by `more_label` while a
    /// `--More--` is pending.
    pub fn line(&self, more_label: &str) -> String {
        if self.more() {
            format!("{}{}", self.line, more_label)
        } else {
            self.line.clone()
        }
    }

    /// Remembered lines, oldest first, at most [`HISTORY_LIMIT`].
    pub fn history(&self) -> impl Iterator<Item = &str> + '_ {
        self.history.iter().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }
    fn hist(w: &MessageWindow) -> Vec<String> {
        w.history().map(str::to_string).collect()
    }

    #[test]
    fn pack_joins_short_messages_with_two_spaces() {
        assert_eq!(
            pack_lines(&s(&["You hit it.", "It dies!"])),
            s(&["You hit it.  It dies!"])
        );
        assert_eq!(pack_lines(&s(&["one"])), s(&["one"]));
        assert!(pack_lines(&[]).is_empty());
    }

    #[test]
    fn pack_boundary_matches_update_topl() {
        // topl.c:262-269: join while n0 + strlen(toplines) + 3 < 72.
        let a34 = "a".repeat(34);
        let b34 = "b".repeat(34);
        let a35 = "a".repeat(35);
        assert_eq!(
            pack_lines(&[a34.clone(), b34.clone()]),
            vec![format!("{a34}  {b34}")]
        );
        assert_eq!(pack_lines(&[a35.clone(), b34.clone()]), vec![a35, b34]);
    }

    #[test]
    fn pack_you_die_never_joins() {
        assert_eq!(
            pack_lines(&s(&["Ouch.", "You die..."])),
            s(&["Ouch.", "You die..."])
        );
        // The following text may still not join a "You die" line? It may: the
        // rule is about the NEW text only.
        assert_eq!(pack_lines(&s(&["You die...", "x"])), s(&["You die...  x"]));
    }

    #[test]
    fn pack_counts_chars_not_bytes() {
        // 30 Cyrillic chars are 60 bytes; 30 + 30 + 3 = 63 < 72 joins.
        let a = "ж".repeat(30);
        let b = "щ".repeat(30);
        assert_eq!(
            pack_lines(&[a.clone(), b.clone()]),
            vec![format!("{a}  {b}")]
        );
        // 34 + 35 + 3 = 72 does not join.
        let c = "ж".repeat(34);
        let d = "щ".repeat(35);
        assert_eq!(pack_lines(&[c.clone(), d.clone()]), vec![c, d]);
    }

    #[test]
    fn pack_splits_overlong_message_at_space_into_pages() {
        let words: Vec<String> = (0..30).map(|i| format!("w{i:02}xx")).collect();
        let text = words.join(" "); // 30 * 5 + 29 = 179 chars
        let pages = pack_lines(&[text.clone()]);
        assert!(pages.len() >= 3, "{pages:?}");
        for p in &pages {
            assert!(p.chars().count() <= PACK_LIMIT, "{p:?}");
            assert!(!p.starts_with(' ') && !p.ends_with(' '), "{p:?}");
        }
        assert_eq!(pages.join(" "), text, "no word is lost or cut");
        // No space at all: left whole.
        let huge = "x".repeat(100);
        assert_eq!(pack_lines(&[huge.clone()]), vec![huge]);
    }

    #[test]
    fn more_flow_dismiss_and_last_page_without_more() {
        let mut w = MessageWindow::new();
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let c = "c".repeat(40);
        w.post_turn(&[a.clone(), b.clone(), c.clone()]);
        assert!(w.more());
        assert_eq!(w.text(), a);
        assert_eq!(w.line("--More--"), format!("{a}--More--"));
        w.dismiss();
        assert_eq!(w.text(), b);
        assert!(w.more());
        w.dismiss();
        assert_eq!(w.text(), c);
        assert!(!w.more());
        assert_eq!(w.line("--More--"), c, "the last page has no label");
        w.dismiss();
        assert_eq!(w.text(), c, "dismiss without pending is a no-op");
    }

    #[test]
    fn esc_skip_rest_clears_queue_but_history_keeps_all_pages() {
        let mut w = MessageWindow::new();
        let pages = ["a".repeat(40), "b".repeat(40), "c".repeat(40)];
        w.post_turn(&pages);
        w.skip_rest();
        assert!(!w.more());
        assert_eq!(w.text(), "");
        assert_eq!(hist(&w), pages.to_vec());
        w.skip_rest();
        assert_eq!(w.text(), "", "skip without --More-- is a no-op");
    }

    #[test]
    fn empty_turn_clears_line_but_keeps_history() {
        let mut w = MessageWindow::new();
        w.post_turn(&s(&["hello"]));
        assert_eq!(w.text(), "hello");
        w.post_turn(&[]);
        assert_eq!(w.text(), "");
        assert!(!w.more());
        assert_eq!(hist(&w), s(&["hello"]));
    }

    #[test]
    fn history_is_capped_at_20_dropping_oldest() {
        let mut w = MessageWindow::new();
        for i in 0..25 {
            w.post(&format!("m{i}"));
        }
        let h = hist(&w);
        assert_eq!(h.len(), HISTORY_LIMIT);
        assert_eq!(h.first().unwrap(), "m5");
        assert_eq!(h.last().unwrap(), "m24");
    }

    #[test]
    fn prev_cycles_newest_first_then_wraps() {
        let mut w = MessageWindow::new();
        w.post("a");
        w.post("b");
        w.post("c");
        w.clear_line();
        assert_eq!(w.recall_prev(), Some("c"));
        assert_eq!(w.text(), "c", "the recalled line is shown on the top line");
        assert_eq!(w.recall_prev(), Some("b"));
        assert_eq!(w.recall_prev(), Some("a"));
        assert_eq!(w.recall_prev(), Some("c"), "wraps to the newest");
    }

    #[test]
    fn prev_cursor_resets_on_new_message_not_on_clear_line() {
        let mut w = MessageWindow::new();
        w.post("a");
        w.post("b");
        w.post("c");
        assert_eq!(w.recall_prev(), Some("c"));
        assert_eq!(w.recall_prev(), Some("b"));
        w.clear_line();
        assert_eq!(w.recall_prev(), Some("a"), "clear_line keeps the cursor");
        w.post("d");
        assert_eq!(w.recall_prev(), Some("d"), "a new line resets the cursor");
        assert_eq!(w.recall_prev(), Some("c"));
        // A prompt is not a message line: the cursor is untouched.
        w.show("What?");
        assert_eq!(w.recall_prev(), Some("b"));
    }

    #[test]
    fn prev_on_empty_history_is_none() {
        let mut w = MessageWindow::new();
        assert_eq!(w.recall_prev(), None);
        assert_eq!(w.text(), "");
    }

    #[test]
    fn show_is_not_recorded_in_history() {
        let mut w = MessageWindow::new();
        w.post("real");
        w.show("In what direction?");
        assert_eq!(w.text(), "In what direction?");
        assert!(!w.more());
        assert_eq!(hist(&w), s(&["real"]));
    }

    #[test]
    fn append_page_forces_more_before_death_banner() {
        let mut w = MessageWindow::new();
        w.post_turn(&s(&["You kill it!"]));
        w.append_page("You die...");
        assert!(w.more(), "the fatal line waits behind --More--");
        assert_eq!(w.text(), "You kill it!");
        w.dismiss();
        assert_eq!(w.text(), "You die...");
        assert!(!w.more());
        assert_eq!(hist(&w), s(&["You kill it!", "You die..."]));
        // On an empty line the banner is shown directly.
        let mut w = MessageWindow::new();
        w.append_page("You die...");
        assert_eq!(w.text(), "You die...");
        assert!(!w.more());
    }

    #[test]
    fn clear_line_is_noop_while_more() {
        let mut w = MessageWindow::new();
        w.post_turn(&["a".repeat(40), "b".repeat(40)]);
        w.clear_line();
        assert!(w.more());
        assert_eq!(w.text(), "a".repeat(40));
    }
}
