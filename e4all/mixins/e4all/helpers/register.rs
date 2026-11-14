// `/e4all` in every player's command tree.
fn e4all_register_commands(commands: &mut Vec<crate::game::native_server::NativeCommand>) {
    commands.extend(crate::game::e4all::commands());
}
