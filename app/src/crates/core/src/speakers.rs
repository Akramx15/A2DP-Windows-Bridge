//! The PC's own speakers while the headphones stream: muted, so the sound
//! the app forwards is all that is heard. The capture is a loopback of the
//! mix, and the endpoint's mute does not empty it.

/// Mutes (or restores) the default render endpoint, off the calling thread
pub fn set_muted(muted: bool) {
    std::thread::spawn(move || match win_sys::audio_policy::EndpointVolume::open(None) {
        Ok(volume) => {
            if let Err(e) = volume.set_muted(muted) {
                tracing::warn!("the PC's speakers: {e}");
            }
        }
        Err(e) => tracing::warn!("the PC's speakers: {e}"),
    });
}
