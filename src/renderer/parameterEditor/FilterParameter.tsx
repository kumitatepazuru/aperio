import Configable from "@/configable/Configable";
import type { ConfigableValue } from "@/configable/utils";
import useStore, {
  getStoreState,
  type TimelineLayerStructure,
} from "@shared/store";
import type { RequestStructureParameter } from "native";
import { useState } from "react";

const FilterParameter = () => {
  const [structures, setStructures] = useState<RequestStructureParameter[][]>(
    [],
  );
  const [tempStructures, setTempStructures] = useState<
    TimelineLayerStructure["obj"]["parameters"] | null
  >(null);

  const selectedItemId = useStore((state) => state.selectedItemId);
  const setTimelineLayers = useStore((state) => state.setTimelineLayers);
  const selectedItem = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === selectedItemId),
  );

  const handleObjChange = async (
    id: string,
    index: number,
    values: Record<string, ConfigableValue>,
  ) => {
    if (!selectedItemId || !selectedItem) return;

    (async () => {
      try {
        const struct = await window.main.requestParameterStruct(id, values);
        const structForCheck = struct.map((param) => param.id);
        const structuresIds = structures[index]?.map((param) => param.id) || [];
        // idがすべて一致するか確認
        if (structForCheck.every((id) => structuresIds.includes(id))) {
          // 変更前と同じ構造なら更新
          const timeline = (await getStoreState()).timelineLayers;
          setTimelineLayers(
            timeline.map((layer) =>
              layer.id === selectedItemId
                ? {
                    ...layer,
                    effects: layer.effects.map((effect, i) =>
                      i === index ? { ...effect, parameters: values } : effect,
                    ),
                  }
                : layer,
            ),
          );
          setTempStructures(null);
        } else {
          // 変更前と構造が違うなら構造を更新してパラメーターは更新しない
          setStructures((prev) => ({
            ...prev,
            [index]: struct,
          }));
          setTempStructures(values);
        }
      } catch (error) {
        console.error("Error fetching parameter structure:", error);
      }
    })();
  };

  return selectedItem?.effects.map((effect, index) => (
    <Configable
      key={`${effect.name}-${index}`}
      structures={structures[index] || []}
      initialValues={tempStructures ?? selectedItem?.obj.parameters ?? {}}
      resetKey={selectedItemId ?? undefined}
      onChange={(values) => handleObjChange(effect.name, index, values)}
      onInit={(values) => handleObjChange(effect.name, index, values)}
    />
  ));
};

export default FilterParameter;
