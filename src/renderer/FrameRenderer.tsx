import { useFrame } from "@react-three/fiber";
import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import type { FrameLayerStructure } from "native";
import useStore from "./store";
import { useShallow } from "zustand/shallow";
import { getFrame } from "./bridge";

const frameStruct: FrameLayerStructure[] = [
  {
    x: 500,
    y: 500,
    scale: 3.0,
    rotation: 40.0,
    alpha: 0.8,
    obj: {
      name: "TestObject",
      parameters: {},
    },
    effects: [],
  },
];

const FrameRenderer = () => {
  // テクスチャとマテリアルへの参照
  const textureRef = useRef<THREE.DataTexture | null>(null);
  const [texture, setTexture] = useState<THREE.DataTexture | null>(null);
  const { frameCount, setFrameCount, state } = useStore(
    useShallow((state) => ({
      frameCount: state.frameCount,
      setFrameCount: state.setFrameCount,
      state: state.viewerState,
    }))
  );

  useEffect(() => {
    (async () => {
      if (!texture) {
        const data = await getFrame(0, frameStruct);

        // THREE.DataTextureを使用して、バイナリデータからテクスチャを生成
        const tex = new THREE.DataTexture(
          new Uint8Array(data),
          1920, // width
          1080, // height
          THREE.RGBAFormat,
          THREE.UnsignedByteType
        );
        tex.needsUpdate = true; // 初回更新を通知
        tex.flipY = true;
        tex.generateMipmaps = false;
        tex.wrapS = THREE.ClampToEdgeWrapping;
        tex.wrapT = THREE.ClampToEdgeWrapping;
        tex.minFilter = THREE.NearestFilter;
        tex.magFilter = THREE.NearestFilter;

        setTexture(tex);
      } else if (textureRef.current) {
        // テクスチャのデータを新しいデータで更新
        const data = await getFrame(frameCount, frameStruct);
        textureRef.current.image.data.set(new Uint8Array(data));
        // needsUpdateをtrueにすることで、GPUにテクスチャデータが再アップロードされます。
        textureRef.current.needsUpdate = true;
      }
    })();
  }, [frameCount, texture]);

  // 毎フレーム実行される処理
  useFrame(async () => {
    if (state !== "playing") return;
    setFrameCount(frameCount + 1);
  });

  return (
    texture && (
      <mesh>
        {/* 描画する平面のサイズを調整 */}
        <planeGeometry args={[8, 4.5]} />
        {/* テクスチャを貼り付けるための基本的なマテリアル */}
        <meshBasicMaterial>
          <primitive attach="map" object={texture} ref={textureRef} />
        </meshBasicMaterial>
      </mesh>
    )
  );
};

export default FrameRenderer;
