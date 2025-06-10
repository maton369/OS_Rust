use crate::serial::SerialPort;
use crate::x86::hlt;
use crate::x86::write_io_port_u8;
use core::fmt::Write;

/// QEMU の終了コード（isa-debug-exit デバイスに渡す）
///
/// QEMU 側の `isa-debug-exit` デバイスは、指定された I/O ポートに書き込まれる値に応じて
/// エミュレータを終了し、対応するホストの終了コードを返します。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x1, // QEMU は exit code 3 で終了する
    Fail = 0x2,    // QEMU は exit code 5 で終了する
}

/// QEMU の仮想 I/O ポートを使って強制終了する関数
///
/// QEMU に `-device isa-debug-exit,iobase=0xf4,iosize=0x01` を指定している前提。
/// I/O ポート `0xf4` に書き込むと QEMU が対応する終了コードで終了する。
///
/// # 引数
/// - `exit_code`: QemuExitCode 列挙型（Success または Fail）
///
/// # 戻り値
/// - 戻らない関数（`!`）：終了後は無限ループでCPU停止
pub fn exit_qemu(exit_code: QemuExitCode) -> ! {
    let mut port = SerialPort::new_for_com1();
    port.init();
    writeln!(port, "[EXIT] Sending exit code: {:?}", exit_code).ok();

    write_io_port_u8(0xf4, exit_code as u8);

    loop {
        hlt();
    }
}
