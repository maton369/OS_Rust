use crate::qemu::*;
use crate::serial::SerialPort;
use core::any::type_name;
use core::fmt::Write;
use core::panic::PanicInfo;

/// 各テスト関数をラップしてログ出力できるようにする trait
pub trait Testable {
    fn run(&self, writer: &mut SerialPort);
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self, writer: &mut SerialPort) {
        writeln!(writer, "[RUNNING] >>> {}", type_name::<T>()).unwrap();
        self();
        writeln!(writer, "[PASS   ] <<< {}", type_name::<T>()).unwrap();
    }
}

/// テストランナー本体（cargo testのように使われる）
pub fn test_runner(tests: &[&dyn Testable]) -> ! {
    let mut sw = SerialPort::new_for_com1();
    sw.init();
    writeln!(sw, "Running {} tests...", tests.len()).unwrap();

    for test in tests {
        test.run(&mut sw);
    }

    writeln!(sw, "All tests passed.").unwrap();

    exit_qemu(QemuExitCode::Success); // <- これで `!` を返す
}

/// panic発生時の処理（テスト失敗と見なして終了）
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut sw = SerialPort::new_for_com1();
    sw.init();
    writeln!(sw, "PANIC during test: {info:?}").unwrap();
    exit_qemu(QemuExitCode::Fail);
}
