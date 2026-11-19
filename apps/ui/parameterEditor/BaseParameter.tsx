import type { ConfigableValue, Vec2Value } from "@/configable/utils";
import Configable from "@/configable/Configable";
import { DEFAULT_BLEND_MODE_ID } from "@aperio/shared";
import useStore, { getStoreState } from "@aperio/ui/store";
import type { RequestStructureParameter } from "native";
import { useEffect, useMemo, useState } from "react";

// TODO: ID重複の時にエラーを出す
// TODO: プラグイン側で書き換えられるようにする
const videoBaseParameterStructure: RequestStructureParameter[] = [
  {
    type: "Vec2Int",
    id: "position",
    title: "位置",
    defaultValue: [0, 0],
    suffix: "px",
  },
  {
    type: "Float",
    id: "scale",
    title: "拡大率",
    defaultValue: 100,
    suffix: "%",
    min: 0,
  },
  {
    type: "Float",
    id: "rotation",
    title: "回転",
    defaultValue: 0,
    suffix: "°",
  },
  {
    type: "Float",
    id: "alpha",
    title: "透明度",
    defaultValue: 100,
    suffix: "%",
    min: 0,
    max: 100,
  },
];

const audioBaseParameterStructure: RequestStructureParameter[] = [
  {
    type: "Float",
    id: "volume",
    title: "音量",
    defaultValue: 100,
    suffix: "%",
    min: 0,
  },
  {
    type: "Float",
    id: "pan",
    title: "パン",
    defaultValue: 0,
    suffix: "%",
    min: -100,
    max: 100,
  },
];

/** 合成モードのプルダウン。 */
const blendModeParameter = (
  blendModes: Record<string, string>,
  current: string,
): RequestStructureParameter => ({
  type: "List",
  id: "blendMode",
  title: "合成モード",
  // 登録が外れた id を保存しているアイテムでも、選択中の値がそのまま見えるようにする
  values:
    current in blendModes
      ? blendModes
      : { ...blendModes, [current]: `${current} (不明)` },
  defaultValue: DEFAULT_BLEND_MODE_ID,
});

const BaseParameter = () => {
  const selectedItemId = useStore((state) => state.mainSelectedItemId);
  const setTimelineItems = useStore((state) => state.setTimelineItems);
  const selectedItem = useStore((state) =>
    state.timelineItems.find((item) => item.id === state.mainSelectedItemId),
  );
  const [blendModes, setBlendModes] = useState<Record<string, string>>({});

  useEffect(() => {
    window.main.getBlendMode().then(setBlendModes);
  }, []);

  const videoBlendMode =
    selectedItem?.type === "Video"
      ? selectedItem.blendMode
      : DEFAULT_BLEND_MODE_ID;

  const videoStructure = useMemo(
    () => [
      ...videoBaseParameterStructure,
      blendModeParameter(blendModes, videoBlendMode),
    ],
    [blendModes, videoBlendMode],
  );

  const baseParams = useMemo<Record<string, ConfigableValue> | null>(() => {
    if (!selectedItem) return null;
    if (selectedItem.type === "Video") {
      const r: Record<string, ConfigableValue> = {
        position: [selectedItem.x, selectedItem.y],
        scale: selectedItem.scale,
        rotation: selectedItem.rotation,
        alpha: selectedItem.alpha,
        blendMode: selectedItem.blendMode,
      };
      return r;
    } else {
      const r: Record<string, ConfigableValue> = {
        volume: selectedItem.volume,
        pan: selectedItem.pan,
      };
      return r;
    }
  }, [selectedItem]);

  const handleBaseChange = async (params: Record<string, ConfigableValue>) => {
    if (!selectedItemId) return;

    let updatedParams = {};

    if (selectedItem?.type === "Video") {
      // positionだけxyに分けて保存する
      const position = params.position as Vec2Value;
      updatedParams = {
        ...params,
        position: undefined,
        x: position[0],
        y: position[1],
      };
    } else {
      updatedParams = params;
    }
    const timeline = (await getStoreState()).timelineItems;
    setTimelineItems(
      timeline.map((item) =>
        item.id === selectedItemId ? { ...item, ...updatedParams } : item,
      ),
    );
  };

  return (
    <Configable
      structures={
        selectedItem?.type === "Video"
          ? videoStructure
          : audioBaseParameterStructure
      }
      value={baseParams ?? {}}
      onChange={handleBaseChange}
    />
  );
};

export default BaseParameter;
