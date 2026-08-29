//! Windows 默认播放设备的系统主音量控制。
use std::ptr;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

fn endpoint() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        device.Activate(CLSCTX_ALL, None)
    }
}

fn volume_scalar(volume: u32) -> f32 {
    volume.min(100) as f32 / 100.0
}

#[tauri::command]
pub fn get_system_volume() -> Result<u32, String> {
    let value = unsafe {
        endpoint()
            .map_err(|e| e.to_string())?
            .GetMasterVolumeLevelScalar()
            .map_err(|e| e.to_string())?
    };
    Ok((value.clamp(0.0, 1.0) * 100.0).round() as u32)
}

#[tauri::command]
pub fn set_system_volume(volume: u32) -> Result<u32, String> {
    let value = volume_scalar(volume);
    unsafe {
        endpoint()
            .map_err(|e| e.to_string())?
            .SetMasterVolumeLevelScalar(value, ptr::null())
            .map_err(|e| e.to_string())?;
    }
    Ok((value * 100.0).round() as u32)
}

#[cfg(test)]
mod tests {
    use super::volume_scalar;

    #[test]
    fn percentage_conversion_is_bounded() {
        assert_eq!(volume_scalar(120), 1.0);
        assert_eq!(volume_scalar(0), 0.0);
        assert_eq!(volume_scalar(55), 0.55);
    }
}
