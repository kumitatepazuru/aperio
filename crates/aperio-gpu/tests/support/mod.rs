// D3D12共有テクスチャ連携テストの共通ヘルパー(Windows専用)。
//
// テスト側でD3D12のデバイスとリソースを作り、共有ハンドルをVulkanへ渡して書き込ませ、
// 結果をD3D12側で読み戻して検証する。

#![cfg(target_os = "windows")]
#![allow(dead_code)]

use std::mem::ManuallyDrop;

use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{GENERIC_ALL, HANDLE, LUID};
use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory4};

pub struct SharedTarget {
    pub device: ID3D12Device,
    pub resource: ID3D12Resource,
    /// CreateSharedHandleで得たNTハンドル。`Drop`で閉じる。
    pub handle: HANDLE,
    pub width: u32,
    pub height: u32,
    pub format: DXGI_FORMAT,
    pub bytes_per_pixel: u32,
}

impl Drop for SharedTarget {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.handle) };
    }
}

impl SharedTarget {
    /// Vulkanデバイスと同じアダプタ(LUID一致)上に、共有可能なレンダーターゲットを作る。
    pub fn new(
        luid: [u8; 8],
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
        bytes_per_pixel: u32,
    ) -> windows::core::Result<Self> {
        let factory: IDXGIFactory4 = unsafe { CreateDXGIFactory1()? };
        let luid = LUID {
            LowPart: u32::from_le_bytes(luid[0..4].try_into().unwrap()),
            HighPart: i32::from_le_bytes(luid[4..8].try_into().unwrap()),
        };
        let adapter: IDXGIAdapter1 = unsafe { factory.EnumAdapterByLuid(luid)? };

        let mut device: Option<ID3D12Device> = None;
        unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device)? };
        let device = device.expect("D3D12CreateDevice returned no device");

        let heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_DEFAULT,
            ..Default::default()
        };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: width as u64,
            Height: height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
        };
        let mut resource: Option<ID3D12Resource> = None;
        unsafe {
            device.CreateCommittedResource(
                &heap,
                D3D12_HEAP_FLAG_SHARED,
                &desc,
                D3D12_RESOURCE_STATE_COMMON,
                None,
                &mut resource,
            )?
        };
        let resource = resource.expect("CreateCommittedResource returned no resource");
        let handle =
            unsafe { device.CreateSharedHandle(&resource, None, GENERIC_ALL.0, PCWSTR::null())? };

        Ok(Self {
            device,
            resource,
            handle,
            width,
            height,
            format,
            bytes_per_pixel,
        })
    }

    /// NTハンドルを`SharedTextureHandle.nt_handle`が期待するバイト列(ネイティブエンディアン)にする。
    pub fn handle_bytes(&self) -> Vec<u8> {
        (self.handle.0 as usize).to_ne_bytes().to_vec()
    }

    /// リソース全体をD3D12側で読み戻す(行詰めのバイト列)。
    pub fn read_back(&self) -> windows::core::Result<Vec<u8>> {
        let row_bytes = self.width * self.bytes_per_pixel;
        // D3D12の行ピッチは256バイト境界に揃える必要がある。
        let row_pitch = row_bytes.div_ceil(D3D12_TEXTURE_DATA_PITCH_ALIGNMENT)
            * D3D12_TEXTURE_DATA_PITCH_ALIGNMENT;
        let buffer_size = row_pitch as u64 * self.height as u64;

        let readback_heap = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE_READBACK,
            ..Default::default()
        };
        let buffer_desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Alignment: 0,
            Width: buffer_size,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT(0),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_NONE,
        };
        let mut readback: Option<ID3D12Resource> = None;
        unsafe {
            self.device.CreateCommittedResource(
                &readback_heap,
                D3D12_HEAP_FLAG_NONE,
                &buffer_desc,
                D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                &mut readback,
            )?
        };
        let readback = readback.expect("CreateCommittedResource returned no buffer");

        let queue: ID3D12CommandQueue = unsafe {
            self.device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            })?
        };
        let allocator: ID3D12CommandAllocator = unsafe {
            self.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)?
        };
        let list: ID3D12GraphicsCommandList = unsafe {
            self.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)?
        };

        unsafe {
            // Vulkan側の書き込みはすでに完了している(同期submit)ので、COMMONからコピー元へ遷移する。
            let barrier = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        pResource: ManuallyDrop::new(Some(self.resource.clone())),
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                        StateBefore: D3D12_RESOURCE_STATE_COMMON,
                        StateAfter: D3D12_RESOURCE_STATE_COPY_SOURCE,
                    }),
                },
            };
            list.ResourceBarrier(&[barrier]);

            let src = D3D12_TEXTURE_COPY_LOCATION {
                pResource: ManuallyDrop::new(Some(self.resource.clone())),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };
            let dst = D3D12_TEXTURE_COPY_LOCATION {
                pResource: ManuallyDrop::new(Some(readback.clone())),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                        Offset: 0,
                        Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                            Format: self.format,
                            Width: self.width,
                            Height: self.height,
                            Depth: 1,
                            RowPitch: row_pitch,
                        },
                    },
                },
            };
            list.CopyTextureRegion(&dst, 0, 0, 0, &src, None);
            list.Close()?;
            queue.ExecuteCommandLists(&[Some(list.cast()?)]);

            let fence: ID3D12Fence = self.device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?;
            queue.Signal(&fence, 1)?;
            while fence.GetCompletedValue() < 1 {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        let mut mapped = std::ptr::null_mut();
        unsafe { readback.Map(0, None, Some(&mut mapped))? };
        let mut out = Vec::with_capacity((row_bytes * self.height) as usize);
        for row in 0..self.height as usize {
            let row_ptr = unsafe { (mapped as *const u8).add(row * row_pitch as usize) };
            out.extend_from_slice(unsafe { std::slice::from_raw_parts(row_ptr, row_bytes as usize) });
        }
        unsafe { readback.Unmap(0, None) };
        Ok(out)
    }
}
