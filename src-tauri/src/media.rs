//! SMTC（Windows 系统媒体传输控制）：捕获网易云等播放器的正在播放信息并发送控制命令。
//! 策略：优先取正在播放的会话，同状态下优先网易云；1.5s 轮询，变化才推送。
use serde::Serialize;
use tauri::Emitter;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Media::MediaPlaybackAutoRepeatMode;

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub play_mode: String,
}

fn com_init() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    unsafe {
        // S_FALSE（已初始化）/ RPC_E_CHANGED_MODE 均可忽略
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn find_session() -> windows::core::Result<Option<Session>> {
    let mgr = Manager::RequestAsync()?.get()?;
    let sessions = mgr.GetSessions()?;
    let mut best: Option<(u8, Session)> = None;
    for s in sessions {
        let aumid = s
            .SourceAppUserModelId()
            .map(|h| h.to_string())
            .unwrap_or_default();
        let playing = s
            .GetPlaybackInfo()
            .ok()
            .and_then(|playback| playback.PlaybackStatus().ok())
            .is_some_and(|status| {
                status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
            });
        let score = session_score(&aumid, playing);
        if best.as_ref().is_none_or(|(current, _)| score > *current) {
            best = Some((score, s));
        }
    }
    Ok(best.map(|(_, session)| session))
}

fn session_score(aumid: &str, playing: bool) -> u8 {
    u8::from(playing) * 2 + u8::from(aumid.to_ascii_lowercase().contains("netease"))
}

fn read_track() -> Option<Track> {
    let s = find_session().ok()??;
    let props = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let playback = s.GetPlaybackInfo().ok()?;
    let playing = matches!(
        playback.PlaybackStatus().unwrap_or_default(),
        GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
    );
    let shuffled = playback
        .IsShuffleActive()
        .ok()
        .and_then(|value| value.Value().ok())
        .unwrap_or(false);
    let repeat_mode = playback
        .AutoRepeatMode()
        .ok()
        .and_then(|value| value.Value().ok())
        .unwrap_or(MediaPlaybackAutoRepeatMode::None);
    let play_mode = if shuffled {
        "shuffle"
    } else if repeat_mode == MediaPlaybackAutoRepeatMode::Track {
        "single"
    } else {
        "sequence"
    };
    Some(Track {
        title: props.Title().map(|h| h.to_string()).unwrap_or_default(),
        artist: props.Artist().map(|h| h.to_string()).unwrap_or_default(),
        playing,
        play_mode: play_mode.into(),
    })
}

fn track_signature(track: Option<&Track>) -> String {
    track
        .map(|t| format!("{}|{}|{}|{}", t.title, t.artist, t.playing, t.play_mode))
        .unwrap_or_default()
}

fn act(f: impl FnOnce(&Session) -> windows::core::Result<()>) -> Result<(), String> {
    let s = find_session()
        .map_err(|e| e.to_string())?
        .ok_or("没有可控制的媒体会话")?;
    f(&s).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn media_toggle() -> Result<(), String> {
    // Try* 系列返回 IAsyncOperation<bool>，bool 只表示命令是否被接受
    act(|s| s.TryTogglePlayPauseAsync()?.get().map(|_| ()))
}

#[tauri::command]
pub fn media_next() -> Result<(), String> {
    act(|s| s.TrySkipNextAsync()?.get().map(|_| ()))
}

#[tauri::command]
pub fn media_prev() -> Result<(), String> {
    act(|s| s.TrySkipPreviousAsync()?.get().map(|_| ()))
}

fn accepted(accepted: bool, mode: &str) -> Result<(), windows::core::Error> {
    if accepted {
        Ok(())
    } else {
        Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005_u32 as i32),
            format!("当前播放器不支持{mode}"),
        ))
    }
}

#[tauri::command]
pub fn media_set_mode(mode: String) -> Result<String, String> {
    let requested = mode.clone();
    act(|s| match mode.as_str() {
        "single" => {
            accepted(s.TryChangeShuffleActiveAsync(false)?.get()?, "单曲循环")?;
            accepted(
                s.TryChangeAutoRepeatModeAsync(MediaPlaybackAutoRepeatMode::Track)?
                    .get()?,
                "单曲循环",
            )
        }
        "shuffle" => {
            accepted(
                s.TryChangeAutoRepeatModeAsync(MediaPlaybackAutoRepeatMode::None)?
                    .get()?,
                "随机播放",
            )?;
            accepted(s.TryChangeShuffleActiveAsync(true)?.get()?, "随机播放")
        }
        "sequence" => {
            accepted(s.TryChangeShuffleActiveAsync(false)?.get()?, "顺序播放")?;
            accepted(
                s.TryChangeAutoRepeatModeAsync(MediaPlaybackAutoRepeatMode::None)?
                    .get()?,
                "顺序播放",
            )
        }
        _ => Err(windows::core::Error::new(
            windows::core::HRESULT(0x80070057_u32 as i32),
            "未知播放模式",
        )),
    })?;
    Ok(requested)
}

pub fn spawn_watcher(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        com_init();
        let mut last = String::new();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let track = read_track();
            let sig = track_signature(track.as_ref());
            if sig != last {
                last = sig;
                let _ = app.emit("media://update", &track);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{session_score, track_signature, Track};

    #[test]
    fn empty_media_session_has_empty_signature() {
        assert_eq!(track_signature(None), "");
    }

    #[test]
    fn signature_changes_with_playback_state() {
        let mut track = Track {
            title: "Song".into(),
            artist: "Artist".into(),
            playing: false,
            play_mode: "sequence".into(),
        };
        let paused = track_signature(Some(&track));
        track.playing = true;
        assert_ne!(paused, track_signature(Some(&track)));
    }

    #[test]
    fn playing_session_beats_paused_preferred_player() {
        assert!(session_score("spotify", true) > session_score("netease.cloudmusic", false));
        assert!(session_score("netease.cloudmusic", true) > session_score("spotify", true));
    }
}
