import { useEffect, useRef } from "react";
import useStore from "@/store";
import { useShallow } from "zustand/shallow";
import {
  useExternalTexturePreviewWebGPU,
  type ExternalTextureWebGPUResources,
} from "@/hooks/useWebGPU";

// VideoFrameをcanvasに描画するためのシェーダー
const fragmentShaderCode = /* wgsl */ `
@group(0) @binding(0) var externalTexture: texture_external;
@group(0) @binding(1) var texSampler: sampler;

@fragment
fn main(@location(0) texCoord: vec2f) -> @location(0) vec4f {
  return textureSampleBaseClampToEdge(externalTexture, texSampler, texCoord);
}
`;

const FRAME_WIDTH = 1920;
const FRAME_HEIGHT = 1080;

const FrameTextureRenderer = () => {
  const { resources, canvas: canvasRef } = useExternalTexturePreviewWebGPU({
    fragmentShaderCode,
  });
  const animationFrameReserve = useRef<number | null>(null);
  const { viewerState, getFrameStruct, getCurrentFrameCount } = useStore(
    useShallow((state) => ({
      viewerState: state.viewerState,
      getFrameStruct: state.getFrameStruct,
      getCurrentFrameCount: state.getCurrentFrameCount,
    })),
  );

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
  const frameLoop = async () => {
    try {
      await window.frame.getFrameSharedTexture(
        getCurrentFrameCount(),
        getFrameStruct(),
      );
    } catch (error) {
      console.error("Error getting frame:", error);
    }
  };

  useEffect(() => {
    if (!resources) return;

    // レシーバーの設定
    window.frame.setReceiver(async (textureInfo) => {
      try {
        // VideoFrameを取得
        const videoFrame = textureInfo.importedSharedTexture.getVideoFrame();
        textureInfo.importedSharedTexture.release();

        // 描画
        renderVideoFrame(videoFrame, resources);

        // VideoFrameを速やかにclose
        videoFrame.close();
      } catch (error) {
        console.error("Error processing texture:", error);
      }

      animationFrameReserve.current =
        viewerState.state === "playing" ? requestAnimationFrame(frameLoop) : null;
    });

    // 最初のフレームをリクエスト
    animationFrameReserve.current = requestAnimationFrame(frameLoop);

    return () => {
      if (animationFrameReserve.current !== null) {
        cancelAnimationFrame(animationFrameReserve.current);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [viewerState, resources]);

  return (
    <canvas
      ref={canvasRef}
      width={FRAME_WIDTH}
      height={FRAME_HEIGHT}
      style={{ width: "100%", height: "100%" }}
    />
  );
};

export default FrameTextureRenderer;
