import { useFrame } from "@react-three/fiber";
import { useEffect, useMemo, useRef, useState } from "react";
import * as THREE from "three";
import Frame from "../bridge";
import type { FrameLayerStructure } from "native";

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

const FrameBufferRenderer = () => {
  // テクスチャとマテリアルへの参照
  const textureRef = useRef<THREE.DataTexture | null>(null);
  const frame = useMemo(() => new Frame(), []);
  const [frameData, setFrameData] =
    useState<Uint8Array<ArrayBufferLike> | null>(null);
  const countRef = useRef(0);

  // DataTextureをメモ化して初期化
  // getFrameから初回データを取得してテクスチャを生成します。
  const texture = useMemo(() => {
    if (!frameData) return null;
    console.log(frameData.length);

    // THREE.DataTextureを使用して、バイナリデータからテクスチャを生成
    const tex = new THREE.DataTexture(
      frameData,
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
    return tex;
  }, [frameData]);

  useEffect(() => {
    (async () => {
      const data = await frame.getBuf(0, frameStruct);

      setFrameData(new Uint8Array(data));
    })();
  }, [frame]);

  // 毎フレーム実行される処理
  useFrame(async () => {
    // テクスチャのデータを新しいデータで更新
    if (textureRef.current) {
      const data = await frame.getBuf(countRef.current, frameStruct);
      textureRef.current.image.data.set(new Uint8Array(data));
      // needsUpdateをtrueにすることで、GPUにテクスチャデータが再アップロードされます。
      textureRef.current.needsUpdate = true;
      countRef.current += 1;
    }
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

export default FrameBufferRenderer;
