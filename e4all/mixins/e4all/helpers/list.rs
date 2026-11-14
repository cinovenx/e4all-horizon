// e4all's row in Options > Mods, where a player can find it and turn it off.
//
// The hook is dispatched once and every mod adds its own row, so this pushes
// e4all's without asking whose turn it is.
fn e4all_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    mods.push(crate::game::native_hooks::NativeMod {
        id: "e4all".into(),
        title: format!("e4all {}", crate::game::e4all::VERSION),
        description: "Share this world over the e4mc relay: friends join without port forwarding."
            .into(),
        keys: Vec::new(),
    });
}
