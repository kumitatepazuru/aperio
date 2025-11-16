import { useFrame } from "@react-three/fiber";
import type { FrameLayerStructure } from "native";
import { useEffect, useMemo } from "react";
import * as THREE from "three";

const frameStruct: FrameLayerStructure[] = [
  {
    x: 500,
    y: 500,
    scale: 3.0,
    rotation: 40.0,
    obj: {
      name: "TestObject",
      parameters: {},
    },
    effects: [],
  },
];

const FrameTextureRenderer = () => {
  // 描画用 canvas
  const canvas = useMemo(() => {
    const canvas = document.createElement("canvas");
    canvas.width = 1920; // TODO
    canvas.height = 1080; // TODO
    const context2d = canvas.getContext("2d");
    if (!context2d) return canvas;

    // 背景を黒で塗りつぶし
    // 最初になにか書かないとWebGPUが動作しない
    context2d.fillStyle = "rgb(255, 0, 0)";
    context2d.fillRect(0, 0, canvas.width, canvas.height);

    return canvas;
  }, []);

  // three.js の CanvasTexture
  const texture = useMemo(() => {
    console.log(canvas);

    const tex = new THREE.CanvasTexture(canvas);
    tex.colorSpace = THREE.SRGBColorSpace; // WebGPURenderer で必須

    return tex;
  }, [canvas]);

  useEffect(() => {
    const ctx = canvas.getContext("2d");

    // ここで sharedTexture の受信設定を行う
    window.frame.setReceiver(async (textureInfo) => {
      if (!ctx) return;
      
      const videoFrame = textureInfo.importedSharedTexture.getVideoFrame();
      textureInfo.importedSharedTexture.release();
      ctx.drawImage(videoFrame, 0, 0, canvas.width, canvas.height);
      videoFrame.close();

      // texture.needsUpdate = true;
    });
  }, [canvas]);

  useFrame(async () => {
    await window.frame.getFrameSharedTexture(0, frameStruct);
  });

  const aspect = 1920 / 1080; // TODO: 動的に取得
  const planeSize = useMemo<[number, number]>(() => {
    const h = 1.5; // 好きな大きさ
    return [h * aspect, h];
  }, [aspect]);

  return (
    <mesh>
      <planeGeometry args={planeSize} />
      <meshBasicMaterial map={texture} toneMapped={false} />
    </mesh>
  );
};

export default FrameTextureRenderer;
