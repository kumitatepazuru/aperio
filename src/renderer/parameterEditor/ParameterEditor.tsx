import { useConfigable } from "@/configable/useConfigable";
import type { ConfigableValue, Vec2Value } from "@/configable/types";
import useStore, { getStoreState } from "@shared/store";
import type { RequestStructureParameter } from "native";
import { useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/shallow";

// TODO: ID重複の時にエラーを出す
const BaseParameter: RequestStructureParameter[] = [
  {
    type: "Vec2Int",
    id: "position",
    title: "位置",
    defaultX: 0,
    defaultY: 0,
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

const ParameterEditor = () => {
  const [structures, setStructures] = useState<
    Record<string, RequestStructureParameter[]>
  >({});
  const { timelineLayers, setTimelineLayers, selectedItemId } = useStore(
    useShallow((state) => ({
      timelineLayers: state.timelineLayers,
      setTimelineLayers: state.setTimelineLayers,
      selectedItemId: state.selectedItemId,
    })),
  );
  const selectedItem = useMemo(
    () => timelineLayers.find((layer) => layer.id === selectedItemId),
    [timelineLayers, selectedItemId],
  );
  const baseParams: Record<string, ConfigableValue> | null = useMemo(() => {
    if (!selectedItem) return null;
    return {
      position: { x: selectedItem.x, y: selectedItem.y },
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
      scale: (values.scale as number) / 100, // 0-100% -> 0-1
      alpha: (values.alpha as number) / 100, // 0-100% -> 0-1
      x: position.x,
      y: position.y,
    };
    const timeline = (await getStoreState()).timelineLayers;
    setTimelineLayers(
      timeline.map((layer) =>
        layer.id === selectedItemId ? { ...layer, ...updatedValues } : layer,
      ),
    );
  };

  const handleObjChange = async (values: Record<string, ConfigableValue>) => {
    if (!selectedItemId) return;
    const timeline = (await getStoreState()).timelineLayers;
    setTimelineLayers(
      timeline.map((layer) =>
        layer.id === selectedItemId
          ? { ...layer, obj: { ...layer.obj, parameters: values } }
          : layer,
      ),
    );
  };

  const { element: baseElement } = useConfigable(
    BaseParameter,
    baseParams ?? {},
    selectedItemId ?? undefined,
    handleBaseChange,
  );

  const { element: objElement } = useConfigable(
    structures[selectedItem?.obj.name ?? ""] ?? [],
    selectedItem?.obj.parameters ?? {},
    selectedItemId ?? undefined,
    handleObjChange,
  );

  useEffect(() => {
    if (!selectedItemId || !selectedItem) return;

    const pluginNames = [];
    pluginNames.push(selectedItem.obj.name);
    pluginNames.push(...selectedItem.effects.map((effect) => effect.name));

    // 取得されてなかったら構造体を取得する
    pluginNames.forEach((pluginName) => {
      if (structures[pluginName]) return;

      window.main.getParameterStruct(pluginName).then((struct) => {
        console.log("get struct", struct);
        setStructures((prev) => ({ ...prev, [pluginName]: struct }));
      });
    });
  }, [selectedItem, selectedItemId, structures]);

  return (
    <div className="p-2">
      {selectedItem ? (
        <div>
          <h2 className="text-lg font-bold mb-2">{selectedItem.obj.name}</h2>
          {baseElement}
          {objElement}
        </div>
      ) : null}
    </div>
  );
};

export default ParameterEditor;
