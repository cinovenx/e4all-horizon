// The world closing: the relay closes its connection, which retires the domain,
// and the account becomes visible again.
fn e4all_lifecycle(phase: crate::game::native_lifecycle::WorldPhase) {
    if matches!(
        phase,
        crate::game::native_lifecycle::WorldPhase::WorldCloseFinal
            | crate::game::native_lifecycle::WorldPhase::WorldCloseFailed
            | crate::game::native_lifecycle::WorldPhase::WorldOpenFailed
    ) {
        crate::game::e4all::stop();
        crate::game::e4all::end_hosting();
    }
}
