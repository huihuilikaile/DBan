//! Windows SMTC media discovery, filtering, playback state, and controls.
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::{Emitter, Manager as _, State};
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
    pub source_id: String,
    pub source_name: String,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaSource {
    pub id: String,
    pub name: String,
    pub playing: bool,
    pub title: String,
    pub artist: String,
}

#[derive(Clone, Default)]
struct MediaFilter {
    selected_only: bool,
    source_ids: HashSet<String>,
}

impl MediaFilter {
    fn allows(&self, source_id: &str) -> bool {
        !self.selected_only || self.source_ids.contains(&normalize_source_id(source_id))
    }
}

#[derive(Default)]
pub struct MediaState {
    filter: Mutex<MediaFilter>,
}

fn com_init() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    unsafe {
        // S_FALSE (already initialized) and RPC_E_CHANGED_MODE are harmless here.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn normalize_source_id(source_id: &str) -> String {
    source_id.trim().to_ascii_lowercase()
}

fn source_name(source_id: &str) -> String {
    let normalized = normalize_source_id(source_id);
    if normalized.contains("netease") || normalized.contains("cloudmusic") {
        "网易云音乐".into()
    } else if normalized.contains("qishui")
        || normalized.contains("luna")
        || normalized.contains("douyinmusic")
    {
        "汽水音乐".into()
    } else if normalized.contains("qqmusic") {
        "QQ音乐".into()
    } else if normalized.contains("kugou") {
        "酷狗音乐".into()
    } else if normalized.contains("kuwo") {
        "酷我音乐".into()
    } else if normalized.contains("spotify") {
        "Spotify".into()
    } else if normalized.contains("chrome") {
        "Google Chrome".into()
    } else if normalized.contains("msedge") || normalized.contains("microsoftedge") {
        "Microsoft Edge".into()
    } else if normalized.contains("firefox") {
        "Firefox".into()
    } else if source_id.trim().is_empty() {
        "未知播放器".into()
    } else {
        source_id.trim().into()
    }
}

fn session_source_id(session: &Session) -> String {
    session
        .SourceAppUserModelId()
        .map(|value| value.to_string())
        .unwrap_or_default()
}

fn session_playing(session: &Session) -> bool {
    session
        .GetPlaybackInfo()
        .ok()
        .and_then(|playback| playback.PlaybackStatus().ok())
        .is_some_and(|status| {
            status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
        })
}

fn session_score(source_id: &str, playing: bool) -> u8 {
    let normalized = normalize_source_id(source_id);
    let preferred_music_app = normalized.contains("netease")
        || normalized.contains("cloudmusic")
        || normalized.contains("qishui")
        || normalized.contains("luna");
    u8::from(playing) * 2 + u8::from(preferred_music_app)
}

fn filter_snapshot(state: &MediaState) -> MediaFilter {
    state.filter.lock().unwrap().clone()
}

fn find_session(filter: &MediaFilter) -> windows::core::Result<Option<Session>> {
    let manager = Manager::RequestAsync()?.get()?;
    let sessions = manager.GetSessions()?;
    let mut best: Option<(u8, Session)> = None;
    for session in sessions {
        let source_id = session_source_id(&session);
        if !filter.allows(&source_id) {
            continue;
        }
        let score = session_score(&source_id, session_playing(&session));
        if best.as_ref().is_none_or(|(current, _)| score > *current) {
            best = Some((score, session));
        }
    }
    Ok(best.map(|(_, session)| session))
}

fn read_track(filter: &MediaFilter) -> Option<Track> {
    let session = find_session(filter).ok()??;
    let source_id = session_source_id(&session);
    let properties = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let playback = session.GetPlaybackInfo().ok()?;
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
        title: properties
            .Title()
            .map(|value| value.to_string())
            .unwrap_or_default(),
        artist: properties
            .Artist()
            .map(|value| value.to_string())
            .unwrap_or_default(),
        playing,
        play_mode: play_mode.into(),
        source_name: source_name(&source_id),
        source_id,
    })
}

