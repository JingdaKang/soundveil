//! Windows capture: a loopback stream on the default output, which hears what the speakers play.
//!
//! This needs no permission and works from Windows 10 on. Windows stops sending packets while
//! nothing is playing, so the quiet is filled in here, and it keeps a stream on the old output when
//! the default changes, so the default is checked once a second.

use super::Listener;
use crate::audio::{QuietClock, Resampler, SampleKind, mix_down};
use std::slice;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eConsole, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};
use windows::core::Result;

const BUFFER: i64 = 2_000_000; // 200 ms, in Windows's 100 ns units.
const PAUSE: Duration = Duration::from_millis(20);
const RETRY: Duration = Duration::from_secs(1);
const CHECK_DEFAULT: Duration = Duration::from_secs(1);

const FORMAT_PCM: u16 = 1;
const FORMAT_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;

struct Layout {
    rate: u32,
    channels: usize,
    kind: SampleKind,
}

/// # Safety
/// `format` must point at the format Windows returned, which may be the longer extensible form.
unsafe fn read_layout(format: *const WAVEFORMATEX) -> Option<Layout> {
    let basic = unsafe { format.read_unaligned() };
    let bits = basic.wBitsPerSample;
    let tag = match basic.wFormatTag {
        FORMAT_EXTENSIBLE => {
            let long = unsafe { format.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned() };
            // The first field of these format GUIDs is the plain format tag.
            let first = { long.SubFormat }.data1;
            u16::try_from(first).ok()?
        }
        tag => tag,
    };
    let kind = match (tag, bits) {
        (FORMAT_FLOAT, 32) => SampleKind::Float32,
        (FORMAT_PCM, 16) => SampleKind::Int16,
        (FORMAT_PCM, 24) => SampleKind::Int24,
        (FORMAT_PCM, 32) => SampleKind::Int32,
        _ => return None,
    };
    // Some drivers pad 24-bit audio to 32, which must not be read as packed.
    if usize::from(basic.nBlockAlign) != kind.bytes() * usize::from(basic.nChannels) {
        return None;
    }
    Some(Layout {
        rate: basic.nSamplesPerSec,
        channels: basic.nChannels.into(),
        kind,
    })
}

struct Stream {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    layout: Layout,
    id: String,
}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: the client is alive and this is the last use of it.
        let _ = unsafe { self.client.Stop() };
    }
}

fn device_id(device: &IMMDevice) -> Result<String> {
    // SAFETY: Windows gives back a string for the caller to free, freed here after copying.
    unsafe {
        let id = device.GetId()?;
        let text = id.to_string().unwrap_or_default();
        CoTaskMemFree(Some(id.as_ptr().cast()));
        Ok(text)
    }
}

fn default_output(enumerator: &IMMDeviceEnumerator) -> Result<IMMDevice> {
    // SAFETY: a plain call on a live COM object.
    unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
}

fn open(enumerator: &IMMDeviceEnumerator) -> std::result::Result<Stream, String> {
    let device = default_output(enumerator).map_err(|_| "No audio output was found.".to_string())?;
    let describe = |error: windows::core::Error| format!("Cannot capture audio: {error}");
    // SAFETY: each call is made on live COM objects from the thread that set COM up, and the
    // format stays allocated until after the stream has been set up with it.
    unsafe {
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(describe)?;
        let format = client.GetMixFormat().map_err(describe)?;
        let layout = read_layout(format);
        let started = client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            BUFFER,
            0,
            format,
            None,
        );
        CoTaskMemFree(Some(format.cast()));
        started.map_err(describe)?;
        let layout = layout.ok_or("This audio output uses a format Soundveil can't read.")?;
        let capture: IAudioCaptureClient = client.GetService().map_err(describe)?;
        client.Start().map_err(describe)?;
        let id = device_id(&device).map_err(describe)?;
        Ok(Stream {
            client,
            capture,
            layout,
            id,
        })
    }
}

