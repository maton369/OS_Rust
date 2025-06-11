extern crate alloc;

use crate::error;
use crate::info;
use crate::result::Result;
use alloc::boxed::Box;
use core::arch::asm;
use core::arch::global_asm;
use core::fmt;
use core::marker::PhantomData;
use core::mem::offset_of;
use core::mem::size_of;
use core::mem::size_of_val;
use core::mem::ManuallyDrop;
use core::mem::MaybeUninit;
use core::pin::Pin;

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

pub fn read_cr3() -> *mut PML4 {
    let mut cr3: *mut PML4;
    unsafe {
        asm!("mov rax, cr3",
            out("rax") cr3)
    }
    cr3
}

pub type RootPageTable = [u8; 1024];

pub const PAGE_SIZE: usize = 4096;
const ATTR_MASK: u64 = 0xFFF;
const ATTR_PRESENT: u64 = 1 << 0;
const ATTR_WRITABLE: u64 = 1 << 1;
const ATTR_WRITE_THROUGH: u64 = 1 << 3;
const ATTR_CACHE_DISABLE: u64 = 1 << 4;

#[derive(Debug, Copy, Clone)]
#[repr(u64)]
pub enum PageAttr {
    NotPresent = 0,
    ReadWriteKernel = ATTR_PRESENT | ATTR_WRITABLE,
    ReadWriteIo = ATTR_PRESENT | ATTR_WRITABLE | ATTR_WRITE_THROUGH | ATTR_CACHE_DISABLE,
}
#[derive(Debug, Eq, PartialEq)]
pub enum TranslationResult {
    PageMapped4K { phys: u64 },
    PageMapped2M { phys: u64 },
    PageMapped1G { phys: u64 },
}

#[repr(transparent)]
pub struct Entry<const LEVEL: usize, NEXT> {
    value: u64,
    next_type: PhantomData<NEXT>,
}
impl<const LEVEL: usize, NEXT> Entry<LEVEL, NEXT> {
    fn read_value(&self) -> u64 {
        self.value
    }
    fn is_present(&self) -> bool {
        (self.read_value() & (1 << 0)) != 0
    }
    fn is_writable(&self) -> bool {
        (self.read_value() & (1 << 1)) != 0
    }
    fn is_user(&self) -> bool {
        (self.read_value() & (1 << 2)) != 0
    }
    fn format(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "L{}Entry @ {:#p} {{ {:#018X} {}{}{} ",
            LEVEL,
            self,
            self.read_value(),
            if self.is_present() { "P" } else { "N" },
            if self.is_writable() { "W" } else { "R" },
            if self.is_user() { "U" } else { "S" }
        )?;
        write!(f, " }}")
    }
    fn table(&self) -> Result<&NEXT> {
        if self.is_present() {
            Ok(unsafe { &*((self.value & !ATTR_MASK) as *const NEXT) })
        } else {
            Err("Page Not Found")
        }
    }
    fn table_mut(&mut self) -> Result<&mut NEXT> {
        if self.is_present() {
            Ok(unsafe { &mut *((self.value & !ATTR_MASK) as *mut NEXT) })
        } else {
            Err("Page Not Found")
        }
    }
    fn set_page(&mut self, phys: u64, attr: PageAttr) -> Result<()> {
        if phys & ATTR_MASK != 0 {
            Err("phys is not aligned")
        } else {
            self.value = phys | attr as u64;
            Ok(())
        }
    }
    fn populate(&mut self) -> Result<&mut Self> {
        if self.is_present() {
            Err("Page is already populated")
        } else {
            let next: Box<NEXT> = Box::new(unsafe { MaybeUninit::zeroed().assume_init() });
            self.value = Box::into_raw(next) as u64 | PageAttr::ReadWriteKernel as u64;
            Ok(self)
        }
    }
    fn ensure_populated(&mut self) -> Result<&mut Self> {
        if self.is_present() {
            Ok(self)
        } else {
            self.populate()
        }
    }
}

impl<const LEVEL: usize, NEXT> fmt::Display for Entry<LEVEL, NEXT> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.format(f)
    }
}

impl<const LEVEL: usize, NEXT> fmt::Debug for Entry<LEVEL, NEXT> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.format(f)
    }
}

