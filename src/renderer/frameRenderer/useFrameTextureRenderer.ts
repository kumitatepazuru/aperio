import { useCallback, useEffect, useRef } from "react";
import useStore, {
  getCurrentAudioItems,
  getCurrentFrameCount,
  getCurrentVideoItems,
  getStoreState,
} from "@shared/store";
import {
  useExternalTexturePreviewWebGPU,
  type ExternalTextureWebGPUResources,
} from "@/hooks/useWebGPU";
import type { ReceivedSharedTextureData } from "electron";
import type { ItemResult } from "native";

// VideoFrameをcanvasに描画するためのシェーダー
const fragmentShaderCode = /* wgsl */ `
@group(0) @binding(0) var externalTexture: texture_external;
@group(0) @binding(1) var texSampler: sampler;

@fragment
fn main(@location(0) texCoord: vec2f) -> @location(0) vec4f {
  return textureSampleBaseClampToEdge(externalTexture, texSampler, texCoord);
}
`;

const useFrameTextureRenderer = () => {
  const { resources, canvas: canvasRef } = useExternalTexturePreviewWebGPU({
    fragmentShaderCode,
  });
  const animationFrameReserve = useRef<number | null>(null);
  const viewerState = useStore((state) => state.viewerState);
  const frameState = useStore((state) => state.frameState);
  const previousFrameCount = useRef<number | null>(null);
  const timelineItems = useStore((state) => state.timelineItems);

  // VideoFrameを描画する関数
  const renderVideoFrame = (
    videoFrame: VideoFrame,
    resources: ExternalTextureWebGPUResources,
  ) => {
    const { device, context, pipeline, sampler, bindGroupLayout } = resources;

    // external textureのインポート
    const externalTexture = device.importExternalTexture({
      source: videoFrame,
    });

    // バインドグループの作成
    const bindGroup = device.createBindGroup({
      layout: bindGroupLayout,
      entries: [
        {
          binding: 0,
          resource: externalTexture,
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
  const frameLoop = useCallback(async () => {
    const currentFrameCount = await getCurrentFrameCount();
    const storeState = await getStoreState();
    if (previousFrameCount.current === currentFrameCount) {
      // フレームが前回と同じならスキップ
      animationFrameReserve.current =
        storeState.viewerState.state === "playing"
          ? requestAnimationFrame(frameLoop)
          : null;
      return;
    }

    // TODO: 音声の生成をフレーム生成と並列で行い、ここでは音声の再生のみにする
    window.audio.play(
      await getCurrentAudioItems(storeState.frameState.fps), // 1秒分
      storeState.audioState.sampleRate,
      storeState.audioState.channels,
      (await getCurrentFrameCount()) / storeState.frameState.fps, // 現在の再生位置
      1.0,
    );
    if (storeState.viewerState.state === "paused") {
      requestAnimationFrame(() => {
        // 次のフレームで音を止める
        window.audio.stop();
      });
    }

    try {
      await window.frame.getFrameSharedTexture(
        currentFrameCount,
        await getCurrentVideoItems(),
      );
      previousFrameCount.current = currentFrameCount;
    } catch (error) {
      console.error("Error getting frame:", error);
    }
  }, []);

  useEffect(() => {
    const receiver = async (
      textureInfo: ReceivedSharedTextureData,
      frameResults: Record<string, ItemResult>,
    ) => {
      try {
        // VideoFrameを取得
        const videoFrame = textureInfo.importedSharedTexture.getVideoFrame();
        textureInfo.importedSharedTexture.release();

        (await getStoreState()).setFrameResults(frameResults);

        // 描画
        if (resources) {
          renderVideoFrame(videoFrame, resources);
        }

        // VideoFrameを速やかにclose
        videoFrame.close();
      } catch (error) {
        console.error("Error processing texture:", error);
      }

      animationFrameReserve.current =
        (await getStoreState()).viewerState.state === "playing"
          ? requestAnimationFrame(frameLoop)
          : null;
    };

    // レシーバーの設定
    window.frame.setReceiver(receiver);
  }, [frameLoop, resources]);

  useEffect(() => {
    if (!resources) return;

    // 初期化して最初のフレームをリクエスト
    previousFrameCount.current = null;
    animationFrameReserve.current = requestAnimationFrame(frameLoop);

    return () => {
      if (animationFrameReserve.current !== null) {
        cancelAnimationFrame(animationFrameReserve.current);
      }
    };
  }, [viewerState, frameState, resources, frameLoop, timelineItems]);

  return canvasRef;
};

export default useFrameTextureRenderer;
