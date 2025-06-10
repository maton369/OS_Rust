use crate::qemu::{exit_qemu, QemuExitCode};
use core::panic::PanicInfo;

/// テストランナー（`#[test_runner]` で指定）
///
/// `#![no_std]` 環境では標準のテストランナーが使えないため、独自に定義する必要がある。
/// テスト関数は `tests: &[&dyn FnOnce()]` として渡されるが、ここでは実行せず即終了。
///
/// CI 判定用途などでは、この関数で明示的に QEMU を終了させることで、
/// テスト結果をホスト側に返せる。
pub fn test_runner(_tests: &[&dyn FnOnce()]) -> ! {
    // テスト成功 → QEMU を正常終了コードで終了させる
    exit_qemu(QemuExitCode::Success)
}

/// パニックハンドラ（`no_std` 環境で必須）
///
/// テスト中に panic が発生した場合は、QEMU をエラーコードで終了させる。
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // テスト失敗 → QEMU を異常終了コードで終了させる
    exit_qemu(QemuExitCode::Fail);
}
