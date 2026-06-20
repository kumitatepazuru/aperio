import type { ConfigableValue, Vec2Value } from "@/configable/utils";
import Configable from "@/configable/Configable";
import useStore, { getStoreState } from "@shared/store";
import type { RequestStructureParameter } from "native";
import { useMemo } from "react";

// TODO: ID重複の時にエラーを出す
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
  },
];

const audioBaseParameterStructure: RequestStructureParameter[] = [
  {
    type: "Float",
    id: "volume",
    title: "音量",
    defaultValue: 100,
    suffix: "%",
  },
  {
    type: "Float",
    id: "pan",
    title: "パン",
    defaultValue: 0,
    suffix: "%",
  },
];

const BaseParameter = () => {
  const selectedItemId = useStore((state) => state.mainSelectedItemId);
  const setTimelineItems = useStore((state) => state.setTimelineItems);
  const selectedItem = useStore((state) =>
    state.timelineItems.find((item) => item.id === state.mainSelectedItemId),
  );

  const baseParams = useMemo<Record<string, ConfigableValue> | null>(() => {
    if (!selectedItem) return null;
    if (selectedItem.type === "Video") {
      const r: Record<string, ConfigableValue> = {
        position: [selectedItem.x, selectedItem.y],
        scale: selectedItem.scale,
        rotation: selectedItem.rotation,
        alpha: selectedItem.alpha,
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
          ? videoBaseParameterStructure
          : audioBaseParameterStructure
      }
      value={baseParams ?? {}}
      onChange={handleBaseChange}
    />
  );
};

export default BaseParameter;
