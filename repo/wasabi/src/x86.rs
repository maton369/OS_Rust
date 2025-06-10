use core::arch::asm;

/// `hlt` 命令を発行して CPU を停止（アイドル状態）させる
///
/// 通常、割り込み待機や busy-loop の終了などに使われる。
/// 無限ループ内で使うことで、CPU 負荷を下げる効果がある。
pub fn hlt() {
    unsafe {
        asm!("hlt");
    }
}

pub fn busy_loop_hint() {
    // CPU のアイドル状態を示すヒントを与える
    // 具体的な実装はアーキテクチャに依存するが、通常は何もしない。
    unsafe {
        asm!("pause");
    }
}

pub fn read_io_port_u8(port: u16) -> u8 {
    let mut data: u8;
    unsafe {
        asm!(
            "in al, dx",
            out("al") data,
            in("dx") port
        );
    }
    data
}

/// 指定した I/O ポートに 8ビットデータを書き込む
///
/// # 引数
/// - `port`: 書き込み対象の I/O ポート番号
/// - `data`: 書き込む 8ビットデータ
///
/// # 安全性
/// - 呼び出し元が、指定したポートが有効かつ安全にアクセス可能であることを保証する必要がある。
pub fn write_io_port_u8(port: u16, data: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("al") data,
            in("dx") port
        );
    }
}
