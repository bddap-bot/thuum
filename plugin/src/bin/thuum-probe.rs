#[cfg(windows)]
fn main() {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::core::{s, w};
    let seconds: u32 = std::env::args().nth(1).map(|a| a.parse().unwrap()).unwrap_or(5);
    let hold = match std::env::args().nth(2).as_deref() {
        None => true,
        Some("nohold") => false,
        Some(other) => panic!("unknown mode {other}"),
    };
    unsafe {
        let module = LoadLibraryW(w!("thuum.dll")).expect("load thuum.dll");
        let probe = GetProcAddress(module, s!("thuum_probe")).expect("thuum_probe export");
        let probe: extern "C" fn(u32, u32, *mut u8, usize) -> usize = std::mem::transmute(probe);
        let mut buffer = vec![0u8; 16 << 20];
        let n = probe(seconds, hold as u32, buffer.as_mut_ptr(), buffer.len());
        println!("{}", String::from_utf8_lossy(&buffer[..n.min(buffer.len())]));
    }
}

#[cfg(not(windows))]
fn main() {}
