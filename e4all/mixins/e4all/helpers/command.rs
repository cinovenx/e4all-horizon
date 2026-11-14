// A player's `/e4all ...`.
fn e4all_perform_command(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    if *ran {
        return;
    }
    let words: Vec<&str> = command.split_whitespace().collect();
    if words.first() != Some(&"e4all") {
        return;
    }
    *ran = crate::game::e4all::command(game, player.uuid, &words);
}
