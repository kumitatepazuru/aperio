import { useState, useEffect, useCallback, type RefCallback } from "react";

type previewWebGPUBase = {
  fragmentShaderCode: string;
  width: number;
  height: number;
};

type webGPUResources = {
  device: GPUDevice;
  context: GPUCanvasContext;
  format: GPUTextureFormat;
  vertexShaderModule: GPUShaderModule;
  fragmentShaderModule: GPUShaderModule;
};

export type BufferWebGPUResources = {
  device: GPUDevice;
  context: GPUCanvasContext;
  pipeline: GPURenderPipeline;
  sampler: GPUSampler;
  bindGroupLayout: GPUBindGroupLayout;
  texture: GPUTexture;
};

export type ExternalTextureWebGPUResources = {
  device: GPUDevice;
  context: GPUCanvasContext;
  pipeline: GPURenderPipeline;
  sampler: GPUSampler;
  bindGroupLayout: GPUBindGroupLayout;
};

const vertexShaderCode = /* wgsl */ `
struct VertexOutput {
  @builtin(position) position: vec4f,
  @location(0) texCoord: vec2f,
}

@vertex
fn main(@builtin(vertex_index) vertexIndex: u32) -> VertexOutput {
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

let globalDevice: GPUDevice | null = null;

const initGlobalDevice = async () => {
  if (globalDevice) return globalDevice;
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) {
    throw new Error("Failed to get GPU adapter");
  }
  globalDevice = await adapter.requestDevice();
  return globalDevice;
};

const initPreviewWebGPU = async ({
  canvas,
  fragmentShaderCode,
  device,
}: {
  fragmentShaderCode: string;
  device: GPUDevice;
  canvas: HTMLCanvasElement | null;
}): Promise<webGPUResources | null> => {
  if (!canvas) return null;
  if (!navigator.gpu) {
    console.error("WebGPU is not supported");
    return null;
  }

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

  const vertexShaderModule = device.createShaderModule({
    code: vertexShaderCode,
  });
  const fragmentShaderModule = device.createShaderModule({
    code: fragmentShaderCode,
  });

  return { device, context, format, vertexShaderModule, fragmentShaderModule };
};

// パイプラインレイアウト・レンダーパイプラインの共通生成ヘルパー
const buildRenderPipeline = (
  { device, format, vertexShaderModule, fragmentShaderModule }: webGPUResources,
  bindGroupLayout: GPUBindGroupLayout,
): GPURenderPipeline => {
  const pipelineLayout = device.createPipelineLayout({
    bindGroupLayouts: [bindGroupLayout],
  });
  return device.createRenderPipeline({
    layout: pipelineLayout,
    vertex: { module: vertexShaderModule, entryPoint: "main" },
    fragment: {
      module: fragmentShaderModule,
      entryPoint: "main",
      targets: [{ format }],
    },
    primitive: { topology: "triangle-list" },
  });
};

// 両hookで共通のcanvas管理・初期化フローをまとめたベースhook
const usePreviewWebGPUBase = <T>(
  fragmentShaderCode: string,
  buildResources: (base: webGPUResources) => T,
  extraDeps: unknown[],
  onReplace?: (prev: T) => void,
): {
  resources: T | null;
  canvas: RefCallback<HTMLCanvasElement>;
} => {
  const [resources, setResources] = useState<T | null>(null);
  const [canvasElement, setCanvasElement] = useState<HTMLCanvasElement | null>(
    null,
  );

  const canvasRef = useCallback<RefCallback<HTMLCanvasElement>>((canvas) => {
    setCanvasElement(canvas);
  }, []);

  useEffect(() => {
    if (!canvasElement) return;

    let cancelled = false;

    const setupResources = async () => {
      try {
        const device = await initGlobalDevice();
        const baseResources = await initPreviewWebGPU({
          canvas: canvasElement,
          fragmentShaderCode,
          device,
        });

        if (cancelled || !baseResources) return;

        const nextResources = buildResources(baseResources);

        setResources((prev) => {
          if (prev !== null) onReplace?.(prev);
          return nextResources;
        });
      } catch (error) {
        if (cancelled) return;
        console.error("Failed to setup WebGPU resources:", error);
        setResources(null);
      }
    };

    setupResources();

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [canvasElement, fragmentShaderCode, ...extraDeps]);

  return { resources, canvas: canvasRef };
};

const buildBufferResources = (
  base: webGPUResources,
  width: number,
  height: number,
): BufferWebGPUResources => {
  console.warn(
    "BufferWebGPUResources is used. This may lead to suboptimal performance. Consider switching to useExternalTexturePreviewWebGPU for better performance.",
  );

  const { device, context } = base;

  const sampler = device.createSampler({
    magFilter: "nearest",
    minFilter: "nearest",
    addressModeU: "clamp-to-edge",
    addressModeV: "clamp-to-edge",
  });

  const texture = device.createTexture({
    size: { width, height },
    format: "rgba8unorm",
    usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
  });

  const bindGroupLayout = device.createBindGroupLayout({
    entries: [
      {
        binding: 0,
        visibility: GPUShaderStage.FRAGMENT,
        texture: { sampleType: "float" },
      },
      { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: {} },
    ],
  });

  const pipeline = buildRenderPipeline(base, bindGroupLayout);

  return { device, context, pipeline, sampler, bindGroupLayout, texture };
};

const buildExternalTextureResources = (
  base: webGPUResources,
): ExternalTextureWebGPUResources => {
  const { device, context } = base;

  const sampler = device.createSampler({
    magFilter: "linear",
    minFilter: "linear",
  });

  const bindGroupLayout = device.createBindGroupLayout({
    entries: [
      { binding: 0, visibility: GPUShaderStage.FRAGMENT, externalTexture: {} },
      { binding: 1, visibility: GPUShaderStage.FRAGMENT, sampler: {} },
    ],
  });

  const pipeline = buildRenderPipeline(base, bindGroupLayout);

  return { device, context, pipeline, sampler, bindGroupLayout };
};

const useBufferPreviewWebGPU = ({
  fragmentShaderCode,
  width,
  height,
}: previewWebGPUBase): {
  resources: BufferWebGPUResources | null;
  canvas: RefCallback<HTMLCanvasElement>;
} =>
  usePreviewWebGPUBase(
    fragmentShaderCode,
    (base) => buildBufferResources(base, width, height),
    [width, height],
    (prev) => prev.texture.destroy(),
  );

const useExternalTexturePreviewWebGPU = ({
  fragmentShaderCode,
  width,
  height,
}: previewWebGPUBase): {
  resources: ExternalTextureWebGPUResources | null;
  canvas: RefCallback<HTMLCanvasElement>;
} => {
  window.main.resizeOsr(width, height);

  return usePreviewWebGPUBase(
    fragmentShaderCode,
    buildExternalTextureResources,
    [],
  );
};

export { useBufferPreviewWebGPU, useExternalTexturePreviewWebGPU };
