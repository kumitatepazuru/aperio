import { useCallback, useEffect, useRef } from "react";
import FrameManager from "../bridge";
import useStore, { getCurrentFrameCount, getFrameStruct } from "@shared/store";
import { useShallow } from "zustand/shallow";
import {
  useBufferPreviewWebGPU,
  type BufferWebGPUResources,
} from "@/hooks/useWebGPU";

// RGBAバッファをテクスチャとして描画するためのシェーダー
const fragmentShaderCode = /* wgsl */ `
@group(0) @binding(0) var frameTexture: texture_2d<f32>;
@group(0) @binding(1) var texSampler: sampler;

@fragment
fn main(@location(0) texCoord: vec2f) -> @location(0) vec4f {
  return textureSample(frameTexture, texSampler, texCoord);
}
`;

const frameManager = new FrameManager();

const useFrameBufferRenderer = () => {
  const { resources, canvas: canvasRef } = useBufferPreviewWebGPU({
    fragmentShaderCode,
  });
  const animationFrameReserve = useRef<number | null>(null);
  const previousFrameCount = useRef<number | null>(null);
  const { viewerState, frameState } = useStore(
    useShallow((state) => ({
      viewerState: state.viewerState,
      frameState: state.frameState,
    })),
  );

  // テクスチャデータを更新して描画する関数
  const updateAndRender = useCallback(
    (data: Uint8Array<ArrayBuffer>, resources: BufferWebGPUResources) => {
      const { device, context, pipeline, sampler, bindGroupLayout, texture } =
        resources;

      // テクスチャデータの更新
      device.queue.writeTexture(
        { texture },
        data.buffer,
        {
          bytesPerRow: frameState.width * 4,
          rowsPerImage: frameState.height,
        },
        { width: frameState.width, height: frameState.height },
      );

      // バインドグループの作成
      const bindGroup = device.createBindGroup({
        layout: bindGroupLayout,
        entries: [
          {
            binding: 0,
            resource: texture.createView(),
          },
          {
            binding: 1,
            resource: sampler,
          },
        ],
      });

      // コマンドエンコーダーの作成
      const commandEncoder = device.createCommandEncoder();

      // レンダーパスの開始
      const renderPass = commandEncoder.beginRenderPass({
        colorAttachments: [
          {
            view: context.getCurrentTexture().createView(),
            clearValue: { r: 0, g: 0, b: 0, a: 1 },
            loadOp: "clear",
            storeOp: "store",
          },
        ],
      });

      renderPass.setPipeline(pipeline);
      renderPass.setBindGroup(0, bindGroup);
      renderPass.draw(6);
      renderPass.end();

      // コマンドの送信
      device.queue.submit([commandEncoder.finish()]);
    },
    [frameState],
  );

  // フレームループ
  const frameLoop = useCallback(async () => {
    const isPlaying = useStore.getState().viewerState.state === "playing";
    const currentFrameCount = getCurrentFrameCount();
    // 前回と同じフレームならスキップ
    // TODO: この前に音声1ブロック分の生成・再生処理を入れる
    if (!resources) {
      // リソースがまだ準備できていない場合はスキップ
      animationFrameReserve.current = null;
      return;
    } else if (previousFrameCount.current === currentFrameCount) {
      // フレームが前回と同じならスキップ
      animationFrameReserve.current = isPlaying
        ? requestAnimationFrame(frameLoop)
        : null;
      return;
    }

    try {
      // フレームデータを取得
      const data = await frameManager.getBuf(
        currentFrameCount,
        frameState.width,
        frameState.height,
        getFrameStruct(),
      );
      const uint8Data = new Uint8Array(data);

      // 描画
      updateAndRender(uint8Data, resources);
    } catch (error) {
      console.error("Error getting frame:", error);
    }

    // 次のフレームをスケジュール
    previousFrameCount.current = currentFrameCount;
    animationFrameReserve.current = isPlaying
      ? requestAnimationFrame(frameLoop)
      : null;
  }, [frameState, resources, updateAndRender]);

  useEffect(() => {
    // 初期化して最初のフレームをリクエスト
    previousFrameCount.current = null;
    animationFrameReserve.current = requestAnimationFrame(frameLoop);

    return () => {
      if (animationFrameReserve.current !== null) {
        cancelAnimationFrame(animationFrameReserve.current);
      }
    };
  }, [viewerState, resources, frameLoop]);

  return canvasRef;
};

export default useFrameBufferRenderer;
