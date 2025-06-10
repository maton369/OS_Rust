#![no_std]
#![no_main]
#![feature(offset_of)]
#![feature(custom_test_frameworks)]
//#![test_runner(crate::test_runner::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::arch::asm;
use core::fmt::Write;
use core::mem::{offset_of, size_of};
use core::panic::PanicInfo;
use core::writeln;
use print::*;

use wasabi::{
    allocator, // allocator モジュール全体をインポート。これで `allocator::*` の代わりに `wasabi::allocator::*` を使う
    graphics::{self, draw_test_pattern, fill_rect},
    init::{self, init_basic_runtime, init_paging},
    print::{self, hexdump},
    qemu::{self, exit_qemu, QemuExitCode},
    serial::{self},
    //test_runner::{self, Testable},
    uefi::{
        self, init_vram, locate_loaded_image_protocol, EfiHandle, EfiMemoryDescriptor,
        EfiMemoryType, EfiSystemTable, VramTextWriter,
    },
    x86::{self, flush_tlb, hlt, init_exceptions, read_cr3, trigger_debug_interrupt, PageAttr},
};

use wasabi::{error, info, println, warn};

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

    let loaded_image_protocol = locate_loaded_image_protocol(image_handle, efi_system_table)
        .expect("Failed to get LoadedImageProtocol");
    println!("image_base: {:#018X}", loaded_image_protocol.image_base);
    println!("image_size: {:#018X}", loaded_image_protocol.image_size);

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

    writeln!(writer, "Hello, Non-UEFI world!").unwrap();
    let cr3 = wasabi::x86::read_cr3();
    println!("cr3 = {cr3:#p}");
    hexdump(unsafe { &*cr3 });

    let t = Some(unsafe { &*cr3 });
    println!("{t:?}");
    let t = t.and_then(|t| t.next_level(0));
    println!("{t:?}");
    let t = t.and_then(|t| t.next_level(0));
    println!("{t:?}");
    let t = t.and_then(|t| t.next_level(0));
    println!("{t:?}");

    let (_gdt, _idt) = init_exceptions();
    info!("Exception initialized!");
    trigger_debug_interrupt();
    info!("Execution continued.");
    init_paging(&memory_map);
    info!("Now we are using our own page tables!");

    let page_table = read_cr3();
    unsafe {
        (*page_table)
            .create_mapping(0, 4096, 0, PageAttr::NotPresent)
            .expect("Failed to unmap page 0");
    }
    flush_tlb();

    loop {
        hlt();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // パニック時の処理をここに記述
    // 例: エラーメッセージをシリアルポートに出力、画面に表示するなど

    // デバッグ目的でパニック情報を表示することもできます
    // use wasabi::print::println; // 必要ならインポート
    // println!("Panicked: {:?}", _info);

    // 致命的なエラーなので、CPUを停止させる
    loop {
        // パニックしたら無限ループで停止
        // CPUを停止させる `hlt` 命令があるならそれを使う
        wasabi::x86::hlt(); // wasabi::x86::hlt() を呼び出す
    }
}
