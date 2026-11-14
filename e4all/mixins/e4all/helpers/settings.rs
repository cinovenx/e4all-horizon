// e4all's settings page: the switches that matter.
//
// Labels are rebuilt each time the page opens, so they always show the values
// in the file rather than what the player last clicked.
fn e4all_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id != "e4all" {
        return;
    }
    let _ = game;
    let config = crate::game::e4all::config();
    buttons.push(format!(
        "Share this world over the relay: {}",
        crate::game::e4all::on_off(config.host_enabled)
    ));
    buttons.push(format!(
        "Keep the world ticking while hosting: {}",
        crate::game::e4all::on_off(config.prevent_pause_while_hosting)
    ));
    // Horizon decides the LAN's online mode from the host's own account
    // (`OnlineMode::lan`): a game started without one hosts offline whatever
    // this switch says. Showing it there would promise a choice the player
    // does not have — guests join offline already — so it is left out until
    // the game has an account to authenticate with.
    if horizon_client::auth::current().is_some() {
        buttons.push(format!(
            "Offline mode (no Mojang login for guests): {}",
            crate::game::e4all::on_off(config.offline_mode)
        ));
    }
    buttons.push(format!(
        "Hide the address in chat: {}",
        crate::game::e4all::on_off(config.hide_domain_in_chat)
    ));
}
