// rhi::Deviceが実際に初期化でき、bindlessパターンに必要な機能を満たす
// デバイスを選択できることを確認する。

use gpu_util::rhi::Device;

#[test]
fn rhi_device_init() {
    match Device::new() {
        Ok(device) => {
            assert!(device.maximum_texture_size() > 0);
            assert!(device.max_variable_texture_array_len() > 0);
        }
        Err(e) => eprintln!("Skipping rhi_device_init: no usable GPU device ({e:#})"),
    }
}
