import { useEffect, useRef, useMemo } from "react";
import FrameManager from "../bridge";
import useStore from "@/store";
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

const useFrameBufferRenderer = (width: number, height: number) => {
  const { resources, canvas: canvasRef } = useBufferPreviewWebGPU({
    fragmentShaderCode,
    width,
    height,
  });
  const animationFrameReserve = useRef<number | null>(null);
  const frameManager = useMemo(() => new FrameManager(), []);
  const previousFrameCount = useRef<number | null>(null);
  const { viewerState, getFrameStruct, getCurrentFrameCount } = useStore(
    useShallow((state) => ({
      viewerState: state.viewerState,
      getFrameStruct: state.getFrameStruct,
      getCurrentFrameCount: state.getCurrentFrameCount,
    })),
  );

  // テクスチャデータを更新して描画する関数
  const updateAndRender = (
    data: Uint8Array<ArrayBuffer>,
    resources: BufferWebGPUResources,
  ) => {
    const { device, context, pipeline, sampler, bindGroupLayout, texture } =
      resources;

    // テクスチャデータの更新
    device.queue.writeTexture(
      { texture },
      data.buffer,
      {
        bytesPerRow: width * 4,
        rowsPerImage: height,
      },
      { width, height },
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
  };

  // フレームループ
  const frameLoop = async () => {
    const isPlaying = viewerState.state === "playing";
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
  };

  useEffect(() => {
    // 最初のフレームをリクエスト
    animationFrameReserve.current = requestAnimationFrame(frameLoop);

    return () => {
      if (animationFrameReserve.current !== null) {
        cancelAnimationFrame(animationFrameReserve.current);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [viewerState, resources]);

  return canvasRef;
};

export default useFrameBufferRenderer;
