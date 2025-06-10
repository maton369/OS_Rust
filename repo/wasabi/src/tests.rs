#![cfg(test)]

use crate::allocator::ALLOCATOR;
use core::alloc::Layout;
use core::ptr::null_mut;

use crate::test_runner::test_runner;

fn malloc_align_test() {
    let mut pointers = [null_mut::<u8>(); 10];
    for align in [1, 2, 4, 8, 16, 32, 64, 4096] {
        for p in &mut pointers {
            unsafe {
                *p = ALLOCATOR.alloc_with_options(Layout::from_size_align(128, align).unwrap());
                assert!((*p as usize) % align == 0, "Pointer not aligned: {:p}", *p);
            }
        }
    }
}

fn malloc_dealloc_test() {
    let layout = Layout::from_size_align(256, 16).unwrap();
    let ptr = unsafe { ALLOCATOR.alloc_with_options(layout) };
    assert!(!ptr.is_null(), "malloc_dealloc_test: allocation failed");
    unsafe {
        ALLOCATOR.dealloc(ptr, layout);
    }
}

/// `cargo run` 時に呼び出されるテストランナーのエントリーポイント
pub fn test_main() -> ! {
    let tests: &[&dyn Fn()] = &[&malloc_align_test, &malloc_dealloc_test];
    test_runner(tests);
}