#[repr(align(4096))]
pub struct Table<const LEVEL: usize, NEXT> {
    entry: [Entry<LEVEL, NEXT>; 512],
}
impl<const LEVEL: usize, NEXT: core::fmt::Debug> Table<LEVEL, NEXT> {
    fn format(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "L{}Table @ {:#p} {{", LEVEL, self)?;
        for i in 0..512 {
            let e = &self.entry[i];
            if !e.is_present() {
                continue;
            }
            writeln!(f, "  entry[{:3}] = {:?}", i, e)?;
        }
        writeln!(f, "}}")
    }
    const fn index_shift() -> usize {
        (LEVEL - 1) * 9 + 12
    }
    pub fn next_level(&self, index: usize) -> Option<&NEXT> {
        self.entry.get(index).and_then(|e| e.table().ok())
    }
    fn calc_index(&self, addr: u64) -> usize {
        ((addr >> Self::index_shift()) & 0b1_1111_1111) as usize
    }
}

impl<const LEVEL: usize, NEXT: fmt::Debug> fmt::Debug for Table<LEVEL, NEXT> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.format(f)
    }
}

pub type PT = Table<1, [u8; PAGE_SIZE]>;
pub type PD = Table<2, PT>;
pub type PDPT = Table<3, PD>;
pub type PML4 = Table<4, PDPT>;

impl PML4 {
    pub fn new() -> Box<Self> {
        Box::new(Self::default())
    }
    fn default() -> Self {
        // This is safe since entries filled with 0 is valid.
        unsafe { MaybeUninit::zeroed().assume_init() }
    }
    pub fn create_mapping(
        &mut self,
        virt_start: u64,
        virt_end: u64,
        phys: u64,
        attr: PageAttr,
    ) -> Result<()> {
        let table = self;
        let mut addr = virt_start;
        loop {
            let index = table.calc_index(addr);
            let table = table.entry[index].ensure_populated()?.table_mut()?;
            loop {
                let index = table.calc_index(addr);
                let table = table.entry[index].ensure_populated()?.table_mut()?;
                loop {
                    let index = table.calc_index(addr);
                    let table = table.entry[index].ensure_populated()?.table_mut()?;
                    loop {
                        let index = table.calc_index(addr);
                        let pte = &mut table.entry[index];
                        let phys_addr = phys + addr - virt_start;
                        pte.set_page(phys_addr, attr)?;
                        addr = addr.wrapping_add(PAGE_SIZE as u64);
                        if index + 1 >= (1 << 9) || addr >= virt_end {
                            break;
                        }
                    }
                    if index + 1 >= (1 << 9) || addr >= virt_end {
                        break;
                    }
                }
                if index + 1 >= (1 << 9) || addr >= virt_end {
                    break;
                }
            }
            if index + 1 >= (1 << 9) || addr >= virt_end {
                break;
            }
        }
        Ok(())
    }
}

// x86-64カーネルの例外処理とセグメンテーション管理
//
// このモジュールは以下の機能を提供します：
// - セグメントレジスタの操作
// - 割り込み処理とIDT管理
// - GDTとTSSの管理

// =============================================================================
// セグメントレジスタ操作
// =============================================================================

/// セグメントレジスタ操作のための安全でないヘルパー関数群
pub mod segment_registers {
    use super::*;

    /// ESレジスタに値を書き込む
    ///
    /// # Safety
    /// 無効なセレクタを指定すると予期しない動作を引き起こす可能性があります
    pub unsafe fn write_es(selector: u16) {
        asm!("mov es, ax", in("ax") selector);
    }

    /// CSレジスタに値を書き込む
    ///
    /// # Safety
    /// 無効なCSを指定すると予期しない動作を引き起こす可能性があります
    pub unsafe fn write_cs(cs: u16) {
        // MOV命令ではCSレジスタを直接ロードできないため、far-jump(ljmp)を使用
        asm!(
            "lea rax, [rip + 2f]",  // ターゲットアドレス
            "push cx",              // スタックにファーポインタを構築
            "push rax",
            "ljmp [rsp]",
            "2:",
            "add rsp, 8 + 2",       // スタックのファーポインタをクリーンアップ
            in("cx") cs
        );
    }

