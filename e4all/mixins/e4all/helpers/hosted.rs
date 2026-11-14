// Before the server loads its worlds (`ServerAboutToStartEvent`), and before
// `integrated_open` decides the LAN's online mode: this is the one moment the
// offline override can be set in time for the world being opened.
//
// A world opened with e4all turned off is an ordinary world, and it keeps
// authenticating its guests unless the player asked for the offline mode.
fn e4all_server_about_to_start(dedicated: bool) {
    if !dedicated {
        crate::game::e4all::begin_hosting();
    }
}
