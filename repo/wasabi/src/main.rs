#![no_std]
#![no_main]
#![feature(offset_of)]
#![feature(custom_test_frameworks)]
//#![test_runner(crate::test_runner::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::time::Duration;
use wasabi::print::*; // `Write` トレイトをインポートして、`write!` マクロを使えるようにする

use wasabi::{
    executor::{sleep, spawn_global, start_global_executor},
    hpet::{global_timestamp, Hpet, HpetRegisters},
    init::{init_allocator, init_basic_runtime, init_display, init_hpet, init_paging, init_pci},
    serial::SerialPort,
    uefi::{init_vram, locate_loaded_image_protocol, EfiHandle, EfiSystemTable},
};

use wasabi::x86::init_exceptions;
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
    use wasabi::hpet::set_global_hpet;

    println!("Booting WasabiOS...");

    let loaded_image_protocol = locate_loaded_image_protocol(image_handle, efi_system_table)
        .expect("Failed to get LoadedImageProtocol");

    info!("info");
    warn!("warn");
    error!("error");

    let mut vram = init_vram(efi_system_table).expect("init_vram failed");

    let acpi = efi_system_table.acpi_table().expect("ACPI table not found");

    init_display(&mut vram);

    set_global_vram(vram);

    set_global_hpet(Hpet::new(unsafe {
        &mut *(0x0000_0000_fed0_0000 as *mut HpetRegisters)
    }));

    // wasabi::print::set_serial_output(true); // Removed: function does not exist
    let mut memory_map = init_basic_runtime(image_handle, efi_system_table);

    info!("Hello, Non-UEFI world!");

    init_allocator(&memory_map);

    let (_gdt, _idt) = init_exceptions();
    init_paging(&memory_map);

    init_hpet(acpi);

    init_pci(acpi);

    init_hpet(acpi);
    let t0 = global_timestamp();
    let task1 = async move {
        for i in 100..=103 {
            info!("{i} hpet.main_counter = {:?}", global_timestamp() - t0);
            sleep(Duration::from_secs(1)).await;
        }
        Ok(())
    };
    let task2 = async move {
        for i in 200..=203 {
            info!("{i} hpet.main_counter = {:?}", global_timestamp() - t0);
            sleep(Duration::from_secs(2)).await;
        }
        Ok(())
    };

    let serial_task = async {
        let sp = SerialPort::default();
        if let Err(e) = sp.loopback_test() {
            error!("{e:?}");
            return Err("serial: loopback test failed");
        }
        info!("Started to monitor serial port");
        loop {
            if let Some(v) = sp.try_read() {
                let c = char::from_u32(v as u32);
                info!("serial input: {v:#04X} = {c:?}");
            }
            sleep(Duration::from_millis(20)).await;
        }
        #[allow(unreachable_code)]
        Ok(())
    };

    spawn_global(task1);
    spawn_global(task2);
    spawn_global(serial_task);
    start_global_executor();
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
