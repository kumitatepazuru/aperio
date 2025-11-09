import { useFrame } from "@react-three/fiber";
import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import useStore from "./store";
import { useShallow } from "zustand/shallow";
import { getFrame } from "./bridge";
import type { LayerStructure } from "native";

const FrameRenderer = () => {
  // テクスチャとマテリアルへの参照
  const textureRef = useRef<THREE.DataTexture | null>(null);
  const [texture, setTexture] = useState<THREE.DataTexture | null>(null);
  const { frameCount, setFrameCount, state, frame } = useStore(
    useShallow((state) => ({
      frame: state.timelineLayers,
      frameCount: state.frameCount,
      setFrameCount: state.setFrameCount,
      state: state.viewerState,
    }))
  );

  useEffect(() => {
    (async () => {
      const frameStruct: LayerStructure[] = frame.filter((layer) => {
        return frameCount >= layer.from && frameCount <= layer.to;
      }).sort((a, b) => a.layer - b.layer);

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
  }, [frame, frameCount, texture]);

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
