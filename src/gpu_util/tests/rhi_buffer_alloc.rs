// CPUから書き込み可能なバッファを確保でき、マップされたポインタへ読み書きできることを確認する。

use gpu_util::rhi::{BufferUsage, Device};

#[test]
fn allocates_and_writes_host_visible_buffer() {
    let device = match Device::new() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Skipping allocates_and_writes_host_visible_buffer: no GPU device ({e:#})");
            return;
        }
    };

    let size = 256u64;
    let buffer = device
        .create_buffer(size, BufferUsage::STORAGE, true, "test host-visible buffer")
        .expect("buffer allocation should succeed");
    let ptr = buffer
        .mapped_ptr()
        .expect("host-visible allocation should be mapped")
        .cast::<u8>();

    unsafe {
        for i in 0..size as usize {
            ptr.as_ptr().add(i).write(i as u8);
        }
        for i in 0..size as usize {
            assert_eq!(ptr.as_ptr().add(i).read(), i as u8);
        }
    }
}
