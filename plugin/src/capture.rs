use crate::resample::Resampler;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::{GUID, Result};

const FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
const STALL: Duration = Duration::from_secs(2);
const WAVE_FORMAT_IEEE_FLOAT: u32 = 3;
const WAVE_FORMAT_EXTENSIBLE: u32 = 0xFFFE;
const PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);

#[derive(Default)]
pub struct Stats {
    pub opens: AtomicU64,
    pub samples: AtomicU64,
    pub errors: AtomicU64,
    pub stalls: AtomicU64,
    pub last_error: std::sync::atomic::AtomicI32,
}

enum Sample {
    F32,
    I16,
    I32,
}

struct Stream {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: Event,
    channels: usize,
    sample: Sample,
    resampler: Resampler,
    device: String,
}

fn default_id(enumerator: &IMMDeviceEnumerator) -> Result<String> {
    unsafe {
        let id = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?.GetId()?;
        let s = id.to_string().unwrap_or_default();
        CoTaskMemFree(Some(id.0 as _));
        Ok(s)
    }
}

struct Format(*mut WAVEFORMATEX);

impl Drop for Format {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0 as _)) };
    }
}

struct Event(HANDLE);

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn open(enumerator: &IMMDeviceEnumerator) -> Result<Stream> {
    unsafe {
        let device = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let format = Format(client.GetMixFormat()?);
        let f = *format.0;
        let tag = f.wFormatTag as u32;
        let sub = if tag == WAVE_FORMAT_EXTENSIBLE {
            (*(format.0 as *const WAVEFORMATEXTENSIBLE)).SubFormat
        } else if tag == WAVE_FORMAT_IEEE_FLOAT {
            FLOAT
        } else {
            PCM
        };
        let sample = match (sub, f.wBitsPerSample) {
            (FLOAT, 32) => Sample::F32,
            (PCM, 16) => Sample::I16,
            (PCM, 32) => Sample::I32,
            _ => return Err(windows::core::Error::from_hresult(AUDCLNT_E_UNSUPPORTED_FORMAT)),
        };
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            10_000_000,
            0,
            format.0,
            None,
        )?;
        let event = Event(CreateEventW(None, false, false, None)?);
        client.SetEventHandle(event.0)?;
        let capture = client.GetService()?;
        let device = default_id(enumerator)?;
        client.Start()?;
        Ok(Stream {
            channels: f.nChannels as usize,
            sample,
            resampler: Resampler::new(f.nSamplesPerSec),
            event,
            capture,
            device,
            client,
        })
    }
}

impl Stream {
    fn drain(&mut self, mono: &mut Vec<f32>, out: &mut Vec<i16>) -> Result<()> {
        unsafe {
            loop {
                if self.capture.GetNextPacketSize()? == 0 {
                    return Ok(());
                }
                let mut data = std::ptr::null_mut();
                let mut frames = 0;
                let mut flags = 0;
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let n = frames as usize * self.channels;
                mono.clear();
                let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                let value = |i: usize| -> f32 {
                    match self.sample {
                        Sample::F32 => *(data as *const f32).add(i),
                        Sample::I16 => *(data as *const i16).add(i) as f32 / 32768.0,
                        Sample::I32 => *(data as *const i32).add(i) as f32 / 2147483648.0,
                    }
                };
                for frame in (0..n).step_by(self.channels) {
                    mono.push(if silent {
                        0.0
                    } else {
                        (0..self.channels).map(|c| value(frame + c)).sum::<f32>() / self.channels as f32
                    });
                }
                self.capture.ReleaseBuffer(frames)?;
                self.resampler.push(mono, out);
            }
        }
    }
}

pub fn run(stop: &AtomicBool, stats: &Stats, mut sink: impl FnMut(&[i16])) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let enumerator: Option<IMMDeviceEnumerator> =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok() };
    let Some(enumerator) = enumerator else {
        stats.errors.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let (mut mono, mut out) = (vec![], vec![]);
    while !stop.load(Ordering::Relaxed) {
        let mut stream = match open(&enumerator) {
            Ok(s) => s,
            Err(e) => {
                stats.errors.fetch_add(1, Ordering::Relaxed);
                stats.last_error.store(e.code().0, Ordering::Relaxed);
                sink(&[]);
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };
        stats.opens.fetch_add(1, Ordering::Relaxed);
        let mut checked = Instant::now();
        let mut heard = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            unsafe { WaitForSingleObject(stream.event.0, 10) };
            out.clear();
            if let Err(e) = stream.drain(&mut mono, &mut out) {
                stats.errors.fetch_add(1, Ordering::Relaxed);
                stats.last_error.store(e.code().0, Ordering::Relaxed);
                break;
            }
            if !out.is_empty() {
                heard = Instant::now();
                stats.samples.fetch_add(out.len() as u64, Ordering::Relaxed);
            }
            sink(&out);
            if heard.elapsed() > STALL {
                stats.stalls.fetch_add(1, Ordering::Relaxed);
                break;
            }
            if checked.elapsed() > Duration::from_secs(1) {
                checked = Instant::now();
                if default_id(&enumerator).ok().as_deref() != Some(stream.device.as_str()) {
                    break;
                }
            }
        }
        unsafe {
            let _ = stream.client.Stop();
        }
    }
}
