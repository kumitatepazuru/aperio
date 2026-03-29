import type { ConfigableValue, Vec2Value } from "@/configable/utils";
import Configable from "@/configable/Configable";
import useStore, { getStoreState } from "@shared/store";
import type { RequestStructureParameter } from "native";
import { useMemo } from "react";

// TODO: ID重複の時にエラーを出す
const BaseParameterStructure: RequestStructureParameter[] = [
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

const BaseParameter = () => {
  const selectedItemId = useStore((state) => state.selectedItemId);
  const setTimelineLayers = useStore((state) => state.setTimelineLayers);
  const selectedItem = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === selectedItemId),
  );

  const baseParams: Record<string, ConfigableValue> | null = useMemo(() => {
    if (!selectedItem) return null;
    return {
      position: [selectedItem.x, selectedItem.y],
      scale: selectedItem.scale,
      rotation: selectedItem.rotation,
      alpha: selectedItem.alpha,
    };
  }, [selectedItem]);

  const handleBaseChange = async (values: Record<string, ConfigableValue>) => {
    if (!selectedItemId) return;

    // positionだけxyに分けて保存する
    const position = values.position as Vec2Value;
    const updatedValues = {
      ...values,
      position: undefined,
      x: position[0],
      y: position[1],
    };
    const timeline = (await getStoreState()).timelineLayers;
    setTimelineLayers(
      timeline.map((layer) =>
        layer.id === selectedItemId ? { ...layer, ...updatedValues } : layer,
      ),
    );
  };

  return (
    <Configable
      structures={BaseParameterStructure}
      initialValues={baseParams ?? {}}
      resetKey={selectedItemId ?? undefined}
      onChange={handleBaseChange}
    />
  );
};

export default BaseParameter;
