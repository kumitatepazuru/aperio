import type { ConfigableValue } from "@/configable/utils";
import Configable from "@/configable/Configable";
import { initValues } from "@/configable/utils";
import useStore, { getStoreState } from "@shared/store";
import type { RequestStructureParameter } from "native";
import { useEffect, useState } from "react";
import { hasSameItems } from "@shared/utils/hasSame";

const ObjectParameter = () => {
  const [structures, setStructures] = useState<RequestStructureParameter[]>([]);
  const [params, setParams] = useState<Record<string, ConfigableValue>>({});

  const selectedItemId = useStore((state) => state.mainSelectedItemId);
  const setTimelineItems = useStore((state) => state.setTimelineItems);
  const selectedItem = useStore((state) =>
    state.timelineItems.find((item) => item.id === state.mainSelectedItemId),
  );

  const updateTimeline = async (newParams: Record<string, ConfigableValue>) => {
    const timeline = (await getStoreState()).timelineItems;
    setTimelineItems(
      timeline.map((item) =>
        item.id === selectedItemId
          ? { ...item, object: { ...item.object, parameters: newParams } }
          : item,
      ),
    );
  };

  useEffect(() => {
    if (!selectedItem) {
      setStructures([]);
      setParams({});
      return;
    }
    window.main
      .requestParameterStruct(
        selectedItem.object.name,
        selectedItem.object.parameters,
      )
      .then((struct) => {
        setStructures(struct.structure);
        setParams(initValues(struct.structure, selectedItem.object.parameters));
      })
      .catch(console.error);
    // selectedItemId が変わったときだけ初期化する
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedItemId]);

  const handleChange = async (newParams: Record<string, ConfigableValue>) => {
    if (!selectedItemId || !selectedItem) return;
    setParams(newParams);

    try {
      const struct = await window.main.requestParameterStruct(
        selectedItem.object.name,
        newParams,
      );
      if (
        hasSameItems(
          struct.structure.map((p) => p.id),
          structures.map((p) => p.id),
        )
      ) {
        updateTimeline(newParams);
      } else {
        setStructures(struct.structure);
        // 追加のパラメータを初期化する
        const newParamsWithInit = {
          ...initValues(struct.structure, newParams),
          ...newParams,
        };
        setParams(newParamsWithInit);
        await updateTimeline(newParamsWithInit);
      }
    } catch (error) {
      console.error("Error fetching parameter structure:", error);
    }
  };

  return (
    <Configable
      structures={structures}
      value={params}
      onChange={handleChange}
    />
  );
};

export default ObjectParameter;