    /// SSレジスタに値を書き込む
    ///
    /// # Safety
    /// 無効なセレクタを指定すると予期しない動作を引き起こす可能性があります
    pub unsafe fn write_ss(selector: u16) {
        asm!("mov ss, ax", in("ax") selector);
    }

    /// DSレジスタに値を書き込む
    ///
    /// # Safety
    /// 無効なセレクタを指定すると予期しない動作を引き起こす可能性があります
    pub unsafe fn write_ds(ds: u16) {
        asm!("mov ds, ax", in("ax") ds);
    }

    /// FSレジスタに値を書き込む
    ///
    /// # Safety
    /// 無効なセレクタを指定すると予期しない動作を引き起こす可能性があります
    pub unsafe fn write_fs(selector: u16) {
        asm!("mov fs, ax", in("ax") selector);
    }

    /// GSレジスタに値を書き込む
    ///
    /// # Safety
    /// 無効なセレクタを指定すると予期しない動作を引き起こす可能性があります
    pub unsafe fn write_gs(selector: u16) {
        asm!("mov gs, ax", in("ax") selector);
    }
}

// =============================================================================
// 割り込みコンテキスト構造体
// =============================================================================

/// FPUコンテキスト（512バイト）
#[repr(C)]
#[derive(Clone, Copy)]
struct FpuContext {
    data: [u8; 512],
}

/// 汎用レジスタコンテキスト
#[repr(C)]
#[derive(Clone, Copy)]
struct GeneralRegisterContext {
    rax: u64,
    rdx: u64,
    rbx: u64,
    rbp: u64,
    rsi: u64,
    rdi: u64,
    r8: u64,
    r9: u64,
    r10: u64,
    r11: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rcx: u64,
}

const _: () = assert!(size_of::<GeneralRegisterContext>() == (16 - 1) * 8);

