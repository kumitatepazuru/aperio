import { useEffect, useRef, useCallback } from "react";
import type { FrameLayerStructure } from "native";

const frameStruct: FrameLayerStructure[] = [
  {
    x: 500,
    y: 500,
    scale: 3.0,
    rotation: 40.0,
    alpha: 1.0,
    obj: {
      name: "TestObject",
      parameters: {},
    },
    effects: [],
  },
];

// VideoFrameをcanvasに描画するためのシェーダー
const vertexShaderCode = /* wgsl */ `
struct VertexOutput {
  @builtin(position) position: vec4f,
  @location(0) texCoord: vec2f,
}

@vertex
fn main(@builtin(vertex_index) vertexIndex: u32) -> VertexOutput {
  // フルスクリーン三角形（2つの三角形で四角形を描画）
  var positions = array<vec2f, 6>(
    vec2f(-1.0, -1.0),
    vec2f( 1.0, -1.0),
    vec2f(-1.0,  1.0),
    vec2f(-1.0,  1.0),
    vec2f( 1.0, -1.0),
    vec2f( 1.0,  1.0),
  );
  
  var texCoords = array<vec2f, 6>(
    vec2f(0.0, 1.0),
    vec2f(1.0, 1.0),
    vec2f(0.0, 0.0),
    vec2f(0.0, 0.0),
    vec2f(1.0, 1.0),
    vec2f(1.0, 0.0),
  );

  var output: VertexOutput;
  output.position = vec4f(positions[vertexIndex], 0.0, 1.0);
  output.texCoord = texCoords[vertexIndex];
  return output;
}
`;

const fragmentShaderCode = /* wgsl */ `
@group(0) @binding(0) var externalTexture: texture_external;
@group(0) @binding(1) var texSampler: sampler;

@fragment
fn main(@location(0) texCoord: vec2f) -> @location(0) vec4f {
  return textureSampleBaseClampToEdge(externalTexture, texSampler, texCoord);
}
`;

interface WebGPUResources {
  device: GPUDevice;
  context: GPUCanvasContext;
  pipeline: GPURenderPipeline;
  sampler: GPUSampler;
  bindGroupLayout: GPUBindGroupLayout;
}

const FrameTextureRenderer = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const resourcesRef = useRef<WebGPUResources | null>(null);
  const animationFrameRef = useRef<number | null>(null);
  const isRunningRef = useRef(false);
  const countRef = useRef(0);

  // WebGPUリソースの初期化
  const initWebGPU = useCallback(async (): Promise<WebGPUResources | null> => {
    const canvas = canvasRef.current;
    if (!canvas) return null;

    // WebGPUのサポートチェック
    if (!navigator.gpu) {
      console.error("WebGPU is not supported");
      return null;
    }

    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) {
      console.error("Failed to get GPU adapter");
      return null;
    }

    const device = await adapter.requestDevice();

    const context = canvas.getContext("webgpu");
    if (!context) {
      console.error("Failed to get WebGPU context");
      return null;
    }

    const format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({
      device,
      format,
      alphaMode: "premultiplied",
    });

    // シェーダーモジュールの作成
    const vertexShaderModule = device.createShaderModule({
      code: vertexShaderCode,
    });

    const fragmentShaderModule = device.createShaderModule({
      code: fragmentShaderCode,
    });

    // サンプラーの作成
    const sampler = device.createSampler({
      magFilter: "linear",
      minFilter: "linear",
    });

    // バインドグループレイアウトの作成
    const bindGroupLayout = device.createBindGroupLayout({
      entries: [
        {
          binding: 0,
          visibility: GPUShaderStage.FRAGMENT,
          externalTexture: {},
        },
        {
          binding: 1,
          visibility: GPUShaderStage.FRAGMENT,
          sampler: {},
        },
      ],
    });

    // パイプラインレイアウトの作成
    const pipelineLayout = device.createPipelineLayout({
      bindGroupLayouts: [bindGroupLayout],
    });

    // レンダーパイプラインの作成
    const pipeline = device.createRenderPipeline({
      layout: pipelineLayout,
      vertex: {
        module: vertexShaderModule,
        entryPoint: "main",
      },
      fragment: {
        module: fragmentShaderModule,
        entryPoint: "main",
        targets: [{ format }],
      },
      primitive: {
        topology: "triangle-list",
      },
    });

    return {
      device,
      context,
      pipeline,
      sampler,
      bindGroupLayout,
    };
  }, []);

  // VideoFrameを描画する関数
  const renderVideoFrame = useCallback(
    (videoFrame: VideoFrame, resources: WebGPUResources) => {
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

      // // コマンドエンコーダーの作成
      const commandEncoder = device.createCommandEncoder();

      // // レンダーパスの開始
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
    []
  );

  // フレームループ
  const frameLoop = useCallback(async () => {
    if (!isRunningRef.current || !resourcesRef.current) return;

    try {
      // getFrameSharedTextureを呼び出し
      await window.frame.getFrameSharedTexture(countRef.current, frameStruct);
      countRef.current += 1;
    } catch (error) {
      console.error("Error getting frame:", error);
    }

    // 次のフレームをスケジュール
    if (isRunningRef.current) {
      animationFrameRef.current = requestAnimationFrame(frameLoop);
    }
  }, []);

  useEffect(() => {
    let mounted = true;

    const setup = async () => {
      // WebGPUリソースの初期化
      const resources = await initWebGPU();
      if (!mounted || !resources) return;

      resourcesRef.current = resources;

      // レシーバーの設定
      window.frame.setReceiver(async (textureInfo) => {
        if (!mounted || !resourcesRef.current) return;

        try {
          // VideoFrameを取得
          const videoFrame = textureInfo.importedSharedTexture.getVideoFrame();
          textureInfo.importedSharedTexture.release();

          // 描画
          renderVideoFrame(videoFrame, resourcesRef.current);

          // VideoFrameを速やかにclose
          videoFrame.close();
        } catch (error) {
          console.error("Error processing texture:", error);
        }
      });

      // フレームループの開始
      isRunningRef.current = true;
      animationFrameRef.current = requestAnimationFrame(frameLoop);
    };

    setup();

    return () => {
      mounted = false;
      isRunningRef.current = false;
      if (animationFrameRef.current !== null) {
        cancelAnimationFrame(animationFrameRef.current);
      }
      // WebGPUリソースのクリーンアップ
      if (resourcesRef.current) {
        resourcesRef.current.device.destroy();
        resourcesRef.current = null;
      }
    };
  }, [initWebGPU, renderVideoFrame, frameLoop]);

  return (
    <canvas
      ref={canvasRef}
      width={1920}
      height={1080}
      style={{ width: "100%", height: "100%" }}
    />
  );
};

export default FrameTextureRenderer;
