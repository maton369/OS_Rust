use crate::graphics::BitmapTextWriter;
use crate::serial::SerialPort;
use crate::uefi::VramBufferInfo;

// SAFETY: This is safe because we are in a single-threaded UEFI environment
unsafe impl Sync for VramBufferInfo {}
// SAFETY: VramBufferInfo is only used with Mutex in a single-threaded context.
unsafe impl Send for VramBufferInfo {}
use core::fmt;
use core::fmt::Write;
use core::mem::size_of;
use core::slice;
use spin::Mutex;

static GLOBAL_VRAM_WRITER: Mutex<Option<BitmapTextWriter<VramBufferInfo>>> = Mutex::new(None);
pub fn set_global_vram(vram: VramBufferInfo) {
    assert!(GLOBAL_VRAM_WRITER.lock().is_none());
    let w = BitmapTextWriter::new(vram);
    *GLOBAL_VRAM_WRITER.lock() = Some(w);
}
pub fn global_print(args: fmt::Arguments) {
    let mut writer = SerialPort::default();
    fmt::write(&mut writer, args).unwrap();
    if let Some(w) = &mut *GLOBAL_VRAM_WRITER.lock() {
        fmt::write(w, args).expect("Failed to write to GLOBAL_VRAM_WRITER");
    }
}

#[macro_export]
macro_rules! print {
        ($($arg:tt)*) => ($crate::print::global_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
        () => ($crate::print!("\n"));
            ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! info {
            ($($arg:tt)*) => ($crate::print!("[INFO]  {}:{:<3}: {}\n",
                    file!(), line!(), format_args!($($arg)*)));
}

#[macro_export]
macro_rules! warn {
            ($($arg:tt)*) => ($crate::print!("[WARN]  {}:{:<3}: {}\n",
                    file!(), line!(), format_args!($($arg)*)));
}

#[macro_export]
macro_rules! error {
            ($($arg:tt)*) => ($crate::print!("[ERROR] {}:{:<3}: {}\n",
                    file!(), line!(), format_args!($($arg)*)));
}

fn hexdump_bytes(bytes: &[u8]) {
    let mut i = 0;
    let mut ascii = [0u8; 16];
    let mut offset = 0;
    for v in bytes.iter() {
        if i == 0 {
            print!("{offset:08X}: ");
        }
        print!("{:02X} ", v);
        ascii[i] = *v;
        i += 1;
        if i == 16 {
            print!("|");
            for c in ascii.iter() {
                print!(
                    "{}",
                    match c {
                        0x20..=0x7e => {
                            *c as char
                        }
                        _ => {
                            '.'
                        }
                    }
                );
            }
            println!("|");
            offset += 16;
            i = 0;
        }
    }
    if i != 0 {
        let old_i = i;
        while i < 16 {
            print!("   ");
            i += 1;
        }
        print!("|");
        for c in ascii[0..old_i].iter() {
            print!(
                "{}",
                if (0x20u8..=0x7fu8).contains(c) {
                    *c as char
                } else {
                    '.'
                }
            );
        }
        println!("|");
    }
}

pub fn hexdump<T: Sized>(data: &T) {
    hexdump_bytes(unsafe { slice::from_raw_parts(data as *const T as *const u8, size_of::<T>()) })
}

/// 任意の fmt::Write 先に hexdump を出力する
pub fn hexdump_to<T: Sized>(data: &T, writer: &mut dyn fmt::Write) {
    let bytes = unsafe { slice::from_raw_parts(data as *const T as *const u8, size_of::<T>()) };

    let mut i = 0;
    let mut ascii = [0u8; 16];
    let mut offset = 0;
    for &b in bytes {
        if i == 0 {
            let _ = write!(writer, "{offset:08X}: ");
        }
        let _ = write!(writer, "{:02X} ", b);
        ascii[i] = b;
        i += 1;

        if i == 16 {
            let _ = write!(writer, "|");
            for &c in &ascii {
                let ch = if (0x20..=0x7E).contains(&c) {
                    c as char
                } else {
                    '.'
                };
                let _ = write!(writer, "{ch}");
            }
            let _ = writeln!(writer, "|");
            offset += 16;
            i = 0;
        }
    }

    if i != 0 {
        for _ in i..16 {
            let _ = write!(writer, "   ");
        }
        let _ = write!(writer, "|");
        for &c in &ascii[..i] {
            let ch = if (0x20..=0x7E).contains(&c) {
                c as char
            } else {
                '.'
            };
            let _ = write!(writer, "{ch}");
        }
        let _ = writeln!(writer, "|");
    }
}
