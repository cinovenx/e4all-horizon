// The pause of a hosted world. Vanilla pauses a single-player world when the
// pause menu is open, and a paused world freezes every guest that is connected
// to it until they are dropped. `preventPauseWhileHosting` keeps the world
// ticking while e4all is hosting it.
//
// This is `Integrated::set_paused`'s own `paused` argument, so the decision is
// made where the game makes it.
fn e4all_pause(value: bool) -> bool {
    if value && crate::game::e4all::prevent_pause() {
        false
    } else {
        value
    }
}