/// 割り込み時のCPUコンテキスト
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct InterruptContext {
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

const _: () = assert!(size_of::<InterruptContext>() == 8 * 5);

/// 割り込み処理に必要な全情報
#[repr(C)]
#[derive(Clone, Copy)]
struct InterruptInfo {
    /// FPUコンテキスト（FXSAVE/FXRSTOR用、16バイト境界に配置）
    fpu_context: FpuContext,
    _dummy: u64,
    /// 汎用レジスタコンテキスト
    greg: GeneralRegisterContext,
    /// エラーコード
    error_code: u64,
    /// 割り込みコンテキスト
    ctx: InterruptContext,
}

const _: () = assert!(size_of::<InterruptInfo>() == (16 + 4 + 1) * 8 + 8 + 512);

impl fmt::Debug for InterruptInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "InterruptInfo {{
    rip: {:#018X}, CS: {:#06X},
    rsp: {:#018X}, SS: {:#06X},
    rbp: {:#018X},
    rflags: {:#018X},
    error_code: {:#018X},
    rax: {:#018X}, rcx: {:#018X},
    rdx: {:#018X}, rbx: {:#018X},
    rsi: {:#018X}, rdi: {:#018X},
    r8:  {:#018X}, r9:  {:#018X},
    r10: {:#018X}, r11: {:#018X},
    r12: {:#018X}, r13: {:#018X},
    r14: {:#018X}, r15: {:#018X},
}}",
            self.ctx.rip,
            self.ctx.cs,
            self.ctx.rsp,
            self.ctx.ss,
            self.greg.rbp,
            self.ctx.rflags,
            self.error_code,
            self.greg.rax,
            self.greg.rcx,
            self.greg.rdx,
            self.greg.rbx,
            self.greg.rsi,
            self.greg.rdi,
            self.greg.r8,
            self.greg.r9,
            self.greg.r10,
            self.greg.r11,
            self.greg.r12,
            self.greg.r13,
            self.greg.r14,
            self.greg.r15,
        )
    }
}

// =============================================================================
// 割り込みエントリポイント生成マクロ
// =============================================================================

/// エラーコードなしの割り込みエントリポイントを生成
macro_rules! interrupt_entrypoint {
    ($index:literal) => {
        global_asm!(concat!(
            ".global interrupt_entrypoint",
            stringify!($index),
            "\n",
            "interrupt_entrypoint",
            stringify!($index),
            ":\n",
            "    push 0                    # No error code\n",
            "    push rcx                  # Save rcx first to reuse\n",
            "    mov rcx, ",
            stringify!($index),
            "\n",
            "    jmp inthandler_common"
        ));
    };
}

/// エラーコードありの割り込みエントリポイントを生成
macro_rules! interrupt_entrypoint_with_ecode {
    ($index:literal) => {
        global_asm!(concat!(
            ".global interrupt_entrypoint",
            stringify!($index),
            "\n",
            "interrupt_entrypoint",
            stringify!($index),
            ":\n",
            "    push rcx                  # Save rcx first to reuse\n",
            "    mov rcx, ",
            stringify!($index),
            "\n",
            "    jmp inthandler_common"
        ));
    };
}

// 割り込みエントリポイントの定義
interrupt_entrypoint!(3); // Breakpoint
interrupt_entrypoint!(6); // Invalid Opcode
interrupt_entrypoint_with_ecode!(8); // Double Fault
interrupt_entrypoint_with_ecode!(13); // General Protection Fault
interrupt_entrypoint_with_ecode!(14); // Page Fault
interrupt_entrypoint!(32); // Timer Interrupt

// 外部関数として宣言
extern "sysv64" {
    fn interrupt_entrypoint3();
    fn interrupt_entrypoint6();
    fn interrupt_entrypoint8();
    fn interrupt_entrypoint13();
    fn interrupt_entrypoint14();
    fn interrupt_entrypoint32();
}

// 共通割り込みハンドラのアセンブリコード
global_asm!(
    r#"
.global inthandler_common
inthandler_common:
    # 汎用レジスタの保存（rspとrcxを除く）
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rdi
    push rsi
    push rbp
    push rbx
    push rdx
    push rax
    
    # FPU状態の保存
    sub rsp, 512 + 8
    fxsave64 [rsp]
    
    # 第1引数: 保存されたCPU状態へのポインタ
    mov rdi, rsp
    
    # スタックを16バイト境界に配置
    mov rbp, rsp
    and rsp, -16
    
    # 第2引数: 割り込み番号
    mov rsi, rcx
    
    call inthandler
    
    # スタックを復元
    mov rsp, rbp
    
    # FPU状態の復元
    fxrstor64 [rsp]
    add rsp, 512 + 8
    
    # 汎用レジスタの復元
    pop rax
    pop rdx
    pop rbx
    pop rbp
    pop rsi
    pop rdi
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    
    # rcxとエラーコードの復元
    pop rcx
    add rsp, 8      # エラーコード分
    
    iretq
"#
);

// =============================================================================
// 割り込みハンドラ
// =============================================================================

/// CR2レジスタの値を読み取る
pub fn read_cr2() -> u64 {
    let mut cr2: u64;
    unsafe {
        asm!("mov rax, cr2", out("rax") cr2);
    }
    cr2
}

/// メイン割り込みハンドラ
#[no_mangle]
extern "sysv64" fn inthandler(info: &InterruptInfo, index: usize) {
    error!("Interrupt Info: {:?}", info);
    error!("Exception {index:#04X}: ");

    match index {
        3 => {
            error!("Breakpoint");
        }
        6 => {
            error!("Invalid Opcode");
        }
        8 => {
            error!("Double Fault");
        }
        13 => {
            error!("General Protection Fault");
            let rip = info.ctx.rip;
            error!("Bytes @ RIP({rip:#018X}):");
            let rip = rip as *const u8;
            let bytes = unsafe { core::slice::from_raw_parts(rip, 16) };
            error!("  = {bytes:02X?}");
        }
        14 => {
            error!("Page Fault");
            error!("CR2={:#018X}", read_cr2());
            let error_code = info.error_code;
            error!(
                "Caused by: A {} mode {} on a {} page, page structures are {}",
                if error_code & 0b0000_0100 != 0 {
                    "user"
                } else {
                    "supervisor"
                },
                if error_code & 0b0001_0000 != 0 {
                    "instruction fetch"
                } else if error_code & 0b0010 != 0 {
                    "data write"
                } else {
                    "data read"
                },
                if error_code & 0b0001 != 0 {
                    "present"
                } else {
                    "non-present"
                },
                if error_code & 0b1000 != 0 {
                    "invalid"
                } else {
                    "valid"
                },
            );
        }
        _ => {
            error!("Not handled");
        }
    }

    panic!("fatal exception");
}

/// 未実装の割り込みハンドラ
#[no_mangle]
extern "sysv64" fn int_handler_unimplemented() {
    panic!("unexpected interrupt!");
}

// =============================================================================
// IDT（Interrupt Descriptor Table）管理
// =============================================================================

/// IDT エントリの属性定数
pub const BIT_FLAGS_INTGATE: u8 = 0b0000_1110u8;
pub const BIT_FLAGS_PRESENT: u8 = 0b1000_0000u8;
pub const BIT_FLAGS_DPL0: u8 = 0 << 5;
pub const BIT_FLAGS_DPL3: u8 = 3 << 5;

/// IDT エントリの属性
#[repr(u8)]
#[derive(Copy, Clone)]
enum IdtAttr {
    /// 非表示（MaybeUninit::zeroed()で未定義動作を避けるため）
    _NotPresent = 0,
    /// 割り込みゲート DPL0
    IntGateDPL0 = BIT_FLAGS_INTGATE | BIT_FLAGS_PRESENT | BIT_FLAGS_DPL0,
    /// 割り込みゲート DPL3
    IntGateDPL3 = BIT_FLAGS_INTGATE | BIT_FLAGS_PRESENT | BIT_FLAGS_DPL3,
}

/// IDT ディスクリプタ
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct IdtDescriptor {
    offset_low: u16,
    segment_selector: u16,
    ist_index: u8,
    attr: IdtAttr,
    offset_mid: u16,
    offset_high: u32,
    _reserved: u32,
}

const _: () = assert!(size_of::<IdtDescriptor>() == 16);

impl IdtDescriptor {
    fn new(
        segment_selector: u16,
        ist_index: u8,
        attr: IdtAttr,
        handler: unsafe extern "sysv64" fn(),
    ) -> Self {
        let handler_addr = handler as *const unsafe extern "sysv64" fn() as usize;
        Self {
            offset_low: handler_addr as u16,
            offset_mid: (handler_addr >> 16) as u16,
            offset_high: (handler_addr >> 32) as u32,
            segment_selector,
            ist_index,
            attr,
            _reserved: 0,
        }
    }
}

/// IDTR パラメータ
#[repr(C, packed)]
#[derive(Debug)]
struct IdtrParameters {
    limit: u16,
    base: *const IdtDescriptor,
}

const _: () = assert!(size_of::<IdtrParameters>() == 10);
const _: () = assert!(offset_of!(IdtrParameters, base) == 2);

/// IDT管理構造体
pub struct Idt {
    entries: Pin<Box<[IdtDescriptor; 0x100]>>,
}

impl Idt {
    /// 新しいIDTを作成し、CPUにロードする
    pub fn new(segment_selector: u16) -> Self {
        let mut entries = [IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            int_handler_unimplemented,
        ); 0x100];

        // 各例外ハンドラを設定
        entries[3] = IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL3, // ユーザーランドからのint3を許可
            interrupt_entrypoint3,
        );
        entries[6] = IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            interrupt_entrypoint6,
        );
        entries[8] = IdtDescriptor::new(
            segment_selector,
            2, // ダブルフォルトは別のスタックを使用
            IdtAttr::IntGateDPL0,
            interrupt_entrypoint8,
        );
        entries[13] = IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            interrupt_entrypoint13,
        );
        entries[14] = IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            interrupt_entrypoint14,
        );
        entries[32] = IdtDescriptor::new(
            segment_selector,
            1,
            IdtAttr::IntGateDPL0,
            interrupt_entrypoint32,
        );

        let entries = Box::pin(entries);
        let params = IdtrParameters {
            limit: (size_of_val(entries.as_ref().get_ref()) - 1) as u16,
            base: entries.as_ptr(),
        };

        info!("Loading IDT: {params:?}");

        // IDTをCPUにロード
        unsafe {
            asm!("lidt [rcx]", in("rcx") &params);
        }

        Self { entries }
    }
}

// =============================================================================
// TSS（Task State Segment）管理
// =============================================================================

/// TSS64の内部構造
#[repr(C, packed)]
struct TaskStateSegment64Inner {
    _reserved0: u32,
    _rsp: [u64; 3], // リング0-2への切り替え用
    _ist: [u64; 8], // ist[1]~ist[7] (ist[0]は予約済み)
    _reserved1: [u16; 5],
    _io_map_base_addr: u16,
}

const _: () = assert!(size_of::<TaskStateSegment64Inner>() == 104);

/// TSS64管理構造体
pub struct TaskStateSegment64 {
    inner: Pin<Box<TaskStateSegment64Inner>>,
}

impl TaskStateSegment64 {
    /// TSS64の物理アドレスを取得
    pub fn phys_addr(&self) -> u64 {
        self.inner.as_ref().get_ref() as *const TaskStateSegment64Inner as u64
    }

    /// 割り込み用スタックを割り当て
    unsafe fn alloc_interrupt_stack() -> u64 {
        const HANDLER_STACK_SIZE: usize = 64 * 1024;
        let stack = Box::new([0u8; HANDLER_STACK_SIZE]);
        let rsp = stack.as_ptr().add(HANDLER_STACK_SIZE) as u64;
        core::mem::forget(stack); // アロケータに所有権を渡さない
        rsp
    }

    /// 新しいTSS64を作成
    pub fn new() -> Self {
        let rsp0 = unsafe { Self::alloc_interrupt_stack() };
        let mut ist = [0u64; 8];

        // IST1-7用のスタックを割り当て
        for ist_entry in ist[1..=7].iter_mut() {
            *ist_entry = unsafe { Self::alloc_interrupt_stack() };
        }

        let tss64 = TaskStateSegment64Inner {
            _reserved0: 0,
            _rsp: [rsp0, 0, 0],
            _ist: ist,
            _reserved1: [0; 5],
            _io_map_base_addr: 0,
        };

        let this = Self {
            inner: Box::pin(tss64),
        };

        info!("TSS64 created @ {:#X}", this.phys_addr());
        this
    }
}

impl Drop for TaskStateSegment64 {
    fn drop(&mut self) {
        panic!("TSS64 being dropped!");
    }
}

// =============================================================================
// GDT（Global Descriptor Table）管理
// =============================================================================

/// GDT属性ビット定数
pub const BIT_TYPE_DATA: u64 = 0b10u64 << 43;
pub const BIT_TYPE_CODE: u64 = 0b11u64 << 43;
pub const BIT_PRESENT: u64 = 1u64 << 47;
pub const BIT_CS_LONG_MODE: u64 = 1u64 << 53;
pub const BIT_CS_READABLE: u64 = 1u64 << 41;
pub const BIT_DS_WRITABLE: u64 = 1u64 << 41;
pub const BIT_DPL0: u64 = 0u64 << 45;
pub const BIT_DPL3: u64 = 3u64 << 45;

/// GDT属性
#[repr(u64)]
enum GdtAttr {
    KernelCode = BIT_TYPE_CODE | BIT_PRESENT | BIT_CS_LONG_MODE | BIT_CS_READABLE,
    KernelData = BIT_TYPE_DATA | BIT_PRESENT | BIT_DS_WRITABLE,
}

/// GDTR パラメータ
#[repr(C, packed)]
struct GdtrParameters {
    limit: u16,
    base: *const Gdt,
}

/// セレクタ定数
pub const KERNEL_CS: u16 = 1 << 3;
pub const KERNEL_DS: u16 = 2 << 3;
pub const TSS64_SEL: u16 = 3 << 3;

/// GDT構造体
#[repr(C, packed)]
pub struct Gdt {
    null_segment: GdtSegmentDescriptor,
    kernel_code_segment: GdtSegmentDescriptor,
    kernel_data_segment: GdtSegmentDescriptor,
    task_state_segment: TaskStateSegment64Descriptor,
}

const _: () = assert!(size_of::<Gdt>() == 40);

/// GDTラッパー
pub struct GdtWrapper {
    inner: Pin<Box<Gdt>>,
    tss64: TaskStateSegment64,
}

impl GdtWrapper {
    /// GDTとTSSをCPUにロード
    pub fn load(&self) {
        let params = GdtrParameters {
            limit: (size_of::<Gdt>() - 1) as u16,
            base: self.inner.as_ref().get_ref() as *const Gdt,
        };

        info!("Loading GDT @ {:#018X}", params.base as u64);

        unsafe {
            asm!("lgdt [rcx]", in("rcx") &params);
        }

        info!("Loading TSS (selector = {:#X})", TSS64_SEL);

        unsafe {
            asm!("ltr cx", in("cx") TSS64_SEL);
        }
    }
}

impl Default for GdtWrapper {
    fn default() -> Self {
        let tss64 = TaskStateSegment64::new();
        let gdt = Gdt {
            null_segment: GdtSegmentDescriptor::null(),
            kernel_code_segment: GdtSegmentDescriptor::new(GdtAttr::KernelCode),
            kernel_data_segment: GdtSegmentDescriptor::new(GdtAttr::KernelData),
            task_state_segment: TaskStateSegment64Descriptor::new(tss64.phys_addr()),
        };

        let gdt = Box::pin(gdt);
        GdtWrapper { inner: gdt, tss64 }
    }
}

/// GDTセグメントディスクリプタ
pub struct GdtSegmentDescriptor {
    value: u64,
}

impl GdtSegmentDescriptor {
    const fn null() -> Self {
        Self { value: 0 }
    }

    const fn new(attr: GdtAttr) -> Self {
        Self { value: attr as u64 }
    }
}

impl fmt::Display for GdtSegmentDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#18X}", self.value)
    }
}

/// TSS64ディスクリプタ
#[repr(C, packed)]
struct TaskStateSegment64Descriptor {
    limit_low: u16,
    base_low: u16,
    base_mid_low: u8,
    attr: u16,
    base_mid_high: u8,
    base_high: u32,
    reserved: u32,
}

impl TaskStateSegment64Descriptor {
    const fn new(base_addr: u64) -> Self {
        Self {
            limit_low: size_of::<TaskStateSegment64Inner>() as u16,
            base_low: (base_addr & 0xffff) as u16,
            base_mid_low: ((base_addr >> 16) & 0xff) as u8,
            attr: 0b1000_0000_1000_1001,
            base_mid_high: ((base_addr >> 24) & 0xff) as u8,
            base_high: ((base_addr >> 32) & 0xffffffff) as u32,
            reserved: 0,
        }
    }
}

const _: () = assert!(size_of::<TaskStateSegment64Descriptor>() == 16);

// =============================================================================
// 公開API
// =============================================================================

/// 例外処理システムを初期化
pub fn init_exceptions() -> (GdtWrapper, Idt) {
    let gdt = GdtWrapper::default();
    gdt.load();

    // セグメントレジスタを設定
    unsafe {
        segment_registers::write_cs(KERNEL_CS);
        segment_registers::write_ss(KERNEL_DS);
        segment_registers::write_es(KERNEL_DS);
        segment_registers::write_ds(KERNEL_DS);
        segment_registers::write_fs(KERNEL_DS);
        segment_registers::write_gs(KERNEL_DS);
    }

    let idt = Idt::new(KERNEL_CS);
    (gdt, idt)
}

/// デバッグ割り込みをトリガー
pub fn trigger_debug_interrupt() {
    unsafe {
        asm!("int3");
    }
}

/// # Safety
/// Writing to CR3 can causes any exceptions so it is
/// programmer's responsibility to setup correct page tables.
#[no_mangle]
pub unsafe fn write_cr3(table: *const PML4) {
    asm!("mov cr3, rax",
            in("rax") table)
}

pub fn flush_tlb() {
    unsafe {
        write_cr3(read_cr3());
    }
}

/// # Safety
/// This will create a mutable reference to the page table structure.
/// So it is programmer's responsibility to ensure that at most one
/// instance of the reference exists at every moment.
pub unsafe fn take_current_page_table() -> ManuallyDrop<Box<PML4>> {
    ManuallyDrop::new(Box::from_raw(read_cr3()))
}

/// # Safety
/// This function sets the CR3 value so that anything bad can happen.
pub unsafe fn put_current_page_table(mut table: ManuallyDrop<Box<PML4>>) {
    // Set CR3 to reflect the updates and drop TLB caches.
    write_cr3(Box::into_raw(ManuallyDrop::take(&mut table)))
}

/// # Safety
/// This function modifies the page table as callback does, so
/// anything bad can happen if there are some mistakes.
pub unsafe fn with_current_page_table<F>(callback: F)
where
    F: FnOnce(&mut PML4),
{
    let mut table = take_current_page_table();
    callback(&mut table);
    put_current_page_table(table)
}
