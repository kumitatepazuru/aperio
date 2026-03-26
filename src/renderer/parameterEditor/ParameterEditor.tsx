import { useConfigable } from "@/configable/useConfigable";
import type { ConfigableValue, Vec2Value } from "@/configable/types";
import useStore, { getStoreState } from "@shared/store";
import type { RequestStructureParameter } from "native";
import { useMemo, useState } from "react";
import { useShallow } from "zustand/shallow";

// TODO: ID重複の時にエラーを出す
const BaseParameter: RequestStructureParameter[] = [
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

const ParameterEditor = () => {
  const [structures, setStructures] = useState<RequestStructureParameter[]>([]);
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
      position: [selectedItem.x, selectedItem.y],
      scale: selectedItem.scale,
      rotation: selectedItem.rotation,
      alpha: selectedItem.alpha,
    };
  }, [selectedItem]);
  const [tempStructures, setTempStructures] = useState<
    (typeof timelineLayers)[0]["obj"]["parameters"] | null
  >(null);

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

  const handleObjChange = async (values: Record<string, ConfigableValue>) => {
    if (!selectedItemId || !selectedItem) return;

    (async () => {
      try {
        const struct = await window.main.requestParameterStruct(
          selectedItem.obj.name,
          values,
        );
        const structForCheck = struct.map((param) => param.id);
        if (structures.length === structForCheck.length) {
          // 変更前と同じ構造なら更新
          console.log("structures same, updating values");
          const timeline = (await getStoreState()).timelineLayers;
          setTimelineLayers(
            timeline.map((layer) =>
              layer.id === selectedItemId
                ? { ...layer, obj: { ...layer.obj, parameters: values } }
                : layer,
            ),
          );
          setTempStructures(null);
        } else {
          // 変更前と構造が違うなら構造を更新してパラメーターは更新しない
          console.log("structures updated");
          setStructures(struct);
          setTempStructures(values);
        }
      } catch (error) {
        console.error("Error fetching parameter structure:", error);
      }
    })();
  };

  const { element: baseElement } = useConfigable(
    BaseParameter,
    baseParams ?? {},
    selectedItemId ?? undefined,
    handleBaseChange,
  );

  const { element: objElement } = useConfigable(
    structures,
    tempStructures ?? selectedItem?.obj.parameters ?? {},
    selectedItemId ?? undefined,
    handleObjChange,
    handleObjChange,
  );

  return (
    <div className="p-2">
      {selectedItem ? (
        <div>
          <h2 className="text-lg font-bold mb-2">{selectedItem.obj.name}</h2>
          <div className="flex flex-col gap-3">
            {baseElement}
            {objElement}
          </div>
        </div>
      ) : null}
    </div>
  );
};

export default ParameterEditor;
