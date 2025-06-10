#![no_std]
#![no_main]
#![feature(offset_of)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner::test_runner)]
#![reexport_test_harness_main = "test_main"]

mod allocator;
mod graphics;
mod init;
mod print;
mod qemu;
mod result;
mod serial;
mod test_runner;
#[cfg(test)]
mod tests;
mod uefi;
mod x86;

use core::arch::asm;
use core::fmt::Write;
use core::mem::{offset_of, size_of};
use core::panic::PanicInfo;
use core::writeln;
use print::*;

use graphics::*;
use init::*;
use qemu::*;
use serial::*;
use test_runner::*;
#[cfg(test)]
use tests::*;
use uefi::*;

/// CPU を停止する命令
pub fn hlt() {
    unsafe {
        asm!("hlt");
    }
}

/// テストモード用の UEFI エントリポイント
#[cfg(feature = "test")]
#[no_mangle]
pub extern "C" fn efi_main(_image_handle: usize, _system_table: usize) -> ! {
    crate::tests::test_main();
    loop {
        hlt();
    }
}

/// 通常時の UEFI エントリポイント
#[cfg(not(feature = "test"))]
#[no_mangle]
pub extern "C" fn efi_main(image_handle: EfiHandle, efi_system_table: &EfiSystemTable) -> ! {
    println!("Booting WasabiOS...");
    println!("image_handle: {:#018X}", image_handle);
    println!("efi_system_table: {:#p}", efi_system_table);

    info!("info");
    warn!("warn");
    error!("error");
    hexdump(efi_system_table);

    let mut vram = init_vram(efi_system_table).expect("init_vram failed");

    let (vw, vh) = (vram.width, vram.height);
    fill_rect(&mut vram, 0x000000, 0, 0, vw, vh).expect("fill_rect failed");

    draw_test_pattern(&mut vram);

    let mut writer = VramTextWriter::new(&mut vram);
    let mut memory_map = init_basic_runtime(image_handle, efi_system_table);

    let mut total_pages = 0;
    for desc in memory_map.iter() {
        if desc.memory_type != EfiMemoryType::CONVENTIONAL_MEMORY {
            continue;
        }
        total_pages += desc.number_of_pages;
        writeln!(writer, "{desc:?}").unwrap();
    }

    let total_mib = total_pages * 4096 / 1024 / 1024;
    writeln!(writer, "Total: {total_pages} pages = {total_mib} MiB").unwrap();

    loop {
        hlt();
    }
}
