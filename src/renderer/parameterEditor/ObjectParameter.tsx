import type { ConfigableValue } from "@/configable/utils";
import Configable from "@/configable/Configable";
import useStore, {
  getStoreState,
  type TimelineLayerStructure,
} from "@shared/store";
import type { RequestStructureParameter } from "native";
import { useState } from "react";
import { hasSameItems } from "@/utils/hasSame";

const ObjectParameter = () => {
  const [structures, setStructures] = useState<RequestStructureParameter[]>([]);
  // 一時的に構造を保存する。構造が変わったときに、構造は更新するがパラメーターは更新しないようにするため
  const [tempStructures, setTempStructures] = useState<
    TimelineLayerStructure["obj"]["parameters"] | null
  >(null);

  const selectedItemId = useStore((state) => state.selectedItemId);
  const setTimelineLayers = useStore((state) => state.setTimelineLayers);
  const selectedItem = useStore((state) =>
    state.timelineLayers.find((layer) => layer.id === selectedItemId),
  );

  const handleObjChange = async (values: Record<string, ConfigableValue>) => {
    if (!selectedItemId || !selectedItem) return;

    (async () => {
      try {
        const struct = await window.main.requestParameterStruct(
          selectedItem.obj.name,
          values,
        );
        const structForCheck = struct.map((param) => param.id);
        const structuresIds = structures.map((param) => param.id);
        // idがすべて一致するか確認
        if (hasSameItems(structForCheck, structuresIds)) {
          // 変更前と同じ構造なら更新
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
          setStructures(struct);
          setTempStructures(values);
        }
      } catch (error) {
        console.error("Error fetching parameter structure:", error);
      }
    })();
  };

  return (
    <Configable
      structures={structures}
      initialValues={tempStructures ?? selectedItem?.obj.parameters ?? {}}
      resetKey={selectedItemId ?? undefined}
      onChange={handleObjChange}
      onInit={handleObjChange}
    />
  );
};

export default ObjectParameter;
