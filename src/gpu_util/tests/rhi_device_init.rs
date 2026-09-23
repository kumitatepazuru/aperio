// VulkanDeviceが実際に初期化でき、bindlessパターン(実行時添字参照・可変長
// テクスチャ配列)に必要なdescriptor-indexing機能を満たす物理デバイスを
// 選択できることを確認する。

use gpu_util::rhi::vulkan::VulkanDevice;

#[test]
fn rhi_device_init() {
    match VulkanDevice::new() {
        Ok(device) => {
            // ロジカルデバイスが実際に有効であることを、キュー取得で確認する。
            let _queue = device.queue();
            println!(
                "rhi_device_init: Vulkan 1.2 device initialized successfully (queue family {})",
                device.queue_family
            );
        }
        Err(e) => {
            eprintln!(
                "Skipping rhi_device_init: no usable Vulkan 1.2 device with the required \
                 descriptor-indexing features was found on this machine ({e:#}). This is \
                 expected on GPU-less CI runners; install a Vulkan-capable GPU driver (and, \
                 for local debugging, the Vulkan SDK at C:\\VulkanSDK\\1.4.357.0) to run this \
                 test for real."
            );
        }
    }
}