/// What came out of one read of the stream.
struct Packets {
    any: bool,
    lost: bool,
}

/// Reads everything waiting, as 16 kHz samples.
fn drain(stream: &Stream, resampler: &mut Resampler, out: &mut Vec<f32>) -> Result<Packets> {
    let mut packets = Packets {
        any: false,
        lost: false,
    };
    let mut mixed = Vec::new();
    let layout = &stream.layout;
    let frame = layout.kind.bytes() * layout.channels;
    // SAFETY: Windows owns the buffer from GetBuffer until ReleaseBuffer, for `frames` frames, and
    // it is only read between the two.
    unsafe {
        while stream.capture.GetNextPacketSize()? > 0 {
            let mut data = std::ptr::null_mut();
            let mut frames = 0;
            let mut flags = 0;
            stream
                .capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
            mixed.clear();
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                mixed.resize(frames as usize, 0.0);
            } else {
                let bytes = slice::from_raw_parts(data, frames as usize * frame);
                mix_down(bytes, layout.kind, layout.channels, &mut mixed);
            }
            packets.lost |= flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0;
            packets.any = true;
            stream.capture.ReleaseBuffer(frames)?;
            resampler.process(&mixed, out);
        }
    }
    Ok(packets)
}

fn listen(
    stream: &Stream,
    enumerator: &IMMDeviceEnumerator,
    listener: &mut Listener,
    stop: &AtomicBool,
) -> Result<()> {
    let mut resampler = Resampler::new(stream.layout.rate);
    let mut quiet = QuietClock::new(Instant::now());
    let mut checked = Instant::now();
    let mut samples = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(PAUSE);
        samples.clear();
        let packets = drain(stream, &mut resampler, &mut samples)?;
        let now = Instant::now();
        if packets.lost {
            listener.gap(Duration::ZERO);
        }
        if packets.any {
            quiet.heard(now);
            listener.hear(&samples);
        } else {
            let missing = quiet.missing(now);
            if missing > 0 {
                listener.hear(&vec![0.0; missing]);
            }
        }
        if now.duration_since(checked) >= CHECK_DEFAULT {
            checked = now;
            let device = default_output(enumerator)?;
            if device_id(&device)? != stream.id {
                return Ok(());
            }
        }
    }
    Ok(())
}

pub fn run(listener: &mut Listener, stop: &AtomicBool, opened: Sender<std::result::Result<(), String>>) {
    // SAFETY: paired with CoUninitialize below, on this same thread.
    let ready = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if ready.is_err() {
        let _ = opened.send(Err(format!("Cannot start audio capture: {ready:?}")));
        return;
    }
    // SAFETY: COM is set up on this thread.
    let enumerator: Result<IMMDeviceEnumerator> =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) };
    match enumerator {
        Ok(enumerator) => {
            let mut stream = match open(&enumerator) {
                Ok(stream) => {
                    let _ = opened.send(Ok(()));
                    Some(stream)
                }
                Err(error) => {
                    let _ = opened.send(Err(error));
                    None
                }
            };
            // A stream ends when the output is unplugged or the default changes. Capture carries
            // on with whichever output is the default by then.
            while let Some(current) = stream.take() {
                let ended = listen(&current, &enumerator, listener, stop);
                drop(current);
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                if ended.is_err() {
                    listener.status("Audio output changed. Reconnecting…");
                }
                listener.gap(Duration::ZERO);
                let lost = Instant::now();
                while stream.is_none() && !stop.load(Ordering::Relaxed) {
                    match open(&enumerator) {
                        Ok(next) => {
                            listener.gap(lost.elapsed());
                            listener.status("Listening");
                            stream = Some(next);
                        }
                        Err(_) => thread::sleep(RETRY),
                    }
                }
            }
        }
        Err(error) => {
            let _ = opened.send(Err(format!("Cannot reach Windows audio: {error}")));
        }
    }
    // SAFETY: everything using COM on this thread has been dropped.
    unsafe { CoUninitialize() };
}