fn list_sources() -> windows::core::Result<Vec<MediaSource>> {
    let manager = Manager::RequestAsync()?.get()?;
    let sessions = manager.GetSessions()?;
    let mut sources = Vec::<MediaSource>::new();
    for session in sessions {
        let id = session_source_id(&session);
        if id.trim().is_empty() {
            continue;
        }
        let playing = session_playing(&session);
        let (title, artist) = session
            .TryGetMediaPropertiesAsync()
            .ok()
            .and_then(|operation| operation.get().ok())
            .map(|properties| {
                (
                    properties
                        .Title()
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    properties
                        .Artist()
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                )
            })
            .unwrap_or_default();

        if let Some(existing) = sources
            .iter_mut()
            .find(|source| normalize_source_id(&source.id) == normalize_source_id(&id))
        {
            if playing || existing.title.is_empty() {
                existing.playing = playing;
                existing.title = title;
                existing.artist = artist;
            }
            continue;
        }
        sources.push(MediaSource {
            name: source_name(&id),
            id,
            playing,
            title,
            artist,
        });
    }
    sources.sort_by(|left, right| {
        right
            .playing
            .cmp(&left.playing)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(sources)
}

fn track_signature(track: Option<&Track>) -> String {
    track
        .map(|track| {
            format!(
                "{}|{}|{}|{}|{}",
                track.source_id, track.title, track.artist, track.playing, track.play_mode
            )
        })
        .unwrap_or_default()
}

fn act(
    state: &MediaState,
    action: impl FnOnce(&Session) -> windows::core::Result<()>,
) -> Result<(), String> {
    com_init();
    let filter = filter_snapshot(state);
    let session = find_session(&filter)
        .map_err(|error| error.to_string())?
        .ok_or(if filter.selected_only {
            "指定的播放器当前没有可控制媒体会话"
        } else {
            "没有可控制的媒体会话"
        })?;
    action(&session).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn media_list_sources() -> Result<Vec<MediaSource>, String> {
    com_init();
    list_sources().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn media_set_filter(
    app: tauri::AppHandle,
    state: State<MediaState>,
    selected_only: bool,
    source_ids: Vec<String>,
) -> Option<Track> {
    let source_ids = source_ids
        .into_iter()
        .map(|source_id| normalize_source_id(&source_id))
        .filter(|source_id| !source_id.is_empty())
        .collect();
    *state.filter.lock().unwrap() = MediaFilter {
        selected_only,
        source_ids,
    };
    com_init();
    let track = read_track(&filter_snapshot(&state));
    let _ = app.emit("media://update", &track);
    track
}

#[tauri::command]
pub fn media_toggle(state: State<MediaState>) -> Result<(), String> {
    act(&state, |session| {
        session.TryTogglePlayPauseAsync()?.get().map(|_| ())
    })
}

#[tauri::command]
pub fn media_next(state: State<MediaState>) -> Result<(), String> {
    act(&state, |session| {
        session.TrySkipNextAsync()?.get().map(|_| ())
    })
}

#[tauri::command]
pub fn media_prev(state: State<MediaState>) -> Result<(), String> {
    act(&state, |session| {
        session.TrySkipPreviousAsync()?.get().map(|_| ())
    })
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
pub fn media_set_mode(state: State<MediaState>, mode: String) -> Result<String, String> {
    let requested = mode.clone();
    act(&state, |session| match mode.as_str() {
        "single" => {
            accepted(
                session.TryChangeShuffleActiveAsync(false)?.get()?,
                "单曲循环",
            )?;
            accepted(
                session
                    .TryChangeAutoRepeatModeAsync(MediaPlaybackAutoRepeatMode::Track)?
                    .get()?,
                "单曲循环",
            )
        }
        "shuffle" => {
            accepted(
                session
                    .TryChangeAutoRepeatModeAsync(MediaPlaybackAutoRepeatMode::None)?
                    .get()?,
                "随机播放",
            )?;
            accepted(
                session.TryChangeShuffleActiveAsync(true)?.get()?,
                "随机播放",
            )
        }
        "sequence" => {
            accepted(
                session.TryChangeShuffleActiveAsync(false)?.get()?,
                "顺序播放",
            )?;
            accepted(
                session
                    .TryChangeAutoRepeatModeAsync(MediaPlaybackAutoRepeatMode::None)?
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
            let filter = filter_snapshot(&app.state::<MediaState>());
            let track = read_track(&filter);
            let signature = track_signature(track.as_ref());
            if signature != last {
                last = signature;
                let _ = app.emit("media://update", &track);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_source_id, session_score, source_name, track_signature, MediaFilter, Track,
    };
    use std::collections::HashSet;

    #[test]
    fn empty_media_session_has_empty_signature() {
        assert_eq!(track_signature(None), "");
    }

    #[test]
    fn signature_changes_with_playback_state_and_source() {
        let mut track = Track {
            title: "Song".into(),
            artist: "Artist".into(),
            playing: false,
            play_mode: "sequence".into(),
            source_id: "netease.cloudmusic".into(),
            source_name: "网易云音乐".into(),
        };
        let paused = track_signature(Some(&track));
        track.playing = true;
        assert_ne!(paused, track_signature(Some(&track)));
        track.source_id = "qishui.music".into();
        assert_ne!(paused, track_signature(Some(&track)));
    }

    #[test]
    fn playing_session_beats_paused_preferred_player() {
        assert!(session_score("spotify", true) > session_score("netease.cloudmusic", false));
        assert!(session_score("netease.cloudmusic", true) > session_score("spotify", true));
    }

    #[test]
    fn selected_filter_matches_source_ids_case_insensitively() {
        let filter = MediaFilter {
            selected_only: true,
            source_ids: HashSet::from([normalize_source_id("NetEase.CloudMusic")]),
        };
        assert!(filter.allows("netease.cloudmusic"));
        assert!(!filter.allows("spotify"));
    }

    #[test]
    fn all_sources_filter_accepts_every_session() {
        assert!(MediaFilter::default().allows("any.player"));
    }

    #[test]
    fn recognizes_known_music_players() {
        assert_eq!(source_name("NetEase.CloudMusic"), "网易云音乐");
        assert_eq!(source_name("com.luna.music"), "汽水音乐");
    }
}
