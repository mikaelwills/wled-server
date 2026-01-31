mod device;
mod files;
mod health;
mod playback;
mod routing;

pub use device::{get_audio_settings, get_device_outputs, list_devices, select_device};
pub use files::{delete_audio, get_audio, get_peaks, save_peaks, test_decode, upload_audio};
pub use health::{get_engine_health, get_memory_stats, get_resampling_status, reset_engine_health};
pub use playback::{
    get_playback_status, get_track_readiness, load_track, pause_playback, play_track,
    resume_playback, seek_playback, stop_playback,
};
pub use routing::{get_routing, set_mute, update_routing};

pub(super) const DEFAULT_CHANNELS: u16 = 2;

pub(super) fn strip_audio_extension(id: &str) -> &str {
    id.strip_suffix(".mp3")
        .or_else(|| id.strip_suffix(".wav"))
        .unwrap_or(id)
}
