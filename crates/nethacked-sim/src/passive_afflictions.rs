//! Current Rust raw affliction phase; scheduling is owned by its caller.
use crate::GameEvent;
use nethacked_core::afflictions::{tick_state, AfflictionTickResult};
use nethacked_types::{AfflictionState, Locale};
pub(crate) fn recovery(
    state: &mut AfflictionState,
) -> (AfflictionTickResult, bool, Vec<GameEvent>) {
    let before = state.transient.clone();
    let result = tick_state(state);
    let t = &state.transient;
    let cleared = before.blinded > 0 && t.blinded == 0;
    let mut events = Vec::new();
    for (emit, text) in [
        (
            before.stunned > 0 && t.stunned == 0,
            "You feel a bit steadier now.",
        ),
        (
            before.confused > 0 && t.confused == 0,
            "You feel less confused now.",
        ),
        (cleared, "You can see again."),
        (before.helpless > 0 && t.helpless == 0, "You wake up."),
    ] {
        if emit {
            events.push(GameEvent::LogMessage { text: text.into() });
        }
    }
    (result, cleared, events)
}
pub(crate) fn fatal_message(result: &AfflictionTickResult, locale: Locale) -> Option<GameEvent> {
    let text = match result {
        AfflictionTickResult::StoneDeath => nethacked_i18n::Messages::petrification_death(locale),
        AfflictionTickResult::SlimeDeath => nethacked_i18n::Messages::sliming_death(locale),
        AfflictionTickResult::Survived => return None,
    };
    Some(GameEvent::LogMessage { text: text.into() })
}
